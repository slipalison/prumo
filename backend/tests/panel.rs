//! Painel da Carteira contra um Postgres de verdade: a faixa do mês e o
//! fechamento anterior de cada ativo. Carteira fictícia.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::history::{HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::{local_date, Performance};
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::{Date, Duration, OffsetDateTime};

/// Fechamentos em memória.
#[derive(Default)]
struct FakeHistory {
    closes: Mutex<HashMap<String, Series>>,
}

#[async_trait]
impl HistoryProvider for FakeHistory {
    async fn index(&self, _: IndexCode, _: Date, _: Date) -> Result<Series, HistoryError> {
        Ok(Series::new())
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        Ok(self
            .closes
            .lock()
            .unwrap()
            .get(symbol)
            .map(|s| {
                s.iter()
                    .filter(|(d, _)| *d >= from && *d <= to)
                    .copied()
                    .collect()
            })
            .unwrap_or_default())
    }
}

async fn backdate(router: &axum::Router, asset_id: &str, date: Date) {
    let list = get(router, &format!("/api/v1/transactions?asset_id={asset_id}")).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text);
    for item in list.body.as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        let moved = patch(
            router,
            &format!("/api/v1/transactions/{id}"),
            json!({ "trade_date": date.to_string() }),
        )
        .await;
        assert_eq!(moved.status, StatusCode::OK, "{}", moved.text);
    }
}

#[tokio::test]
async fn panel_end_to_end() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "painel@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );

    // Sem série: o painel responde vazio, não falha.
    let empty = get(&app.router, "/api/v1/portfolio/month").await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text);
    assert_eq!(empty.body["as_of"], Value::Null);
    assert_eq!(empty.body["current"]["income"], 0.0);
    let none = get(&app.router, "/api/v1/portfolio/day-reference").await;
    assert_eq!(none.body["assets"], json!([]));

    let knri = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    assert_eq!(knri.status, StatusCode::CREATED, "{}", knri.text);
    let cdb = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 1000 }),
    )
    .await;
    assert_eq!(cdb.status, StatusCode::CREATED, "{}", cdb.text);

    // O fechamento de ontem (140) é diferente da cotação do momento (142).
    let now = OffsetDateTime::now_utc();
    let yesterday = local_date(now) - Duration::days(1);
    // A tarefa de ontem roda depois do fechamento (19h30 de Brasília): antes
    // disso a barra do dia ainda não é o fechamento e não é gravada.
    let yesterday_at = yesterday
        .with_time(time::macros::time!(19:30))
        .assume_offset(prumo::performance::BRASILIA);
    let history = std::sync::Arc::new(FakeHistory::default());
    history
        .closes
        .lock()
        .unwrap()
        .insert("KNRI11.SA".to_owned(), vec![(yesterday, dec!(140))]);
    backdate(&app.router, knri.body["id"].as_str().unwrap(), yesterday).await;
    backdate(&app.router, cdb.body["id"].as_str().unwrap(), yesterday).await;
    let job = Performance::new(db.store.clone(), history);
    assert!(job.run_daily(yesterday_at).await.unwrap().ran);

    let month = get(&app.router, "/api/v1/portfolio/month").await;
    assert_eq!(month.status, StatusCode::OK, "{}", month.text);
    let today = local_date(now);
    assert_eq!(month.body["as_of"], today.to_string());
    // O saldo inicial entra como aporte, no mês de ontem ou no de hoje.
    let total_flow = month.body["current"]["contribution"].as_f64().unwrap()
        + month.body["previous"]["contribution"].as_f64().unwrap();
    assert_eq!(total_flow, 2400.0);
    // Só há "fim do mês anterior" se a série já existia nele.
    let first_of_month = today.replace_day(1).unwrap();
    assert_eq!(
        month.body["previous_month_end"].is_object(),
        yesterday < first_of_month
    );

    // O fechamento anterior vem da foto de ontem, não da cotação de agora.
    let reference = get(&app.router, "/api/v1/portfolio/day-reference").await;
    assert_eq!(reference.status, StatusCode::OK, "{}", reference.text);
    let assets = reference.body["assets"].as_array().unwrap();
    assert_eq!(
        assets.len(),
        1,
        "renda fixa por valor fica de fora: {assets:?}"
    );
    assert_eq!(assets[0]["asset_id"], knri.body["id"]);
    assert_eq!(assets[0]["previous_close"], 140.0);
    assert_eq!(assets[0]["date"], yesterday.to_string());
    assert_eq!(assets[0]["previous_quantity"], 10.0);
    assert_eq!(assets[0]["bought"], 0.0);
    assert_eq!(assets[0]["sold"], 0.0);

    // Compra de hoje: a referência leva as cotas que havia no fechamento e o
    // que se pagou, para a compra não virar ganho do dia.
    let buy = post(
        &app.router,
        "/api/v1/transactions",
        json!({ "asset_id": knri.body["id"], "kind": "buy", "trade_date": today.to_string(),
                "quantity": 5, "unit_price": 141.5, "fees": 1 }),
    )
    .await;
    assert_eq!(buy.status, StatusCode::CREATED, "{}", buy.text);
    let sell = post(
        &app.router,
        "/api/v1/transactions",
        json!({ "asset_id": knri.body["id"], "kind": "sell", "trade_date": today.to_string(),
                "quantity": 3, "unit_price": 142, "fees": 0.5 }),
    )
    .await;
    assert_eq!(sell.status, StatusCode::CREATED, "{}", sell.text);
    // Um ativo que não estava na foto, comprado pela primeira vez hoje.
    let hglg = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 0 }),
    )
    .await;
    assert_eq!(hglg.status, StatusCode::CREATED, "{}", hglg.text);
    let first = post(
        &app.router,
        "/api/v1/transactions",
        json!({ "asset_id": hglg.body["id"], "kind": "buy", "trade_date": today.to_string(),
                "quantity": 2, "unit_price": 140 }),
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.text);

    let reference = get(&app.router, "/api/v1/portfolio/day-reference").await;
    assert_eq!(reference.status, StatusCode::OK, "{}", reference.text);
    let assets = reference.body["assets"].as_array().unwrap();
    let of = |id: &Value| {
        assets
            .iter()
            .find(|a| a["asset_id"] == *id)
            .unwrap_or_else(|| panic!("{id} sem referência: {assets:?}"))
    };
    let k = of(&knri.body["id"]);
    assert_eq!(k["previous_close"], 140.0);
    assert_eq!(k["previous_quantity"], 10.0);
    assert_eq!(k["bought"], 708.5);
    assert_eq!(k["sold"], 425.5);
    let h = of(&hglg.body["id"]);
    assert_eq!(h["previous_close"], Value::Null);
    assert_eq!(h["previous_quantity"], 0.0);
    assert_eq!(h["bought"], 280.0);

    // Cada pessoa vê só o que é dela.
    let other = common::app(
        &db.store,
        "outra-painel@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let theirs = get(&other.router, "/api/v1/portfolio/day-reference").await;
    assert_eq!(theirs.body["assets"], json!([]));
}
