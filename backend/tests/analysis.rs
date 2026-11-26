//! Análise da carteira contra um Postgres de verdade: as dimensões e as regras
//! (diversificação, dividendos, rentabilidade), o "sem dado" que não é falha, a
//! configuração (e a compatibilidade com `/health/rules`), perfil, destaques,
//! referências de preço, o dono e a migração 0021. Carteira fictícia.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::dividends::{DividendEvent, DividendHistory};
use prumo::domain::income::{today_in_brazil, YearMonth};
use prumo::history::{HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::{local_date, Performance};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use time::macros::format_description;
use time::{Date, Duration, Month, OffsetDateTime};
use uuid::Uuid;

// ------------------------------------------------------------ utilidades --

fn rule<'a>(analysis: &'a Value, id: &str) -> &'a Value {
    analysis["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["rules"].as_array().unwrap())
        .find(|r| r["rule"] == id)
        .unwrap_or_else(|| panic!("regra {id} ausente"))
}

fn dimension<'a>(analysis: &'a Value, id: &str) -> &'a Value {
    find(&analysis["dimensions"], "id", id)
}

fn status<'a>(analysis: &'a Value, id: &str) -> &'a str {
    rule(analysis, id)["status"].as_str().unwrap()
}

async fn analysis(app: &TestApp) -> Value {
    let reply = get(&app.router, "/api/v1/analysis").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply.body
}

async fn add_asset(app: &TestApp, body: Value) -> String {
    let created = post(&app.router, "/api/v1/assets", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    created.body["id"].as_str().unwrap().to_owned()
}

fn close(value: &Value, expected: f64) {
    let got = value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} não é número"));
    assert!((got - expected).abs() < 0.011, "{got} != {expected}");
}

fn person(db: &Database, dir: &tempfile_dir::Dir, email: &str) -> TestApp {
    app(&db.store, email, FakeQuotes::with_market(), &dir.path())
}

/// O dia 1 do mês que ficou `back` meses para trás (0 = o mês de hoje).
fn month_day(back: i32) -> String {
    let month = YearMonth::of(today_in_brazil(OffsetDateTime::now_utc())).shift(-back);
    Date::from_calendar_date(month.year, Month::try_from(month.month).unwrap(), 1)
        .unwrap()
        .format(format_description!("[year]-[month]-[day]"))
        .unwrap()
}

async fn pay(app: &TestApp, asset: &str, back: i32, per_unit: f64) {
    let created = post(
        &app.router,
        "/api/v1/income",
        json!({
            "asset_id": asset, "kind": "reit_income",
            "pay_date": month_day(back), "amount_per_unit": per_unit, "quantity": 10
        }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
}

/// Leva todos os lançamentos do ativo para `date`.
async fn backdate(app: &TestApp, asset_id: &str, date: Date) {
    let list = get(
        &app.router,
        &format!("/api/v1/transactions?asset_id={asset_id}"),
    )
    .await;
    for item in list.body.as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        let moved = patch(
            &app.router,
            &format!("/api/v1/transactions/{id}"),
            json!({ "trade_date": date.to_string() }),
        )
        .await;
        assert_eq!(moved.status, StatusCode::OK, "{}", moved.text);
    }
}

/// Séries em memória, como as do Desempenho.
#[derive(Default)]
struct FakeHistory {
    closes: Mutex<HashMap<String, Series>>,
    indexes: Mutex<HashMap<IndexCode, Series>>,
}

fn window(series: Option<&Series>, from: Date, to: Date) -> Series {
    series
        .map(|s| {
            s.iter()
                .filter(|(d, _)| *d >= from && *d <= to)
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait]
impl HistoryProvider for FakeHistory {
    async fn index(&self, code: IndexCode, from: Date, to: Date) -> Result<Series, HistoryError> {
        Ok(window(self.indexes.lock().unwrap().get(&code), from, to))
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        Ok(window(self.closes.lock().unwrap().get(symbol), from, to))
    }
}

/// `value(n)` para cada dia de `from` a `to`, com `n` contando os dias desde `from`.
fn daily(from: Date, to: Date, value: impl Fn(i64) -> Decimal) -> Series {
    let mut out = Series::new();
    let mut day = from;
    let mut n = 0;
    while day <= to {
        out.push((day, value(n)));
        day += Duration::days(1);
        n += 1;
    }
    out
}

fn compound(base: Decimal, rate: Decimal, n: i64) -> Decimal {
    let mut value = base;
    for _ in 0..n {
        value *= Decimal::ONE + rate;
    }
    value.round_dp(8)
}

// ----------------------------------------------------------------- testes --

#[tokio::test]
async fn analysis_contract() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();

    // A série é montada primeiro: a tarefa diária refaz a de todo mundo.
    returns_compare_with_the_indexes_and_short_series_have_no_data(&db, &dir).await;
    an_empty_portfolio_has_nothing_to_warn_about(&db, &dir).await;
    diversification_profile_and_the_contribution_to_fix_it(&db, &dir).await;
    dividends_rules_read_the_income_book(&db, &dir).await;
    losses_highlights_and_the_loss_rule(&db, &dir).await;
    price_references_follow_the_dividend_history(&db, &dir).await;
    configuration_is_validated_shared_with_health_and_owned(&db, &dir).await;
}

async fn an_empty_portfolio_has_nothing_to_warn_about(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "vazia@prumo.test");
    let body = analysis(&app).await;
    assert_eq!(body["total_value"], 0.0);
    assert_eq!(body["attention_count"], 0);
    assert_eq!(body["unclassified_count"], 0);
    let ids: Vec<&str> = body["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["diversification", "dividends", "returns", "quality"]);
    // Tudo ligado e sem dado: nenhuma regra falha, e a pontuação não existe.
    for id in ["diversification", "dividends", "returns", "quality"] {
        let d = dimension(&body, id);
        assert!(
            d["rules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["status"] == "no_data"),
            "{id}"
        );
        assert_eq!(d["score"]["percent"], Value::Null);
        assert_eq!(d["score"]["measured"], 0);
    }
    assert_eq!(
        dimension(&body, "diversification")["rules"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(
        dimension(&body, "dividends")["rules"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        dimension(&body, "returns")["rules"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    // A Qualidade tem as suas oito regras, todas sem dado (nenhum fundamento).
    let quality = dimension(&body, "quality");
    assert_eq!(quality["rules"].as_array().unwrap().len(), 8);
    assert_eq!(quality["score"]["no_data"], 8);
    // Os pontos de atenção também: nada avaliado, nada a dizer.
    assert_eq!(
        body["attention"],
        json!({ "assessed": 0, "without_data": 0, "items": [] })
    );
    // Perfil, destaques e referências vazios; o Bazin tem o rendimento-alvo de fábrica.
    assert_eq!(body["profile"]["current"], Value::Null);
    assert_eq!(body["profile"]["target"], Value::Null);
    assert_eq!(body["profile"]["conservative_max"], 30.0);
    assert_eq!(body["profile"]["moderate_max"], 70.0);
    assert_eq!(body["highlights"]["price_12m"]["best"], json!([]));
    assert_eq!(body["highlights"]["since_purchase"]["worst"], json!([]));
    assert_eq!(body["references"]["bazin_yield"], 6.0);
    assert_eq!(body["references"]["items"], json!([]));
    assert_eq!(body["sources"].as_array().unwrap().len(), 2);
}

async fn diversification_profile_and_the_contribution_to_fix_it(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let app = person(db, dir, "diversifica@prumo.test");
    for body in [
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 20 }),
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 2 }),
        json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 1000 }),
    ] {
        add_asset(&app, body).await;
    }
    let goals = put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 40, "br_reits": 20, "fixed_income": 40 } }),
    )
    .await;
    assert_eq!(goals.status, StatusCode::OK, "{}", goals.text);

    // WEGE3 1.000 + KNRI11 1.420 + AAPL 2.200 + CDB 1.000 = 5.620.
    let body = analysis(&app).await;
    close(&body["total_value"], 5620.0);

    let abroad = rule(&body, "min_abroad");
    assert_eq!(abroad["status"], "ok");
    close(&abroad["current"], 39.15);
    assert_eq!(abroad["direction"], "at_least");
    assert_eq!(abroad["unit"], "percent");
    let low_risk = rule(&body, "min_low_risk");
    assert_eq!(low_risk["status"], "ok");
    close(&low_risk["current"], 17.79);
    // Quatro classes com dinheiro e quatro ativos.
    assert_eq!(rule(&body, "min_classes")["status"], "ok");
    assert_eq!(rule(&body, "min_classes")["current"], 4.0);
    assert_eq!(
        rule(&body, "min_classes")["detail"]["asset_classes"],
        json!(["br_stocks", "br_reits", "intl_stocks", "fixed_income"])
    );
    let few = rule(&body, "min_portfolio_assets");
    assert_eq!(few["status"], "warning");
    assert_eq!(few["current"], 4.0);
    assert_eq!(few["limit"], 10.0);
    // Sem fonte de setor buscada: nada a medir em setor e em tijolo. Não é falha.
    assert_eq!(status(&body, "min_sectors"), "no_data");
    assert_eq!(status(&body, "brick_reit"), "no_data");
    // A regra original da Saúde agora diz o número mesmo dentro do limite e,
    // passando dele, os avisos com o aporte que corrige.
    let big = rule(&body, "max_asset");
    assert_eq!(big["status"], "warning");
    close(&big["current"], 39.15);
    assert_eq!(big["detail"]["kind"], "findings");
    assert_eq!(big["detail"]["warnings"][0]["subject"]["label"], "AAPL");
    assert!(
        big["detail"]["warnings"][0]["needed_contribution"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(rule(&body, "top_five")["status"], "no_data");

    // O perfil: 17,79% de renda fixa agora (arrojado); a meta tem 40% (moderado).
    let profile = &body["profile"];
    close(&profile["current"]["fixed_income"], 17.79);
    close(&profile["current"]["variable"], 82.21);
    assert_eq!(profile["current"]["profile"], "aggressive");
    assert_eq!(profile["target"]["fixed_income"], 40.0);
    assert_eq!(profile["target"]["variable"], 60.0);
    assert_eq!(profile["target"]["profile"], "moderate");

    // A pontuação é "ok de medidas", com os números à mostra.
    let diversification = dimension(&body, "diversification");
    let rules = diversification["rules"].as_array().unwrap();
    let count = |s: &str| rules.iter().filter(|r| r["status"] == s).count() as u64;
    let score = &diversification["score"];
    assert_eq!(score["ok"], count("ok"));
    assert_eq!(score["measured"], count("ok") + count("warning"));
    assert_eq!(score["no_data"], count("no_data"));
    let expected = (count("ok") as f64) * 100.0 / ((count("ok") + count("warning")) as f64);
    assert!((score["percent"].as_f64().unwrap() - expected).abs() < 0.06);
    assert_eq!(
        body["attention_count"].as_u64().unwrap(),
        body["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|d| d["rules"].as_array().unwrap())
            .filter(|r| r["status"] == "warning")
            .count() as u64
    );

    // Exterior mínimo de 50%: aviso, com o aporte que leva a fatia a 50%.
    // (50 × 5.620 − 100 × 2.200) / 50 = 1.220.
    let tuned = put(
        &app.router,
        "/api/v1/analysis/rules",
        json!({ "rules": { "min_abroad": { "threshold": 50 } } }),
    )
    .await;
    assert_eq!(tuned.status, StatusCode::OK, "{}", tuned.text);
    let body = analysis(&app).await;
    let abroad = rule(&body, "min_abroad");
    assert_eq!(abroad["status"], "warning");
    assert_eq!(abroad["limit"], 50.0);
    close(&abroad["needed_contribution"], 1220.0);
    // Desligada, a regra sai da conta e não tem número.
    let off = put(
        &app.router,
        "/api/v1/analysis/rules",
        json!({ "rules": { "min_abroad": { "enabled": false } } }),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK);
    let body = analysis(&app).await;
    assert_eq!(status(&body, "min_abroad"), "off");
    assert_eq!(rule(&body, "min_abroad")["current"], Value::Null);
}

async fn dividends_rules_read_the_income_book(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "renda@prumo.test");
    let knri = add_asset(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    add_asset(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 10 }),
    )
    .await;

    // Sem nenhum provento: tudo sem dado, nenhum aviso de renda.
    let body = analysis(&app).await;
    for id in [
        "min_yield",
        "min_paying_share",
        "income_growth",
        "income_every_month",
        "max_payer",
    ] {
        assert_eq!(status(&body, id), "no_data", "{id}");
    }

    // 26 meses de história: os 12 anteriores pagam 0,50 por cota e os 12 de
    // agora, 1,00 (10 cotas); dois meses mais velhos fecham os 24 meses.
    for back in 12..=25 {
        pay(&app, &knri, back, 0.5).await;
    }
    for back in 0..=11 {
        pay(&app, &knri, back, 1.0).await;
    }
    let body = analysis(&app).await;
    // 120 recebidos em 12 meses sobre 2.828 (KNRI11 1.420 + HGLG11 1.408) = 4,24%.
    let yield_rule = rule(&body, "min_yield");
    assert_eq!(yield_rule["status"], "warning");
    close(&yield_rule["current"], 4.24);
    assert_eq!(yield_rule["detail"]["kind"], "yield");
    close(&yield_rule["detail"]["income_12m"], 120.0);
    close(&yield_rule["detail"]["base_value"], 2828.0);
    // Só o KNRI11 pagou: 1 de 2 (50%) contra 70%, e o aviso diz quem ficou de fora.
    let paying = rule(&body, "min_paying_share");
    assert_eq!(paying["status"], "warning");
    assert_eq!(paying["current"], 50.0);
    assert_eq!(paying["detail"]["silent"], json!(["HGLG11"]));
    assert_eq!(paying["detail"]["paying"], 1);
    // A renda dobrou: de 60 para 120.
    let growth = rule(&body, "income_growth");
    assert_eq!(growth["status"], "ok");
    close(&growth["current"], 100.0);
    close(&growth["detail"]["current_12m"], 120.0);
    close(&growth["detail"]["previous_12m"], 60.0);
    // As duas regras que já existiam moram aqui agora.
    assert_eq!(
        dimension(&body, "dividends")["rules"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(status(&body, "income_every_month"), "ok");
    assert_eq!(status(&body, "max_payer"), "no_data");

    // O limite é da pessoa: 4% passa, 5% (padrão) não.
    let tuned = put(
        &app.router,
        "/api/v1/analysis/rules",
        json!({ "rules": { "min_yield": { "threshold": 4 }, "min_paying_share": { "enabled": false } } }),
    )
    .await;
    assert_eq!(tuned.status, StatusCode::OK, "{}", tuned.text);
    let body = analysis(&app).await;
    assert_eq!(status(&body, "min_yield"), "ok");
    assert_eq!(status(&body, "min_paying_share"), "off");

    // Quem tem pouca história não é cobrado: só três meses, sem dado.
    let new = person(db, dir, "novata@prumo.test");
    let id = add_asset(
        &new,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    for back in 0..3 {
        pay(&new, &id, back, 1.0).await;
    }
    let body = analysis(&new).await;
    for id in ["min_yield", "min_paying_share", "income_growth"] {
        assert_eq!(status(&body, id), "no_data", "{id}");
    }
}

async fn losses_highlights_and_the_loss_rule(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "perdas@prumo.test");
    // WEGE3 vale 50: custo médio 100 é perda de 50%. BTLG11 vale 94,55: custo 80.
    add_asset(
        &app,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 20, "average_price": 100 }),
    )
    .await;
    let btlg = add_asset(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "BTLG11", "quantity": 10, "average_price": 80 }),
    )
    .await;
    add_asset(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "MXRF11", "quantity": 10 }),
    )
    .await;

    let body = analysis(&app).await;
    let loss = rule(&body, "max_loss");
    assert_eq!(loss["status"], "warning");
    assert_eq!(loss["current"], 50.0);
    assert_eq!(loss["limit"], 30.0);
    assert_eq!(loss["detail"]["kind"], "losses");
    assert_eq!(loss["detail"]["assets"][0]["label"], "WEGE3");
    assert_eq!(loss["detail"]["assets"][0]["loss_percent"], 50.0);
    // MXRF11 não tem custo conhecido: fora da conta, sem aviso.
    assert_eq!(loss["detail"]["assets"].as_array().unwrap().len(), 1);

    // Pela rentabilidade desde a compra: BTLG11 +18,19%, WEGE3 −50%.
    let since = &body["highlights"]["since_purchase"];
    assert_eq!(since["best"][0]["label"], "BTLG11");
    close(&since["best"][0]["change_percent"], 18.19);
    assert_eq!(since["worst"][0]["label"], "WEGE3");
    assert_eq!(since["worst"][0]["change_percent"], -50.0);
    assert_eq!(since["best"].as_array().unwrap().len(), 1);
    // Sem histórico de preço de 12 meses, o ativo fica fora da lista de preço.
    assert_eq!(body["highlights"]["price_12m"]["best"], json!([]));
    assert_eq!(body["highlights"]["price_12m"]["worst"], json!([]));

    // Pelo preço em 12 meses: o fechamento de 12 meses atrás está no cache de
    // preços (WEGE3 a 100 e BTLG11 a 70).
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let a_year_ago = prumo::domain::performance::months_before(today, 12) - Duration::days(2);
    db.store
        .insert_prices("WEGE3.SA", &vec![(a_year_ago, dec!(100))])
        .await
        .unwrap();
    db.store
        .insert_prices("BTLG11.SA", &vec![(a_year_ago, dec!(70))])
        .await
        .unwrap();
    let body = analysis(&app).await;
    let price = &body["highlights"]["price_12m"];
    // 94,55 sobre 70 = +35,07%; 50 sobre 100 = −50%.
    assert_eq!(price["best"][0]["label"], "BTLG11");
    close(&price["best"][0]["change_percent"], 35.07);
    assert_eq!(price["worst"][0]["label"], "WEGE3");
    close(&price["worst"][0]["change_percent"], -50.0);
    assert_eq!(price["best"].as_array().unwrap().len(), 1);

    // Um desdobramento 1 para 2 depois da data-base não pode parecer queda.
    let split = post(
        &app.router,
        "/api/v1/transactions",
        json!({ "asset_id": btlg, "kind": "split", "trade_date": today.to_string(), "quantity": 10 }),
    )
    .await;
    assert_eq!(split.status, StatusCode::CREATED, "{}", split.text);
    let body = analysis(&app).await;
    // 70 antes do desdobramento valem 35 de hoje; a cota vale 94,55 → +170,14%.
    let best = &body["highlights"]["price_12m"]["best"][0];
    assert_eq!(best["label"], "BTLG11");
    close(&best["change_percent"], 170.14);
}

async fn price_references_follow_the_dividend_history(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "bazin@prumo.test");
    let knri = add_asset(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    add_asset(
        &app,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 10 }),
    )
    .await;
    let today = today_in_brazil(OffsetDateTime::now_utc());
    // Três janelas de 12 meses: 3,00, 2,40 e 1,80 por cota (média 2,40). O
    // dividendo de 1,20, de quase 4 anos atrás, mostra que o papel já pagava
    // antes da 3ª janela (ela entra inteira) e cai numa 4ª que não entra.
    let events = [
        (100, dec!(3.0)),
        (465, dec!(2.4)),
        (830, dec!(1.8)),
        (1200, dec!(1.2)),
    ]
    .into_iter()
    .map(|(back, amount)| DividendEvent {
        ex_date: today - Duration::days(back),
        amount,
    })
    .collect::<Vec<_>>();
    db.store
        .save_dividends(
            "KNRI11.SA",
            &DividendHistory {
                currency: "BRL".into(),
                events,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    let body = analysis(&app).await;
    let references = &body["references"];
    assert_eq!(references["bazin_yield"], 6.0);
    let items = references["items"].as_array().unwrap();
    // WEGE3 não tem dividendo guardado: sem referência.
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["label"], "KNRI11");
    assert_eq!(items[0]["bazin"]["years"], 3);
    close(&items[0]["bazin"]["annual_average"], 2.4);
    // 2,40 ÷ 6% = 40.
    assert_eq!(items[0]["bazin"]["price"], 40.0);
    assert_eq!(items[0]["graham"], Value::Null);
    assert_eq!(items[0]["max_price"], Value::Null);
    close(&items[0]["unit_price"], 142.0);

    // O rendimento-alvo é da pessoa: com 8%, a referência cai para 30.
    let tuned = put(
        &app.router,
        "/api/v1/analysis/rules",
        json!({ "bazin_yield": 8 }),
    )
    .await;
    assert_eq!(tuned.status, StatusCode::OK, "{}", tuned.text);
    assert_eq!(tuned.body["bazin_yield"], 8.0);
    let body = analysis(&app).await;
    assert_eq!(body["references"]["items"][0]["bazin"]["price"], 30.0);

    // O botão "usar como meu teto" é o PATCH que o ativo já tem.
    let applied = patch(
        &app.router,
        &format!("/api/v1/assets/{knri}"),
        json!({ "max_price": 30 }),
    )
    .await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text);
    let body = analysis(&app).await;
    assert_eq!(body["references"]["items"][0]["max_price"], 30.0);

    // Outra pessoa não vê o rendimento-alvo nem o teto desta.
    let other = person(db, dir, "bazin-outra@prumo.test");
    assert_eq!(analysis(&other).await["references"]["bazin_yield"], 6.0);
}

async fn configuration_is_validated_shared_with_health_and_owned(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let app = person(db, dir, "config@prumo.test");
    let defaults = get(&app.router, "/api/v1/analysis/rules").await;
    assert_eq!(defaults.status, StatusCode::OK);
    let rules = defaults.body["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 33);
    assert_eq!(defaults.body["bazin_yield"], 6.0);
    for (id, threshold, unit) in [
        ("min_portfolio_assets", Some(10.0), "count"),
        ("min_classes", Some(3.0), "count"),
        ("min_sectors", Some(4.0), "count"),
        ("min_abroad", Some(10.0), "percent"),
        ("min_low_risk", Some(10.0), "percent"),
        ("min_yield", Some(5.0), "percent"),
        ("min_paying_share", Some(70.0), "percent"),
        ("min_positive_years", Some(70.0), "percent"),
        ("max_volatility", Some(20.0), "percent"),
        ("max_loss", Some(30.0), "percent"),
        ("beat_cdi_12m", None, "percent"),
        ("income_growth", None, "percent"),
        ("brick_reit", None, "count"),
        ("quality_roe", Some(70.0), "percent"),
        ("quality_fii_vacancy", Some(70.0), "percent"),
        ("quality_recovery", None, "count"),
    ] {
        let r = find(&defaults.body["rules"], "rule", id);
        assert_eq!(r["enabled"], true, "{id}");
        assert_eq!(r["unit"], unit, "{id}");
        match threshold {
            Some(t) => assert_eq!(r["threshold"], t, "{id}"),
            None => assert_eq!(r["threshold"], Value::Null, "{id}"),
        }
    }

    // Validação: limite fora da faixa, regra sem limite, corpo vazio e campo desconhecido.
    for (body, code) in [
        (
            json!({ "rules": { "min_classes": { "threshold": 8 } } }),
            "validation_failed",
        ),
        (
            json!({ "rules": { "min_abroad": { "threshold": 0 } } }),
            "validation_failed",
        ),
        (
            json!({ "rules": { "max_volatility": { "threshold": 100.5 } } }),
            "validation_failed",
        ),
        (
            json!({ "rules": { "beat_cdi_12m": { "threshold": 5 } } }),
            "validation_failed",
        ),
        (
            json!({ "rules": { "min_abroad": {} } }),
            "validation_failed",
        ),
        (json!({ "bazin_yield": 0 }), "validation_failed"),
        (json!({ "bazin_yield": 120 }), "validation_failed"),
        (json!({}), "validation_failed"),
        (
            json!({ "rules": { "nota_final": { "enabled": true } } }),
            "validation_failed",
        ),
        (
            json!({ "rules": { "min_abroad": { "enabled": true, "peso": 3 } } }),
            "validation_failed",
        ),
        (json!({ "regras": {} }), "validation_failed"),
    ] {
        let reply = put(&app.router, "/api/v1/analysis/rules", body.clone()).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            reply.text
        );
        assert_eq!(reply.body["code"], code, "{body}");
    }
    // Nada foi gravado pelas recusas.
    let after = get(&app.router, "/api/v1/analysis/rules").await;
    assert_eq!(after.body["rules"], defaults.body["rules"]);

    // A configuração é uma só: o que se muda na Análise aparece em /health/rules
    // (as sete originais) e vice-versa; as regras novas não vazam para a Saúde.
    let tuned = put(
        &app.router,
        "/api/v1/analysis/rules",
        json!({ "rules": {
            "max_asset": { "threshold": 12.5 },
            "max_loss": { "threshold": 40, "enabled": false }
        } }),
    )
    .await;
    assert_eq!(tuned.status, StatusCode::OK, "{}", tuned.text);
    let health = get(&app.router, "/api/v1/health/rules").await;
    assert_eq!(health.body["rules"].as_array().unwrap().len(), 7);
    assert_eq!(
        find(&health.body["rules"], "rule", "max_asset")["threshold"],
        12.5
    );
    let legacy = put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "min_assets": { "threshold": 5 } } }),
    )
    .await;
    assert_eq!(legacy.status, StatusCode::OK, "{}", legacy.text);
    let both = get(&app.router, "/api/v1/analysis/rules").await;
    assert_eq!(
        find(&both.body["rules"], "rule", "min_assets")["threshold"],
        5.0
    );
    let loss = find(&both.body["rules"], "rule", "max_loss");
    assert_eq!(
        (loss["enabled"].clone(), loss["threshold"].clone()),
        (json!(false), json!(40.0))
    );
    // O endereço antigo segue respondendo, com as sete regras.
    let old = get(&app.router, "/api/v1/health").await;
    assert_eq!(old.status, StatusCode::OK, "{}", old.text);
    assert_eq!(old.body["rules"].as_array().unwrap().len(), 7);

    // Cada pessoa tem as suas.
    let other = person(db, dir, "config-outra@prumo.test");
    let theirs = get(&other.router, "/api/v1/analysis/rules").await;
    assert_eq!(
        find(&theirs.body["rules"], "rule", "max_asset")["threshold"],
        15.0
    );
    assert_eq!(
        find(&theirs.body["rules"], "rule", "max_loss")["enabled"],
        true
    );
}

/// Seis anos de série (e índices de ponta a ponta): CDI e Ibovespa passam a
/// carteira, o IPCA não; a pessoa com três meses de série não tem comparação
/// nenhuma, e "sem dado" não vira aviso.
async fn returns_compare_with_the_indexes_and_short_series_have_no_data(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let now = OffsetDateTime::now_utc();
    let today = local_date(now);
    let start = today - Duration::days(365 * 6 + 30);
    let history = Arc::new(FakeHistory::default());

    // O KNRI11 sobe 0,03% ao dia e fecha hoje no preço que a tela mostra (142).
    let days = (today - start).whole_days();
    let end_price = dec!(142);
    let knri: Series = daily(start, today, |n| {
        let back = days - n;
        let mut price = end_price;
        for _ in 0..back {
            price /= dec!(1.0003);
        }
        price.round_dp(8)
    });
    history
        .closes
        .lock()
        .unwrap()
        .insert("KNRI11.SA".into(), knri);
    // O CDI rende 0,0455% ao dia (~18% ao ano); o Ibovespa sobe 0,05% ao dia.
    history.indexes.lock().unwrap().insert(
        IndexCode::Cdi,
        daily(start - Duration::days(30), today, |_| dec!(0.0455)),
    );
    history.indexes.lock().unwrap().insert(
        IndexCode::Ibov,
        daily(start - Duration::days(30), today, |n| {
            compound(dec!(100000), dec!(0.0005), n)
        }),
    );
    // O IPCA, 0,30% ao mês, só até dois meses atrás (ainda não saiu).
    let mut ipca = Series::new();
    let mut month = YearMonth::of(start).shift(-1);
    while month < YearMonth::of(today).shift(-2) {
        ipca.push((
            Date::from_calendar_date(month.year, Month::try_from(month.month).unwrap(), 1).unwrap(),
            dec!(0.30),
        ));
        month = month.shift(1);
    }
    history
        .indexes
        .lock()
        .unwrap()
        .insert(IndexCode::Ipca, ipca);

    let long = person(db, dir, "longa@prumo.test");
    let id = add_asset(
        &long,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    backdate(&long, &id, start).await;

    // Só três meses de série: nada a comparar.
    let short = person(db, dir, "curta@prumo.test");
    let id = add_asset(
        &short,
        json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 10 }),
    )
    .await;
    let short_start = today - Duration::days(90);
    backdate(&short, &id, short_start).await;
    history.closes.lock().unwrap().insert(
        "HGLG11.SA".into(),
        daily(short_start, today, |n| {
            dec!(140) + Decimal::from(n) / dec!(100)
        }),
    );

    let job = Performance::new(db.store.clone(), history);
    let report = job.run_daily(now).await.unwrap();
    assert!(report.ran);
    assert_eq!(report.failed, 0);

    let body = analysis(&long).await;
    // Contra o CDI e o Ibovespa a carteira perde, nas duas janelas.
    for id in [
        "beat_cdi_12m",
        "beat_cdi_5y",
        "beat_ibov_12m",
        "beat_ibov_5y",
    ] {
        assert_eq!(status(&body, id), "warning", "{id}");
    }
    // Contra o IPCA ela ganha, e a janela termina onde o IPCA acabou.
    for id in ["beat_ipca_12m", "beat_ipca_5y"] {
        assert_eq!(status(&body, id), "ok", "{id}");
    }
    let ipca = rule(&body, "beat_ipca_12m");
    assert_eq!(ipca["detail"]["benchmark"], "ipca");
    assert_eq!(ipca["detail"]["horizon"], "12m");
    assert!(ipca["detail"]["to"].as_str().unwrap() < today.to_string().as_str());
    // 1,0003^365 − 1 = 11,57% de carteira; 1,000455^365 − 1 = 18,05% de CDI.
    let cdi = rule(&body, "beat_cdi_12m");
    assert!(
        (cdi["current"].as_f64().unwrap() - 11.57).abs() < 0.2,
        "{cdi}"
    );
    assert!(
        (cdi["limit"].as_f64().unwrap() - 18.05).abs() < 0.3,
        "{cdi}"
    );
    // O mesmo número do Desempenho (12 meses, mesma janela).
    let perf = get(
        &long.router,
        "/api/v1/performance?period=12m&benchmarks=cdi",
    )
    .await;
    let performance_return = perf.body["return_percent"].as_f64().unwrap();
    assert!((cdi["current"].as_f64().unwrap() - performance_return).abs() < 0.02);
    // Anos civis fechados, todos no azul; volatilidade baixa de uma série lisa.
    let years = rule(&body, "min_positive_years");
    assert_eq!(years["status"], "ok");
    assert_eq!(years["current"], 100.0);
    assert!(years["detail"]["total"].as_u64().unwrap() >= 5);
    assert_eq!(status(&body, "max_volatility"), "ok");
    assert!(rule(&body, "max_volatility")["current"].as_f64().unwrap() < 5.0);
    // A pontuação de Rentabilidade conta só o que mediu: 4 avisos e 5 ok
    // (as seis comparações, anos e volatilidade; a perda desde a compra não
    // tem custo conhecido e fica sem dado).
    let score = &dimension(&body, "returns")["score"];
    assert_eq!(score["measured"], 8);
    assert_eq!(score["ok"], 4);
    assert_eq!(score["no_data"], 1);
    assert_eq!(score["percent"], 50.0);

    let body = analysis(&short).await;
    for id in [
        "beat_cdi_12m",
        "beat_cdi_5y",
        "beat_ibov_12m",
        "beat_ibov_5y",
        "beat_ipca_12m",
        "beat_ipca_5y",
        "min_positive_years",
        "max_volatility",
    ] {
        assert_eq!(status(&body, id), "no_data", "{id}");
        assert_eq!(rule(&body, id)["current"], Value::Null);
    }
    assert_eq!(dimension(&body, "returns")["score"]["measured"], 0);
    assert_eq!(dimension(&body, "returns")["score"]["percent"], Value::Null);
    // Dar "sem dado" não é aviso: a pessoa da série curta não tem aviso de rentabilidade.
    let warnings = dimension(&body, "returns")["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "warning")
        .count();
    assert_eq!(warnings, 0);
}

/// Produção já tem regras salvas antes da 0021: monta o banco até a 0019,
/// grava a configuração de uma pessoa, aplica a 0021 por cima e confere que
/// nada mudou, que as regras novas passam a ser aceitas e que a Análise lê a
/// configuração antiga.
#[tokio::test]
async fn migration_0021_keeps_the_saved_rules_and_accepts_the_new_ones() {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");
    migrator.run_to(19, &pool).await.unwrap();

    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, 'migra-analise@prumo.test')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    for (rule, enabled, threshold) in [
        ("max_asset", false, Some("12.5")),
        ("top_five", true, Some("70")),
        ("empty_class", false, None),
        ("income_every_month", true, Some("9")),
    ] {
        sqlx::query(
            "insert into health_rules (user_id, rule, enabled, threshold)
             values ($1, $2, $3, $4::numeric)",
        )
        .bind(user)
        .bind(rule)
        .bind(enabled)
        .bind(threshold)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Antes da 0021 a regra nova não cabe.
    let before = sqlx::query(
        "insert into health_rules (user_id, rule, enabled, threshold)
         values ($1, 'min_abroad', true, 10)",
    )
    .bind(user)
    .execute(&pool)
    .await;
    assert!(before.is_err());

    // A 0021 aceita as regras de dividendos e rentabilidade, mas ainda não as
    // de Qualidade (0022).
    migrator.run_to(21, &pool).await.unwrap();
    let quality_early = sqlx::query(
        "insert into health_rules (user_id, rule, enabled, threshold)
         values ($1, 'quality_roe', true, 70)",
    )
    .bind(user)
    .execute(&pool)
    .await;
    assert!(quality_early.is_err());

    migrator.run(&pool).await.unwrap();

    let rows: Vec<(String, bool, Option<String>)> = sqlx::query_as(
        "select rule, enabled, threshold::text from health_rules where user_id = $1 order by rule",
    )
    .bind(user)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            ("empty_class".to_owned(), false, None),
            (
                "income_every_month".to_owned(),
                true,
                Some("9.00".to_owned())
            ),
            ("max_asset".to_owned(), false, Some("12.50".to_owned())),
            ("top_five".to_owned(), true, Some("70.00".to_owned())),
        ]
    );
    // Cada regra nova passa a ser aceita; uma inventada, não.
    for rule in [
        "min_portfolio_assets",
        "min_classes",
        "min_sectors",
        "min_abroad",
        "min_low_risk",
        "min_yield",
        "min_paying_share",
        "income_growth",
        "brick_reit",
        "beat_cdi_12m",
        "beat_cdi_5y",
        "beat_ibov_12m",
        "beat_ibov_5y",
        "beat_ipca_12m",
        "beat_ipca_5y",
        "min_positive_years",
        "max_volatility",
        "max_loss",
        "quality_roe",
        "quality_profit",
        "quality_leverage",
        "quality_margin",
        "quality_cash",
        "quality_recovery",
        "quality_fii_pvp",
        "quality_fii_vacancy",
    ] {
        sqlx::query(
            "insert into health_rules (user_id, rule, enabled, threshold) values ($1, $2, true, 5)",
        )
        .bind(user)
        .bind(rule)
        .execute(&pool)
        .await
        .unwrap();
    }
    let bogus = sqlx::query(
        "insert into health_rules (user_id, rule, enabled, threshold) values ($1, 'nota_final', true, 1)",
    )
    .bind(user)
    .execute(&pool)
    .await;
    assert!(bogus.is_err());
    // A tabela do Bazin existe e recusa rendimento fora da faixa.
    sqlx::query("insert into analysis_settings (user_id, bazin_yield) values ($1, 7.5)")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let too_high = sqlx::query("update analysis_settings set bazin_yield = 101 where user_id = $1")
        .bind(user)
        .execute(&pool)
        .await;
    assert!(too_high.is_err());
    sqlx::query("delete from health_rules where user_id = $1 and rule not in ('max_asset','top_five','empty_class','income_every_month')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // O código lê a configuração antiga pela Análise e pela Saúde.
    let store = prumo::store::Store::from_pool(pool.clone());
    let config = store.analysis_config(user).await.unwrap();
    use prumo::domain::analysis::RuleId;
    assert!(!config.get(RuleId::MaxAsset).enabled);
    assert_eq!(config.get(RuleId::MaxAsset).threshold, Some(dec!(12.5)));
    assert_eq!(
        config.get(RuleId::IncomeEveryMonth).threshold,
        Some(dec!(9))
    );
    // Quem nunca mexeu nas regras novas as vê ligadas, com o limite de fábrica.
    assert!(config.get(RuleId::MinAbroad).enabled);
    assert_eq!(config.get(RuleId::MinAbroad).threshold, Some(dec!(10)));
    assert_eq!(store.bazin_yield(user).await.unwrap(), dec!(7.5));
    let health = store.health_config(user).await.unwrap();
    assert!(!health.get(prumo::domain::health::RuleId::MaxAsset).enabled);
}
