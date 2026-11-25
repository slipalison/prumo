//! O cliente de dividendos do Yahoo contra respostas no formato real do
//! `chart` com `events=div` (conferido em 25/11/2026 com `curl`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prumo::dividends::{DividendError, DividendEvent, DividendProvider, YahooDividends};
use rust_decimal_macros::dec;
use serde_json::json;
use time::macros::date;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn provider() -> (MockServer, YahooDividends) {
    let server = MockServer::start().await;
    let provider = YahooDividends::with_base(reqwest::Client::new(), &format!("{}/", server.uri()));
    (server, provider)
}

fn chart(currency: &str, gmtoffset: i64, events: serde_json::Value) -> serde_json::Value {
    let mut result = json!({
        "meta": { "currency": currency, "symbol": "KNRI11.SA", "gmtoffset": gmtoffset },
        "timestamp": [1733022000, 1735700400],
        "indicators": { "quote": [{ "close": [140.5, 141.2] }] }
    });
    if !events.is_null() {
        result["events"] = events;
    }
    json!({ "chart": { "result": [result], "error": null } })
}

#[tokio::test]
async fn reads_dividends_by_exchange_day() {
    let (server, yahoo) = provider().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/KNRI11.SA"))
        .and(query_param("events", "div"))
        .and(query_param("interval", "1d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            "brl",
            -10800,
            json!({ "dividends": {
                "1733144400": { "amount": 1.0, "date": 1733144400 },
                "1735822800": { "amount": 1.25, "date": 1735822800 },
                "1767358800": { "amount": 0.88, "date": 1767358800 }
            }}),
        )))
        .mount(&server)
        .await;
    let history = yahoo.history("KNRI11.SA").await.unwrap();
    assert_eq!(history.currency, "BRL");
    assert_eq!(
        history.events,
        [
            DividendEvent {
                ex_date: date!(2024 - 12 - 02),
                amount: dec!(1.0)
            },
            DividendEvent {
                ex_date: date!(2025 - 01 - 02),
                amount: dec!(1.25)
            },
            DividendEvent {
                ex_date: date!(2026 - 01 - 02),
                amount: dec!(0.88)
            },
        ]
    );
}

#[tokio::test]
async fn same_day_payments_are_summed_and_bad_rows_are_skipped() {
    let (server, yahoo) = provider().await;
    // 1733144400 = 2024-12-02 13:00 UTC (10h em Brasília). Duas linhas no mesmo
    // dia (dividendo e JCP), uma de valor zero e uma de valor negativo.
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/ITSA4.SA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            "BRL",
            -10800,
            json!({ "dividends": {
                "1733144400": { "amount": 0.25, "date": 1733144400 },
                "1733144401": { "amount": 0.125, "date": 1733144401 },
                "1735822800": { "amount": 0.0, "date": 1735822800 },
                "1738500000": { "amount": -1.0, "date": 1738500000 }
            }}),
        )))
        .mount(&server)
        .await;
    let history = yahoo.history("ITSA4.SA").await.unwrap();
    assert_eq!(
        history.events,
        [DividendEvent {
            ex_date: date!(2024 - 12 - 02),
            amount: dec!(0.375)
        }]
    );
}

#[tokio::test]
async fn foreign_stock_keeps_its_currency_and_exchange_calendar() {
    let (server, yahoo) = provider().await;
    // 2025-02-10 14:30 UTC é 09:30 em Nova York (−5 h): mesmo dia.
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/AAPL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            "USD",
            -18000,
            json!({ "dividends": { "1739197800": { "amount": 0.25, "date": 1739197800 } } }),
        )))
        .mount(&server)
        .await;
    let history = yahoo.history("AAPL").await.unwrap();
    assert_eq!(history.currency, "USD");
    assert_eq!(history.events[0].ex_date, date!(2025 - 02 - 10));
}

#[tokio::test]
async fn a_symbol_that_pays_nothing_has_an_empty_history() {
    let (server, yahoo) = provider().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/BTC-USD"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            "USD",
            0,
            serde_json::Value::Null,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/SEMEVENTO.SA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart("BRL", -10800, json!({}))))
        .mount(&server)
        .await;
    for symbol in ["BTC-USD", "SEMEVENTO.SA"] {
        let history = yahoo.history(symbol).await.unwrap();
        assert!(history.events.is_empty(), "{symbol}");
    }
}

#[tokio::test]
async fn missing_and_broken_responses_are_told_apart() {
    let (server, yahoo) = provider().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/NAOEXISTE.SA"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "chart": { "result": null,
            "error": { "code": "Not Found", "description": "No data found, symbol may be delisted" } } })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/VAZIO.SA"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "chart": { "result": null, "error": null } })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/QUEBRADO.SA"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/LIXO.SA"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>nao e json</html>"))
        .mount(&server)
        .await;
    assert_eq!(
        yahoo.history("NAOEXISTE.SA").await,
        Err(DividendError::NotFound)
    );
    assert_eq!(
        yahoo.history("VAZIO.SA").await,
        Err(DividendError::NotFound)
    );
    assert!(matches!(
        yahoo.history("QUEBRADO.SA").await,
        Err(DividendError::Unavailable(_))
    ));
    assert!(matches!(
        yahoo.history("LIXO.SA").await,
        Err(DividendError::Unavailable(_))
    ));
}

#[tokio::test]
async fn an_unreachable_provider_is_unavailable() {
    let yahoo = YahooDividends::with_base(reqwest::Client::new(), "http://127.0.0.1:9");
    assert!(matches!(
        yahoo.history("KNRI11.SA").await,
        Err(DividendError::Unavailable(_))
    ));
}
