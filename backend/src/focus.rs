//! As expectativas do Focus (pesquisa do Banco Central com o mercado), pela
//! API aberta Olinda, sem chave nem cadastro:
//!
//! - `ExpectativasMercadoAnuais`: IPCA no ano e meta Selic no fim do ano, do
//!   ano de hoje até uns quatro à frente;
//! - `ExpectativaMercadoMensais`: IPCA mês a mês, uns 24 meses à frente.
//!
//! Medido com `curl` em 27/11/2026: a pesquisa mais recente era de
//! 19/11/2026; as duas consultas (15 dias de pesquisas, só a mediana, só IPCA
//! e Selic, `baseCalculo = 0`, a janela de 30 dias que o relatório Focus usa)
//! somam uns 18 kB. **A Olinda não aceita `+` como espaço** no `$filter`
//! (`The types 'Edm.Boolean' and 'Edm.String' are not compatible`): a consulta
//! vai com `%20`, montada à mão em [`odata_query`].
//!
//! A busca é diária, em segundo plano ([`FocusSync::spawn`]), e nenhuma
//! requisição da tela espera por ela: a tela lê o que está no banco. Falha não
//! apaga nada — o Focus de ontem continua valendo, com a data da pesquisa dele.
//! Duas réplicas: quem reivindica a vez em `focus_sync` busca; a outra pula.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use time::macros::format_description;
use time::{Date, Month, OffsetDateTime};

use crate::domain::income::today_in_brazil;
use crate::domain::projection::rates::{Expectation, Indicator, Period};
use crate::store::{Store, StoreError};

pub const OLINDA_BASE: &str = "https://olinda.bcb.gov.br";
const SERVICE: &str = "/olinda/servico/Expectativas/versao/v1/odata";

/// Quantos dias de pesquisa a consulta pede: o Focus sai todo dia útil, e
/// quinze dias cobrem feriado e atraso de publicação com folga.
pub const LOOKBACK_DAYS: i64 = 15;

/// De quanto em quanto tempo a tarefa confere se o Focus está velho.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// Idade da última busca a partir da qual busca de novo: um pouco menos de um
/// dia, para o horário não escorregar.
pub const MAX_AGE: time::Duration = time::Duration::hours(20);
/// Por quanto tempo a réplica que reivindicou a busca tem a vez.
pub const CLAIM_FOR: time::Duration = time::Duration::minutes(15);

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FocusError {
    #[error("focus indisponivel: {0}")]
    Unavailable(String),
    #[error("focus fora do formato esperado: {0}")]
    Malformed(String),
}

/// De onde vêm as expectativas: atrás de um trait, para o teste não depender
/// da rede.
#[async_trait]
pub trait FocusSource: Send + Sync {
    /// As medianas da pesquisa mais recente desde `since`, de cada indicador e
    /// período que a fonte tiver.
    async fn latest(&self, since: Date) -> Result<Vec<Expectation>, FocusError>;
}

/// A API Olinda do Banco Central.
pub struct OlindaFocus {
    http: reqwest::Client,
    base: String,
}

#[derive(Deserialize)]
struct Page {
    value: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "Indicador")]
    indicator: String,
    #[serde(rename = "Data")]
    date: String,
    #[serde(rename = "DataReferencia")]
    reference: String,
    #[serde(rename = "Mediana")]
    median: Option<Decimal>,
}

/// Os pares da consulta OData, com `%20` no lugar do espaço (ver o topo).
pub fn odata_query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Mantém o que a OData lê como sintaxe (`$`, aspas, parênteses, vírgula) e o
/// que não precisa de escape; o resto vai em `%XX`.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'$'
            | b'\''
            | b'('
            | b')'
            | b',' => out.push(char::from(byte)),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// `2027` (anual) ou `11/2028` (mensal) → o dia 1º do período.
pub fn parse_reference(period: Period, text: &str) -> Option<Date> {
    let text = text.trim();
    let (month, year) = match period {
        Period::Annual => (1u8, text.parse::<i32>().ok()?),
        Period::Monthly => {
            let (month, year) = text.split_once('/')?;
            (month.parse().ok()?, year.parse().ok()?)
        }
    };
    Date::from_calendar_date(year, Month::try_from(month).ok()?, 1).ok()
}

fn indicator_of(name: &str) -> Option<Indicator> {
    match name {
        "IPCA" => Some(Indicator::Ipca),
        "Selic" => Some(Indicator::Selic),
        _ => None,
    }
}

/// As linhas de uma página como medianas, só as da pesquisa mais recente de
/// cada indicador. Linha ilegível (indicador que não interessa, data ou
/// referência fora do formato, mediana vazia) é pulada.
fn latest_of(period: Period, rows: Vec<Row>) -> Vec<Expectation> {
    let iso = format_description!("[year]-[month]-[day]");
    let mut parsed = Vec::with_capacity(rows.len());
    for row in rows {
        let (Some(indicator), Some(median)) = (indicator_of(&row.indicator), row.median) else {
            continue;
        };
        let (Ok(survey_date), Some(reference)) = (
            Date::parse(row.date.trim(), iso),
            parse_reference(period, &row.reference),
        ) else {
            continue;
        };
        parsed.push(Expectation {
            indicator,
            period,
            reference,
            median,
            survey_date,
        });
    }
    let mut newest: BTreeMap<Indicator, Date> = BTreeMap::new();
    for row in &parsed {
        let slot = newest.entry(row.indicator).or_insert(row.survey_date);
        *slot = (*slot).max(row.survey_date);
    }
    let mut latest: Vec<Expectation> = parsed
        .into_iter()
        .filter(|row| newest.get(&row.indicator) == Some(&row.survey_date))
        .collect();
    latest.sort_by_key(|row| (row.indicator, row.reference));
    latest.dedup_by_key(|row| (row.indicator, row.reference));
    latest
}

impl OlindaFocus {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, OLINDA_BASE)
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    async fn page(&self, resource: &str, filter: &str) -> Result<Vec<Row>, FocusError> {
        let query = odata_query(&[
            ("$filter", filter),
            ("$select", "Indicador,Data,DataReferencia,Mediana"),
            ("$format", "json"),
            ("$top", "1000"),
        ]);
        let url = format!("{}{SERVICE}/{resource}?{query}", self.base);
        let unavailable = |e: reqwest::Error| FocusError::Unavailable(e.to_string());
        let page: Page = self
            .http
            .get(url)
            .timeout(TIMEOUT)
            .send()
            .await
            .map_err(unavailable)?
            .error_for_status()
            .map_err(unavailable)?
            .json()
            .await
            .map_err(|e| FocusError::Malformed(e.to_string()))?;
        Ok(page.value)
    }
}

#[async_trait]
impl FocusSource for OlindaFocus {
    async fn latest(&self, since: Date) -> Result<Vec<Expectation>, FocusError> {
        let since = since
            .format(format_description!("[year]-[month]-[day]"))
            .map_err(|e| FocusError::Malformed(e.to_string()))?;
        let annual = self
            .page(
                "ExpectativasMercadoAnuais",
                &format!(
                    "(Indicador eq 'IPCA' or Indicador eq 'Selic') and baseCalculo eq 0 and Data ge '{since}'"
                ),
            )
            .await?;
        let monthly = self
            .page(
                "ExpectativaMercadoMensais",
                &format!("Indicador eq 'IPCA' and baseCalculo eq 0 and Data ge '{since}'"),
            )
            .await?;
        let mut rows = latest_of(Period::Annual, annual);
        rows.extend(latest_of(Period::Monthly, monthly));
        if rows.is_empty() {
            return Err(FocusError::Malformed(
                "nenhuma mediana de IPCA ou Selic na janela".to_owned(),
            ));
        }
        Ok(rows)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Source(#[from] FocusError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// A busca diária: reivindica a vez, busca e grava.
pub struct FocusSync {
    store: Store,
    source: Arc<dyn FocusSource>,
}

impl FocusSync {
    pub fn new(store: Store, source: Arc<dyn FocusSource>) -> Self {
        Self { store, source }
    }

    /// Busca e grava se a última busca é velha e nenhuma outra réplica está
    /// buscando. `Ok(None)`: não era a vez. `Ok(Some(n))`: `n` medianas
    /// gravadas.
    pub async fn sync_if_stale(&self, now: OffsetDateTime) -> Result<Option<usize>, SyncError> {
        if !self
            .store
            .claim_focus_sync(now, now - MAX_AGE, now + CLAIM_FOR)
            .await?
        {
            return Ok(None);
        }
        let since = today_in_brazil(now) - time::Duration::days(LOOKBACK_DAYS);
        match self.source.latest(since).await {
            Ok(rows) => {
                self.store.save_focus(&rows, now).await?;
                Ok(Some(rows.len()))
            }
            Err(error) => {
                // A vez volta para a fila: a próxima conferência tenta de novo.
                self.store.release_focus_claim().await?;
                Err(error.into())
            }
        }
    }

    /// A rodada periódica, até o processo encerrar. Falha não derruba nada.
    pub fn spawn(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match self.sync_if_stale(OffsetDateTime::now_utc()).await {
                    Ok(Some(rows)) => tracing::info!(rows, "focus atualizado"),
                    Ok(None) => {}
                    Err(error) => tracing::warn!(error = %error, "focus nao atualizado"),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn row(indicator: &str, date: &str, reference: &str, median: Option<Decimal>) -> Row {
        Row {
            indicator: indicator.into(),
            date: date.into(),
            reference: reference.into(),
            median,
        }
    }

    #[test]
    fn spaces_go_as_percent_twenty() {
        assert_eq!(
            odata_query(&[
                ("$filter", "Indicador eq 'IPCA' and Data ge '2026-11-12'"),
                ("$top", "10")
            ]),
            "$filter=Indicador%20eq%20'IPCA'%20and%20Data%20ge%20'2026-11-12'&$top=10"
        );
        assert_eq!(encode("(a or b)"), "(a%20or%20b)");
        assert_eq!(encode("ç&+"), "%C3%A7%26%2B");
    }

    #[test]
    fn references_are_the_first_day_of_the_period() {
        assert_eq!(
            parse_reference(Period::Annual, "2027"),
            Some(date!(2027 - 01 - 01))
        );
        assert_eq!(
            parse_reference(Period::Monthly, "11/2028"),
            Some(date!(2028 - 11 - 01))
        );
        assert_eq!(parse_reference(Period::Monthly, "13/2028"), None);
        assert_eq!(parse_reference(Period::Monthly, "2028"), None);
        assert_eq!(parse_reference(Period::Annual, "R1/2027"), None);
    }

    #[test]
    fn only_the_newest_survey_of_each_indicator_is_kept() {
        let rows = vec![
            row("IPCA", "2026-11-18", "2027", Some(dec!(4.6))),
            row("IPCA", "2026-11-19", "2027", Some(dec!(4.5))),
            row("IPCA", "2026-11-19", "2026", Some(dec!(5.1))),
            // A Selic da véspera ainda é a mais nova dela.
            row("Selic", "2026-11-18", "2026", Some(dec!(13.75))),
            row("IGP-M", "2026-11-19", "2026", Some(dec!(3.0))),
            row("IPCA", "2026-11-19", "2028", None),
            row("IPCA", "ontem", "2029", Some(dec!(4.0))),
        ];
        let latest = latest_of(Period::Annual, rows);
        let seen: Vec<(Indicator, i32, Decimal)> = latest
            .iter()
            .map(|r| (r.indicator, r.reference.year(), r.median))
            .collect();
        assert_eq!(
            seen,
            [
                (Indicator::Ipca, 2026, dec!(5.1)),
                (Indicator::Ipca, 2027, dec!(4.5)),
                (Indicator::Selic, 2026, dec!(13.75)),
            ]
        );
        assert!(latest.iter().all(|r| r.period == Period::Annual));
    }
}
