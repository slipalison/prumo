//! Histórico de dividendo por cota, para a **estimativa** de renda.
//!
//! A fonte é o `chart` do Yahoo com `events=div`: data ex e valor bruto por
//! cota, sem data de pagamento, e com JCP misturado ao dividendo nas ações da
//! B3. Serve para projetar, nunca para extrato: o que a pessoa recebeu vem do
//! livro de proventos (`income_events`).
//!
//! O provedor é outro trait, e não um método de `QuoteProvider`, para a frente
//! de cotações e a de proventos evoluírem sem mexer no mesmo contrato.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use time::{Date, OffsetDateTime};

/// A janela de dividendos que o provedor devolve. A estimativa de renda olha
/// 12 meses; a referência de Bazin da Análise tira a média de até 5 anos
/// (`domain::analysis::references::MAX_YEARS`).
const HISTORY_RANGE: &str = "5y";

/// Um dividendo por cota, bruto, na data ex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DividendEvent {
    pub ex_date: Date,
    pub amount: Decimal,
}

/// O histórico de um símbolo e a moeda em que o provedor informa o valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DividendHistory {
    pub currency: String,
    pub events: Vec<DividendEvent>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DividendError {
    #[error("simbolo sem historico no provedor")]
    NotFound,
    #[error("provedor indisponivel: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait DividendProvider: Send + Sync {
    /// O histórico dos últimos 5 anos de `provider_symbol` (`KNRI11.SA`). Um
    /// símbolo que não paga nada volta com `events` vazio.
    async fn history(&self, provider_symbol: &str) -> Result<DividendHistory, DividendError>;
}

/// Sem provedor: `QUOTES_ENABLED=false`.
pub struct DisabledDividends;

#[async_trait]
impl DividendProvider for DisabledDividends {
    async fn history(&self, _provider_symbol: &str) -> Result<DividendHistory, DividendError> {
        Err(DividendError::Unavailable(
            "historico de dividendos desligado".into(),
        ))
    }
}

pub struct YahooDividends {
    http: reqwest::Client,
    chart_base: String,
}

impl YahooDividends {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, "https://query1.finance.yahoo.com")
    }

    pub fn with_base(http: reqwest::Client, chart_base: &str) -> Self {
        Self {
            http,
            chart_base: chart_base.trim_end_matches('/').to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    chart: Chart,
}

#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}

#[derive(Deserialize)]
struct ChartResult {
    meta: Meta,
    events: Option<Events>,
}

#[derive(Deserialize)]
struct Meta {
    currency: Option<String>,
    /// Segundos de diferença do UTC no pregão (B3: −10800).
    gmtoffset: Option<i64>,
}

#[derive(Deserialize)]
struct Events {
    dividends: Option<BTreeMap<String, RawDividend>>,
}

#[derive(Deserialize)]
struct RawDividend {
    amount: f64,
    date: i64,
}

#[async_trait]
impl DividendProvider for YahooDividends {
    async fn history(&self, provider_symbol: &str) -> Result<DividendHistory, DividendError> {
        let url = format!("{}/v8/finance/chart/{}", self.chart_base, provider_symbol);
        // Intervalo diário: o evento é indexado pelo dia (com `1mo` dois
        // pagamentos no mesmo mês cairiam na mesma chave).
        let response = self
            .http
            .get(url)
            .query(&[
                ("range", HISTORY_RANGE),
                ("interval", "1d"),
                ("events", "div"),
            ])
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|e| DividendError::Unavailable(e.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(DividendError::NotFound);
        }
        let envelope: Envelope = response
            .error_for_status()
            .map_err(|e| DividendError::Unavailable(e.to_string()))?
            .json()
            .await
            .map_err(|e| DividendError::Unavailable(e.to_string()))?;
        let result = envelope
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .ok_or(DividendError::NotFound)?;
        let offset = result.meta.gmtoffset.unwrap_or(0);
        // Duas linhas no mesmo dia (dividendo e JCP) viram uma só, somadas.
        let mut by_day: BTreeMap<Date, Decimal> = BTreeMap::new();
        let raw = result.events.and_then(|e| e.dividends).unwrap_or_default();
        for dividend in raw.into_values() {
            let Some(date) = exchange_date(dividend.date, offset) else {
                continue;
            };
            let Some(amount) = Decimal::try_from(dividend.amount)
                .ok()
                .map(|a| a.round_dp(8))
                .filter(|a| *a > Decimal::ZERO)
            else {
                continue;
            };
            *by_day.entry(date).or_insert(Decimal::ZERO) += amount;
        }
        Ok(DividendHistory {
            currency: result
                .meta
                .currency
                .unwrap_or_else(|| "BRL".to_owned())
                .to_uppercase(),
            events: by_day
                .into_iter()
                .map(|(ex_date, amount)| DividendEvent { ex_date, amount })
                .collect(),
        })
    }
}

/// O dia do calendário da bolsa para um instante Unix.
fn exchange_date(unix: i64, offset_seconds: i64) -> Option<Date> {
    OffsetDateTime::from_unix_timestamp(unix.checked_add(offset_seconds)?)
        .ok()
        .map(OffsetDateTime::date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn exchange_date_follows_the_exchange_offset() {
        // 2024-12-02 02:00 UTC é 01/12 à noite em Brasília.
        assert_eq!(
            exchange_date(1_733_104_800, -10_800),
            Some(date!(2024 - 12 - 01))
        );
        assert_eq!(exchange_date(1_733_104_800, 0), Some(date!(2024 - 12 - 02)));
        assert_eq!(exchange_date(i64::MAX, 10), None);
    }
}
