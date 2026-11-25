//! O cliente do Yahoo Finance contra respostas gravadas no formato real
//! (conferido em 25/11/2026 com `curl` nas duas rotas).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prumo::domain::asset_class::AssetClass;
use prumo::quotes::{QuoteError, QuoteProvider, YahooProvider};
use rust_decimal_macros::dec;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn provider() -> (MockServer, YahooProvider) {
    let server = MockServer::start().await;
    let provider = YahooProvider::with_bases(
        reqwest::Client::new(),
        &server.uri(),
        &format!("{}/", server.uri()),
    );
    (server, provider)
}

fn chart(symbol: &str, currency: &str, price: serde_json::Value) -> serde_json::Value {
    json!({ "chart": { "result": [{ "meta": {
        "currency": currency,
        "symbol": symbol,
        "regularMarketPrice": price,
        "regularMarketTime": 1795638630,
        "longName": " Kinea Renda Imobiliária Fundo de Investimento Imobiliário ",
        "shortName": "FII KINEA   CI"
    }}], "error": null }})
}

#[tokio::test]
async fn reads_price_currency_and_name() {
    let (server, yahoo) = provider().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/KNRI11.SA"))
        .and(query_param("range", "1d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            "KNRI11.SA",
            "brl",
            json!(142.92),
        )))
        .mount(&server)
        .await;
    let quote = yahoo.quote("KNRI11.SA").await.unwrap();
    assert_eq!(quote.symbol, "KNRI11.SA");
    assert_eq!(quote.price, dec!(142.92));
    assert_eq!(quote.currency, "BRL");
    assert_eq!(
        quote.traded_at.map(|t| t.unix_timestamp()),
        Some(1_795_638_630)
    );
    assert_eq!(
        quote.name.as_deref(),
        Some("Kinea Renda Imobiliária Fundo de Investimento Imobiliário")
    );
}

#[tokio::test]
async fn delisted_or_empty_symbols_are_not_found() {
    let (server, yahoo) = provider().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/BTC-BRL"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "chart": { "result": null, "error": {
            "code": "Not Found", "description": "No data found, symbol may be delisted" } } })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/VAZIO"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "chart": { "result": null, "error": null } })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/SEMPRECO"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            "SEMPRECO",
            "USD",
            json!(null),
        )))
        .mount(&server)
        .await;
    assert_eq!(yahoo.quote("BTC-BRL").await, Err(QuoteError::NotFound));
    assert_eq!(yahoo.quote("VAZIO").await, Err(QuoteError::NotFound));
    assert_eq!(yahoo.quote("SEMPRECO").await, Err(QuoteError::NotFound));
}

#[tokio::test]
async fn outages_and_garbage_are_unavailable() {
    let (server, yahoo) = provider().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/ERRO"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/LIXO"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/finance/search"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    assert!(matches!(
        yahoo.quote("ERRO").await,
        Err(QuoteError::Unavailable(_))
    ));
    assert!(matches!(
        yahoo.quote("LIXO").await,
        Err(QuoteError::Unavailable(_))
    ));
    assert!(matches!(
        yahoo.search("knri").await,
        Err(QuoteError::Unavailable(_))
    ));

    let offline = YahooProvider::with_bases(
        reqwest::Client::new(),
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
    );
    assert!(matches!(
        offline.quote("X").await,
        Err(QuoteError::Unavailable(_))
    ));
    assert!(matches!(
        offline.search("x").await,
        Err(QuoteError::Unavailable(_))
    ));
}

#[tokio::test]
async fn search_classifies_dedupes_and_skips_what_cannot_be_held() {
    let (server, yahoo) = provider().await;
    let mut quotes = vec![
        json!({ "symbol": "KNRI11.SA", "shortname": "FII KINEA   CI", "longname": "Kinea Renda Imobiliária Fundo de Investimento Imobiliário", "exchange": "SAO", "quoteType": "EQUITY" }),
        json!({ "symbol": "BTC-USD", "shortname": "Bitcoin USD", "exchange": "CCC", "quoteType": "CRYPTOCURRENCY" }),
        json!({ "symbol": "BTC-EUR", "shortname": "Bitcoin EUR", "exchange": "CCC", "quoteType": "CRYPTOCURRENCY" }),
        json!({ "symbol": "BTC=F", "shortname": "Bitcoin Futures", "exchange": "CME", "quoteType": "FUTURE" }),
        json!({ "symbol": "IBIT", "shortname": "iShares Bitcoin Trust ETF", "exchange": "NGM", "quoteType": "ETF" }),
        json!({ "symbol": "^BVSP", "shortname": "IBOVESPA", "exchange": "SAO", "quoteType": "INDEX" }),
    ];
    for i in 0..10 {
        quotes.push(json!({ "symbol": format!("AB{i}"), "shortname": "Outra", "exchange": "NMS", "quoteType": "EQUITY" }));
    }
    Mock::given(method("GET"))
        .and(path("/v1/finance/search"))
        .and(query_param("q", "bitcoin"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "quotes": quotes, "news": [] })),
        )
        .mount(&server)
        .await;
    let found = yahoo.search("bitcoin").await.unwrap();
    assert_eq!(found.len(), 8, "no maximo oito sugestoes");
    assert_eq!(found[0].symbol, "KNRI11");
    assert_eq!(found[0].suggested_class, AssetClass::BrReits);
    assert_eq!(found[1].symbol, "BTC");
    assert_eq!(found[1].suggested_class, AssetClass::Crypto);
    assert_eq!(
        found.iter().filter(|m| m.symbol == "BTC").count(),
        1,
        "BTC-USD e BTC-EUR viram um so"
    );
    assert_eq!(found[2].symbol, "IBIT");
    assert_eq!(found[2].suggested_class, AssetClass::IntlStocks);
    assert!(found
        .iter()
        .all(|m| m.symbol != "BTC=F" && m.symbol != "^BVSP"));

    Mock::given(method("GET"))
        .and(path("/v1/finance/search"))
        .and(query_param("q", "nada"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "count": 0 })))
        .mount(&server)
        .await;
    assert!(yahoo.search("nada").await.unwrap().is_empty());
}
