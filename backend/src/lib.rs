//! Prumo — mantém a carteira no eixo.
//!
//! `domain` tem as regras puras (classes, nota, o motor do aporte); `store` o
//! Postgres; `quotes` o provedor de cotações; `treasury` a tabela diária do
//! Tesouro Direto; `service` junta tudo; `http` expõe a API e os estáticos.

pub mod auth;
pub mod b3_xlsx;
pub mod config;
pub mod dividends;
pub mod domain;
pub mod error;
pub mod history;
pub mod http;
pub mod income;
pub mod live;
pub mod performance;
pub mod quotes;
pub mod sectors;
pub mod service;
pub mod store;
pub mod telemetry;
pub mod treasury;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::auth::{AccessAuthenticator, Authenticator, DevAuthenticator};
use crate::config::{AuthConfig, Config};
use crate::dividends::{DisabledDividends, DividendProvider, YahooDividends};
use crate::history::{DisabledHistory, HistoryProvider, PublicHistory};
use crate::http::{AppState, BuildInfo};
use crate::income::IncomeService;
use crate::live::yahoo::YahooStreamer;
use crate::live::LiveQuotes;
use crate::quotes::{QuoteError, QuoteProvider, SymbolMatch, YahooProvider};
use crate::sectors::SectorService;
use crate::service::QuoteService;
use crate::store::Store;
use crate::treasury::{TransparenteFeed, TreasurySync};

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("banco: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migracao: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("rede: {0}")]
    Io(#[from] std::io::Error),
    #[error("cliente http: {0}")]
    Http(#[from] reqwest::Error),
}

/// De quanto em quanto tempo os preços do streamer vão para o banco (e para a
/// tela). Um segundo é o passo do próprio streamer na B3.
const LIVE_FLUSH_EVERY: Duration = Duration::from_secs(1);
/// Rodada de conferência do que assinar; ativo novo ou apagado já refaz na hora.
const LIVE_TRACK_EVERY: Duration = Duration::from_secs(60);

/// Provedor que não cota nada — para `QUOTES_ENABLED=false`.
pub struct DisabledQuotes;

#[async_trait]
impl QuoteProvider for DisabledQuotes {
    async fn quote(&self, _symbol: &str) -> Result<quotes::Quote, QuoteError> {
        Err(QuoteError::Unavailable("cotacoes desligadas".into()))
    }

    async fn search(&self, _query: &str) -> Result<Vec<SymbolMatch>, QuoteError> {
        Err(QuoteError::Unavailable("cotacoes desligadas".into()))
    }
}

fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (compatible; prumo/",
            env!("CARGO_PKG_VERSION"),
            "; +https://github.com/slipalison/prumo)"
        ))
        .connect_timeout(Duration::from_secs(5))
        .build()
}

/// Sobe tudo e só volta quando o processo recebe sinal para encerrar.
pub async fn run(config: Config) -> Result<(), StartError> {
    let store = connect_with_retry(&config.database_url, config.database_max_connections).await?;
    store.migrate().await?;
    tracing::info!("migracoes aplicadas");

    let http = http_client()?;
    if config.sectors_enabled {
        Arc::new(SectorService::with_public_sources(store.clone(), &http)).spawn();
    }
    let auth: Arc<dyn Authenticator> = match &config.auth {
        AuthConfig::CloudflareAccess {
            team_domain,
            audience,
        } => Arc::new(AccessAuthenticator::new(
            team_domain,
            audience,
            http.clone(),
        )),
        AuthConfig::Dev { email } => {
            tracing::warn!(email = %email, "AUTH_MODE=dev: toda requisicao entra como esta pessoa");
            Arc::new(DevAuthenticator::new(email.clone()))
        }
    };
    if let Some(at) = config.snapshot_at {
        let history: Arc<dyn HistoryProvider> = if config.quotes_enabled {
            Arc::new(PublicHistory::new(http.clone()))
        } else {
            Arc::new(DisabledHistory)
        };
        Arc::new(performance::Performance::new(store.clone(), history)).spawn(at);
    }
    let (provider, dividends): (Arc<dyn QuoteProvider>, Arc<dyn DividendProvider>) =
        if config.quotes_enabled {
            (
                Arc::new(YahooProvider::new(http.clone())),
                Arc::new(YahooDividends::new(http.clone())),
            )
        } else {
            (Arc::new(DisabledQuotes), Arc::new(DisabledDividends))
        };
    let income = Arc::new(IncomeService::new(store.clone(), dividends));
    let live = LiveQuotes::new(store.clone());
    let quotes = Arc::new(QuoteService::new(
        store.clone(),
        provider,
        Arc::clone(&live),
    ));
    if config.quotes_enabled {
        Arc::clone(&quotes).spawn_refresher(config.quotes_refresh);
        // O histórico de dividendos da estimativa de renda: fora do caminho das
        // requisições, uma conferência por hora (só o vencido vai ao Yahoo).
        Arc::clone(&income).spawn_refresher(income::CHECK_EVERY);
        // O preço do Tesouro Direto: uma tabela por dia, fora do streamer.
        Arc::new(TreasurySync::new(
            store.clone(),
            Arc::new(TransparenteFeed::new(http.clone())),
            Arc::clone(&live),
        ))
        .spawn(treasury::CHECK_EVERY, treasury::MAX_AGE);
        if config.quotes_stream {
            live.spawn(LIVE_FLUSH_EVERY, LIVE_TRACK_EVERY);
            tokio::spawn(YahooStreamer::default().run(live.symbols(), live.ticks()));
        }
    }

    let state = AppState::new(
        store,
        auth,
        quotes,
        BuildInfo {
            version: config.version.clone(),
            commit: config.commit.clone(),
        },
        telemetry::install_metrics(),
    )
    .with_income_service(income);
    let public = http::public_router(state.clone(), &config.static_dir);
    let operational = http::operational_router(state.clone());

    let public_listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    let operational_listener =
        tokio::net::TcpListener::bind(("0.0.0.0", config.operational_port)).await?;
    tracing::info!(port = config.port, operational_port = config.operational_port, version = %config.version, commit = %config.commit, "prumo no ar");

    let wait = |mut rx: tokio::sync::watch::Receiver<bool>| async move {
        let _ = rx.wait_for(|stop| *stop).await;
    };
    let public_server =
        axum::serve(public_listener, public).with_graceful_shutdown(wait(state.stopping()));
    let operational_server = axum::serve(operational_listener, operational)
        .with_graceful_shutdown(wait(state.stopping()));
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("encerrando");
        state.stop();
    });
    let (public_result, operational_result) = tokio::join!(public_server, operational_server);
    public_result?;
    operational_result?;
    Ok(())
}

/// O Postgres pode demorar a aceitar conexão logo depois de um deploy.
async fn connect_with_retry(url: &str, max_connections: u32) -> Result<Store, sqlx::Error> {
    let mut attempt = 0u32;
    loop {
        match Store::connect_with(url, max_connections).await {
            Ok(store) => return Ok(store),
            Err(error) if attempt < 10 => {
                attempt += 1;
                tracing::warn!(attempt, error = %error, "banco ainda indisponivel, tentando de novo");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
