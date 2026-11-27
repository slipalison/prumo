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

use std::collections::{BTreeMap, HashMap};

use crate::domain::fund_issuer::{choose_fund, cnpj_digits, ChosenBy, FundChoice, FundClaim};
use crate::domain::sector::{FundComposition, SectorSource};
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
    /// Só nos FIIs (CVM): o que a carteira do fundo tem, que pondera o alvo de
    /// rendimento do híbrido.
    pub composition: Option<FundComposition>,
    /// Só nos FIIs (CVM): o fundo de onde a linha saiu (CNPJ, primeiro e
    /// último informe). A fonte pode trazer mais de um fundo para o mesmo
    /// emissor; [`settle`] fica com um.
    pub fund: Option<FundClaim>,
}

/// Uma linha por emissor: a do fundo do ticker. `chosen` é o caminho único de
/// ticker de FII → fundo (`Store::fund_cnpjs`: o que a B3 lista, ou o desempate
/// sobre os dois anos de informe guardados); emissor que ele não conhece cai no
/// desempate sobre os informes do próprio arquivo ([`choose_fund`]). Se a B3
/// lista um fundo que o arquivo não tem, o emissor fica sem linha: melhor sem
/// tipo que com o tipo de outro fundo. Devolve as linhas e os emissores
/// disputados, com a escolha.
pub fn settle(
    entries: Vec<SectorEntry>,
    chosen: &HashMap<String, FundChoice>,
) -> (Vec<SectorEntry>, Vec<(String, String, ChosenBy)>) {
    let mut by_code: BTreeMap<String, Vec<SectorEntry>> = BTreeMap::new();
    for entry in entries {
        by_code.entry(entry.code.clone()).or_default().push(entry);
    }
    let mut settled = Vec::with_capacity(by_code.len());
    let mut contested = Vec::new();
    for (code, mut group) in by_code {
        let claims: Vec<FundClaim> = group.iter().filter_map(|e| e.fund.clone()).collect();
        if claims.is_empty() {
            settled.extend(group.into_iter().take(1));
            continue;
        }
        let position = |group: &[SectorEntry], cnpj: &str| {
            let wanted = cnpj_digits(cnpj);
            group.iter().position(|e| {
                e.fund
                    .as_ref()
                    .is_some_and(|f| cnpj_digits(&f.cnpj) == wanted)
            })
        };
        let known = chosen.get(&code);
        let picked = match known {
            // A B3 manda: o fundo dela, ou nenhum.
            Some(choice) if choice.by == ChosenBy::B3 => {
                position(&group, &choice.cnpj).map(|i| (i, choice.clone()))
            }
            Some(choice) if position(&group, &choice.cnpj).is_some() => {
                position(&group, &choice.cnpj).map(|i| (i, choice.clone()))
            }
            _ => choose_fund(&claims, None)
                .and_then(|choice| position(&group, &choice.cnpj).map(|i| (i, choice))),
        };
        let Some((i, choice)) = picked else {
            continue;
        };
        if group.len() > 1 || !choice.rivals.is_empty() {
            contested.push((code.clone(), choice.cnpj.clone(), choice.by));
        }
        settled.push(group.swap_remove(i));
    }
    (settled, contested)
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

/// Se uma fonte em dia precisa ser lida de novo porque a referência guardada
/// está incompleta (`incomplete`). Uma falha recente segura a nova tentativa
/// pelo mesmo [`RETRY_AFTER`] de sempre: a fonte fora do ar não é martelada.
pub fn needs_backfill(state: Option<&SyncState>, incomplete: bool, now: OffsetDateTime) -> bool {
    if !incomplete {
        return false;
    }
    match state {
        Some(state) if state.last_error.is_some() => now - state.last_attempt_at >= RETRY_AFTER,
        _ => true,
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
            // A referência guardada sem o que o código de hoje grava (a
            // composição e o fundo de origem dos FIIs, que vieram depois), ou
            // com um fundo que não é o que a B3 lista para a sigla, é lida de
            // novo, mesmo em dia: a condição é o dado, não uma marca da
            // migração, então um pod antigo que releia a fonte durante o deploy
            // não a "consome".
            let incomplete = match self.store.sector_reference_incomplete(source).await {
                Ok(incomplete) => incomplete,
                Err(error) => {
                    tracing::error!(source = source.as_str(), error = %error, "referencia de setor ilegivel");
                    false
                }
            };
            if !is_due(state.as_ref(), now) && !needs_backfill(state.as_ref(), incomplete, now) {
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
        let codes: Vec<String> = entries
            .iter()
            .filter(|e| e.fund.is_some())
            .map(|e| e.code.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let chosen = self
            .store
            .fund_cnpjs(&codes)
            .await
            .map_err(|e| e.to_string())?;
        let (entries, contested) = settle(entries, &chosen);
        for (code, cnpj, by) in contested {
            if by != ChosenBy::B3 {
                tracing::warn!(
                    code = %code,
                    chosen = %cnpj,
                    "mais de um fundo declara o ISIN da cota; vale o que declara ha mais tempo"
                );
            }
        }
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
    use time::macros::date;
    use time::{Date, Duration as Span};

    /// Um FII da CVM de um fundo (CNPJ e meses fictícios).
    fn reit(code: &str, sector: &str, cnpj: &str, first: Date, last: Date) -> SectorEntry {
        SectorEntry {
            code: code.to_owned(),
            sector: sector.to_owned(),
            subsector: None,
            segment: None,
            composition: None,
            fund: Some(FundClaim {
                cnpj: cnpj.to_owned(),
                first_reported: first,
                last_reported: last,
            }),
        }
    }

    /// O caso de 27/11/2026, fictício: o fundo antigo (tijolo) e um novo que
    /// declarou o mesmo ISIN (híbrido), os dois desde janeiro no arquivo.
    fn contested() -> Vec<SectorEntry> {
        vec![
            reit(
                "FICT",
                "Tijolo",
                "11.111.111/0001-11",
                date!(2026 - 01 - 01),
                date!(2026 - 09 - 01),
            ),
            reit(
                "FICT",
                "Híbrido",
                "99.999.999/0001-99",
                date!(2026 - 01 - 01),
                date!(2026 - 10 - 01),
            ),
            reit(
                "OUTR",
                "Papel",
                "22.222.222/0001-22",
                date!(2026 - 01 - 01),
                date!(2026 - 10 - 01),
            ),
        ]
    }

    fn choice(cnpj: &str, by: ChosenBy) -> FundChoice {
        FundChoice {
            cnpj: cnpj.to_owned(),
            by,
            rivals: Vec::new(),
        }
    }

    #[test]
    fn the_store_choice_settles_a_contested_issuer() {
        // O store (dois anos de informe) sabe que o antigo declara há mais tempo.
        let chosen = HashMap::from([(
            "FICT".to_owned(),
            choice("11.111.111/0001-11", ChosenBy::Oldest),
        )]);
        let (entries, contested_codes) = settle(contested(), &chosen);
        assert_eq!(entries.len(), 2);
        let fict = entries.iter().find(|e| e.code == "FICT").unwrap();
        assert_eq!(fict.sector, "Tijolo");
        assert_eq!(fict.fund.as_ref().unwrap().cnpj, "11.111.111/0001-11");
        assert_eq!(
            contested_codes,
            vec![(
                "FICT".to_owned(),
                "11.111.111/0001-11".to_owned(),
                ChosenBy::Oldest
            )]
        );
        // A B3 manda, mesmo contra o desempate.
        let b3 = HashMap::from([("FICT".to_owned(), choice("99999999000199", ChosenBy::B3))]);
        let (entries, _) = settle(contested(), &b3);
        assert_eq!(
            entries.iter().find(|e| e.code == "FICT").unwrap().sector,
            "Híbrido"
        );
        // A B3 lista um fundo que o arquivo não tem: sem linha, nunca a de outro.
        let unseen = HashMap::from([("FICT".to_owned(), choice("33333333000133", ChosenBy::B3))]);
        let (entries, _) = settle(contested(), &unseen);
        assert!(entries.iter().all(|e| e.code != "FICT"));
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn without_the_store_the_file_decides_and_b3_rows_pass_through() {
        // Sem escolha guardada: o desempate sobre o arquivo (o mesmo começo,
        // então o informe mais novo; o store, com dois anos, resolve melhor).
        let (entries, _) = settle(contested(), &HashMap::new());
        assert_eq!(entries.len(), 2);
        // Escolha do store que o arquivo não tem (sem a B3): o arquivo decide.
        let stale = HashMap::from([(
            "FICT".to_owned(),
            choice("44.444.444/0001-44", ChosenBy::Oldest),
        )]);
        let (entries, _) = settle(contested(), &stale);
        assert!(entries.iter().any(|e| e.code == "FICT"));
        // Linha da B3 (sem fundo) passa como está.
        let b3_row = SectorEntry {
            code: "WEGE".into(),
            sector: "Bens Industriais".into(),
            subsector: None,
            segment: None,
            composition: None,
            fund: None,
        };
        let (entries, contested_codes) = settle(vec![b3_row.clone()], &HashMap::new());
        assert_eq!(entries, vec![b3_row]);
        assert!(contested_codes.is_empty());
    }

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
    fn an_incomplete_reference_is_read_again_even_when_fresh() {
        let now = OffsetDateTime::now_utc();
        let fresh = state(Span::days(2), Some(Span::days(2)), None, now);
        assert!(!is_due(Some(&fresh), now));
        assert!(needs_backfill(Some(&fresh), true, now));
        assert!(!needs_backfill(Some(&fresh), false, now));
        assert!(needs_backfill(None, true, now));
        // Falhou há pouco: espera, como qualquer nova tentativa.
        let failed = state(Span::hours(1), Some(Span::days(2)), Some("fora"), now);
        assert!(!needs_backfill(Some(&failed), true, now));
        let retry = state(Span::hours(7), Some(Span::days(2)), Some("fora"), now);
        assert!(needs_backfill(Some(&retry), true, now));
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
