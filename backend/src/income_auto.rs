//! Proventos automáticos: a busca dos anúncios na B3 e a rodada que refaz os
//! automáticos de cada pessoa. A regra está em `domain::income_auto`; o
//! porquê, em `docs/proventos-automaticos.md`.
//!
//! **Fonte.** As páginas públicas de eventos corporativos da B3
//! (`sistemaswebb3-listados.b3.com.br`, sem login): uma por emissor, com o
//! valor por cota, a data com e a data de pagamento de cada provento em
//! dinheiro, por ISIN, e os eventos em ações. O ISIN de cada código vem do
//! cadastro do emissor (`GetDetail`, campo `otherCodes`). O provedor é um
//! trait próprio, e não um método de `QuoteProvider`.
//!
//! **Duas réplicas, quatro conexões.** Nenhuma conexão fica presa durante a
//! rede. Cada emissor vencido é *reivindicado* por um `update` condicional em
//! `b3_issuer_fetches` (quem marca primeiro busca; a outra réplica pula), e
//! cada gravação é uma transação curta com `pg_advisory_xact_lock`: a do
//! emissor ao gravar os anúncios, a da pessoa ao refazer os automáticos. Um
//! lock de sessão (`pg_try_advisory_lock`) prenderia uma das quatro conexões
//! durante a busca inteira.
//!
//! **Sem esperar a rede.** Nenhuma requisição da tela fala com a B3: a tela
//! lê o que a rodada gravou.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::income::today_in_brazil;
use crate::domain::income_auto::{
    parse_br_date, parse_br_decimal, parse_ticker, CashAnnouncement, StockAnnouncement, Venue,
};
use crate::store::Store;

/// De quanto em quanto tempo a rodada roda. A busca só vai à B3 para o emissor
/// vencido ([`FETCH_TTL`]); o resto da rodada refaz os automáticos com o que
/// já está no banco (o livro pode ter mudado no passado).
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

/// Quanto tempo os anúncios de um emissor valem: a busca é diária.
pub const FETCH_TTL: time::Duration = time::Duration::hours(20);

/// Por quanto tempo a réplica que reivindicou um emissor tem a vez. Se a busca
/// falhar (ou o processo cair), o emissor volta à fila depois disso.
pub const CLAIM_FOR: time::Duration = time::Duration::minutes(15);

/// A pausa entre duas requisições à B3: a busca é gentil com a fonte.
pub const PAUSE: Duration = Duration::from_millis(800);

const B3_BASE: &str = "https://sistemaswebb3-listados.b3.com.br";

/// O que a página de eventos de um emissor traz.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supplement {
    /// O código CVM, para o cadastro (`None` em fundo e em emissor sem cadastro).
    pub code_cvm: Option<String>,
    pub cash: Vec<CashAnnouncement>,
    pub stock: Vec<StockAnnouncement>,
}

/// Um código de negociação e o ISIN dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub ticker: String,
    pub isin: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SourceError {
    #[error("fonte indisponivel: {0}")]
    Unavailable(String),
    #[error("emissor invalido")]
    InvalidIssuer,
}

#[async_trait]
pub trait AnnouncementSource: Send + Sync {
    /// Os anúncios de um emissor (`ITSA`, `KNRI`).
    async fn supplement(&self, issuer: &str, venue: Venue) -> Result<Supplement, SourceError>;
    /// Os códigos de negociação e os ISIN de uma empresa, pelo código CVM.
    async fn listings(&self, code_cvm: &str) -> Result<Vec<Listing>, SourceError>;
}

/// As páginas públicas da B3.
pub struct B3Listed {
    http: reqwest::Client,
    base: String,
}

impl B3Listed {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, B3_BASE)
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    async fn get_json(&self, url: String) -> Result<serde_json::Value, SourceError> {
        let response = self
            .http
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| SourceError::Unavailable(e.to_string()))?;
        response
            .error_for_status()
            .map_err(|e| SourceError::Unavailable(e.to_string()))?
            .json()
            .await
            .map_err(|e| SourceError::Unavailable(e.to_string()))
    }
}

/// O parâmetro das páginas da B3: o JSON da consulta em base64, no caminho.
/// Com o emissor em letras e dígitos, o base64 nunca tem `/` nem `+`.
pub fn encoded(query: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(query)
}

/// A consulta da página de eventos de um emissor, na ordem de campos que a
/// própria B3 usa.
pub fn supplement_query(issuer: &str, venue: Venue) -> String {
    match venue {
        Venue::Company => format!(r#"{{"issuingCompany":"{issuer}","language":"pt-br"}}"#),
        // O `typeFund` 7 é o do FII; a B3 devolve a mesma página para ETF.
        Venue::Fund => format!(r#"{{"typeFund":7,"cnpj":"0","identifierFund":"{issuer}"}}"#),
    }
}

pub fn detail_query(code_cvm: &str) -> String {
    format!(r#"{{"codeCVM":"{code_cvm}","language":"pt-br"}}"#)
}

fn valid_issuer(issuer: &str) -> bool {
    issuer.len() == 4
        && issuer
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

#[async_trait]
impl AnnouncementSource for B3Listed {
    async fn supplement(&self, issuer: &str, venue: Venue) -> Result<Supplement, SourceError> {
        if !valid_issuer(issuer) {
            return Err(SourceError::InvalidIssuer);
        }
        let path = match venue {
            Venue::Company => "listedCompaniesProxy/CompanyCall/GetListedSupplementCompany",
            Venue::Fund => "fundsProxy/fundsCall/GetListedSupplementFunds",
        };
        let url = format!(
            "{}/{path}/{}",
            self.base,
            encoded(&supplement_query(issuer, venue))
        );
        Ok(parse_supplement(issuer, &self.get_json(url).await?))
    }

    async fn listings(&self, code_cvm: &str) -> Result<Vec<Listing>, SourceError> {
        if code_cvm.is_empty() || !code_cvm.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Vec::new());
        }
        let url = format!(
            "{}/listedCompaniesProxy/CompanyCall/GetDetail/{}",
            self.base,
            encoded(&detail_query(code_cvm))
        );
        Ok(parse_listings(&self.get_json(url).await?))
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawSupplement {
    #[serde(rename = "codeCVM")]
    code_cvm: Option<String>,
    cash_dividends: Option<Vec<RawCash>>,
    stock_dividends: Option<Vec<RawStock>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCash {
    asset_issued: Option<String>,
    isin_code: Option<String>,
    label: Option<String>,
    rate: Option<String>,
    last_date_prior: Option<String>,
    payment_date: Option<String>,
    approved_on: Option<String>,
    related_to: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStock {
    asset_issued: Option<String>,
    isin_code: Option<String>,
    label: Option<String>,
    factor: Option<String>,
    last_date_prior: Option<String>,
    approved_on: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDetail {
    other_codes: Option<Vec<RawCode>>,
}

#[derive(Deserialize)]
struct RawCode {
    code: Option<String>,
    isin: Option<String>,
}

/// `BRITSAACNPR7`: país, emissor e tipo, e o dígito.
fn valid_isin(isin: &str) -> bool {
    let bytes = isin.as_bytes();
    bytes.len() == 12
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..11]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && bytes[11].is_ascii_digit()
}

fn isin_of(isin_code: Option<&str>, asset_issued: Option<&str>) -> Option<String> {
    [isin_code, asset_issued]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_ascii_uppercase())
        .find(|s| valid_isin(s))
}

fn label_of(label: Option<&str>) -> Option<String> {
    let label = label?.trim().to_ascii_uppercase();
    (!label.is_empty() && label.chars().count() <= 40).then_some(label)
}

/// A página vem como lista de um objeto (empresa) ou como o objeto (fundo).
fn single(value: &serde_json::Value) -> Option<&serde_json::Value> {
    match value {
        serde_json::Value::Array(items) => items.first(),
        serde_json::Value::Object(_) => Some(value),
        _ => None,
    }
}

/// Lê a página de eventos. Linha ilegível (sem ISIN, sem data com, valor
/// zerado) fica de fora; a mesma linha duas vezes entra uma só.
pub fn parse_supplement(issuer: &str, value: &serde_json::Value) -> Supplement {
    let raw: RawSupplement = single(value)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // A data de pagamento faz parte da chave: um provento pago em parcelas vem
    // da B3 numa linha por parcela, com o mesmo valor e a mesma data com (o
    // dividendo de 12/2025 da WEGE3 em três pagamentos anuais).
    type CashKey = (String, String, time::Date, String, Option<time::Date>);
    let mut cash: BTreeMap<CashKey, CashAnnouncement> = BTreeMap::new();
    for line in raw.cash_dividends.unwrap_or_default() {
        let parsed = (|| {
            let isin = isin_of(line.isin_code.as_deref(), line.asset_issued.as_deref())?;
            let label = label_of(line.label.as_deref())?;
            let record_date = parse_br_date(line.last_date_prior.as_deref()?)?;
            let rate = parse_br_decimal(line.rate.as_deref()?)?;
            (rate > rust_decimal::Decimal::ZERO).then(|| CashAnnouncement {
                issuer: issuer.to_owned(),
                isin,
                label,
                record_date,
                rate: rate.normalize(),
                pay_date: line.payment_date.as_deref().and_then(parse_br_date),
                approved_on: line.approved_on.as_deref().and_then(parse_br_date),
                related_to: line
                    .related_to
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.chars().take(80).collect()),
            })
        })();
        if let Some(a) = parsed {
            cash.entry((
                a.isin.clone(),
                a.label.clone(),
                a.record_date,
                a.rate.to_string(),
                a.pay_date,
            ))
            .or_insert(a);
        }
    }
    let mut stock: BTreeMap<(String, String, time::Date, String), StockAnnouncement> =
        BTreeMap::new();
    for line in raw.stock_dividends.unwrap_or_default() {
        let parsed = (|| {
            let isin = isin_of(line.isin_code.as_deref(), line.asset_issued.as_deref())?;
            let label = label_of(line.label.as_deref())?;
            let record_date = parse_br_date(line.last_date_prior.as_deref()?)?;
            let factor = parse_br_decimal(line.factor.as_deref()?)?;
            (factor > rust_decimal::Decimal::ZERO).then(|| StockAnnouncement {
                issuer: issuer.to_owned(),
                isin,
                label,
                record_date,
                factor: factor.normalize(),
                approved_on: line.approved_on.as_deref().and_then(parse_br_date),
            })
        })();
        if let Some(a) = parsed {
            stock
                .entry((
                    a.isin.clone(),
                    a.label.clone(),
                    a.record_date,
                    a.factor.to_string(),
                ))
                .or_insert(a);
        }
    }
    Supplement {
        code_cvm: raw
            .code_cvm
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty() && c != "0"),
        cash: cash.into_values().collect(),
        stock: stock.into_values().collect(),
    }
}

/// Lê o cadastro: só os códigos de negociação (`ITSA4`), sem debênture nem
/// recibo.
pub fn parse_listings(value: &serde_json::Value) -> Vec<Listing> {
    let raw: Option<RawDetail> = single(value).and_then(|v| serde_json::from_value(v.clone()).ok());
    let mut listings: Vec<Listing> = raw
        .and_then(|r| r.other_codes)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|code| {
            let raw = code.code?.trim().to_ascii_uppercase();
            let ticker = parse_ticker(&raw).filter(|t| t.code == raw)?;
            let isin = code.isin?.trim().to_ascii_uppercase();
            valid_isin(&isin).then_some(Listing {
                ticker: ticker.code,
                isin,
            })
        })
        .collect();
    listings.sort_by(|a, b| a.ticker.cmp(&b.ticker));
    listings.dedup_by(|a, b| a.ticker == b.ticker);
    listings
}

/// O que uma rodada fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoundReport {
    pub fetched: usize,
    pub failed: usize,
    pub users: usize,
    pub inserted: usize,
    pub updated: usize,
    pub deleted: usize,
}

pub struct IncomeAutoService {
    store: Store,
    /// `None`: fontes externas desligadas (`QUOTES_ENABLED=false`); a rodada
    /// só refaz com o que já está no banco.
    source: Option<Arc<dyn AnnouncementSource>>,
    pause: Duration,
}

impl IncomeAutoService {
    pub fn new(store: Store, source: Arc<dyn AnnouncementSource>) -> Self {
        Self {
            store,
            source: Some(source),
            pause: PAUSE,
        }
    }

    /// Sem fonte: só refaz com o que já está no banco.
    pub fn disabled(store: Store) -> Self {
        Self {
            store,
            source: None,
            pause: PAUSE,
        }
    }

    /// A pausa entre requisições (os testes usam zero).
    pub fn with_pause(mut self, pause: Duration) -> Self {
        self.pause = pause;
        self
    }

    /// Busca os emissores vencidos que esta réplica conseguiu reivindicar.
    /// Devolve (buscados, falhos).
    pub async fn fetch_due(&self, now: OffsetDateTime) -> (usize, usize) {
        let Some(source) = &self.source else {
            return (0, 0);
        };
        let issuers = match self.store.income_auto_issuers().await {
            Ok(issuers) => issuers,
            Err(error) => {
                tracing::error!(error = %error, "nao foi possivel listar os emissores");
                return (0, 0);
            }
        };
        let (mut fetched, mut failed) = (0, 0);
        let mut first = true;
        for (issuer, venue) in issuers {
            match self
                .store
                .claim_issuer_fetch(&issuer, now, FETCH_TTL, CLAIM_FOR)
                .await
            {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::error!(error = %error, "reivindicacao do emissor falhou");
                    continue;
                }
            }
            if !first {
                tokio::time::sleep(self.pause).await;
            }
            first = false;
            match self
                .fetch_issuer(source.as_ref(), &issuer, venue, now)
                .await
            {
                Ok(()) => fetched += 1,
                Err(reason) => {
                    failed += 1;
                    tracing::warn!(issuer = %issuer, reason = %reason, "anuncios da B3 nao atualizados");
                }
            }
        }
        (fetched, failed)
    }

    async fn fetch_issuer(
        &self,
        source: &dyn AnnouncementSource,
        issuer: &str,
        venue: Venue,
        now: OffsetDateTime,
    ) -> Result<(), String> {
        let supplement = source
            .supplement(issuer, venue)
            .await
            .map_err(|e| e.to_string())?;
        let listings = match &supplement.code_cvm {
            Some(code) => {
                tokio::time::sleep(self.pause).await;
                // Sem o cadastro, vale a regra do ISIN: a busca não falha por isso.
                source.listings(code).await.unwrap_or_else(|error| {
                    tracing::warn!(issuer = %issuer, error = %error, "cadastro da B3 indisponivel");
                    Vec::new()
                })
            }
            None => Vec::new(),
        };
        self.store
            .save_b3_announcements(issuer, &supplement, &listings, now)
            .await
            .map_err(|e| e.to_string())
    }

    /// Refaz os automáticos de quem está com eles ligados.
    pub async fn regenerate_all(&self, report: &mut RoundReport) {
        let users = match self.store.income_auto_users().await {
            Ok(users) => users,
            Err(error) => {
                tracing::error!(error = %error, "nao foi possivel listar as pessoas");
                return;
            }
        };
        for user in users {
            self.regenerate(user, report).await;
        }
    }

    async fn regenerate(&self, user: Uuid, report: &mut RoundReport) {
        match self.store.regenerate_income_auto(user).await {
            Ok(outcome) => {
                report.users += 1;
                report.inserted += outcome.inserted;
                report.updated += outcome.updated;
                report.deleted += outcome.deleted;
            }
            Err(error) => {
                tracing::error!(user = %user, error = %error, "proventos automaticos nao refeitos");
            }
        }
    }

    /// Uma rodada: busca o vencido e refaz os automáticos de todo mundo.
    pub async fn round(&self, now: OffsetDateTime) -> RoundReport {
        let (fetched, failed) = self.fetch_due(now).await;
        let mut report = RoundReport {
            fetched,
            failed,
            ..RoundReport::default()
        };
        self.regenerate_all(&mut report).await;
        report
    }

    /// A rodada periódica, até o processo encerrar: a primeira na hora.
    pub fn spawn(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let now = OffsetDateTime::now_utc();
                let report = self.round(now).await;
                tracing::info!(
                    day = %today_in_brazil(now),
                    fetched = report.fetched,
                    failed = report.failed,
                    users = report.users,
                    inserted = report.inserted,
                    updated = report.updated,
                    deleted = report.deleted,
                    "proventos automaticos conferidos"
                );
            }
        })
    }
}

/// Os emissores distintos, cada um com a página certa: se algum ativo do
/// emissor é fundo, a de fundo (a B3 devolve o mesmo conteúdo nas duas).
pub fn issuers_of(tickers: impl IntoIterator<Item = (String, Venue)>) -> Vec<(String, Venue)> {
    let mut by_issuer: BTreeMap<String, Venue> = BTreeMap::new();
    let mut seen: HashSet<(String, Venue)> = HashSet::new();
    for (issuer, venue) in tickers {
        if !seen.insert((issuer.clone(), venue)) {
            continue;
        }
        by_issuer
            .entry(issuer)
            .and_modify(|v| {
                if venue == Venue::Fund {
                    *v = Venue::Fund;
                }
            })
            .or_insert(venue);
    }
    by_issuer.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use serde_json::json;
    use time::macros::date;
    use time::Date;

    #[test]
    fn the_query_has_no_path_breaking_characters() {
        for issuer in ["ITSA", "KNRI", "B3SA", "ZZZZ", "AAAA", "OOOO", "0000"] {
            for venue in [Venue::Company, Venue::Fund] {
                let text = encoded(&supplement_query(issuer, venue));
                assert!(!text.contains('/') && !text.contains('+'), "{text}");
            }
        }
        assert_eq!(
            encoded(&supplement_query("ITSA", Venue::Company)),
            "eyJpc3N1aW5nQ29tcGFueSI6IklUU0EiLCJsYW5ndWFnZSI6InB0LWJyIn0="
        );
        assert_eq!(
            detail_query("7617"),
            r#"{"codeCVM":"7617","language":"pt-br"}"#
        );
    }

    #[test]
    fn unreadable_lines_are_left_out() {
        let page = json!([{
            "codeCVM": "0",
            "cashDividends": [
                { "isinCode": "BRABCDACNPR1", "label": "DIVIDENDO", "rate": "0,5", "lastDatePrior": "01/04/2026", "paymentDate": "" },
                { "isinCode": "", "assetIssued": "BRABCDACNOR3", "label": "DIVIDENDO", "rate": "0,5", "lastDatePrior": "01/04/2026", "paymentDate": "20/04/2026", "relatedTo": "  " },
                { "isinCode": "BRABCDACNPR1", "label": "DIVIDENDO", "rate": "0,5", "lastDatePrior": "01/04/2026", "paymentDate": "20/04/2026" },
                { "isinCode": "XX", "label": "DIVIDENDO", "rate": "0,5", "lastDatePrior": "01/04/2026" },
                { "isinCode": "BRABCDACNPR1", "label": "", "rate": "0,5", "lastDatePrior": "01/04/2026" },
                { "isinCode": "BRABCDACNPR1", "label": "DIVIDENDO", "rate": "0,0", "lastDatePrior": "01/04/2026" },
                { "isinCode": "BRABCDACNPR1", "label": "DIVIDENDO", "rate": "1,0", "lastDatePrior": "sem data" }
            ],
            "stockDividends": [
                { "isinCode": "BRABCDACNPR1", "label": "BONIFICACAO", "factor": "10,0", "lastDatePrior": "18/12/2025" },
                { "isinCode": "BRABCDACNPR1", "label": "BONIFICACAO", "factor": "", "lastDatePrior": "18/12/2025" }
            ]
        }]);
        let s = parse_supplement("ABCD", &page);
        assert_eq!(s.code_cvm, None);
        assert_eq!(s.cash.len(), 3);
        // A mesma data com e o mesmo valor com outra data de pagamento é outra
        // parcela: uma a definir, outra em 20/04.
        let pn: Vec<Option<Date>> = s
            .cash
            .iter()
            .filter(|c| c.isin == "BRABCDACNPR1")
            .map(|c| c.pay_date)
            .collect();
        assert_eq!(pn, [None, Some(date!(2026 - 04 - 20))]);
        assert!(s.cash.iter().all(|c| c.rate == dec!(0.5)));
        let on = s.cash.iter().find(|c| c.isin == "BRABCDACNOR3").unwrap();
        assert_eq!(on.pay_date, Some(date!(2026 - 04 - 20)));
        assert_eq!(on.related_to, None);
        assert_eq!(s.stock.len(), 1);
        assert_eq!(s.stock[0].factor, dec!(10));
        // O que não é página.
        assert_eq!(
            parse_supplement("ABCD", &json!(null)),
            Supplement::default()
        );
        assert_eq!(parse_supplement("ABCD", &json!([])), Supplement::default());
    }

    #[test]
    fn installments_are_one_announcement_each() {
        // Um dividendo em três parcelas anuais e a mesma linha repetida.
        let line = |pay: &str| {
            json!({ "isinCode": "BRABCDACNOR3", "label": "DIVIDENDO", "rate": "0,40000000000",
                    "lastDatePrior": "19/12/2025", "paymentDate": pay })
        };
        let page = json!([{
            "cashDividends": [line("12/08/2026"), line("11/08/2027"), line("16/08/2028"), line("12/08/2026")],
        }]);
        let s = parse_supplement("ABCD", &page);
        let pays: Vec<Option<Date>> = s.cash.iter().map(|c| c.pay_date).collect();
        assert_eq!(
            pays,
            [
                Some(date!(2026 - 08 - 12)),
                Some(date!(2027 - 08 - 11)),
                Some(date!(2028 - 08 - 16)),
            ]
        );
    }

    #[test]
    fn listings_keep_only_trading_codes() {
        let detail = json!({ "otherCodes": [
            { "code": "PETR4", "isin": "BRPETRACNPR6" },
            { "code": "PETR3", "isin": "BRPETRACNOR9" },
            { "code": "PETR-DEB62", "isin": "BRPETRDBS092" },
            { "code": "PETR4", "isin": "BRPETRACNPR6" },
            { "code": "PETR5", "isin": "invalido" },
            { "code": null, "isin": "BRPETRACNPA1" }
        ]});
        let listings = parse_listings(&detail);
        assert_eq!(
            listings,
            vec![
                Listing {
                    ticker: "PETR3".into(),
                    isin: "BRPETRACNOR9".into()
                },
                Listing {
                    ticker: "PETR4".into(),
                    isin: "BRPETRACNPR6".into()
                },
            ]
        );
        assert!(parse_listings(&json!("x")).is_empty());
    }

    #[test]
    fn an_issuer_with_a_fund_uses_the_fund_page() {
        let issuers = issuers_of([
            ("ITSA".to_owned(), Venue::Company),
            ("KNRI".to_owned(), Venue::Fund),
            ("ABCD".to_owned(), Venue::Company),
            ("ABCD".to_owned(), Venue::Fund),
            ("ITSA".to_owned(), Venue::Company),
        ]);
        assert_eq!(
            issuers,
            vec![
                ("ABCD".to_owned(), Venue::Fund),
                ("ITSA".to_owned(), Venue::Company),
                ("KNRI".to_owned(), Venue::Fund),
            ]
        );
        assert!(valid_issuer("B3SA"));
        assert!(!valid_issuer("itsa"));
        assert!(!valid_issuer("ITSA4"));
    }
}
