//! Teto de preço por ativo e limite de concentração, de ponta a ponta contra um
//! Postgres de verdade (docs/api.md e docs/alocacao.md).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{json, Value};

#[tokio::test]
async fn ceiling_and_concentration_limit() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();

    the_limit_is_saved_validated_and_per_person(&db, &dir).await;
    an_asset_above_its_ceiling_sits_out_of_the_plan(&db, &dir).await;
    the_plan_respects_the_concentration_limit(&db, &dir).await;
}

fn notices<'a>(plan: &'a Value, code: &str) -> Vec<&'a Value> {
    plan["notices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["code"] == code)
        .collect()
}

fn item<'a>(plan: &'a Value, symbol: &str) -> Option<&'a Value> {
    plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["symbol"] == symbol)
}

async fn create(app: &TestApp, body: Value) -> Value {
    let reply = post(&app.router, "/api/v1/assets", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body
}

async fn the_limit_is_saved_validated_and_per_person(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "limite@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let start = get(&app.router, "/api/v1/settings/allocation").await;
    assert_eq!(start.status, StatusCode::OK);
    assert_eq!(start.body, json!({ "max_asset_percent": null }));

    let saved = put(
        &app.router,
        "/api/v1/settings/allocation",
        json!({ "max_asset_percent": 12.5 }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);
    assert_eq!(saved.body["max_asset_percent"], 12.5);
    let again = get(&app.router, "/api/v1/settings/allocation").await;
    assert_eq!(again.body["max_asset_percent"], 12.5);

    for bad in [
        json!({ "max_asset_percent": 0 }),
        json!({ "max_asset_percent": 100.5 }),
        json!({ "max_asset_percent": 12.345 }),
        json!({ "max_asset_percent": "muito" }),
        json!({}),
        json!({ "max_asset_percent": 10, "extra": 1 }),
    ] {
        let reply = put(&app.router, "/api/v1/settings/allocation", bad.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        assert_eq!(reply.body["code"], "validation_failed");
    }
    // O que foi recusado não mexeu no que estava salvo.
    let kept = get(&app.router, "/api/v1/settings/allocation").await;
    assert_eq!(kept.body["max_asset_percent"], 12.5);

    // Cada pessoa tem o próprio limite.
    let other = app_for(db, dir, "outra-limite@prumo.test");
    let theirs = get(&other.router, "/api/v1/settings/allocation").await;
    assert_eq!(theirs.body, json!({ "max_asset_percent": null }));

    let off = put(
        &app.router,
        "/api/v1/settings/allocation",
        json!({ "max_asset_percent": null }),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK);
    assert_eq!(off.body, json!({ "max_asset_percent": null }));
}

fn app_for(db: &Database, dir: &tempfile_dir::Dir, email: &str) -> TestApp {
    app(&db.store, email, FakeQuotes::with_market(), &dir.path())
}

async fn an_asset_above_its_ceiling_sits_out_of_the_plan(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app_for(db, dir, "teto@prumo.test");
    put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_reits": 50, "fixed_income": 50 } }),
    )
    .await;
    // KNRI11 está a R$ 142 no provedor falso; o teto da pessoa é R$ 140.
    let knri = create(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0, "score_override": 8, "max_price": 140 }),
    )
    .await;
    assert_eq!(knri["max_price"], 140.0);
    create(
        &app,
        json!({ "asset_class": "fixed_income", "display_name": "Tesouro Selic", "amount": 0, "score_override": 5 }),
    )
    .await;

    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    assert!(item(&plan.body, "KNRI11").is_none(), "{}", plan.body);
    let above = notices(&plan.body, "asset_above_ceiling");
    assert_eq!(above.len(), 1, "{}", plan.body);
    assert_eq!(above[0]["asset_id"], knri["id"]);
    assert_eq!(above[0]["symbol"], "KNRI11");
    assert_eq!(above[0]["unit_price"], 142.0);
    assert_eq!(above[0]["max_price"], 140.0);
    // Sem outro FII, a classe fica bloqueada pelo teto e a renda fixa vai só
    // até a meta dela.
    let blocked = notices(&plan.body, "class_blocked");
    assert_eq!(blocked[0]["asset_class"], "br_reits");
    assert_eq!(blocked[0]["reason"], "above_ceiling");
    assert_eq!(plan.body["unallocated"], 1000.0);
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(
        find(&portfolio.body["classes"], "asset_class", "br_reits")["eligible"],
        false
    );
    // O teto é filtro de compra, não composição: o ideal de KNRI11 continua
    // sendo a classe inteira (é o único FII com nota).
    assert_eq!(
        find(&portfolio.body["assets"], "symbol", "KNRI11")["ideal_class_percent"],
        100.0
    );

    // Subir o teto devolve o ativo à divisão.
    let id = knri["id"].as_str().unwrap();
    let raised = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "max_price": 150 }),
    )
    .await;
    assert_eq!(raised.status, StatusCode::OK, "{}", raised.text);
    assert_eq!(raised.body["max_price"], 150.0);
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(item(&plan.body, "KNRI11").unwrap()["quantity"], 7.0);
    assert!(notices(&plan.body, "asset_above_ceiling").is_empty());

    let cleared = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "max_price": null }),
    )
    .await;
    assert_eq!(cleared.body["max_price"], Value::Null);

    for (body, why) in [
        (json!({ "max_price": 0 }), "teto zero"),
        (json!({ "max_price": -1 }), "teto negativo"),
    ] {
        let reply = patch(&app.router, &format!("/api/v1/assets/{id}"), body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{why}");
    }
    let by_value = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "display_name": "CDB", "amount": 10, "max_price": 1 }),
    )
    .await;
    assert_eq!(by_value.status, StatusCode::UNPROCESSABLE_ENTITY);
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let selic = find(&portfolio.body["assets"], "display_name", "Tesouro Selic");
    let selic_id = selic["id"].as_str().unwrap();
    let reply = patch(
        &app.router,
        &format!("/api/v1/assets/{selic_id}"),
        json!({ "max_price": 1 }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);

    // O teto é da pessoa: outra não enxerga nem mexe no ativo.
    let other = app_for(db, dir, "outra-teto@prumo.test");
    let reply = patch(
        &other.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "max_price": 1 }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}

async fn the_plan_respects_the_concentration_limit(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app_for(db, dir, "concentracao@prumo.test");
    put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_reits": 100 } }),
    )
    .await;
    create(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0, "score_override": 8 }),
    )
    .await;
    create(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "BTLG11", "quantity": 0, "score_override": 2 }),
    )
    .await;

    // Sem limite, a nota 8 leva 80%: 56 cotas de KNRI11 (R$ 7.952).
    let free = post(&app.router, "/api/v1/plans", json!({ "amount": 10000 })).await;
    assert_eq!(item(&free.body, "KNRI11").unwrap()["quantity"], 56.0);
    assert!(notices(&free.body, "concentration_limit").is_empty());

    put(
        &app.router,
        "/api/v1/settings/allocation",
        json!({ "max_asset_percent": 60 }),
    )
    .await;
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 10000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    let body = &plan.body;
    let sum = body["invested"].as_f64().unwrap()
        + body["change"].as_f64().unwrap()
        + body["unallocated"].as_f64().unwrap();
    assert!((sum - 10000.0).abs() < 0.005, "{body}");
    // Ninguém passa de R$ 6.000; o que KNRI11 não comporta vai para BTLG11.
    for item in body["items"].as_array().unwrap() {
        assert!(item["value"].as_f64().unwrap() <= 6000.0, "{item}");
    }
    assert_eq!(item(body, "KNRI11").unwrap()["quantity"], 42.0);
    assert_eq!(item(body, "BTLG11").unwrap()["quantity"], 42.0);
    let limit = notices(body, "concentration_limit");
    assert_eq!(limit.len(), 1, "{body}");
    assert_eq!(limit[0]["max_percent"], 60.0);
    assert_eq!(limit[0]["symbols"], json!(["KNRI11"]));
    assert_eq!(limit[0]["amount"], 0.0);

    // Com 20%, os dois juntos só comportam R$ 4.000: o resto fica sem destino
    // e o aviso diz quanto.
    put(
        &app.router,
        "/api/v1/settings/allocation",
        json!({ "max_asset_percent": 20 }),
    )
    .await;
    let tight = post(&app.router, "/api/v1/plans", json!({ "amount": 10000 })).await;
    let limit = notices(&tight.body, "concentration_limit");
    assert_eq!(limit[0]["amount"], 6000.0, "{}", tight.body);
    assert_eq!(tight.body["unallocated"], 6000.0);
    assert_eq!(
        limit[0]["symbols"].as_array().unwrap().len(),
        2,
        "{}",
        tight.body
    );
}
