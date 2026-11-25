//! Fechamento diário pelo `chart` do Yahoo:
//! `GET /v8/finance/chart/{symbol}?period1=..&period2=..&interval=1d`.
//!
//! Medido com `curl` em 25/11/2026 (`^BVSP` e `KNRI11.SA`):
//!
//! - `timestamp[i]` é a abertura do pregão em segundos Unix; a data do pregão
//!   é a do horário da bolsa (`meta.gmtoffset`, −10800 na B3);
//! - `indicators.quote[0].close[i]` é o fechamento (ajustado só por
//!   desdobramento); `indicators.adjclose[0].adjclose[i]` desconta também os
//!   proventos. O Prumo guarda o **`close`** — ver `docs/desempenho.md`;
//! - o pregão de hoje vem com o preço do momento, e pode haver `null` em dia
//!   sem negócio: os dois ficam de fora;
//! - símbolo inexistente responde `404` com `{"chart":{"result":null,...}}`.

use std::time::Duration;

use rust_decimal::Decimal;
use serde::Deserialize;
use time::{Date, OffsetDateTime, UtcOffset};

use super::{HistoryError, Series};

pub struct YahooHistory {
    http: reqwest::Client,
    base: String,
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
    #[serde(default)]
    timestamp: Vec<i64>,
    indicators: Indicators,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[serde(default)]
    gmtoffset: i32,
    current_trading_period: Option<TradingPeriods>,
}

#[derive(Deserialize)]
struct TradingPeriods {
    regular: Option<TradingPeriod>,
}

#[derive(Deserialize)]
struct TradingPeriod {
    end: i64,
}

#[derive(Deserialize)]
struct Indicators {
    #[serde(default)]
    quote: Vec<QuoteSeries>,
}

#[derive(Deserialize)]
struct QuoteSeries {
    #[serde(default)]
    close: Vec<Option<f64>>,
}

impl YahooHistory {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, "https://query1.finance.yahoo.com")
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// Fechamentos de `from` a `to`, inclusive. O pregão que ainda não fechou
    /// fica de fora: o preço dele muda até o fim do dia.
    pub async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        self.closes_at(symbol, from, to, OffsetDateTime::now_utc())
            .await
    }

    /// [`YahooHistory::closes`] com o relógio dado (para teste).
    pub async fn closes_at(
        &self,
        symbol: &str,
        from: Date,
        to: Date,
        now: OffsetDateTime,
    ) -> Result<Series, HistoryError> {
        let unavailable = |e: &dyn std::fmt::Display| HistoryError::Unavailable(e.to_string());
        // Margem de um dia nas pontas: a data do pregão é no fuso da bolsa.
        let period1 = (from - time::Duration::days(1)).midnight().assume_utc();
        let period2 = (to + time::Duration::days(2)).midnight().assume_utc();
        let url = format!(
            "{}/v8/finance/chart/{}",
            self.base,
            urlencoding_path(symbol)
        );
        let response = self
            .http
            .get(url)
            .query(&[
                ("period1", period1.unix_timestamp().to_string()),
                ("period2", period2.unix_timestamp().to_string()),
                ("interval", "1d".to_owned()),
            ])
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| unavailable(&e))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(HistoryError::NotFound);
        }
        let envelope: Envelope = response
            .error_for_status()
            .map_err(|e| unavailable(&e))?
            .json()
            .await
            .map_err(|e| unavailable(&e))?;
        let result = envelope
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .ok_or(HistoryError::NotFound)?;
        let offset = UtcOffset::from_whole_seconds(result.meta.gmtoffset).unwrap_or(UtcOffset::UTC);
        // Pregão em andamento: o último negócio é anterior ao fim do pregão
        // regular de hoje, e o "fechamento" dele ainda é o preço do momento.
        let open_session = result
            .meta
            .current_trading_period
            .and_then(|p| p.regular)
            .and_then(|regular| {
                let end = OffsetDateTime::from_unix_timestamp(regular.end).ok()?;
                (now < end).then(|| end.to_offset(offset).date())
            });
        let closes = result
            .indicators
            .quote
            .into_iter()
            .next()
            .map(|q| q.close)
            .unwrap_or_default();
        let mut out: Series = result
            .timestamp
            .iter()
            .zip(closes)
            .filter_map(|(at, close)| {
                let date = OffsetDateTime::from_unix_timestamp(*at)
                    .ok()?
                    .to_offset(offset)
                    .date();
                let close = Decimal::try_from(close?).ok()?.round_dp(8);
                (close > Decimal::ZERO && date >= from && date <= to && Some(date) != open_session)
                    .then_some((date, close))
            })
            .collect();
        out.sort_by_key(|(date, _)| *date);
        out.dedup_by_key(|(date, _)| *date);
        Ok(out)
    }
}

/// `^BVSP` e `USDBRL=X` no caminho: só `^` e `=` precisam de escape aqui.
fn urlencoding_path(symbol: &str) -> String {
    symbol.replace('^', "%5E").replace('=', "%3D")
}
