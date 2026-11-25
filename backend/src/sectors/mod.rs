//! Setor e segmento dos papéis, de fontes públicas e gratuitas, guardados na
//! tabela de referência (`sector_reference`) e atualizados de tempos em tempos.
//!
//! - [`b3`]: Classificação Setorial da B3 (planilha pública) — Ações Nacionais.
//! - [`cvm`]: informe mensal de FII da CVM (dados abertos) — Fundos Imobiliários.
//!
//! Ações dos EUA ficam **sem setor**: a única fonte pública seria o código SIC
//! do SEC EDGAR, que exige um User-Agent com contato real (nome e e-mail) e
//! responde 403 ao cliente genérico do Prumo. O Prumo não manda dado de
//! ninguém a serviço externo e não contorna bloqueio; ver `docs/saude.md`.
//!
//! Fonte fora do ar não quebra a carteira: o setor fica vazio (ou como estava
//! na última busca que deu certo) e a próxima rodada tenta de novo.

pub mod b3;
pub mod cvm;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::sector::SectorSource;
use crate::store::sectors::SyncState;
use crate::store::Store;

/// Uma linha da classificação: o código de quatro caracteres do emissor e onde
/// ele está.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorEntry {
    pub code: String,
    pub sector: String,
    pub subsector: Option<String>,
    pub segment: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SectorError {
    #[error("fonte indisponivel: {0}")]
    Unavailable(String),
    #[error("formato inesperado: {0}")]
    Format(String),
}

/// Uma fonte pública de setor.
#[async_trait]
pub trait SectorFeed: Send + Sync {
    fn source(&self) -> SectorSource;

    /// A classificação inteira, como a fonte a publica hoje.
    async fn fetch(&self) -> Result<Vec<SectorEntry>, SectorError>;
}

/// Passado este tempo desde a última busca que deu certo, busca de novo. As
/// duas fontes mudam devagar (a B3 revisa de vez em quando; a CVM publica um
/// informe por mês).
pub const REFRESH_AFTER: Duration = Duration::from_secs(30 * 24 * 3600);
/// Depois de uma falha, espera isto antes de tentar de novo: não bate na fonte
/// a cada rodada enquanto ela está fora.
pub const RETRY_AFTER: Duration = Duration::from_secs(6 * 3600);
/// De quanto em quanto tempo o serviço confere se alguma fonte venceu.
pub const CHECK_EVERY: Duration = Duration::from_secs(3600);
/// Maior corpo aceito de uma fonte. O informe da CVM compactado passa de 1 MB.
const MAX_DOWNLOAD: usize = 64 * 1024 * 1024;
/// Prazo de cada download.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// Baixa `url` com teto de tamanho. 404 e erro de rede viram
/// [`SectorError::Unavailable`] com o motivo.
pub(crate) async fn download(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, SectorError> {
    let mut response = http
        .get(url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|e| SectorError::Unavailable(e.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(SectorError::Unavailable(format!("status {status}")));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| SectorError::Unavailable(e.without_url().to_string()))?
    {
        if body.len() + chunk.len() > MAX_DOWNLOAD {
            return Err(SectorError::Format("arquivo grande demais".to_owned()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Se uma fonte precisa ser buscada agora, dado o que a última rodada deixou.
pub fn is_due(state: Option<&SyncState>, now: OffsetDateTime) -> bool {
    let Some(state) = state else {
        return true;
    };
    let waited = |since: OffsetDateTime, wait: Duration| now - since >= wait;
    if !waited(state.last_attempt_at, RETRY_AFTER) && state.last_error.is_some() {
        return false;
    }
    match state.last_success_at {
        None => waited(state.last_attempt_at, RETRY_AFTER),
        Some(at) => waited(at, REFRESH_AFTER),
    }
}

/// O resultado de buscar uma fonte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub source: SectorSource,
    pub result: Result<usize, String>,
}

/// Mantém a tabela de referência em dia.
pub struct SectorService {
    store: Store,
    feeds: Vec<Arc<dyn SectorFeed>>,
}

impl SectorService {
    pub fn new(store: Store, feeds: Vec<Arc<dyn SectorFeed>>) -> Self {
        Self { store, feeds }
    }

    /// As fontes de produção, com o cliente HTTP do app.
    pub fn with_public_sources(store: Store, http: &reqwest::Client) -> Self {
        Self::new(
            store,
            vec![
                Arc::new(b3::B3Classification::new(http.clone())),
                Arc::new(cvm::CvmFunds::new(http.clone())),
            ],
        )
    }

    /// Busca as fontes vencidas. Falha de uma não impede a outra.
    pub async fn refresh_due(&self, now: OffsetDateTime) -> Vec<SyncOutcome> {
        let mut outcomes = Vec::new();
        for feed in &self.feeds {
            let source = feed.source();
            let state = match self.store.sector_sync_state(source).await {
                Ok(state) => state,
                Err(error) => {
                    tracing::error!(source = source.as_str(), error = %error, "estado da referencia de setor ilegivel");
                    continue;
                }
            };
            if !is_due(state.as_ref(), now) {
                continue;
            }
            outcomes.push(self.refresh_one(feed.as_ref(), state, now).await);
        }
        outcomes
    }

    /// Busca uma fonte agora, vencida ou não.
    pub async fn refresh_one(
        &self,
        feed: &dyn SectorFeed,
        state: Option<SyncState>,
        now: OffsetDateTime,
    ) -> SyncOutcome {
        let source = feed.source();
        let result = self.fetch_and_store(feed, state.as_ref(), now).await;
        match &result {
            Ok(entries) => {
                tracing::info!(
                    source = source.as_str(),
                    entries,
                    "referencia de setor atualizada"
                );
            }
            Err(reason) => {
                tracing::warn!(source = source.as_str(), reason = %reason, "referencia de setor nao atualizada");
                if let Err(error) = self
                    .store
                    .record_sector_sync_failure(source, now, reason)
                    .await
                {
                    tracing::error!(source = source.as_str(), error = %error, "falha nao registrada");
                }
            }
        }
        SyncOutcome { source, result }
    }

    async fn fetch_and_store(
        &self,
        feed: &dyn SectorFeed,
        state: Option<&SyncState>,
        now: OffsetDateTime,
    ) -> Result<usize, String> {
        let entries = feed.fetch().await.map_err(|e| e.to_string())?;
        if entries.is_empty() {
            return Err("a fonte voltou sem nenhuma linha".to_owned());
        }
        // Arquivo cortado ou formato que mudou: não troca uma referência boa
        // por uma que perdeu mais da metade.
        let previous = state.map_or(0, |s| s.entries);
        if previous > 0 && entries.len() < previous / 2 {
            return Err(format!(
                "queda suspeita: {} linhas contra {previous} da ultima vez",
                entries.len()
            ));
        }
        self.store
            .replace_sector_reference(feed.source(), &entries, now)
            .await
            .map_err(|e| e.to_string())?;
        Ok(entries.len())
    }

    /// Rodada periódica, até o processo encerrar. A primeira confere já.
    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(CHECK_EVERY);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                self.refresh_due(OffsetDateTime::now_utc()).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration as Span;

    fn state(
        attempt_ago: Span,
        success_ago: Option<Span>,
        error: Option<&str>,
        now: OffsetDateTime,
    ) -> SyncState {
        SyncState {
            last_attempt_at: now - attempt_ago,
            last_success_at: success_ago.map(|ago| now - ago),
            entries: 10,
            last_error: error.map(str::to_owned),
        }
    }

    #[test]
    fn a_source_never_fetched_is_due() {
        assert!(is_due(None, OffsetDateTime::now_utc()));
    }

    #[test]
    fn a_fresh_success_waits_the_month() {
        let now = OffsetDateTime::now_utc();
        let fresh = state(Span::days(2), Some(Span::days(2)), None, now);
        assert!(!is_due(Some(&fresh), now));
        let old = state(Span::days(31), Some(Span::days(31)), None, now);
        assert!(is_due(Some(&old), now));
    }

    #[test]
    fn a_failure_waits_before_trying_again() {
        let now = OffsetDateTime::now_utc();
        // Falhou há 1 h: espera. A última boa tem 40 dias, mas a fonte acabou de falhar.
        let failed = state(Span::hours(1), Some(Span::days(40)), Some("fora"), now);
        assert!(!is_due(Some(&failed), now));
        let retry = state(Span::hours(7), Some(Span::days(40)), Some("fora"), now);
        assert!(is_due(Some(&retry), now));
        // Nunca deu certo e a tentativa é recente: espera também.
        let never = state(Span::hours(1), None, Some("fora"), now);
        assert!(!is_due(Some(&never), now));
        let never_retry = state(Span::hours(7), None, Some("fora"), now);
        assert!(is_due(Some(&never_retry), now));
    }
}
