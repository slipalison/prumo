//! Caixa, yield on cost, quantidade na data com e lucro total contra um
//! Postgres de verdade: a API rota por rota, o dono, a corrida de dois aportes
//! pelo mesmo caixa e a migração 0012 por cima de dado de antes.
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
use prumo::domain::income::today_in_brazil;
use prumo::http::dto::LivePricesDto;
use prumo::http::{AppState, BuildInfo};
use prumo::income::IncomeService;
use prumo::live::LiveQuotes;
use prumo::service::{self, QuoteService};
use prumo::store::Store;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Default)]
struct FakeDividends {
    histories: Mutex<HashMap<String, DividendHistory>>,
}

impl FakeDividends {
    fn set(&self, symbol: &str, events: Vec<DividendEvent>) {
        self.histories.lock().unwrap().insert(
            symbol.to_owned(),
            DividendHistory {
                currency: "BRL".to_owned(),
                events,
            },
        );
    }
}

#[async_trait]
impl DividendProvider for FakeDividends {
    async fn history(&self, provider_symbol: &str) -> Result<DividendHistory, DividendError> {
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
    email: String,
    router: axum::Router,
    dividends: Arc<FakeDividends>,
    income: Arc<IncomeService>,
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
        email: email.to_owned(),
        income: state.income.clone(),
        router: prumo::http::public_router(state, &dir.path()),
        dividends,
    }
}

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

fn day(offset: i64) -> String {
    today()
        .saturating_add(Duration::days(offset))
        .format(format_description!("[year]-[month]-[day]"))
        .unwrap()
}

fn day_date(offset: i64) -> Date {
    today().saturating_add(Duration::days(offset))
}

/// Recua o início do caixa da pessoa: uma pessoa nova só conta o que vem de hoje em diante.
async fn start_cash_at(env: &Env, p: &Person, days_ago: i32) {
    sqlx::query(
        "update users set cash_since = (now() at time zone 'America/Sao_Paulo')::date - $2::int
          where email = $1",
    )
    .bind(&p.email)
    .bind(days_ago)
    .execute(&env.pool)
    .await
    .unwrap();
}

async fn add_asset(p: &Person, body: Value) -> String {
    let reply = post(&p.router, "/api/v1/assets", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

async fn ledger(
    p: &Person,
    asset: &str,
    kind: &str,
    offset: i64,
    quantity: f64,
    price: Option<f64>,
) {
    let mut body =
        json!({ "asset_id": asset, "kind": kind, "trade_date": day(offset), "quantity": quantity });
    if let Some(price) = price {
        body["unit_price"] = json!(price);
    }
    let reply = post(&p.router, "/api/v1/transactions", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
}

async fn income(p: &Person, body: Value) -> Value {
    let reply = post(&p.router, "/api/v1/income", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body
}

async fn cash(p: &Person) -> Value {
    let reply = get(&p.router, "/api/v1/cash").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply.body
}

async fn contribute(p: &Person, asset: &str, amount: f64, cash_used: Option<f64>) -> Reply {
    let mut body = json!({
        "amount": amount,
        "items": [{ "asset_id": asset, "quantity": 1, "unit_price": 142 }]
    });
    if let Some(used) = cash_used {
        body["cash_used"] = json!(used);
    }
    post(&p.router, "/api/v1/contributions", body).await
}

#[tokio::test]
async fn cash_income_yield_and_profit() {
    let env = environment().await;
    let dir = tempfile_dir::Dir::new();

    cash_comes_from_income_and_sales_and_goes_out_with_contributions(&env, &dir).await;
    cash_never_goes_negative_and_outside_money_does_not_touch_it(&env, &dir).await;
    two_contributions_cannot_spend_the_same_cash(&env, &dir).await;
    manual_income_uses_the_book_quantity_on_the_record_date(&env, &dir).await;
    yield_on_cost_and_the_estimate_on_the_record_date(&env, &dir).await;
    total_profit_is_capital_gain_plus_income_and_says_when_partial(&env, &dir).await;
}

async fn cash_comes_from_income_and_sales_and_goes_out_with_contributions(
    env: &Env,
    dir: &tempfile_dir::Dir,
) {
    let me = person(env, "cx-fluxo@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 100 }),
    )
    .await;

    let empty = cash(&me).await;
    assert_eq!(empty["balance"], 0.0);
    assert_eq!(empty["since"], day(0).as_str());
    assert_eq!(empty["recent"], json!([]));

    start_cash_at(env, &me, 30).await;
    // Entra: 20 (FII) e 8,50 líquido de JCP (15% retido). Fica de fora: o que
    // pagou antes do início do caixa e o que ainda vai pagar.
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(-10), "amount_per_unit": 2 }),
    )
    .await;
    income(
        &me,
        json!({ "asset_id": knri, "kind": "jcp", "pay_date": day(-5), "amount_per_unit": 1 }),
    )
    .await;
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(-40), "amount_per_unit": 5 }),
    )
    .await;
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(5), "amount_per_unit": 9 }),
    )
    .await;
    // Vende 2 cotas a 150 com 1,50 de taxa: entra 298,50 líquido.
    let sale = post(
        &me.router,
        "/api/v1/transactions",
        json!({ "asset_id": knri, "kind": "sell", "trade_date": day(0), "quantity": 2, "unit_price": 150, "fees": 1.5 }),
    )
    .await;
    assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.text);

    let state = cash(&me).await;
    assert_eq!(state["balance"], 327.0, "{state}");
    assert_eq!(state["income"], json!({ "amount": 28.5, "count": 2 }));
    assert_eq!(state["sales"], json!({ "amount": 298.5, "count": 1 }));
    assert_eq!(state["used"], 0.0);
    assert_eq!(state["adjustment"], 0.0);
    assert_eq!(state["since"], day(-30).as_str());
    // O detalhe diz de onde veio, do mais novo ao mais velho.
    let recent = state["recent"].as_array().unwrap();
    let kinds: Vec<&str> = recent.iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["sale", "income", "income"]);
    assert_eq!(recent[0]["label"], "KNRI11");
    assert_eq!(recent[0]["amount"], 298.5);
    assert_eq!(recent[1]["date"], day(-5).as_str());

    // Usa parte do caixa: o saldo baixa.
    let partial = contribute(&me, &knri, 500.0, Some(100.0)).await;
    assert_eq!(partial.status, StatusCode::CREATED, "{}", partial.text);
    assert_eq!(partial.body["cash_used"], 100.0);
    let after = cash(&me).await;
    assert_eq!(after["balance"], 227.0);
    assert_eq!(after["used"], 100.0);
    // O aporte aparece no detalhe como saída, depois do que entrou no dia.
    assert_eq!(after["recent"][0]["kind"], "sale");
    assert!(after["recent"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["kind"] == "contribution" && m["amount"] == 100.0));

    // Usa o resto: zera.
    let rest = contribute(&me, &knri, 227.0, Some(227.0)).await;
    assert_eq!(rest.status, StatusCode::CREATED, "{}", rest.text);
    assert_eq!(cash(&me).await["balance"], 0.0);

    // O histórico de aportes guarda quanto de cada um veio do caixa.
    let history = get(&me.router, "/api/v1/contributions").await;
    let used: Vec<f64> = history
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["cash_used"].as_f64().unwrap())
        .collect();
    assert_eq!(used.len(), 2);
    assert!(used.contains(&100.0) && used.contains(&227.0));

    // Outra pessoa não vê o que não é dela.
    let other = person(env, "cx-fluxo-outra@prumo.test", dir);
    let theirs = cash(&other).await;
    assert_eq!(theirs["balance"], 0.0);
    assert_eq!(theirs["income"]["count"], 0);
    assert_eq!(theirs["sales"]["count"], 0);
}

async fn cash_never_goes_negative_and_outside_money_does_not_touch_it(
    env: &Env,
    dir: &tempfile_dir::Dir,
) {
    let me = person(env, "cx-negativo@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 100 }),
    )
    .await;
    start_cash_at(env, &me, 20).await;
    let paid = income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(-3), "amount_per_unit": 10 }),
    )
    .await;
    assert_eq!(cash(&me).await["balance"], 100.0);

    // Mais que o saldo, mais que o aporte e negativo: nada disso grava.
    for (amount, used, why) in [
        (1000.0, 100.01, "passa do saldo"),
        (50.0, 60.0, "passa do aporte"),
        (1000.0, -1.0, "negativo"),
    ] {
        let refused = contribute(&me, &knri, amount, Some(used)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{why}");
    }
    let refused = contribute(&me, &knri, 1000.0, Some(100.01)).await;
    assert!(
        refused.text.contains("O caixa tem R$ 100,00"),
        "{}",
        refused.text
    );
    assert_eq!(cash(&me).await["balance"], 100.0);
    assert_eq!(
        get(&me.router, "/api/v1/contributions")
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        0,
        "o aporte recusado não deixou rastro"
    );

    // Dinheiro de fora: aporte maior que o caixa, e aporte sem tocar nele.
    let outside = contribute(&me, &knri, 5000.0, None).await;
    assert_eq!(outside.status, StatusCode::CREATED, "{}", outside.text);
    assert_eq!(outside.body["cash_used"], 0.0);
    assert_eq!(
        cash(&me).await["balance"],
        100.0,
        "dinheiro de fora não baixa o caixa"
    );
    let mixed = contribute(&me, &knri, 3000.0, Some(100.0)).await;
    assert_eq!(mixed.status, StatusCode::CREATED, "{}", mixed.text);
    assert_eq!(cash(&me).await["balance"], 0.0);

    // O aporte foi há dois dias (o provento, há três).
    sqlx::query(
        "update contributions set created_at = now() - interval '2 days'
          where user_id = (select id from users where email = $1)",
    )
    .bind(&me.email)
    .execute(&env.pool)
    .await
    .unwrap();
    // Apagar o provento que sustentava o aporte não deixa o caixa negativo, e o
    // provento seguinte vale inteiro.
    let deleted = delete(
        &me.router,
        &format!("/api/v1/income/{}", paid["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let hole = cash(&me).await;
    assert_eq!(hole["balance"], 0.0);
    assert_eq!(hole["adjustment"], 100.0);
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(0), "amount_per_unit": 1.5 }),
    )
    .await;
    // 1,50 × 12 cotas (as 10 do saldo inicial e as 2 dos aportes).
    let again = cash(&me).await;
    assert_eq!(again["balance"], 18.0);
    assert_eq!(again["adjustment"], 100.0);
}

async fn two_contributions_cannot_spend_the_same_cash(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "cx-corrida@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 100 }),
    )
    .await;
    start_cash_at(env, &me, 5).await;
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(-1), "amount_per_unit": 10 }),
    )
    .await;
    let (a, b) = tokio::join!(
        contribute(&me, &knri, 100.0, Some(100.0)),
        contribute(&me, &knri, 100.0, Some(100.0))
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::UNPROCESSABLE_ENTITY],
        "{} / {}",
        a.text,
        b.text
    );
    assert_eq!(cash(&me).await["balance"], 0.0);
    assert_eq!(cash(&me).await["used"], 100.0);
}

async fn manual_income_uses_the_book_quantity_on_the_record_date(
    env: &Env,
    dir: &tempfile_dir::Dir,
) {
    let me = person(env, "cx-data-com@prumo.test", dir);
    // 10 cotas de saldo inicial (cadastro de hoje) + 5 compradas há 40 dias + 7 há 10.
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    ledger(&me, &knri, "buy", -40, 5.0, Some(100.0)).await;
    ledger(&me, &knri, "buy", -10, 7.0, Some(100.0)).await;
    let qty = |event: &Value| event["quantity"].as_f64().unwrap();
    let new_income = |ex: Option<i64>, pay: i64, quantity: Option<f64>| {
        let mut body = json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(pay), "amount_per_unit": 1 });
        if let Some(ex) = ex {
            body["ex_date"] = json!(day(ex));
        }
        if let Some(q) = quantity {
            body["quantity"] = json!(q);
        }
        body
    };
    // O saldo inicial vale desde sempre; as compras, na data delas.
    assert_eq!(
        qty(&income(&me, new_income(Some(-50), -45, None)).await),
        10.0
    );
    assert_eq!(
        qty(&income(&me, new_income(Some(-20), -15, None)).await),
        15.0
    );
    assert_eq!(
        qty(&income(&me, new_income(Some(-5), -1, None)).await),
        22.0
    );
    // Sem data ex, vale a posição na data do pagamento.
    assert_eq!(qty(&income(&me, new_income(None, -12, None)).await), 15.0);
    // Comprou no dia da data ex: sem direito. Comprou na data com: com direito.
    assert_eq!(
        qty(&income(&me, new_income(Some(-10), -2, None)).await),
        15.0
    );
    assert_eq!(
        qty(&income(&me, new_income(Some(-9), -2, None)).await),
        22.0
    );
    // A quantidade dita pela pessoa sempre vence.
    assert_eq!(
        qty(&income(&me, new_income(Some(-5), -1, Some(3.0))).await),
        3.0
    );

    // A tela consulta a mesma regra antes de lançar.
    let on = get(
        &me.router,
        &format!(
            "/api/v1/income/entitlement?asset={knri}&ex_date={}&pay_date={}",
            day(-20),
            day(-15)
        ),
    )
    .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text);
    assert_eq!(on.body["quantity"], 15.0);
    assert_eq!(on.body["record_date"], day(-21).as_str());
    let stranger = get(
        &person(env, "cx-data-com-outra@prumo.test", dir).router,
        &format!("/api/v1/income/entitlement?asset={knri}"),
    )
    .await;
    assert_eq!(stranger.status, StatusCode::NOT_FOUND);

    // Desdobramento antes da data com: já está na quantidade.
    let wege = add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 100 }),
    )
    .await;
    ledger(&me, &wege, "split", -30, 100.0, None).await;
    let before_split = income(
        &me,
        json!({ "asset_id": wege, "kind": "dividend", "ex_date": day(-40), "pay_date": day(-35), "amount_per_unit": 0.5 }),
    )
    .await;
    assert_eq!(qty(&before_split), 100.0);
    let after_split = income(
        &me,
        json!({ "asset_id": wege, "kind": "dividend", "ex_date": day(-20), "pay_date": day(-15), "amount_per_unit": 0.5 }),
    )
    .await;
    assert_eq!(qty(&after_split), 200.0);
    assert_eq!(after_split["gross"], 100.0);

    // Quem não tinha cota na data com recebe um aviso claro, e pode informar a quantidade.
    let hglg = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "HGLG11" }),
    )
    .await;
    ledger(&me, &hglg, "buy", 0, 4.0, Some(140.0)).await;
    let none = post(
        &me.router,
        "/api/v1/income",
        json!({ "asset_id": hglg, "kind": "reit_income", "pay_date": day(-20), "amount_per_unit": 1 }),
    )
    .await;
    assert_eq!(
        none.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        none.text
    );
    assert!(none.text.contains("data com"), "{}", none.text);
    let told = income(
        &me,
        json!({ "asset_id": hglg, "kind": "reit_income", "pay_date": day(-20), "amount_per_unit": 1, "quantity": 4 }),
    )
    .await;
    assert_eq!(qty(&told), 4.0);

    // Renda fixa por valor segue sendo "1 × valor".
    let cdb = add_asset(
        &me,
        json!({ "asset_class": "fixed_income", "display_name": "CDB teste", "amount": 1000, "invested": 1000 }),
    )
    .await;
    let bond = income(
        &me,
        json!({ "asset_id": cdb, "kind": "other", "pay_date": day(-3), "amount_per_unit": 12 }),
    )
    .await;
    assert_eq!(qty(&bond), 1.0);
}

async fn yield_on_cost_and_the_estimate_on_the_record_date(env: &Env, dir: &tempfile_dir::Dir) {
    let me = person(env, "cx-yoc@prumo.test", dir);
    // O histórico por cota entra antes do ativo (o cadastro já o busca em segundo plano).
    // O histórico por cota do provedor: R$ 1 por cota há 100, 60 e 20 dias. Na
    // data com delas o livro tinha 10, 20 e 20 cotas (não as 20 de hoje em todas).
    me.dividends.set(
        "KNRI11.SA",
        [-100, -60, -20]
            .into_iter()
            .map(|offset| DividendEvent {
                ex_date: day_date(offset),
                amount: Decimal::ONE,
            })
            .collect(),
    );
    // Custo: 10 × 10 (saldo inicial) + 10 × 12 (compra há 70 dias) = 220.
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 10 }),
    )
    .await;
    ledger(&me, &knri, "buy", -70, 10.0, Some(12.0)).await;
    // Saldo inicial sem preço médio: custo desconhecido.
    let wege = add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 10 }),
    )
    .await;

    // Sem proventos recebidos: o yield vem da estimativa, e diz que é estimativa.
    me.income
        .fill(&["KNRI11.SA".to_owned()], OffsetDateTime::now_utc())
        .await;
    let summary = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(summary.status, StatusCode::OK, "{}", summary.text);
    let asset = find(&summary.body["estimate"]["assets"], "symbol", "KNRI11");
    assert_eq!(asset["trailing_12m"], 50.0, "{asset}");
    // A renda mensal projetada segue na quantidade de hoje: 3 × 20 ÷ 12.
    assert_eq!(asset["monthly"], 5.0);
    let portfolio_yield = &summary.body["yield_on_cost"];
    assert_eq!(portfolio_yield["basis"], "estimate");
    assert_eq!(portfolio_yield["partial"], true);
    assert_eq!(portfolio_yield["unknown_cost"], json!(["WEGE3"]));
    // 50 ÷ 220.
    assert_eq!(portfolio_yield["percent"], 22.73);

    // O recebido vence a estimativa.
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(-15), "amount_per_unit": 1.1, "quantity": 20 }),
    )
    .await;
    income(
        &me,
        json!({ "asset_id": wege, "kind": "dividend", "pay_date": day(-15), "amount_per_unit": 3, "quantity": 10 }),
    )
    .await;
    let summary = get(&me.router, "/api/v1/income/summary").await;
    let payers = summary.body["by_payer"].as_array().unwrap();
    let knri_payer = payers.iter().find(|p| p["label"] == "KNRI11").unwrap();
    assert_eq!(knri_payer["yield_on_cost"]["income_12m"], 22.0);
    assert_eq!(knri_payer["yield_on_cost"]["basis"], "received");
    assert_eq!(knri_payer["yield_on_cost"]["cost"], 220.0);
    assert_eq!(knri_payer["yield_on_cost"]["percent"], 10.0);
    // Custo desconhecido: sem percentual, e a frase que manda informar o preço médio.
    let wege_payer = payers.iter().find(|p| p["label"] == "WEGE3").unwrap();
    assert_eq!(wege_payer["yield_on_cost"]["percent"], Value::Null);
    assert_eq!(wege_payer["yield_on_cost"]["gap"], "unknown_cost");
    assert_eq!(
        wege_payer["yield_on_cost"]["message"],
        "informe o preco medio"
    );
    assert_eq!(wege_payer["yield_on_cost"]["income_12m"], 30.0);
    // A carteira conta só quem tem custo: 22 ÷ 220, parcial.
    assert_eq!(summary.body["yield_on_cost"]["percent"], 10.0);
    assert_eq!(summary.body["yield_on_cost"]["basis"], "received");
    assert_eq!(summary.body["yield_on_cost"]["partial"], true);

    // A folha do ativo lê o mesmo número, por ativo.
    let report = get(&me.router, "/api/v1/income/yield-on-cost").await;
    assert_eq!(report.status, StatusCode::OK, "{}", report.text);
    let by_asset = report.body["assets"].as_array().unwrap();
    let knri_yield = by_asset
        .iter()
        .find(|a| a["asset_id"] == knri.as_str())
        .unwrap();
    assert_eq!(knri_yield["percent"], 10.0);
    let wege_yield = by_asset
        .iter()
        .find(|a| a["asset_id"] == wege.as_str())
        .unwrap();
    assert_eq!(wege_yield["message"], "informe o preco medio");
    assert_eq!(report.body["portfolio"]["percent"], 10.0);

    // Informado o preço médio (o preço do saldo inicial), o livro refaz o custo
    // (10 × 20 = 200) e o yield aparece: 30 ÷ 200.
    let book = get(&me.router, &format!("/api/v1/transactions?asset_id={wege}")).await;
    let opening = find(&book.body, "kind", "opening");
    let fixed = patch(
        &me.router,
        &format!("/api/v1/transactions/{}", opening["id"].as_str().unwrap()),
        json!({ "unit_price": 20 }),
    )
    .await;
    assert_eq!(fixed.status, StatusCode::OK, "{}", fixed.text);
    let report = get(&me.router, "/api/v1/income/yield-on-cost").await;
    let wege_yield = report.body["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["asset_id"] == wege.as_str())
        .unwrap();
    assert_eq!(wege_yield["percent"], 15.0);
    assert_eq!(report.body["portfolio"]["partial"], false);
    // (22 + 30) ÷ (220 + 200).
    assert_eq!(report.body["portfolio"]["percent"], 12.38);
}

async fn total_profit_is_capital_gain_plus_income_and_says_when_partial(
    env: &Env,
    dir: &tempfile_dir::Dir,
) {
    let me = person(env, "cx-lucro@prumo.test", dir);
    // Custo 10 × 100; a cotação do falso é 142.
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 100 }),
    )
    .await;
    // Vende 2 a 150: realizado 2 × (150 − 100) = 100; sobram 8 de custo 800.
    ledger(&me, &knri, "sell", 0, 2.0, Some(150.0)).await;
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(-3), "amount_per_unit": 5, "quantity": 10 }),
    )
    .await;
    // Ainda a receber: não é lucro.
    income(
        &me,
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day(4), "amount_per_unit": 9, "quantity": 8 }),
    )
    .await;

    let whole = get(&me.router, "/api/v1/portfolio").await;
    let profit = &whole.body["profit"];
    // Não realizado: 8 × 142 − 800 = 336.
    assert_eq!(profit["unrealized"], 336.0, "{profit}");
    assert_eq!(profit["realized"], 100.0);
    assert_eq!(profit["income_received"], 50.0);
    assert_eq!(profit["total"], 486.0);
    assert_eq!(profit["partial"], false);
    assert_eq!(profit["partial_assets"], json!([]));

    // Um ativo de custo desconhecido fica de fora, dito, e o total não inventa.
    add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 5 }),
    )
    .await;
    let partial = get(&me.router, "/api/v1/portfolio").await;
    let profit = &partial.body["profit"];
    assert_eq!(profit["total"], 486.0);
    assert_eq!(profit["partial"], true);
    assert_eq!(profit["partial_assets"][0]["label"], "WEGE3");
    assert_eq!(profit["partial_assets"][0]["reason"], "unknown_cost");

    // O stream carrega o mesmo bloco: o ganho não realizado muda com o preço.
    let user = env.store.ensure_user("cx-lucro@prumo.test").await.unwrap();
    let view = service::portfolio(&env.store, user).await.unwrap();
    let live = serde_json::to_value(LivePricesDto::from(&view)).unwrap();
    assert_eq!(live["profit"]["total"], 486.0);
    assert_eq!(live["profit"]["partial"], true);
}

// ---------------------------------------------------------------- migração --

/// A 0012 por cima de dado de antes: o caixa de quem já existe começa no dia
/// seguinte ao último aporte (ou hoje, sem aporte), e os aportes velhos não
/// consumiram caixa nenhum.
#[tokio::test]
async fn migration_starts_the_cash_after_the_last_contribution() {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@{host}:{port}/postgres"
    ))
    .await
    .unwrap();
    let migrator = sqlx::migrate!("./migrations");
    migrator.run_to(10, &pool).await.unwrap();

    let (with_contribution, without) = (Uuid::new_v4(), Uuid::new_v4());
    for (id, email) in [
        (with_contribution, "caixa-a@prumo.test"),
        (without, "caixa-b@prumo.test"),
    ] {
        sqlx::query("insert into users (id, email) values ($1, $2)")
            .bind(id)
            .bind(email)
            .execute(&pool)
            .await
            .unwrap();
    }
    // 23h30 de Brasília do dia 10 já é dia 11 em UTC: o dia do aporte é 10.
    for (at, amount) in [("2026-03-02T15:00:00Z", 500), ("2026-03-11T02:30:00Z", 800)] {
        sqlx::query(
            "insert into contributions (id, user_id, amount, created_at) values ($1, $2, $3, $4::timestamptz)",
        )
        .bind(Uuid::new_v4())
        .bind(with_contribution)
        .bind(amount)
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();
    }

    migrator.run(&pool).await.unwrap();

    let since = |user: Uuid| {
        let pool = pool.clone();
        async move {
            let (since,): (Date,) = sqlx::query_as("select cash_since from users where id = $1")
                .bind(user)
                .fetch_one(&pool)
                .await
                .unwrap();
            since
        }
    };
    assert_eq!(
        since(with_contribution).await,
        time::macros::date!(2026 - 03 - 11)
    );
    let (brasilia_today,): (Date,) =
        sqlx::query_as("select (now() at time zone 'America/Sao_Paulo')::date")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(since(without).await, brasilia_today);
    let (used,): (Decimal,) =
        sqlx::query_as("select coalesce(sum(cash_used), 0) from contributions")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(used, Decimal::ZERO);

    // O banco também não deixa o caixa usado passar do aporte nem ficar negativo.
    for used in [900, -1] {
        let refused = sqlx::query(
            "update contributions set cash_used = $1 where user_id = $2 and amount = 800",
        )
        .bind(used)
        .bind(with_contribution)
        .execute(&pool)
        .await;
        assert!(refused.is_err(), "cash_used = {used} deveria ser recusado");
    }
    drop(container);
}
