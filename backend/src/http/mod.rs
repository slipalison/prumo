//! Rotas, estado e extratores.

mod alerts;
mod allocation;
mod analysis;
mod b3_import;
mod cash;
pub mod dto;
mod handlers;
pub(crate) mod health;
mod icons;
mod income;
mod income_auto;
mod ledger;
mod live;
mod objectives;
mod ops;
mod performance;
mod profit;
pub(crate) mod tax;
mod treasury;
mod yoc;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, MatchedPath, Request};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use time::OffsetDateTime;
use tokio::sync::{watch, Mutex, RwLock};
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::alerts::AlertService;
use crate::auth::Authenticator;
use crate::dividends::DividendProvider;
use crate::error::ApiError;
use crate::icons::IconService;
use crate::income::IncomeService;
use crate::service::QuoteService;
use crate::store::Store;

/// Versão e commit, para `/api/v1/info`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub auth: Arc<dyn Authenticator>,
    pub quotes: Arc<QuoteService>,
    pub income: Arc<IncomeService>,
    pub alerts: Arc<AlertService>,
    pub icons: Arc<IconService>,
    pub info: Arc<BuildInfo>,
    pub metrics: Option<PrometheusHandle>,
    users: Arc<RwLock<HashMap<String, Uuid>>>,
    refreshes: Arc<Mutex<HashMap<Uuid, Instant>>>,
    stop: Arc<watch::Sender<bool>>,
}

/// Intervalo mínimo entre duas atualizações manuais de cotação da mesma pessoa.
pub const MANUAL_REFRESH_COOLDOWN: Duration = Duration::from_secs(60);

impl AppState {
    pub fn new(
        store: Store,
        auth: Arc<dyn Authenticator>,
        quotes: Arc<QuoteService>,
        info: BuildInfo,
        metrics: Option<PrometheusHandle>,
    ) -> Self {
        Self {
            income: Arc::new(IncomeService::disabled(store.clone())),
            alerts: Arc::new(AlertService::disabled(store.clone())),
            icons: Arc::new(IconService::disabled(store.clone())),
            store,
            auth,
            quotes,
            info: Arc::new(info),
            metrics,
            users: Arc::default(),
            refreshes: Arc::default(),
            stop: Arc::new(watch::channel(false).0),
        }
    }

    /// Liga a estimativa de renda a um provedor de histórico de dividendos
    /// (sem ele, só vale o que já está no cache).
    pub fn with_dividend_provider(self, provider: Arc<dyn DividendProvider>) -> Self {
        let income = Arc::new(IncomeService::new(self.store.clone(), provider));
        self.with_income_service(income)
    }

    /// Usa um serviço de renda já montado (o `run` o guarda para a tarefa
    /// periódica que enche o cache).
    pub fn with_income_service(mut self, income: Arc<IncomeService>) -> Self {
        self.income = income;
        self
    }

    /// Usa o serviço de alertas já montado (o `run` o guarda para as tarefas de
    /// leitura do Telegram e de resumo diário).
    pub fn with_alerts_service(mut self, alerts: Arc<AlertService>) -> Self {
        self.alerts = alerts;
        self
    }

    /// Usa o serviço de logos já montado (o `run` o guarda para a tarefa
    /// periódica e para a busca de ativo novo).
    pub fn with_icon_service(mut self, icons: Arc<IconService>) -> Self {
        self.icons = icons;
        self
    }

    /// Começa o encerramento: os servidores param de aceitar conexão e as
    /// conexões longas (o stream da carteira) terminam, para o desligamento
    /// gracioso não esperar por elas.
    pub fn stop(&self) {
        self.stop.send_replace(true);
    }

    /// `true` a partir do [`AppState::stop`].
    pub fn stopping(&self) -> watch::Receiver<bool> {
        self.stop.subscribe()
    }

    /// `true` se a pessoa pode atualizar agora (e marca o momento).
    async fn take_refresh_slot(&self, user: Uuid) -> bool {
        let mut marks = self.refreshes.lock().await;
        let now = Instant::now();
        match marks.get(&user) {
            Some(at) if now.duration_since(*at) < MANUAL_REFRESH_COOLDOWN => false,
            _ => {
                marks.insert(user, now);
                true
            }
        }
    }
}

/// A pessoa autenticada, já com a carteira garantida no banco.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    /// Quando a credencial vence (ver [`crate::auth::Identity`]).
    pub expires_at: Option<OffsetDateTime>,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = state.auth.authenticate(&parts.headers).await?;
        if let Some(id) = state.users.read().await.get(&identity.email) {
            return Ok(CurrentUser {
                id: *id,
                email: identity.email,
                expires_at: identity.expires_at,
            });
        }
        let id = state
            .store
            .ensure_user(&identity.email)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "nao foi possivel garantir a carteira");
                ApiError::internal()
            })?;
        state.users.write().await.insert(identity.email.clone(), id);
        Ok(CurrentUser {
            id,
            email: identity.email,
            expires_at: identity.expires_at,
        })
    }
}

/// `Json` com rejeição em problem+json. Só aceita OBJETO: o serde montaria
/// uma struct a partir de um array (`[6000]` viraria `{"amount": 6000}`), e um
/// contrato de objetos não deve aceitar a forma posicional.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) =
            axum::Json::<serde_json::Value>::from_request(request, state).await?;
        if !value.is_object() {
            return Err(ApiError::validation("O corpo precisa ser um objeto JSON."));
        }
        serde_json::from_value(value)
            .map(ApiJson)
            .map_err(|error| ApiError::validation(error.to_string()))
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        ApiError::validation(rejection.body_text())
    }
}

/// `Path` cujo valor inválido é 404, como um id que não existe.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        ApiError::not_found()
    }
}

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        ApiError::validation(rejection.body_text())
    }
}

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data:; font-src 'self'; connect-src 'self'; manifest-src 'self'; \
                   object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

/// A porta pública: API e os estáticos do PWA.
pub fn public_router(state: AppState, static_dir: &str) -> Router {
    let api = Router::new()
        .route("/info", get(handlers::info))
        .route("/me", get(handlers::me))
        .route("/portfolio", get(handlers::portfolio))
        .route("/portfolio/stream", get(live::portfolio_stream))
        .route("/goals", put(handlers::set_goals))
        .route("/assets", post(handlers::create_asset))
        .route(
            "/assets/{id}",
            patch(handlers::update_asset).delete(handlers::delete_asset),
        )
        .route(
            "/checklists/{checklist}/questions",
            post(handlers::add_question),
        )
        .route(
            "/checklists/{checklist}/reset",
            post(handlers::reset_checklist),
        )
        .route(
            "/checklist-questions/{id}",
            patch(handlers::update_question).delete(handlers::delete_question),
        )
        .route("/plans", post(handlers::plan))
        .route(
            "/contributions",
            get(handlers::contributions).post(handlers::record_contribution),
        )
        .route("/analysis", get(analysis::analysis))
        .route(
            "/analysis/rules",
            get(analysis::rules).put(analysis::save_rules),
        )
        .route("/health", get(health::health))
        .route("/health/rules", get(health::rules).put(health::save_rules))
        .route("/transactions", get(ledger::list).post(ledger::create))
        .route(
            "/transactions/{id}",
            patch(ledger::update).delete(ledger::delete),
        )
        .route("/income", get(income::list).post(income::create))
        .route("/income/summary", get(income::summary))
        .route("/income/entitlement", get(income::entitlement))
        .route("/income/yield-on-cost", get(yoc::show))
        .route("/income/auto", get(income_auto::get).put(income_auto::save))
        .route("/cash", get(cash::show))
        .route(
            "/income/goal",
            get(income::get_goal)
                .put(income::set_goal)
                .delete(income::delete_goal),
        )
        .route("/income/{id}", patch(income::update).delete(income::delete))
        .route(
            "/objectives",
            get(objectives::list).post(objectives::create),
        )
        .route("/objectives/impact", post(objectives::impact))
        .route(
            "/objectives/{id}",
            patch(objectives::update).delete(objectives::delete),
        )
        .route("/alerts", get(alerts::get).put(alerts::save))
        .route(
            "/alerts/telegram",
            post(alerts::link).delete(alerts::unlink),
        )
        .route("/alerts/test", post(alerts::test))
        .route("/symbols", get(handlers::symbols))
        .route("/quotes/refresh", post(handlers::refresh_quotes))
        .route("/history/net-worth", get(performance::net_worth))
        .route("/performance", get(performance::performance))
        .route(
            "/settings/allocation",
            get(allocation::settings).put(allocation::save_settings),
        )
        .route("/tax/months", get(tax::months))
        .route("/tax/irpf/{year}", get(tax::irpf))
        .route(
            "/tax/darfs/{year}/{month}/paid",
            post(tax::mark_paid).delete(tax::unmark_paid),
        )
        .route(
            "/tax/carryforward",
            get(tax::get_carryforward).put(tax::put_carryforward),
        )
        .fallback(|| async { ApiError::not_found() })
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        // Depois do teto geral: a importacao da B3 tem o corpo maior, so dela.
        .merge(b3_import::router())
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));

    let index = format!("{}/index.html", static_dir.trim_end_matches('/'));
    let assets = Router::new()
        .fallback_service(ServeDir::new(format!(
            "{}/assets",
            static_dir.trim_end_matches('/')
        )))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ));
    let shell = ServeDir::new(static_dir).fallback(ServeFile::new(index));
    let not_public = || async { StatusCode::NOT_FOUND };

    Router::new()
        // Os logos ficam fora do `api`: a resposta do logo traz o próprio
        // `Cache-Control` (um dia, com validador), e a camada acima o trocaria
        // por `no-store`. Mesmo prefixo, mesma autenticação.
        .merge(icons::router())
        .nest("/api/v1", api)
        .route("/api", get(|| async { ApiError::not_found() }))
        .route(
            "/api/{*rest}",
            axum::routing::any(|| async { ApiError::not_found() }),
        )
        .route("/healthz", get(not_public))
        .route("/readyz", get(not_public))
        .route("/metrics", get(not_public))
        .nest("/assets", assets)
        .fallback_service(
            tower::ServiceBuilder::new()
                // `no-transform`: a Cloudflare não mexe no HTML. Sem ele, ela
                // injeta o beacon do Web Analytics (static.cloudflareinsights.com)
                // e o script inline da detecção de robôs, que a nossa CSP
                // (`script-src 'self'`) recusa com erro no console. O app fica
                // atrás do Access: a detecção de robôs não tem o que medir aqui,
                // e no plano gratuito ela não desliga por host. A documentação
                // da Cloudflare: com `no-transform`, nada é injetado.
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-cache, no-transform"),
                ))
                .service(shell),
        )
        .layer(middleware::from_fn(record_metrics))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// A porta interna: sondas e métricas. A internet não alcança.
pub fn operational_router(state: AppState) -> Router {
    ops::router(state)
}

async fn record_metrics(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "estaticos".to_owned());
    let method = request.method().to_string();
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16().to_string();
    metrics::counter!("http_requests_total", "method" => method.clone(), "route" => route.clone(), "status" => status)
        .increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method, "route" => route)
        .record(started.elapsed().as_secs_f64());
    response.into_response()
}
