//! O contrato de `docs/api.md`, rota por rota, contra um Postgres de verdade.
//!
//! Um contêiner para o arquivo inteiro (subir um por teste custaria minutos);
//! cada cenário usa um e-mail próprio, e como tudo é filtrado pelo dono, um
//! cenário não enxerga o dado do outro.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use axum::http::{header, Method, StatusCode};
use common::*;
use serde_json::{json, Value};

fn static_dir() -> tempfile_dir::Dir {
    tempfile_dir::Dir::new()
}

#[tokio::test]
async fn api_contract() {
    let db = database().await;
    let dir = static_dir();

    info_is_public_and_carries_the_commit(&db, &dir).await;
    first_access_creates_an_empty_portfolio_with_default_checklists(&db, &dir).await;
    goals_must_sum_one_hundred(&db, &dir).await;
    assets_are_created_priced_and_validated(&db, &dir).await;
    b3_listings_and_assets_not_bought_yet(&db, &dir).await;
    checklist_answers_drive_the_score(&db, &dir).await;
    plan_and_contribution_round_trip(&db, &dir).await;
    checklist_questions_can_be_edited_and_reset(&db, &dir).await;
    symbols_and_quote_refresh(&db, &dir).await;
    one_person_never_sees_another(&db, &dir).await;
    routing_static_files_and_ops(&db, &dir).await;
}

async fn info_is_public_and_carries_the_commit(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "info@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let reply = get(&app.router, "/api/v1/info").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.body,
        json!({ "version": "9.9.9", "commit": "abc1234" })
    );
    assert_eq!(
        reply.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(
        reply.headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff"
    );
    assert!(reply.headers.get(header::CONTENT_SECURITY_POLICY).is_some());
}

async fn first_access_creates_an_empty_portfolio_with_default_checklists(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let app = app(
        &db.store,
        "Nova@Prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let me = get(&app.router, "/api/v1/me").await;
    assert_eq!(me.body["email"], "Nova@Prumo.test");

    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(portfolio.status, StatusCode::OK);
    let body = &portfolio.body;
    assert_eq!(body["total_value"], 0.0);
    assert_eq!(body["assets"], json!([]));
    let classes: Vec<&str> = body["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["asset_class"].as_str().unwrap())
        .collect();
    assert_eq!(
        classes,
        [
            "br_stocks",
            "br_reits",
            "intl_stocks",
            "us_reits",
            "crypto",
            "fixed_income",
            "intl_fixed_income"
        ]
    );
    assert_eq!(body["checklists"]["stocks"].as_array().unwrap().len(), 10);
    assert_eq!(
        body["checklists"]["real_estate"].as_array().unwrap().len(),
        8
    );
    assert_eq!(body["checklists"]["stocks"][0]["position"], 1);

    // O segundo acesso não duplica as perguntas.
    let again = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(
        again.body["checklists"]["real_estate"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
}

async fn goals_must_sum_one_hundred(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "metas@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let ok = put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 33, "intl_stocks": 33, "br_reits": 5, "crypto": 5, "fixed_income": 24 } }),
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);
    assert_eq!(ok.body["targets"]["br_reits"], 5.0);
    assert_eq!(ok.body["targets"]["us_reits"], 0.0);

    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(
        find(&portfolio.body["classes"], "asset_class", "intl_stocks")["target_percent"],
        33.0
    );

    let short = put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 95 } }),
    )
    .await;
    assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(short.body["code"], "targets_must_sum_100");
    assert_eq!(short.body["detail"], "A soma enviada foi 95%.");
    assert_eq!(
        short.headers.get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );

    let decimals = put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 50.25, "fixed_income": 49.75 } }),
    )
    .await;
    assert_eq!(decimals.status, StatusCode::OK);
    for bad in [
        json!({ "br_stocks": 100.001 }),
        json!({ "br_stocks": -1, "crypto": 101 }),
        json!({ "br_stocks": "100" }),
    ] {
        let reply = put(&app.router, "/api/v1/goals", json!({ "targets": bad })).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        assert_eq!(reply.body["code"], "validation_failed");
    }
    let unknown = put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "imoveis": 100 } }),
    )
    .await;
    assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
}

async fn assets_are_created_priced_and_validated(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "ativos@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );

    let knri = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "knri11.sa", "quantity": 12 }),
    )
    .await;
    assert_eq!(knri.status, StatusCode::CREATED, "{}", knri.text);
    assert_eq!(knri.body["symbol"], "KNRI11");
    assert_eq!(knri.body["display_name"], "Kinea Renda Imobiliaria");
    assert_eq!(knri.body["unit_price"], 142.0);
    assert_eq!(knri.body["price_source"], "quote");
    assert_eq!(knri.body["current_value"], 1704.0);
    assert_eq!(knri.body["score"], 0);
    assert_eq!(knri.body["score_source"], "none");
    assert_eq!(knri.body["manual_price"], Value::Null);

    let duplicate = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 1 }),
    )
    .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    assert_eq!(duplicate.body["code"], "asset_already_exists");

    let btc = post(&app.router, "/api/v1/assets", json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 0.0005, "score_override": 8 })).await;
    assert_eq!(btc.status, StatusCode::CREATED, "{}", btc.text);
    assert_eq!(btc.body["unit_price"], 330000.0);
    assert_eq!(btc.body["currency"], "USD");
    assert_eq!(btc.body["current_value"], 165.0);
    assert_eq!(btc.body["score"], 8);
    assert_eq!(btc.body["score_source"], "manual");

    let rf = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "display_name": "Tesouro IPCA+ 2035", "amount": 50000, "score_override": 7 }),
    )
    .await;
    assert_eq!(rf.status, StatusCode::CREATED, "{}", rf.text);
    assert_eq!(rf.body["symbol"], Value::Null);
    assert_eq!(rf.body["current_value"], 50000.0);
    assert_eq!(rf.body["price_source"], "manual");

    let unpriced = post(&app.router, "/api/v1/assets", json!({ "asset_class": "br_stocks", "symbol": "SEMCOT3", "quantity": 10, "display_name": "Sem cotacao" })).await;
    assert_eq!(unpriced.status, StatusCode::CREATED);
    assert_eq!(unpriced.body["price_source"], "missing");
    assert_eq!(unpriced.body["unit_price"], Value::Null);
    assert_eq!(unpriced.body["display_name"], "Sem cotacao");

    for (body, why) in [
        (
            json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 1.5 }),
            "fracao em cota",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "", "quantity": 1 }),
            "simbolo vazio",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": -1 }),
            "negativo",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 1, "score_override": 11 }),
            "nota 11",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 1, "manual_price": 0 }),
            "preco zero",
        ),
        (
            json!({ "asset_class": "fixed_income", "amount": 10 }),
            "renda fixa sem nome",
        ),
        (
            json!({ "asset_class": "fixed_income", "display_name": "CDB", "amount": 10, "manual_price": 2 }),
            "renda fixa com preco",
        ),
        (
            json!({ "asset_class": "imoveis", "symbol": "X" }),
            "classe desconhecida",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 1, "x": 1 }),
            "campo desconhecido",
        ),
    ] {
        let reply = post(&app.router, "/api/v1/assets", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
    }

    let id = knri.body["id"].as_str().unwrap();
    let patched = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "quantity": 15, "manual_price": 150, "display_name": "Kinea" }),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text);
    assert_eq!(patched.body["quantity"], 15.0);
    assert_eq!(patched.body["unit_price"], 150.0);
    assert_eq!(patched.body["manual_price"], 150.0);
    assert_eq!(patched.body["price_source"], "manual");
    assert_eq!(patched.body["display_name"], "Kinea");
    let cleared = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "manual_price": null }),
    )
    .await;
    assert_eq!(cleared.body["price_source"], "quote");
    assert_eq!(cleared.body["manual_price"], Value::Null);

    let rf_id = rf.body["id"].as_str().unwrap();
    let rf_patched = patch(
        &app.router,
        &format!("/api/v1/assets/{rf_id}"),
        json!({ "amount": 52000.5 }),
    )
    .await;
    assert_eq!(rf_patched.body["current_value"], 52000.5);
    for (body, why) in [
        (json!({ "quantity": 1.5 }), "fracao"),
        (json!({ "amount": 10 }), "amount em ativo cotado"),
        (json!({ "score_override": -1 }), "nota negativa"),
        (json!({ "display_name": "   " }), "nome vazio"),
    ] {
        let reply = patch(&app.router, &format!("/api/v1/assets/{id}"), body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{why}");
    }
    let rf_price = patch(
        &app.router,
        &format!("/api/v1/assets/{rf_id}"),
        json!({ "manual_price": 3 }),
    )
    .await;
    assert_eq!(rf_price.status, StatusCode::UNPROCESSABLE_ENTITY);

    let missing = patch(
        &app.router,
        &format!("/api/v1/assets/{}", uuid::Uuid::new_v4()),
        json!({ "quantity": 1 }),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let garbage = patch(
        &app.router,
        "/api/v1/assets/nao-e-uuid",
        json!({ "quantity": 1 }),
    )
    .await;
    assert_eq!(garbage.status, StatusCode::NOT_FOUND);

    let removed = delete(
        &app.router,
        &format!("/api/v1/assets/{}", unpriced.body["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let again = delete(
        &app.router,
        &format!("/api/v1/assets/{}", unpriced.body["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

fn class_blocked<'a>(plan: &'a Value, class: &str) -> Option<&'a Value> {
    plan["notices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["code"] == "class_blocked" && n["asset_class"] == class)
}

/// Ativo negociado na B3 dentro de outra classe (ETF de bitcoin em Cripto) e
/// ativo cadastrado sem posição, só com a nota — os dois juntos destravam o
/// aporte de quem ainda não tem nada nas classes abaixo da meta.
async fn b3_listings_and_assets_not_bought_yet(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "b3@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_reits": 40, "crypto": 30, "intl_stocks": 10, "fixed_income": 20 } }),
    )
    .await;

    // Sem a marca, o ETF é procurado como cripto (`QBTC11-USD`) e fica sem preço.
    let etf = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "crypto", "symbol": "QBTC11", "quantity": 0, "score_override": 7 }),
    )
    .await;
    assert_eq!(etf.status, StatusCode::CREATED, "{}", etf.text);
    assert_eq!(etf.body["listed_on_b3"], false);
    assert_eq!(etf.body["price_source"], "missing");
    let etf_id = etf.body["id"].as_str().unwrap().to_owned();

    let blocked = post(&app.router, "/api/v1/plans", json!({ "amount": 1000 })).await;
    assert_eq!(
        class_blocked(&blocked.body, "crypto").map(|n| n["reason"].clone()),
        Some(json!("unpriced"))
    );
    assert_eq!(
        class_blocked(&blocked.body, "br_reits").map(|n| n["reason"].clone()),
        Some(json!("no_assets"))
    );

    // Ligada a marca, a cotação vem na hora, em reais.
    let listed = patch(
        &app.router,
        &format!("/api/v1/assets/{etf_id}"),
        json!({ "listed_on_b3": true }),
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text);
    assert_eq!(listed.body["listed_on_b3"], true);
    assert_eq!(listed.body["unit_price"], 52.0);
    assert_eq!(listed.body["currency"], "BRL");
    assert_eq!(listed.body["price_source"], "quote");
    assert_eq!(listed.body["asset_class"], "crypto");

    // Na B3 a cota é inteira, inclusive dentro de Cripto.
    let fraction = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "crypto", "symbol": "HASH11", "quantity": 0.5, "listed_on_b3": true, "score_override": 5 }),
    )
    .await;
    assert_eq!(fraction.status, StatusCode::UNPROCESSABLE_ENTITY);
    let btc = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 0.5, "score_override": 5 }),
    )
    .await;
    let btc_id = btc.body["id"].as_str().unwrap().to_owned();
    let half_on_b3 = patch(
        &app.router,
        &format!("/api/v1/assets/{btc_id}"),
        json!({ "listed_on_b3": true }),
    )
    .await;
    assert_eq!(half_on_b3.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        delete(&app.router, &format!("/api/v1/assets/{btc_id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    // Ações Nacionais e FIIs são da B3 por definição.
    let not_b3 = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0, "listed_on_b3": false }),
    )
    .await;
    assert_eq!(not_b3.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Sem comprar ainda: quantidade 0, nota dada, e ele já entra na divisão.
    for (symbol, score) in [("KNRI11", 6), ("MXRF11", 2)] {
        let reit = post(
            &app.router,
            "/api/v1/assets",
            json!({ "asset_class": "br_reits", "symbol": symbol, "quantity": 0, "score_override": score }),
        )
        .await;
        assert_eq!(reit.status, StatusCode::CREATED, "{}", reit.text);
        assert_eq!(reit.body["listed_on_b3"], true);
        assert_eq!(reit.body["current_value"], 0.0);
    }

    // O % ideal é a fatia nota/soma das notas da classe: a mesma do motor, e
    // soma 100 entre os ativos que recebem.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let reits: Vec<&Value> = portfolio.body["assets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["asset_class"] == "br_reits")
        .collect();
    let ideal = |symbol: &str| {
        reits.iter().find(|a| a["symbol"] == symbol).unwrap()["ideal_class_percent"]
            .as_f64()
            .unwrap()
    };
    assert!((ideal("KNRI11") - 3.0 * ideal("MXRF11")).abs() < 0.02);
    let sum: f64 = reits
        .iter()
        .map(|a| a["ideal_class_percent"].as_f64().unwrap())
        .sum();
    assert!((sum - 100.0).abs() < 0.05, "soma {sum}");

    // Renda fixa negociada na B3 (ETF de IMA-B) tem ticker e cota; virar
    // renda fixa por valor é outro cadastro.
    let ima_b = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "IMAB11", "quantity": 0, "listed_on_b3": true, "score_override": 6 }),
    )
    .await;
    assert_eq!(ima_b.status, StatusCode::CREATED, "{}", ima_b.text);
    assert_eq!(ima_b.body["unit_price"], 100.0);
    let ima_b_id = ima_b.body["id"].as_str().unwrap().to_owned();
    let swap = patch(
        &app.router,
        &format!("/api/v1/assets/{ima_b_id}"),
        json!({ "listed_on_b3": false }),
    )
    .await;
    assert_eq!(swap.status, StatusCode::UNPROCESSABLE_ENTITY);

    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 1000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    let qbtc = find(&plan.body["items"], "symbol", "QBTC11");
    assert_eq!(qbtc["listed_on_b3"], true);
    assert_eq!(qbtc["divisible"], false);
    assert_eq!(qbtc["asset_class"], "crypto");
    find(&plan.body["items"], "symbol", "KNRI11");
    find(&plan.body["items"], "symbol", "IMAB11");
    // Só Ações Internacionais segue sem ativo — e o aviso diz isso.
    let blocked: Vec<&Value> = plan.body["notices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["code"] == "class_blocked")
        .collect();
    assert_eq!(blocked.len(), 1, "{}", plan.text);
    assert_eq!(blocked[0]["asset_class"], "intl_stocks");
    assert_eq!(blocked[0]["reason"], "no_assets");

    // A rodada de cotações acompanha o mercado de cada ativo.
    let refreshed = post(&app.router, "/api/v1/quotes/refresh", json!({})).await;
    assert_eq!(refreshed.body["refreshed"], 4, "{}", refreshed.text);
    assert_eq!(refreshed.body["failed"], json!([]));
}

async fn checklist_answers_drive_the_score(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "nota@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let asset = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "TRXF11", "quantity": 25 }),
    )
    .await;
    let id = asset.body["id"].as_str().unwrap().to_owned();
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let reits: Vec<String> = portfolio.body["checklists"]["real_estate"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_str().unwrap().to_owned())
        .collect();
    let stock_question = portfolio.body["checklists"]["stocks"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut answers = serde_json::Map::new();
    for (i, q) in reits.iter().take(6).enumerate() {
        answers.insert(q.clone(), json!(i != 4));
    }
    let scored = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "answers": answers }),
    )
    .await;
    assert_eq!(scored.status, StatusCode::OK, "{}", scored.text);
    assert_eq!(scored.body["score"], 4);
    assert_eq!(scored.body["score_source"], "checklist");
    assert_eq!(scored.body["answers"][&reits[4]], false);

    let removed = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "answers": { reits[4].clone(): null } }),
    )
    .await;
    assert_eq!(removed.body["score"], 5);
    assert!(removed.body["answers"].get(&reits[4]).is_none());

    let wrong = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "answers": { stock_question: true } }),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);

    let manual = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "score_override": 9 }),
    )
    .await;
    assert_eq!(
        (
            manual.body["score"].clone(),
            manual.body["score_source"].clone()
        ),
        (json!(9), json!("manual"))
    );
    let back = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "score_override": null }),
    )
    .await;
    assert_eq!(back.body["score"], 5);

    let btc = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 0.1 }),
    )
    .await;
    let crypto_answers = patch(
        &app.router,
        &format!("/api/v1/assets/{}", btc.body["id"].as_str().unwrap()),
        json!({ "answers": { reits[0].clone(): true } }),
    )
    .await;
    assert_eq!(crypto_answers.status, StatusCode::UNPROCESSABLE_ENTITY);
}

async fn plan_and_contribution_round_trip(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "aporte@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_reits": 60, "crypto": 10, "intl_stocks": 30 } }),
    )
    .await;

    let empty = post(&app.router, "/api/v1/plans", json!({ "amount": 1000 })).await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(empty.body["unallocated"], 1000.0);
    assert_eq!(empty.body["items"], json!([]));

    let mut ids = Vec::new();
    for (symbol, quantity) in [("BTLG11", 18), ("TRXF11", 25), ("KNRI11", 9)] {
        let reply = post(&app.router, "/api/v1/assets", json!({ "asset_class": "br_reits", "symbol": symbol, "quantity": quantity, "score_override": 6 })).await;
        ids.push(reply.body["id"].as_str().unwrap().to_owned());
    }
    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 0.001, "score_override": 8 }),
    )
    .await;

    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 6000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    let body = &plan.body;
    let sum = body["invested"].as_f64().unwrap()
        + body["change"].as_f64().unwrap()
        + body["unallocated"].as_f64().unwrap();
    assert!((sum - 6000.0).abs() < 0.005, "{body}");
    assert!(
        body["unallocated"].as_f64().unwrap() > 0.0,
        "acoes internacionais sem ativo: {body}"
    );
    let blocked = find(&body["notices"], "code", "class_blocked");
    assert_eq!(blocked["asset_class"], "intl_stocks");
    assert_eq!(
        find(&body["classes"], "asset_class", "intl_stocks")["status"],
        "blocked"
    );
    assert_eq!(
        find(&body["classes"], "asset_class", "br_reits")["status"],
        "receiving"
    );
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(item["symbol"].is_string() && item["display_name"].is_string());
        assert!(item["value"].as_f64().unwrap() > 0.0);
    }
    let values: Vec<f64> = items.iter().map(|i| i["value"].as_f64().unwrap()).collect();
    assert!(
        values.windows(2).all(|w| w[0] >= w[1]),
        "itens do maior para o menor"
    );

    for bad in [
        json!({ "amount": -5 }),
        json!({ "amount": "muito" }),
        json!({}),
        json!([1]),
    ] {
        let reply = post(&app.router, "/api/v1/plans", bad.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    let not_json = call(&app.router, Method::POST, "/api/v1/plans", None).await;
    assert_ne!(not_json.status, StatusCode::OK);

    // Registra o plano como veio.
    let request_items: Vec<Value> = items
        .iter()
        .map(|i| json!({ "asset_id": i["asset_id"], "quantity": i["quantity"], "unit_price": i["unit_price"] }))
        .collect();
    let recorded = post(
        &app.router,
        "/api/v1/contributions",
        json!({ "amount": 6000, "items": request_items }),
    )
    .await;
    assert_eq!(recorded.status, StatusCode::CREATED, "{}", recorded.text);
    assert_eq!(recorded.body["amount"], 6000.0);
    assert_eq!(
        recorded.body["items"].as_array().unwrap().len(),
        items.len()
    );

    let after = get(&app.router, "/api/v1/portfolio").await;
    let total_after = after.body["total_value"].as_f64().unwrap();
    assert!(
        (total_after - body["total_after"].as_f64().unwrap()).abs() < 1.0,
        "{total_after} vs {}",
        body["total_after"]
    );

    let history = get(&app.router, "/api/v1/contributions").await;
    assert_eq!(history.status, StatusCode::OK);
    assert_eq!(history.body.as_array().unwrap().len(), 1);
    assert_eq!(history.body[0]["id"], recorded.body["id"]);
    assert!(history.body[0]["invested"].as_f64().unwrap() > 0.0);

    let ghost = post(
        &app.router,
        "/api/v1/contributions",
        json!({ "amount": 10, "items": [{ "asset_id": uuid::Uuid::new_v4(), "quantity": 1, "unit_price": 10 }] }),
    )
    .await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    for (body, why) in [
        (json!({ "amount": 10, "items": [] }), "sem itens"),
        (
            json!({ "amount": 10, "items": [{ "asset_id": ids[0], "quantity": 1, "unit_price": 10 }, { "asset_id": ids[0], "quantity": 1, "unit_price": 10 }] }),
            "repetido",
        ),
        (
            json!({ "amount": 10, "items": [{ "asset_id": ids[0], "quantity": 0, "unit_price": 10 }] }),
            "quantidade zero",
        ),
        (
            json!({ "amount": 10, "items": [{ "asset_id": ids[0], "quantity": 0.0000001, "unit_price": 1 }] }),
            "valor abaixo do centavo",
        ),
    ] {
        let reply = post(&app.router, "/api/v1/contributions", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
    }
    let still_one = get(&app.router, "/api/v1/contributions").await;
    assert_eq!(
        still_one.body.as_array().unwrap().len(),
        1,
        "aporte recusado nao deixa rastro"
    );
}

async fn checklist_questions_can_be_edited_and_reset(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "criterios@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let added = post(
        &app.router,
        "/api/v1/checklists/stocks/questions",
        json!({ "prompt": "  Tem moat claro?  " }),
    )
    .await;
    assert_eq!(added.status, StatusCode::CREATED);
    assert_eq!(added.body["prompt"], "Tem moat claro?");
    assert_eq!(added.body["position"], 11);
    let id = added.body["id"].as_str().unwrap();

    let edited = patch(
        &app.router,
        &format!("/api/v1/checklist-questions/{id}"),
        json!({ "prompt": "Tem vantagem competitiva?" }),
    )
    .await;
    assert_eq!(edited.body["prompt"], "Tem vantagem competitiva?");
    let empty = patch(
        &app.router,
        &format!("/api/v1/checklist-questions/{id}"),
        json!({ "prompt": " " }),
    )
    .await;
    assert_eq!(empty.status, StatusCode::UNPROCESSABLE_ENTITY);
    let long = post(
        &app.router,
        "/api/v1/checklists/stocks/questions",
        json!({ "prompt": "x".repeat(301) }),
    )
    .await;
    assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);
    let unknown = post(
        &app.router,
        "/api/v1/checklists/crypto/questions",
        json!({ "prompt": "x" }),
    )
    .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    let removed = delete(&app.router, &format!("/api/v1/checklist-questions/{id}")).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        delete(&app.router, &format!("/api/v1/checklist-questions/{id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let missing = patch(
        &app.router,
        &format!("/api/v1/checklist-questions/{}", uuid::Uuid::new_v4()),
        json!({ "prompt": "x" }),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let first = get(&app.router, "/api/v1/portfolio").await.body["checklists"]["real_estate"][0]
        ["id"]
        .clone();
    delete(
        &app.router,
        &format!("/api/v1/checklist-questions/{}", first.as_str().unwrap()),
    )
    .await;
    let reset = post(
        &app.router,
        "/api/v1/checklists/real_estate/reset",
        json!({}),
    )
    .await;
    assert_eq!(reset.status, StatusCode::OK);
    assert_eq!(reset.body.as_array().unwrap().len(), 8);
    assert_eq!(
        post(&app.router, "/api/v1/checklists/nada/reset", json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

async fn symbols_and_quote_refresh(db: &Database, dir: &tempfile_dir::Dir) {
    let quotes = FakeQuotes::with_market();
    let app = app(
        &db.store,
        "cotacao@prumo.test",
        Arc::clone(&quotes),
        &dir.path(),
    );

    let found = get(&app.router, "/api/v1/symbols?query=knri").await;
    assert_eq!(found.status, StatusCode::OK);
    assert_eq!(found.body[0]["symbol"], "KNRI");
    assert_eq!(found.body[0]["suggested_class"], "br_reits");
    assert_eq!(found.body[0]["listed_on_b3"], true);
    assert_eq!(
        get(&app.router, "/api/v1/symbols?query=").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        get(&app.router, "/api/v1/symbols").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 2 }),
    )
    .await;
    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "SUMIU3", "quantity": 2 }),
    )
    .await;
    let refreshed = post(&app.router, "/api/v1/quotes/refresh", json!({})).await;
    assert_eq!(refreshed.status, StatusCode::OK);
    assert_eq!(refreshed.body["refreshed"], 1);
    assert_eq!(refreshed.body["failed"], json!(["SUMIU3"]));
    let throttled = post(&app.router, "/api/v1/quotes/refresh", json!({})).await;
    assert_eq!(throttled.body, json!({ "refreshed": 0, "failed": [] }));

    // A rodada periódica cobre todas as carteiras.
    let report = app.state.quotes.refresh_all().await;
    assert!(report.refreshed >= 1);

    *quotes.fail.lock().unwrap() = true;
    let down = get(&app.router, "/api/v1/symbols?query=knri").await;
    assert_eq!(down.status, StatusCode::BAD_GATEWAY);
    assert_eq!(down.body["code"], "quote_provider_unavailable");
    // Sem provedor o ativo nasce sem preço, e não falha.
    let offline = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 1 }),
    )
    .await;
    assert_eq!(offline.status, StatusCode::CREATED);
    assert_eq!(offline.body["display_name"], "WEGE3");
}

async fn one_person_never_sees_another(db: &Database, dir: &tempfile_dir::Dir) {
    let alice = app(
        &db.store,
        "alice@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let bruno = app(
        &db.store,
        "bruno@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let asset = post(
        &alice.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 3 }),
    )
    .await;
    let id = asset.body["id"].as_str().unwrap();
    let alice_question =
        get(&alice.router, "/api/v1/portfolio").await.body["checklists"]["stocks"][0]["id"].clone();

    assert_eq!(
        get(&bruno.router, "/api/v1/portfolio").await.body["assets"],
        json!([])
    );
    assert_eq!(
        patch(
            &bruno.router,
            &format!("/api/v1/assets/{id}"),
            json!({ "quantity": 99 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&bruno.router, &format!("/api/v1/assets/{id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let steal = post(
        &bruno.router,
        "/api/v1/contributions",
        json!({ "amount": 10, "items": [{ "asset_id": id, "quantity": 1, "unit_price": 10 }] }),
    )
    .await;
    assert_eq!(steal.status, StatusCode::NOT_FOUND);
    let edit = patch(
        &bruno.router,
        &format!(
            "/api/v1/checklist-questions/{}",
            alice_question.as_str().unwrap()
        ),
        json!({ "prompt": "x" }),
    )
    .await;
    assert_eq!(edit.status, StatusCode::NOT_FOUND);
    assert_eq!(
        get(&alice.router, "/api/v1/portfolio").await.body["assets"][0]["quantity"],
        3.0
    );
}

async fn routing_static_files_and_ops(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "rotas@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );

    let unknown_api = get(&app.router, "/api/v1/nao-existe").await;
    assert_eq!(unknown_api.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown_api.body["code"], "not_found");
    assert_eq!(
        get(&app.router, "/api/v2/qualquer").await.body["code"],
        "not_found"
    );
    assert_eq!(get(&app.router, "/api").await.status, StatusCode::NOT_FOUND);

    for path in ["/healthz", "/readyz", "/metrics"] {
        assert_eq!(
            get(&app.router, path).await.status,
            StatusCode::NOT_FOUND,
            "{path} na porta publica"
        );
    }

    let shell = get(&app.router, "/").await;
    assert_eq!(shell.status, StatusCode::OK);
    assert!(shell.text.contains("<title>Prumo</title>"));
    // `no-transform`: a Cloudflare não injeta script que a CSP recusaria.
    assert_eq!(
        shell.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-cache, no-transform"
    );
    let deep_link = get(&app.router, "/carteira/qualquer").await;
    assert!(deep_link.text.contains("<title>Prumo</title>"));
    let asset = get(&app.router, "/assets/app-123.js").await;
    assert_eq!(asset.status, StatusCode::OK);
    assert_eq!(
        asset.headers.get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        get(&app.router, "/assets/nao-existe.js").await.status,
        StatusCode::NOT_FOUND
    );

    assert_eq!(get(&app.ops, "/healthz").await.text, "ok");
    assert_eq!(get(&app.ops, "/readyz").await.status, StatusCode::OK);
    let metrics = get(&app.ops, "/metrics").await;
    assert_eq!(metrics.status, StatusCode::OK);
    assert_eq!(
        metrics.headers.get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
}

#[tokio::test]
async fn readyz_says_503_when_the_database_is_gone() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(500))
        .connect_lazy("postgres://ninguem:nada@127.0.0.1:1/nada")
        .unwrap();
    let store = prumo::store::Store::from_pool(pool);
    let app = app(&store, "x@prumo.test", FakeQuotes::with_market(), "static");
    assert_eq!(
        get(&app.ops, "/readyz").await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    // Sem banco, a pessoa recebe 500 em problem+json — nunca um 200 vazio.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(portfolio.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(portfolio.body["code"], "internal");
}
