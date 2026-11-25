//! As regras de renda da saúde (`income_every_month` e `max_payer`) contra um
//! Postgres de verdade: proventos fictícios viram avisos, pouca história vira
//! "sem o que medir", cada pessoa vê só os seus e as regras salvas antes da
//! migração 0016 continuam como estavam.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use prumo::domain::income::{today_in_brazil, YearMonth};
use serde_json::{json, Value};
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use time::macros::format_description;
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

/// O dia 1 do mês que ficou `back` meses para trás (0 = o mês de hoje): sempre
/// já passou, então o provento é "recebido".
fn month_day(back: i32) -> (YearMonth, String) {
    let month = YearMonth::of(today_in_brazil(OffsetDateTime::now_utc())).shift(-back);
    let date =
        Date::from_calendar_date(month.year, Month::try_from(month.month).unwrap(), 1).unwrap();
    let text = date
        .format(format_description!("[year]-[month]-[day]"))
        .unwrap();
    (month, text)
}

async fn add_asset(app: &TestApp, class: &str, symbol: &str) -> String {
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": class, "symbol": symbol, "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    created.body["id"].as_str().unwrap().to_owned()
}

/// Um provento de `per_unit` por cota (10 cotas), pago no mês de `back` atrás.
async fn pay(app: &TestApp, asset: &str, back: i32, per_unit: f64) {
    let (_, pay_date) = month_day(back);
    let created = post(
        &app.router,
        "/api/v1/income",
        json!({
            "asset_id": asset, "kind": "reit_income",
            "pay_date": pay_date, "amount_per_unit": per_unit, "quantity": 10
        }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
}

fn warnings_of<'a>(health: &'a Value, rule: &str) -> Vec<&'a Value> {
    health["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["rule"] == rule)
        .collect()
}

fn rule_status<'a>(health: &'a Value, rule: &str) -> &'a str {
    find(&health["rules"], "rule", rule)["status"]
        .as_str()
        .unwrap()
}

fn person(db: &Database, dir: &tempfile_dir::Dir, email: &str) -> TestApp {
    app(&db.store, email, FakeQuotes::with_market(), &dir.path())
}

#[tokio::test]
async fn income_rules_contract() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();

    without_history_the_rules_have_nothing_to_measure(&db, &dir).await;
    gaps_name_the_empty_months_and_who_pays_in_them(&db, &dir).await;
    a_big_payer_is_flagged_and_the_limit_is_editable(&db, &dir).await;
    each_person_sees_only_their_own_income(&db, &dir).await;
    new_rules_are_configurable_and_validated(&db, &dir).await;
}

async fn without_history_the_rules_have_nothing_to_measure(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "sem-dado@prumo.test");
    let knri = add_asset(&app, "br_reits", "KNRI11").await;
    let hglg = add_asset(&app, "br_reits", "HGLG11").await;
    // Sem nenhum provento.
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(
        rule_status(&health.body, "income_every_month"),
        "not_applicable"
    );
    assert_eq!(rule_status(&health.body, "max_payer"), "not_applicable");
    assert!(warnings_of(&health.body, "income_every_month").is_empty());

    // Dois meses de história (o de hoje e o anterior): ainda pouco.
    pay(&app, &knri, 1, 9.0).await;
    pay(&app, &hglg, 0, 1.0).await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(
        rule_status(&health.body, "income_every_month"),
        "not_applicable"
    );
    assert_eq!(rule_status(&health.body, "max_payer"), "not_applicable");
    assert!(warnings_of(&health.body, "max_payer").is_empty());

    // Três meses: já dá para medir.
    pay(&app, &knri, 2, 9.0).await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(rule_status(&health.body, "income_every_month"), "warning");
    assert_eq!(rule_status(&health.body, "max_payer"), "warning");
}

async fn gaps_name_the_empty_months_and_who_pays_in_them(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "lacunas@prumo.test");
    let knri = add_asset(&app, "br_reits", "KNRI11").await;
    let hglg = add_asset(&app, "br_reits", "HGLG11").await;
    // KNRI paga em meses alternados (1, 3, ..., 11 atrás) e antes da janela.
    for back in [1, 3, 5, 7, 9, 11, 13, 15] {
        pay(&app, &knri, back, 1.0).await;
    }
    // HGLG pagou, há mais de um ano, nos mesmos meses do calendário de 2 e 4 atrás.
    pay(&app, &hglg, 14, 1.0).await;
    pay(&app, &hglg, 16, 1.0).await;

    let health = get(&app.router, "/api/v1/health").await;
    let found = warnings_of(&health.body, "income_every_month");
    assert_eq!(found.len(), 1, "{}", health.text);
    let warning = found[0];
    assert_eq!(warning["unit"], "count");
    assert_eq!(warning["current"], 6.0);
    assert_eq!(warning["limit"], 12.0);
    assert_eq!(warning["needed_contribution"], Value::Null);
    let subject = &warning["subject"];
    assert_eq!(subject["kind"], "income_months");
    let empty: Vec<String> = [12, 10, 8, 6, 4, 2]
        .into_iter()
        .map(|back| month_day(back).0.label())
        .collect();
    assert_eq!(subject["empty_months"], json!(empty));
    // Só HGLG já pagou em mês hoje vazio; KNRI paga nos meses que já cobre.
    let payers = subject["payers"].as_array().unwrap();
    assert_eq!(payers.len(), 1);
    assert_eq!(payers[0]["label"], "HGLG11");
    assert_eq!(payers[0]["asset_id"], hglg);
    assert_eq!(payers[0]["asset_class"], "br_reits");
    let mut months = vec![month_day(2).0.month, month_day(4).0.month];
    months.sort_unstable();
    assert_eq!(payers[0]["months"], json!(months));
    assert_eq!(rule_status(&health.body, "income_every_month"), "warning");

    // Com limite 6 a pessoa está dentro.
    let tuned = put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "income_every_month": { "threshold": 6 } } }),
    )
    .await;
    assert_eq!(tuned.status, StatusCode::OK);
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(rule_status(&health.body, "income_every_month"), "ok");
    assert!(warnings_of(&health.body, "income_every_month").is_empty());
    assert!(!knri.is_empty());
}

async fn a_big_payer_is_flagged_and_the_limit_is_editable(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "pagador@prumo.test");
    let knri = add_asset(&app, "br_reits", "KNRI11").await;
    let hglg = add_asset(&app, "br_reits", "HGLG11").await;
    for back in 1..=4 {
        pay(&app, &knri, back, 2.0).await; // 20 por mês, 80 no total
    }
    pay(&app, &hglg, 2, 2.0).await; // 20
                                    // Fora da janela de 12 meses: não entra na conta.
    pay(&app, &hglg, 20, 90.0).await;

    let health = get(&app.router, "/api/v1/health").await;
    let found = warnings_of(&health.body, "max_payer");
    assert_eq!(found.len(), 1, "{}", health.text);
    let warning = found[0];
    assert_eq!(warning["unit"], "percent");
    assert_eq!(warning["current"], 80.0);
    assert_eq!(warning["limit"], 25.0);
    assert_eq!(warning["subject"]["kind"], "payer");
    assert_eq!(warning["subject"]["label"], "KNRI11");
    assert_eq!(warning["subject"]["asset_id"], knri);
    assert_eq!(warning["subject"]["received"], 80.0);
    assert_eq!(warning["subject"]["total"], 100.0);

    // No limite exato está tudo bem.
    put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "max_payer": { "threshold": 80 } } }),
    )
    .await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(rule_status(&health.body, "max_payer"), "ok");
    // Desligada, a regra some dos avisos.
    put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "max_payer": { "enabled": false, "threshold": 10 } } }),
    )
    .await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(rule_status(&health.body, "max_payer"), "off");
    assert!(warnings_of(&health.body, "max_payer").is_empty());
}

async fn each_person_sees_only_their_own_income(db: &Database, dir: &tempfile_dir::Dir) {
    let mine = person(db, dir, "dona@prumo.test");
    let theirs = person(db, dir, "outra@prumo.test");
    let knri = add_asset(&mine, "br_reits", "KNRI11").await;
    let hglg = add_asset(&mine, "br_reits", "HGLG11").await;
    for back in 1..=4 {
        pay(&mine, &knri, back, 2.0).await;
    }
    pay(&mine, &hglg, 1, 2.0).await;
    let health = get(&mine.router, "/api/v1/health").await;
    assert_eq!(warnings_of(&health.body, "max_payer").len(), 1);

    add_asset(&theirs, "br_reits", "KNRI11").await;
    let health = get(&theirs.router, "/api/v1/health").await;
    assert_eq!(rule_status(&health.body, "max_payer"), "not_applicable");
    assert_eq!(
        rule_status(&health.body, "income_every_month"),
        "not_applicable"
    );
    assert!(warnings_of(&health.body, "max_payer").is_empty());
}

async fn new_rules_are_configurable_and_validated(db: &Database, dir: &tempfile_dir::Dir) {
    let app = person(db, dir, "config@prumo.test");
    let rules = get(&app.router, "/api/v1/health/rules").await;
    let every = find(&rules.body["rules"], "rule", "income_every_month");
    assert_eq!(every["enabled"], true);
    assert_eq!(every["unit"], "count");
    assert_eq!(every["threshold"], 12.0);
    assert_eq!(every["default_threshold"], 12.0);
    let payer = find(&rules.body["rules"], "rule", "max_payer");
    assert_eq!(payer["unit"], "percent");
    assert_eq!(payer["threshold"], 25.0);
    assert_eq!(rules.body["rules"].as_array().unwrap().len(), 7);

    for body in [
        json!({ "rules": { "income_every_month": { "threshold": 0 } } }),
        json!({ "rules": { "income_every_month": { "threshold": 13 } } }),
        json!({ "rules": { "income_every_month": { "threshold": 5.5 } } }),
        json!({ "rules": { "max_payer": { "threshold": 0 } } }),
        json!({ "rules": { "max_payer": { "threshold": 100.5 } } }),
    ] {
        let reply = put(&app.router, "/api/v1/health/rules", body.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let saved = put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": {
            "income_every_month": { "threshold": 10 },
            "max_payer": { "threshold": 33.5, "enabled": false },
        }}),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK);
    let again = get(&app.router, "/api/v1/health/rules").await;
    assert_eq!(
        find(&again.body["rules"], "rule", "income_every_month")["threshold"],
        10.0
    );
    let payer = find(&again.body["rules"], "rule", "max_payer");
    assert_eq!(payer["threshold"], 33.5);
    assert_eq!(payer["enabled"], false);
}

/// Produção já tem regras salvas antes das duas novas. Monta o banco até a
/// 0010, grava a configuração de uma pessoa, aplica a 0016 por cima e confere
/// que nada mudou e que as regras novas passam a ser aceitas.
#[tokio::test]
async fn migration_keeps_saved_rules_and_accepts_the_new_ones() {
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
    migrator.run_to(10, &pool).await.unwrap();

    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, 'migra-saude@prumo.test')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    for (rule, enabled, threshold) in [
        ("max_asset", false, Some("12.5")),
        ("top_five", true, Some("70")),
        ("empty_class", false, None),
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
    // Antes da 0016 a regra nova não cabe.
    let before = sqlx::query(
        "insert into health_rules (user_id, rule, enabled, threshold)
         values ($1, 'max_payer', true, 25)",
    )
    .bind(user)
    .execute(&pool)
    .await;
    assert!(before.is_err());

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
            ("max_asset".to_owned(), false, Some("12.50".to_owned())),
            ("top_five".to_owned(), true, Some("70.00".to_owned())),
        ]
    );
    for rule in ["income_every_month", "max_payer"] {
        sqlx::query(
            "insert into health_rules (user_id, rule, enabled, threshold)
             values ($1, $2, true, 12)",
        )
        .bind(user)
        .bind(rule)
        .execute(&pool)
        .await
        .unwrap();
    }
    let bogus = sqlx::query(
        "insert into health_rules (user_id, rule, enabled, threshold)
         values ($1, 'nota_final', true, 1)",
    )
    .bind(user)
    .execute(&pool)
    .await;
    assert!(bogus.is_err());
}
