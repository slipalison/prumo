//! Proventos, renda e meta de renda, rota por rota, contra um Postgres de
//! verdade e um provedor de dividendos falso (a estimativa nunca vai à rede).
//!
//! Um contêiner para o arquivo inteiro; cada cenário tem a própria pessoa, e
//! como tudo é filtrado pelo dono, um cenário não enxerga o dado do outro.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::auth::DevAuthenticator;
use prumo::dividends::{DividendError, DividendEvent, DividendHistory, DividendProvider};
use prumo::domain::income::{today_in_brazil, IncomeKind, IncomeSource};
use prumo::http::{AppState, BuildInfo};
use prumo::live::LiveQuotes;
use prumo::service::QuoteService;
use prumo::store::{NewIncomeEvent, Store, StoreError};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

/// O histórico por símbolo do provedor (`KNRI11.SA`) e quantas vezes o
/// provedor foi chamado.
#[derive(Default)]
struct FakeDividends {
    histories: Mutex<HashMap<String, DividendHistory>>,
    calls: Mutex<HashMap<String, usize>>,
    down: Mutex<bool>,
}

impl FakeDividends {
    fn set(&self, symbol: &str, currency: &str, events: Vec<DividendEvent>) {
        self.histories.lock().unwrap().insert(
            symbol.to_owned(),
            DividendHistory {
                currency: currency.to_owned(),
                events,
            },
        );
    }

    fn calls(&self, symbol: &str) -> usize {
        self.calls.lock().unwrap().get(symbol).copied().unwrap_or(0)
    }
}

#[async_trait]
impl DividendProvider for FakeDividends {
    async fn history(&self, provider_symbol: &str) -> Result<DividendHistory, DividendError> {
        *self
            .calls
            .lock()
            .unwrap()
            .entry(provider_symbol.to_owned())
            .or_default() += 1;
        if *self.down.lock().unwrap() {
            return Err(DividendError::Unavailable("falso fora do ar".into()));
        }
        self.histories
            .lock()
            .unwrap()
            .get(provider_symbol)
            .cloned()
            .ok_or(DividendError::NotFound)
    }
}

struct Env {
    store: Store,
    pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

async fn environment() -> Env {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let store = Store::connect(&url).await.unwrap();
    store.migrate().await.unwrap();
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    Env {
        store,
        pool,
        _container: container,
    }
}

struct Person {
    router: axum::Router,
    dividends: Arc<FakeDividends>,
}

fn person(env: &Env, email: &str, dir: &tempfile_dir::Dir) -> Person {
    let dividends = Arc::new(FakeDividends::default());
    let live = LiveQuotes::new(env.store.clone());
    let quotes = Arc::new(QuoteService::new(
        env.store.clone(),
        FakeQuotes::with_market(),
        live,
    ));
    let state = AppState::new(
        env.store.clone(),
        Arc::new(DevAuthenticator::new(email)),
        quotes,
        BuildInfo {
            version: "9.9.9".into(),
            commit: "abc1234".into(),
        },
        None,
    )
    .with_dividend_provider(dividends.clone());
    Person {
        router: prumo::http::public_router(state, &dir.path()),
        dividends,
    }
}

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

fn iso(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap()
}

fn days_from_today(days: i64) -> String {
    iso(today().saturating_add(Duration::days(days)))
}

fn monthly_dividends(amount: Decimal, months: i64) -> Vec<DividendEvent> {
    (1..=months)
        .map(|i| DividendEvent {
            ex_date: today().saturating_sub(Duration::days(30 * i)),
            amount,
        })
        .collect()
}

async fn add_asset(p: &Person, body: Value) -> String {
    let reply = post(&p.router, "/api/v1/assets", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

fn money(value: &Value) -> Decimal {
    Decimal::try_from(
        value
            .as_f64()
            .unwrap_or_else(|| panic!("nao e numero: {value}")),
    )
    .unwrap()
    .round_dp(2)
}

#[tokio::test]
async fn income_contract() {
    let env = environment().await;
    let dir = tempfile_dir::Dir::new();

    manual_events_are_calculated_filtered_and_owned(&env, &dir).await;
    summary_sums_the_ledger(&env, &dir).await;
    estimate_comes_from_the_provider_and_is_cached(&env, &dir).await;
    estimate_in_other_currency_and_unavailable_provider(&env, &dir).await;
    income_goal_crud_and_progress(&env, &dir).await;
    plan_shows_the_income_it_adds(&env, &dir).await;
    cash_suggestion_counts_since_the_last_contribution(&env, &dir).await;
    store_keeps_one_event_per_external_ref(&env).await;
}

async fn manual_events_are_calculated_filtered_and_owned(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-manual@prumo.test", dir);
    let other = person(env, "renda-outra@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    let itsa = add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "ITSA4", "quantity": 200 }),
    )
    .await;

    // Rendimento de FII: sem imposto retido; quantidade padrão = a da carteira.
    let reit = post(
        &me.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": days_from_today(-20), "amount_per_unit": 1.1 }),
    )
    .await;
    assert_eq!(reit.status, StatusCode::CREATED, "{}", reit.text);
    assert_eq!(reit.body["gross"], 11.0);
    assert_eq!(reit.body["withholding"], 0.0);
    assert_eq!(reit.body["net"], 11.0);
    assert_eq!(reit.body["quantity"], 10.0);
    assert_eq!(reit.body["status"], "received");
    assert_eq!(reit.body["source"], "manual");
    assert_eq!(reit.body["symbol"], "KNRI11");

    // JCP: 15% retido na fonte.
    let jcp = post(
        &me.router,
        "/api/v1/income",
        json!({
            "asset_id": itsa, "kind": "jcp", "ex_date": days_from_today(-40),
            "pay_date": days_from_today(10), "amount_per_unit": 0.5, "quantity": 200
        }),
    )
    .await;
    assert_eq!(jcp.status, StatusCode::CREATED, "{}", jcp.text);
    assert_eq!(jcp.body["gross"], 100.0);
    assert_eq!(jcp.body["withholding"], 15.0);
    assert_eq!(jcp.body["net"], 85.0);
    assert_eq!(jcp.body["status"], "upcoming");
    let jcp_id = jcp.body["id"].as_str().unwrap().to_owned();

    // Pagamento hoje já é recebido.
    let same_day = post(
        &me.router,
        "/api/v1/income",
        json!({ "asset_id": itsa, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 0.1, "quantity": 200 }),
    )
    .await;
    assert_eq!(same_day.body["status"], "received");

    // Filtros.
    let all = get(&me.router, "/api/v1/income").await;
    assert_eq!(all.status, StatusCode::OK);
    assert_eq!(all.body.as_array().unwrap().len(), 3);
    let dates: Vec<&str> = all
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["pay_date"].as_str().unwrap())
        .collect();
    let mut sorted = dates.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(dates, sorted, "do pagamento mais novo para o mais velho");

    let upcoming = get(&me.router, "/api/v1/income?status=upcoming").await;
    assert_eq!(upcoming.body.as_array().unwrap().len(), 1);
    assert_eq!(upcoming.body[0]["id"], jcp_id.as_str());
    let received = get(&me.router, "/api/v1/income?status=received").await;
    assert_eq!(received.body.as_array().unwrap().len(), 2);
    let by_asset = get(&me.router, &format!("/api/v1/income?asset={knri}")).await;
    assert_eq!(by_asset.body.as_array().unwrap().len(), 1);
    let window = get(
        &me.router,
        &format!(
            "/api/v1/income?from={}&to={}",
            days_from_today(-30),
            days_from_today(-10)
        ),
    )
    .await;
    assert_eq!(window.body.as_array().unwrap().len(), 1);
    assert_eq!(window.body[0]["kind"], "reit_income");
    let limited = get(&me.router, "/api/v1/income?limit=1").await;
    assert_eq!(limited.body.as_array().unwrap().len(), 1);
    for bad in [
        "/api/v1/income?limit=0",
        "/api/v1/income?limit=5000",
        "/api/v1/income?status=talvez",
        "/api/v1/income?from=ontem",
        "/api/v1/income?asset=nao-e-uuid",
        "/api/v1/income?from=2026-12-01&to=2026-01-01",
    ] {
        let reply = get(&me.router, bad).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    // Edição: o valor por cota e o tipo mudam o líquido (a retenção segue o tipo).
    let patched = patch(
        &me.router,
        &format!("/api/v1/income/{jcp_id}"),
        json!({ "amount_per_unit": 1, "kind": "dividend", "ex_date": null }),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text);
    assert_eq!(patched.body["gross"], 200.0);
    assert_eq!(patched.body["withholding"], 0.0);
    assert_eq!(patched.body["net"], 200.0);
    assert_eq!(patched.body["ex_date"], Value::Null);
    let explicit = patch(
        &me.router,
        &format!("/api/v1/income/{jcp_id}"),
        json!({ "withholding": 30 }),
    )
    .await;
    assert_eq!(explicit.body["withholding"], 30.0);
    assert_eq!(explicit.body["net"], 170.0);

    // Validação.
    for (body, why) in [
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 1, "withholding": 99 }),
            "retencao acima do bruto",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 0 }),
            "valor zero",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 1, "quantity": -2 }),
            "quantidade negativa",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "ex_date": days_from_today(5), "pay_date": days_from_today(0), "amount_per_unit": 1 }),
            "ex depois do pagamento",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": "1999-12-31", "amount_per_unit": 1 }),
            "ano fora da faixa",
        ),
        (
            json!({ "asset_id": knri, "kind": "juros", "pay_date": days_from_today(0), "amount_per_unit": 1 }),
            "tipo desconhecido",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": "25/11/2026", "amount_per_unit": 1 }),
            "data fora do formato",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 0.0001, "quantity": 1 }),
            "bruto abaixo de um centavo",
        ),
        (
            json!({ "asset_id": knri, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 1, "extra": 1 }),
            "campo desconhecido",
        ),
    ] {
        let reply = post(&me.router, "/api/v1/income", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
        assert_eq!(reply.body["code"], "validation_failed", "{why}");
    }
    let bad_patch = patch(
        &me.router,
        &format!("/api/v1/income/{jcp_id}"),
        json!({ "withholding": 5000 }),
    )
    .await;
    assert_eq!(bad_patch.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Quem é de outra pessoa não existe.
    let stranger = post(
        &other.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "dividend", "pay_date": days_from_today(0), "amount_per_unit": 1 }),
    )
    .await;
    assert_eq!(stranger.status, StatusCode::NOT_FOUND);
    assert_eq!(get(&other.router, "/api/v1/income").await.body, json!([]));
    assert_eq!(
        patch(
            &other.router,
            &format!("/api/v1/income/{jcp_id}"),
            json!({ "amount_per_unit": 9 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&other.router, &format!("/api/v1/income/{jcp_id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(
            &me.router,
            &format!("/api/v1/income/{}", Uuid::new_v4()),
            json!({ "amount_per_unit": 9 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(&me.router, "/api/v1/income/nao-e-uuid", json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // O que veio de extrato não se edita (mas se apaga).
    let user_id = env
        .store
        .ensure_user("renda-manual@prumo.test")
        .await
        .unwrap();
    let imported = env
        .store
        .insert_income_event(
            user_id,
            &NewIncomeEvent {
                asset_id: None,
                symbol: Some("XPML11".into()),
                display_name: "XP Malls".into(),
                kind: IncomeKind::ReitIncome,
                ex_date: None,
                pay_date: today(),
                amount_per_unit: dec!(0.9),
                quantity: dec!(10),
                amounts: prumo::domain::income::amounts(
                    IncomeKind::ReitIncome,
                    dec!(0.9),
                    dec!(10),
                    None,
                )
                .unwrap(),
                source: IncomeSource::B3Import,
                external_ref: Some("b3:2026-11-xpml11".into()),
            },
        )
        .await
        .unwrap();
    let blocked = patch(
        &me.router,
        &format!("/api/v1/income/{}", imported.id),
        json!({ "amount_per_unit": 5 }),
    )
    .await;
    assert_eq!(blocked.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        delete(&me.router, &format!("/api/v1/income/{}", imported.id))
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    // Apagar.
    assert_eq!(
        delete(&me.router, &format!("/api/v1/income/{jcp_id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        delete(&me.router, &format!("/api/v1/income/{jcp_id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&me.router, "/api/v1/income")
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Apagar o ativo não apaga o histórico de renda dele: o evento guarda o nome.
    assert_eq!(
        delete(&me.router, &format!("/api/v1/assets/{knri}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let after = get(&me.router, "/api/v1/income").await;
    let orphan = find(&after.body, "symbol", "KNRI11");
    assert_eq!(orphan["asset_id"], Value::Null);
    assert_eq!(orphan["net"], 11.0);
}

async fn summary_sums_the_ledger(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-resumo@prumo.test", dir);
    let empty = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text);
    assert_eq!(empty.body["total_12m"], 0.0);
    assert_eq!(empty.body["monthly_average"], 0.0);
    assert_eq!(empty.body["months"].as_array().unwrap().len(), 12);
    assert_eq!(empty.body["by_payer"], json!([]));
    assert_eq!(empty.body["years"], json!([]));
    assert_eq!(empty.body["upcoming"], json!([]));
    assert_eq!(empty.body["goal"], Value::Null);
    assert_eq!(empty.body["estimate"]["monthly"], 0.0);
    assert_eq!(empty.body["today"], iso(today()).as_str());

    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    let itsa = add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "ITSA4", "quantity": 100 }),
    )
    .await;
    for (asset, kind, days, per_unit, quantity) in [
        (&knri, "reit_income", -15, 1.0, 10),
        (&knri, "reit_income", -45, 1.0, 10),
        (&itsa, "jcp", -100, 0.4, 100),
        (&itsa, "dividend", -420, 9.0, 100),
        (&knri, "reit_income", 20, 1.0, 10),
    ] {
        let reply = post(
            &me.router,
            "/api/v1/income",
            json!({ "asset_id": asset, "kind": kind, "pay_date": days_from_today(days), "amount_per_unit": per_unit, "quantity": quantity }),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    }
    let summary = get(&me.router, "/api/v1/income/summary").await;
    let body = &summary.body;
    // Recebido nos 12 meses: 10 + 10 + (40 − 6 de IR) = 54; o de 420 dias e o
    // futuro ficam fora do total.
    assert_eq!(money(&body["total_12m"]), dec!(54.00));
    assert_eq!(money(&body["monthly_average"]), dec!(4.50));
    assert_eq!(money(&body["upcoming_total"]), dec!(10.00));
    let months = body["months"].as_array().unwrap();
    assert_eq!(months.len(), 12);
    let received: Decimal = months.iter().map(|m| money(&m["received"])).sum();
    let upcoming_in_window: Decimal = months.iter().map(|m| money(&m["upcoming"])).sum();
    assert_eq!(received, dec!(54.00));
    assert!(upcoming_in_window <= dec!(10.00));
    let payers = body["by_payer"].as_array().unwrap();
    // O maior pagador vem primeiro.
    assert_eq!(payers[0]["label"], "ITSA4");
    assert_eq!(money(&payers[0]["total"]), dec!(34.00));
    assert_eq!(payers[1]["label"], "KNRI11");
    assert_eq!(money(&payers[1]["total"]), dec!(20.00));
    assert_eq!(money(&payers[0]["percent"]), dec!(62.96));
    let years = body["years"].as_array().unwrap();
    let all_time: Decimal = years.iter().map(|y| money(&y["total"])).sum();
    assert_eq!(all_time, dec!(54.00) + dec!(10.00) + dec!(900.00));
    for year in years {
        let cells: Decimal = year["months"].as_array().unwrap().iter().map(money).sum();
        assert_eq!(cells, money(&year["total"]));
        assert_eq!(year["months"].as_array().unwrap().len(), 12);
    }
    let next = body["upcoming"].as_array().unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0]["status"], "upcoming");
    assert_eq!(next[0]["symbol"], "KNRI11");
}

async fn estimate_comes_from_the_provider_and_is_cached(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-estimativa@prumo.test", dir);
    // Um real por mês nos últimos 12 meses (mais dois meses fora da janela).
    me.dividends
        .set("KNRI11.SA", "BRL", monthly_dividends(dec!(1), 14));
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    // Cripto e renda fixa por valor não pagam provento: o provedor nem é chamado.
    add_asset(
        &me,
        json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 0.5 }),
    )
    .await;
    add_asset(
        &me,
        json!({ "asset_class": "fixed_income", "display_name": "CDB", "amount": 1000 }),
    )
    .await;

    let summary = get(&me.router, "/api/v1/income/summary").await;
    let estimate = &summary.body["estimate"];
    // 12 reais por cota em 12 meses × 10 cotas / 12 = 10 reais por mês.
    assert_eq!(estimate["monthly"], 10.0, "{estimate}");
    let assets = estimate["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["asset_id"], knri.as_str());
    assert_eq!(assets[0]["per_unit_12m"], 12.0);
    assert_eq!(assets[0]["monthly"], 10.0);
    assert_eq!(estimate["missing"], json!([]));
    assert!(estimate["updated_at"].is_string());
    // O recebido não se mistura com a estimativa.
    assert_eq!(summary.body["total_12m"], 0.0);
    assert_eq!(me.dividends.calls("KNRI11.SA"), 1);
    assert_eq!(me.dividends.calls("BTC-USD"), 0);

    // Segunda tela: sai do cache, sem bater no provedor.
    let again = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(again.body["estimate"]["monthly"], 10.0);
    assert_eq!(me.dividends.calls("KNRI11.SA"), 1);

    // O cache é do símbolo, compartilhado: outra pessoa com o mesmo FII não chama o provedor.
    let neighbour = person(env, "renda-vizinha@prumo.test", dir);
    add_asset(
        &neighbour,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 3 }),
    )
    .await;
    let theirs = get(&neighbour.router, "/api/v1/income/summary").await;
    assert_eq!(theirs.body["estimate"]["monthly"], 3.0);
    assert_eq!(neighbour.dividends.calls("KNRI11.SA"), 0);

    // Cache vencido: busca de novo e troca o histórico pelo novo.
    sqlx::query("update dividend_fetches set fetched_at = now() - interval '2 days' where symbol = 'KNRI11.SA'")
        .execute(&env.pool)
        .await
        .unwrap();
    me.dividends
        .set("KNRI11.SA", "BRL", monthly_dividends(dec!(2), 12));
    let refreshed = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(refreshed.body["estimate"]["monthly"], 20.0);
    assert_eq!(me.dividends.calls("KNRI11.SA"), 2);

    // Provedor fora do ar com o cache vencido: vale o que está guardado.
    sqlx::query("update dividend_fetches set fetched_at = now() - interval '2 days' where symbol = 'KNRI11.SA'")
        .execute(&env.pool)
        .await
        .unwrap();
    *me.dividends.down.lock().unwrap() = true;
    let stale = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(stale.status, StatusCode::OK);
    assert_eq!(stale.body["estimate"]["monthly"], 20.0);
    // E descansa: a próxima tela não repete a tentativa.
    let calls = me.dividends.calls("KNRI11.SA");
    get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(me.dividends.calls("KNRI11.SA"), calls);
}

async fn estimate_in_other_currency_and_unavailable_provider(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-dolar@prumo.test", dir);
    // Quatro trimestres de US$ 0,25: US$ 1,00 por cota em 12 meses, a R$ 5,50.
    me.dividends.set(
        "AAPL",
        "USD",
        (0..4)
            .map(|i| DividendEvent {
                ex_date: today().saturating_sub(Duration::days(80 * i + 5)),
                amount: dec!(0.25),
            })
            .collect(),
    );
    let apple = add_asset(
        &me,
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 12 }),
    )
    .await;
    // Sem histórico no provedor: fica de fora e é dito.
    add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 100 }),
    )
    .await;

    let summary = get(&me.router, "/api/v1/income/summary").await;
    let estimate = &summary.body["estimate"];
    let apple_row = find(&estimate["assets"], "asset_id", &apple);
    assert_eq!(apple_row["per_unit_12m"], 5.5);
    // 5,50 × 12 cotas / 12 meses.
    assert_eq!(apple_row["monthly"], 5.5);
    assert_eq!(estimate["missing"], json!(["WEGE3"]));
    assert_eq!(estimate["assets"].as_array().unwrap().len(), 1);

    // Sem nenhum histórico de nenhum ativo, o resumo continua de pé.
    let nobody = person(env, "renda-sem-historico@prumo.test", dir);
    *nobody.dividends.down.lock().unwrap() = true;
    add_asset(
        &nobody,
        json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 4 }),
    )
    .await;
    let reply = get(&nobody.router, "/api/v1/income/summary").await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["estimate"]["monthly"], 0.0);
    assert_eq!(reply.body["estimate"]["missing"], json!(["HGLG11"]));
}

async fn income_goal_crud_and_progress(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-meta@prumo.test", dir);
    let other = person(env, "renda-meta-outra@prumo.test", dir);
    assert_eq!(
        get(&me.router, "/api/v1/income/goal").await.body,
        Value::Null
    );
    assert_eq!(
        delete(&me.router, "/api/v1/income/goal").await.status,
        StatusCode::NOT_FOUND
    );

    for bad in [
        json!({ "target": 0 }),
        json!({ "target": -3 }),
        json!({}),
        json!({ "target": "x" }),
    ] {
        let reply = put(&me.router, "/api/v1/income/goal", bad).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Sem nada recebido, o progresso vem da estimativa e a meta não se cumpre.
    me.dividends
        .set("KNRI11.SA", "BRL", monthly_dividends(dec!(1), 12));
    add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    let created = put(&me.router, "/api/v1/income/goal", json!({ "target": 20 })).await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text);
    assert_eq!(created.body["target"], 20.0);
    assert_eq!(created.body["basis"], "estimate");
    assert_eq!(created.body["current_monthly"], 10.0);
    assert_eq!(created.body["percent"], 50.0);
    assert_eq!(created.body["reached"], false);
    assert_eq!(created.body["achieved_at"], Value::Null);
    let id = created.body["id"].clone();

    // Trocar o valor mantém uma meta só.
    let changed = put(&me.router, "/api/v1/income/goal", json!({ "target": 25 })).await;
    assert_eq!(changed.body["id"], id);
    assert_eq!(changed.body["target"], 25.0);
    assert_eq!(
        get(&me.router, "/api/v1/income/goal").await.body["target"],
        25.0
    );
    assert_eq!(
        get(&other.router, "/api/v1/income/goal").await.body,
        Value::Null
    );

    // Recebido de verdade vence a estimativa, e alcançar a meta registra a data.
    let asset = find(
        &get(&me.router, "/api/v1/portfolio").await.body["assets"],
        "symbol",
        "KNRI11",
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    post(
        &me.router,
        "/api/v1/income",
        json!({ "asset_id": asset, "kind": "reit_income", "pay_date": days_from_today(-3), "amount_per_unit": 36, "quantity": 10 }),
    )
    .await;
    // 360 reais em 12 meses = 30 por mês, acima da meta de 25.
    let summary = get(&me.router, "/api/v1/income/summary").await;
    let goal = &summary.body["goal"];
    assert_eq!(goal["basis"], "received");
    assert_eq!(goal["current_monthly"], 30.0);
    assert_eq!(goal["reached"], true);
    assert_eq!(goal["percent"], 120.0);
    let achieved = goal["achieved_at"].as_str().unwrap().to_owned();
    // A data fica: a segunda leitura não a troca.
    let again = get(&me.router, "/api/v1/income/goal").await;
    assert_eq!(again.body["achieved_at"], achieved.as_str());
    // Meta nova, conquista nova.
    let raised = put(&me.router, "/api/v1/income/goal", json!({ "target": 500 })).await;
    assert_eq!(raised.body["reached"], false);
    assert_eq!(raised.body["achieved_at"], Value::Null);

    assert_eq!(
        delete(&me.router, "/api/v1/income/goal").await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get(&me.router, "/api/v1/income/goal").await.body,
        Value::Null
    );
}

async fn plan_shows_the_income_it_adds(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-plano@prumo.test", dir);
    me.dividends
        .set("KNRI11.SA", "BRL", monthly_dividends(dec!(1), 12));
    put(
        &me.router,
        "/api/v1/goals",
        json!({ "targets": { "br_reits": 100 } }),
    )
    .await;
    add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "score_override": 8 }),
    )
    .await;

    let plan = post(&me.router, "/api/v1/plans", json!({ "amount": 1420 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    let item = &plan.body["items"][0];
    let bought = item["quantity"].as_f64().unwrap();
    assert!(bought >= 1.0);
    // R$ 1 por cota por mês.
    assert_eq!(item["estimated_monthly_income"], bought);
    let income = &plan.body["income"];
    assert_eq!(income["before_monthly"], 10.0);
    assert_eq!(income["added_monthly"], bought);
    assert_eq!(income["after_monthly"], 10.0 + bought);

    // Sem histórico de ninguém: o plano segue sem o bloco de renda.
    let bare = person(env, "renda-plano-vazio@prumo.test", dir);
    put(
        &bare.router,
        "/api/v1/goals",
        json!({ "targets": { "br_reits": 100 } }),
    )
    .await;
    add_asset(
        &bare,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "score_override": 8 }),
    )
    .await;
    *bare.dividends.down.lock().unwrap() = true;
    sqlx::query("delete from dividend_fetches")
        .execute(&env.pool)
        .await
        .unwrap();
    sqlx::query("delete from dividend_history")
        .execute(&env.pool)
        .await
        .unwrap();
    let plain = post(&bare.router, "/api/v1/plans", json!({ "amount": 1420 })).await;
    assert_eq!(plain.status, StatusCode::OK);
    assert_eq!(plain.body["income"], Value::Null);
    assert_eq!(
        plain.body["items"][0]["estimated_monthly_income"],
        Value::Null
    );
}

async fn cash_suggestion_counts_since_the_last_contribution(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "renda-caixa@prumo.test", dir);
    let before = get(&me.router, "/api/v1/income/cash-suggestion").await;
    assert_eq!(before.status, StatusCode::OK);
    assert_eq!(
        before.body,
        json!({ "amount": 0.0, "count": 0, "since": null })
    );

    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    // Provento antes de qualquer aporte: sem aporte registrado não há de onde contar.
    post(
        &me.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": days_from_today(-60), "amount_per_unit": 1 }),
    )
    .await;
    let contribution = post(
        &me.router,
        "/api/v1/contributions",
        json!({ "amount": 1000, "items": [{ "asset_id": knri, "quantity": 5, "unit_price": 142 }] }),
    )
    .await;
    assert_eq!(
        contribution.status,
        StatusCode::CREATED,
        "{}",
        contribution.text
    );
    let none = get(&me.router, "/api/v1/income/cash-suggestion").await;
    assert_eq!(none.body["amount"], 0.0);
    assert_eq!(none.body["since"], iso(today()).as_str());

    // O aporte foi há 20 dias: o que caiu depois entra; o do próprio dia, o
    // anterior e o ainda a receber, não.
    let user = env
        .store
        .ensure_user("renda-caixa@prumo.test")
        .await
        .unwrap();
    sqlx::query(
        "update contributions set created_at = now() - interval '20 days' where user_id = $1",
    )
    .bind(user)
    .execute(&env.pool)
    .await
    .unwrap();
    for (days, per_unit) in [
        (-25, 7.0),
        (-20, 3.0),
        (-10, 2.0),
        (-1, 1.5),
        (0, 0.5),
        (5, 9.0),
    ] {
        post(
            &me.router,
            "/api/v1/income",
            json!({ "asset_id": knri, "kind": "reit_income", "pay_date": days_from_today(days), "amount_per_unit": per_unit, "quantity": 10 }),
        )
        .await;
    }
    let cash = get(&me.router, "/api/v1/income/cash-suggestion").await;
    // (−10) 20 + (−1) 15 + (hoje) 5.
    assert_eq!(cash.body["amount"], 40.0, "{}", cash.text);
    assert_eq!(cash.body["count"], 3);
    assert_eq!(cash.body["since"], days_from_today(-20).as_str());

    // Outra pessoa não soma o que não é dela.
    let other = person(env, "renda-caixa-outra@prumo.test", dir);
    assert_eq!(
        get(&other.router, "/api/v1/income/cash-suggestion")
            .await
            .body["amount"],
        0.0
    );
}

async fn store_keeps_one_event_per_external_ref(env: &Env) {
    let user = env
        .store
        .ensure_user("renda-store@prumo.test")
        .await
        .unwrap();
    let new = |reference: Option<&str>| NewIncomeEvent {
        asset_id: None,
        symbol: Some("KNRI11".into()),
        display_name: "Kinea".into(),
        kind: IncomeKind::ReitIncome,
        ex_date: None,
        pay_date: today(),
        amount_per_unit: dec!(1),
        quantity: dec!(10),
        amounts: prumo::domain::income::amounts(IncomeKind::ReitIncome, dec!(1), dec!(10), None)
            .unwrap(),
        source: IncomeSource::B3Import,
        external_ref: reference.map(str::to_owned),
    };
    env.store
        .insert_income_event(user, &new(Some("b3:1")))
        .await
        .unwrap();
    assert!(matches!(
        env.store
            .insert_income_event(user, &new(Some("b3:1")))
            .await,
        Err(StoreError::Duplicate)
    ));
    // Sem referência não há o que repetir; outra pessoa pode ter a mesma.
    env.store
        .insert_income_event(user, &new(None))
        .await
        .unwrap();
    env.store
        .insert_income_event(user, &new(None))
        .await
        .unwrap();
    let neighbour = env
        .store
        .ensure_user("renda-store-outra@prumo.test")
        .await
        .unwrap();
    env.store
        .insert_income_event(neighbour, &new(Some("b3:1")))
        .await
        .unwrap();
}
