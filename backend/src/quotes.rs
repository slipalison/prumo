//! Cotações e busca de símbolos.
//!
//! O provedor é o Yahoo Finance (rotas públicas `chart` e `search`, sem chave).
//! Ele fica atrás de [`QuoteProvider`] para os testes não dependerem da rede e
//! para a troca de provedor não tocar o resto do código.

use std::time::Duration;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::domain::asset_class::{normalize_symbol, AssetClass};

#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    /// Símbolo no provedor (`KNRI11.SA`, `BTC-USD`, `USDBRL=X`).
    pub symbol: String,
    pub price: Decimal,
    pub currency: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolMatch {
    pub symbol: String,
    pub display_name: String,
    pub exchange: String,
    pub suggested_class: AssetClass,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuoteError {
    #[error("simbolo sem cotacao no provedor")]
    NotFound,
    #[error("provedor indisponivel: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait QuoteProvider: Send + Sync {
    async fn quote(&self, provider_symbol: &str) -> Result<Quote, QuoteError>;
    async fn search(&self, query: &str) -> Result<Vec<SymbolMatch>, QuoteError>;
}

/// Símbolo do câmbio para reais (`USD` → `USDBRL=X`).
pub fn fx_symbol(currency: &str) -> String {
    format!("{}BRL=X", currency.to_uppercase())
}

pub struct YahooProvider {
    http: reqwest::Client,
    chart_base: String,
    search_base: String,
}

impl YahooProvider {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_bases(
            http,
            "https://query1.finance.yahoo.com",
            "https://query2.finance.yahoo.com",
        )
    }

    pub fn with_bases(http: reqwest::Client, chart_base: &str, search_base: &str) -> Self {
        Self {
            http,
            chart_base: chart_base.trim_end_matches('/').to_owned(),
            search_base: search_base.trim_end_matches('/').to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct ChartEnvelope {
    chart: Chart,
}

#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}

#[derive(Deserialize)]
struct ChartResult {
    meta: ChartMeta,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartMeta {
    symbol: String,
    currency: Option<String>,
    regular_market_price: Option<f64>,
    long_name: Option<String>,
    short_name: Option<String>,
}

#[derive(Deserialize)]
struct SearchEnvelope {
    #[serde(default)]
    quotes: Vec<SearchQuote>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuote {
    symbol: String,
    #[serde(default)]
    shortname: Option<String>,
    #[serde(default)]
    longname: Option<String>,
    #[serde(default)]
    exchange: Option<String>,
    #[serde(default)]
    quote_type: Option<String>,
}

#[async_trait]
impl QuoteProvider for YahooProvider {
    async fn quote(&self, provider_symbol: &str) -> Result<Quote, QuoteError> {
        let url = format!("{}/v8/finance/chart/{}", self.chart_base, provider_symbol);
        let response = self
            .http
            .get(url)
            .query(&[("range", "1d"), ("interval", "1d")])
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .map_err(|e| QuoteError::Unavailable(e.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(QuoteError::NotFound);
        }
        let envelope: ChartEnvelope = response
            .error_for_status()
            .map_err(|e| QuoteError::Unavailable(e.to_string()))?
            .json()
            .await
            .map_err(|e| QuoteError::Unavailable(e.to_string()))?;
        let meta = envelope
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .map(|r| r.meta)
            .ok_or(QuoteError::NotFound)?;
        let price = meta
            .regular_market_price
            .and_then(|p| Decimal::try_from(p).ok())
            .map(|p| p.round_dp(8))
            .filter(|p| *p > Decimal::ZERO)
            .ok_or(QuoteError::NotFound)?;
        Ok(Quote {
            symbol: meta.symbol,
            price,
            currency: meta
                .currency
                .unwrap_or_else(|| "BRL".to_owned())
                .to_uppercase(),
            name: meta
                .long_name
                .or(meta.short_name)
                .map(|n| n.trim().to_owned()),
        })
    }

    async fn search(&self, query: &str) -> Result<Vec<SymbolMatch>, QuoteError> {
        let url = format!("{}/v1/finance/search", self.search_base);
        let envelope: SearchEnvelope = self
            .http
            .get(url)
            .query(&[
                ("q", query),
                ("quotesCount", "10"),
                ("newsCount", "0"),
                ("listsCount", "0"),
            ])
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| QuoteError::Unavailable(e.to_string()))?
            .json()
            .await
            .map_err(|e| QuoteError::Unavailable(e.to_string()))?;
        let mut matches: Vec<SymbolMatch> = Vec::new();
        for quote in envelope.quotes {
            let Some(found) = classify(&quote) else {
                continue;
            };
            if !matches
                .iter()
                .any(|m| m.symbol == found.symbol && m.suggested_class == found.suggested_class)
            {
                matches.push(found);
            }
        }
        matches.truncate(8);
        Ok(matches)
    }
}

/// Palpite de classe para um resultado de busca. Quem confirma é a pessoa.
fn classify(quote: &SearchQuote) -> Option<SymbolMatch> {
    let kind = quote.quote_type.as_deref().unwrap_or_default();
    let name = quote
        .longname
        .clone()
        .or_else(|| quote.shortname.clone())
        .unwrap_or_else(|| quote.symbol.clone())
        .trim()
        .to_owned();
    let exchange = quote.exchange.clone().unwrap_or_default();
    let upper_name = name.to_uppercase();
    let short = quote.shortname.clone().unwrap_or_default().to_uppercase();

    let (raw_symbol, class) = if kind == "CRYPTOCURRENCY" {
        let base = quote
            .symbol
            .split('-')
            .next()
            .unwrap_or(&quote.symbol)
            .to_owned();
        (base, AssetClass::Crypto)
    } else if !matches!(kind, "EQUITY" | "ETF") {
        return None;
    } else if let Some(base) = quote.symbol.strip_suffix(".SA") {
        let looks_like_reit = upper_name.contains("IMOBILI")
            || upper_name.contains("IMOB")
            || short.starts_with("FII")
            || (base.ends_with("11")
                && kind != "ETF"
                && !short.contains("UNT")
                && !short.contains("UNIT"));
        (
            base.to_owned(),
            if looks_like_reit {
                AssetClass::BrReits
            } else {
                AssetClass::BrStocks
            },
        )
    } else {
        let looks_like_reit = upper_name.contains("REIT") || upper_name.contains("REALTY");
        (
            quote.symbol.clone(),
            if looks_like_reit {
                AssetClass::UsReits
            } else {
                AssetClass::IntlStocks
            },
        )
    };
    let symbol = normalize_symbol(&raw_symbol)?;
    let display_name = if class == AssetClass::Crypto {
        name.trim_end_matches(" USD").to_owned()
    } else {
        name
    };
    Some(SymbolMatch {
        symbol,
        display_name,
        exchange,
        suggested_class: class,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_quote(
        symbol: &str,
        short: &str,
        long: Option<&str>,
        exchange: &str,
        kind: &str,
    ) -> SearchQuote {
        SearchQuote {
            symbol: symbol.to_owned(),
            shortname: Some(short.to_owned()),
            longname: long.map(str::to_owned),
            exchange: Some(exchange.to_owned()),
            quote_type: Some(kind.to_owned()),
        }
    }

    #[test]
    fn classifies_b3_reits_stocks_and_units() {
        let reit = classify(&search_quote(
            "KNRI11.SA",
            "FII KINEA   CI",
            Some("Kinea Renda Imobiliária Fundo de Investimento Imobiliário"),
            "SAO",
            "EQUITY",
        ))
        .unwrap();
        assert_eq!(reit.symbol, "KNRI11");
        assert_eq!(reit.suggested_class, AssetClass::BrReits);
        assert_eq!(reit.exchange, "SAO");

        let stock = classify(&search_quote(
            "WEGE3.SA",
            "WEG ON",
            Some("WEG S.A."),
            "SAO",
            "EQUITY",
        ))
        .unwrap();
        assert_eq!(stock.suggested_class, AssetClass::BrStocks);

        let unit = classify(&search_quote(
            "TAEE11.SA",
            "TAESA UNT N2",
            Some("Transmissora Aliança"),
            "SAO",
            "EQUITY",
        ))
        .unwrap();
        assert_eq!(unit.suggested_class, AssetClass::BrStocks);

        let etf = classify(&search_quote(
            "BOVA11.SA",
            "ISHARES BOVA CI",
            None,
            "SAO",
            "ETF",
        ))
        .unwrap();
        assert_eq!(etf.suggested_class, AssetClass::BrStocks);
    }

    #[test]
    fn classifies_crypto_and_us_listings() {
        let btc = classify(&search_quote(
            "BTC-USD",
            "Bitcoin USD",
            None,
            "CCC",
            "CRYPTOCURRENCY",
        ))
        .unwrap();
        assert_eq!(btc.symbol, "BTC");
        assert_eq!(btc.display_name, "Bitcoin");
        assert_eq!(btc.suggested_class, AssetClass::Crypto);

        let reit = classify(&search_quote(
            "O",
            "Realty Income Corporation",
            None,
            "NYQ",
            "EQUITY",
        ))
        .unwrap();
        assert_eq!(reit.suggested_class, AssetClass::UsReits);

        let stock = classify(&search_quote(
            "AAPL",
            "Apple Inc.",
            Some("Apple Inc."),
            "NMS",
            "EQUITY",
        ))
        .unwrap();
        assert_eq!(stock.suggested_class, AssetClass::IntlStocks);

        assert!(classify(&search_quote(
            "BTC=F",
            "Bitcoin Futures",
            None,
            "CME",
            "FUTURE"
        ))
        .is_none());
    }

    #[test]
    fn fx_symbol_targets_brl() {
        assert_eq!(fx_symbol("usd"), "USDBRL=X");
    }
}
