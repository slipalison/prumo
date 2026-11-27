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
    a_ceiling_by_method_follows_the_reference_and_never_falls_silent(&db, &dir).await;
    the_plan_respects_the_concentration_limit(&db, &dir).await;
}

/// Os dividendos de WEGE3 no cache do Yahoo: `amount` por cota, há 30 dias.
async fn wege_paid(db: &Database, amount: rust_decimal::Decimal) {
    let today = prumo::domain::income::today_in_brazil(time::OffsetDateTime::now_utc());
    db.store
        .save_dividends(
            "WEGE3.SA",
            &prumo::dividends::DividendHistory {
                currency: "BRL".into(),
                events: vec![prumo::dividends::DividendEvent {
                    ex_date: today - time::Duration::days(30),
                    amount,
                }],
            },
            time::OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
}

/// O teto pelo Bazin: grava-se o método, o valor sai dos dividendos de hoje.
/// Sem dividendos, o ativo fica fora do aporte com o motivo (nunca "sem teto");
/// com eles, o teto é o Bazin, e o plano compara o preço com ele.
async fn a_ceiling_by_method_follows_the_reference_and_never_falls_silent(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let app = app_for(db, dir, "teto-metodo@prumo.test");
    put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 50, "fixed_income": 50 } }),
    )
    .await;
    // Ativo novo nasce sem teto.
    let plain = create(
        &app,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 0, "score_override": 8 }),
    )
    .await;
    assert_eq!(plain["ceiling"], Value::Null);
    let id = plain["id"].as_str().unwrap();
    create(
        &app,
        json!({ "asset_class": "fixed_income", "display_name": "Tesouro Selic", "amount": 0, "score_override": 5 }),
    )
    .await;
    let chosen = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "ceiling": { "method": "bazin" } }),
    )
    .await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text);
    assert_eq!(
        chosen.body["ceiling"],
        json!({ "method": "bazin", "value": null, "unavailable_reason": "no_dividends" })
    );

    // Sem dividendo guardado: fora do aporte, com o método e o motivo.
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    assert!(item(&plan.body, "WEGE3").is_none(), "{}", plan.body);
    let unavailable = notices(&plan.body, "asset_ceiling_unavailable");
    assert_eq!(unavailable.len(), 1, "{}", plan.body);
    assert_eq!(unavailable[0]["asset_id"], plain["id"]);
    assert_eq!(unavailable[0]["symbol"], "WEGE3");
    assert_eq!(unavailable[0]["method"], "bazin");
    assert_eq!(unavailable[0]["reason"], "no_dividends");
    let blocked = notices(&plan.body, "class_blocked");
    assert_eq!(blocked[0]["asset_class"], "br_stocks");
    assert_eq!(blocked[0]["reason"], "ceiling_unavailable");

    // R$ 6,00 em 12 meses a 6%: Bazin de R$ 100, acima do preço (R$ 50). Compra.
    wege_paid(db, rust_decimal_macros::dec!(6)).await;
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(item(&plan.body, "WEGE3").unwrap()["quantity"], 20.0);
    assert!(notices(&plan.body, "asset_ceiling_unavailable").is_empty());
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(
        find(&portfolio.body["assets"], "symbol", "WEGE3")["ceiling"],
        json!({ "method": "bazin", "value": 100.0, "unavailable_reason": null })
    );

    // R$ 0,60: Bazin de R$ 10, abaixo do preço. O aviso diz que o teto é o Bazin.
    wege_paid(db, rust_decimal_macros::dec!(0.6)).await;
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert!(item(&plan.body, "WEGE3").is_none());
    let above = notices(&plan.body, "asset_above_ceiling");
    assert_eq!(above.len(), 1, "{}", plan.body);
    assert_eq!(above[0]["max_price"], 10.0);
    assert_eq!(above[0]["method"], "bazin");

    // "Sem teto" tira o teto: o ativo volta à divisão.
    let removed = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "ceiling": null }),
    )
    .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text);
    assert_eq!(removed.body["ceiling"], Value::Null);
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(item(&plan.body, "WEGE3").unwrap()["quantity"], 20.0);

    // Método e valor próprio não coexistem: trocar um pelo outro troca a escolha.
    let manual = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "ceiling": { "method": "manual", "value": 45 } }),
    )
    .await;
    assert_eq!(manual.body["ceiling"]["method"], "manual");
    let graham = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "ceiling": { "method": "graham" } }),
    )
    .await;
    assert_eq!(graham.body["ceiling"]["method"], "graham");
    assert_eq!(
        graham.body["ceiling"]["unavailable_reason"],
        "no_fundamentals"
    );
    let pool = sqlx::PgPool::connect(&db.url).await.unwrap();
    let (method, max_price): (Option<String>, Option<rust_decimal::Decimal>) =
        sqlx::query_as("select ceiling_method, max_price from assets where id = $1::uuid")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((method.as_deref(), max_price), (Some("graham"), None));
    // O banco recusa os dois juntos e o método de outra família.
    for update in [
        "update assets set max_price = 10 where id = $1::uuid",
        "update assets set ceiling_method = 'nav', max_price = null where id = $1::uuid",
    ] {
        let refused = sqlx::query(update).bind(id).execute(&pool).await;
        assert!(refused.is_err(), "{update}");
    }
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
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0, "score_override": 8, "ceiling": { "method": "manual", "value": 140 } }),
    )
    .await;
    assert_eq!(
        knri["ceiling"],
        json!({ "method": "manual", "value": 140.0, "unavailable_reason": null })
    );
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
    assert_eq!(above[0]["method"], "manual");
    // Sem outro FII, a classe fica bloqueada pelo teto e a renda fixa vai só
    // até a meta dela.
    let blocked = notices(&plan.body, "class_blocked");
    assert_eq!(blocked[0]["asset_class"], "br_reits");
    assert_eq!(blocked[0]["reason"], "above_ceiling");
    assert_eq!(plan.body["unallocated"], 1000.0);
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    // `eligible` é nota e preço (a fatia na composição-alvo); o teto fica no
    // ativo e no plano.
    assert_eq!(
        find(&portfolio.body["classes"], "asset_class", "br_reits")["eligible"],
        true
    );
    assert_eq!(
        find(&portfolio.body["assets"], "symbol", "KNRI11")["ceiling"]["value"],
        140.0
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
        json!({ "ceiling": { "method": "manual", "value": 150 } }),
    )
    .await;
    assert_eq!(raised.status, StatusCode::OK, "{}", raised.text);
    assert_eq!(raised.body["ceiling"]["value"], 150.0);
    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(item(&plan.body, "KNRI11").unwrap()["quantity"], 7.0);
    assert!(notices(&plan.body, "asset_above_ceiling").is_empty());

    let cleared = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "ceiling": null }),
    )
    .await;
    assert_eq!(cleared.body["ceiling"], Value::Null);

    for (body, why) in [
        (
            json!({ "ceiling": { "method": "manual", "value": 0 } }),
            "teto zero",
        ),
        (
            json!({ "ceiling": { "method": "manual", "value": -1 } }),
            "teto negativo",
        ),
        (
            json!({ "ceiling": { "method": "manual" } }),
            "valor proprio sem valor",
        ),
        (
            json!({ "ceiling": { "method": "graham" } }),
            "graham em FII",
        ),
        (json!({ "ceiling": { "method": "bazin" } }), "bazin em FII"),
        (
            json!({ "ceiling": { "method": "nav", "value": 3 } }),
            "metodo com valor",
        ),
        (json!({ "max_price": 150 }), "campo antigo"),
    ] {
        let reply = patch(&app.router, &format!("/api/v1/assets/{id}"), body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{why}");
    }
    let by_value = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "display_name": "CDB", "amount": 10, "ceiling": { "method": "manual", "value": 1 } }),
    )
    .await;
    assert_eq!(by_value.status, StatusCode::UNPROCESSABLE_ENTITY);
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let selic = find(&portfolio.body["assets"], "display_name", "Tesouro Selic");
    let selic_id = selic["id"].as_str().unwrap();
    let reply = patch(
        &app.router,
        &format!("/api/v1/assets/{selic_id}"),
        json!({ "ceiling": { "method": "manual", "value": 1 } }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);

    // O teto é da pessoa: outra não enxerga nem mexe no ativo.
    let other = app_for(db, dir, "outra-teto@prumo.test");
    let reply = patch(
        &other.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "ceiling": { "method": "manual", "value": 1 } }),
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
