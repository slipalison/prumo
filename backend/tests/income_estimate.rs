//! A estimativa da carteira de hoje com as duas fontes, contra um Postgres de
//! verdade: os anúncios da B3 gravados no banco (como a rodada dos proventos
//! automáticos os deixa) e um histórico do Yahoo falso. Carteira, emissores e
//! valores FICTÍCIOS.
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
use prumo::http::{AppState, BuildInfo};
use prumo::income::IncomeService;
use prumo::live::LiveQuotes;
use prumo::service::QuoteService;
use prumo::store::Store;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use time::{Date, Duration, OffsetDateTime};

/// O histórico do Yahoo por símbolo (`ABCD4.SA`).
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
    router: axum::Router,
    dividends: Arc<FakeDividends>,
    income: Arc<IncomeService>,
}

fn person(env: &Env, email: &str, dir: &tempfile_dir::Dir) -> Person {
    let dividends = Arc::new(FakeDividends::default());
    let quotes = FakeQuotes::with_market();
    quotes.prices.lock().unwrap().insert(
        "ABCD4.SA".to_owned(),
        (dec!(20), "BRL", "Empresa Ficticia PN"),
    );
    let state = AppState::new(
        env.store.clone(),
        Arc::new(DevAuthenticator::new(email)),
        Arc::new(QuoteService::new(
            env.store.clone(),
            quotes,
            LiveQuotes::new(env.store.clone()),
        )),
        BuildInfo {
            version: "9.9.9".into(),
            commit: "abc1234".into(),
        },
        None,
    )
    .with_dividend_provider(dividends.clone());
    Person {
        income: state.income.clone(),
        router: prumo::http::public_router(state, &dir.path()),
        dividends,
    }
}

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

fn days_ago(days: i64) -> Date {
    today().saturating_sub(Duration::days(days))
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

/// Um anúncio em dinheiro da B3, como a rodada grava.
async fn announce(
    env: &Env,
    issuer: &str,
    isin: &str,
    label: &str,
    record: Date,
    rate: Decimal,
    pay: Option<Date>,
) {
    sqlx::query(
        "insert into b3_cash_announcements (issuer, isin, label, record_date, rate, pay_date)
         values ($1, $2, $3, $4, $5, $6)",
    )
    .bind(issuer)
    .bind(isin)
    .bind(label)
    .bind(record)
    .bind(rate)
    .bind(pay)
    .execute(&env.pool)
    .await
    .unwrap();
}

async fn fetched(env: &Env, issuer: &str, at: OffsetDateTime) {
    sqlx::query(
        "insert into b3_issuer_fetches (issuer, fetched_at) values ($1, $2)
         on conflict (issuer) do update set fetched_at = excluded.fetched_at",
    )
    .bind(issuer)
    .bind(at)
    .execute(&env.pool)
    .await
    .unwrap();
}

/// Um pagamento de `amount` por ano, há 100 dias, há 465, ... `years` vezes.
fn yearly(amount: Decimal, years: i64) -> Vec<DividendEvent> {
    (0..years)
        .map(|k| DividendEvent {
            ex_date: days_ago(100 + 365 * k),
            amount,
        })
        .collect()
}

#[tokio::test]
async fn estimate_uses_the_b3_net_of_jcp_and_falls_back_to_yahoo() {
    let env = environment().await;
    let dir = tempfile_dir::Dir::new();
    let me = person(&env, "estimativa-b3@prumo.test", &dir);

    // Ação fictícia: JCP de 1,02 por cota da época, depois bonificação de 2%,
    // e um dividendo de 1,00. O que é velho (400 dias) ou de outro ISIN não entra.
    sqlx::query(
        "insert into b3_listings (ticker, issuer, isin, fetched_at)
         values ('ABCD4', 'ABCD', 'BRABCDACNPR1', now())",
    )
    .execute(&env.pool)
    .await
    .unwrap();
    let pn = "BRABCDACNPR1";
    announce(
        &env,
        "ABCD",
        pn,
        "JRS CAP PROPRIO",
        days_ago(200),
        dec!(1.02),
        Some(days_ago(180)),
    )
    .await;
    announce(
        &env,
        "ABCD",
        pn,
        "DIVIDENDO",
        days_ago(100),
        dec!(1),
        Some(days_ago(90)),
    )
    .await;
    announce(
        &env,
        "ABCD",
        pn,
        "DIVIDENDO",
        days_ago(400),
        dec!(5),
        Some(days_ago(390)),
    )
    .await;
    announce(
        &env,
        "ABCD",
        "BRABCDACNOR3",
        "DIVIDENDO",
        days_ago(100),
        dec!(9),
        Some(days_ago(90)),
    )
    .await;
    sqlx::query(
        "insert into b3_stock_announcements (issuer, isin, label, record_date, factor)
         values ('ABCD', $1, 'BONIFICACAO', $2, 2)",
    )
    .bind(pn)
    .bind(days_ago(150))
    .execute(&env.pool)
    .await
    .unwrap();
    fetched(&env, "ABCD", OffsetDateTime::now_utc()).await;
    // O Yahoo da mesma ação: o histórico longo da média do habitual.
    me.dividends.set("ABCD4.SA", yearly(dec!(1), 7));

    // Fundo fictício, sem cadastro na B3: o ISIN sai do tipo de papel (CTF).
    for month in 0..12 {
        announce(
            &env,
            "FICT",
            "BRFICTCTF001",
            "RENDIMENTO",
            days_ago(15 + 30 * month),
            dec!(1),
            Some(days_ago(5 + 30 * month)),
        )
        .await;
    }
    fetched(&env, "FICT", OffsetDateTime::now_utc()).await;
    // O Yahoo do fundo tem um ano só (sem média do habitual) e valores
    // diferentes: com a B3 em dia, ele não conta.
    me.dividends.set(
        "FICT11.SA",
        (0..12)
            .map(|m| DividendEvent {
                ex_date: days_ago(14 + 30 * m),
                amount: dec!(0.9),
            })
            .collect(),
    );

    // Ação que a B3 não tem (emissor nunca buscado): reserva no Yahoo.
    me.dividends.set(
        "WXYZ3.SA",
        (0..4)
            .map(|q| DividendEvent {
                ex_date: days_ago(20 + 90 * q),
                amount: dec!(0.6),
            })
            .collect(),
    );

    put(
        &me.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 100 } }),
    )
    .await;
    let abcd = add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "ABCD4", "quantity": 120, "score_override": 8 }),
    )
    .await;
    let fict = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "FICT11", "quantity": 10 }),
    )
    .await;
    let wxyz = add_asset(
        &me,
        json!({ "asset_class": "br_stocks", "symbol": "WXYZ3", "quantity": 100 }),
    )
    .await;
    me.income.settle().await;

    let summary = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(summary.status, StatusCode::OK, "{}", summary.text);
    let estimate = &summary.body["estimate"];

    let stock = find(&estimate["assets"], "asset_id", &abcd);
    assert_eq!(stock["source"], "b3", "{stock}");
    // Por cota de hoje: 1,02 ÷ 1,02 = 1,00 de JCP (0,825 líquido) + 1,00.
    assert_eq!(money(&stock["per_unit_12m_gross"]), dec!(2.00));
    assert_eq!(stock["per_unit_12m"].as_f64(), Some(1.825));
    assert_eq!(money(&stock["monthly_gross"]), dec!(20.00));
    assert_eq!(money(&stock["monthly"]), dec!(18.25));
    // Média: (2,00 dos 12 meses + 4 anos de 1,00) ÷ 5 = 1,20 por ano.
    let usual = &stock["usual"];
    assert_eq!(usual["years"], 5, "{usual}");
    assert_eq!(money(&usual["monthly_gross"]), dec!(12.00));
    assert_eq!(money(&usual["monthly"]), dec!(10.95));
    assert_eq!(stock["above_usual"], true);
    // Com o livro (120 cotas na data com): 122,40 de JCP menos 17,5% + 120,00.
    assert_eq!(money(&stock["trailing_12m"]), dec!(220.98));

    let reit = find(&estimate["assets"], "asset_id", &fict);
    assert_eq!(reit["source"], "b3", "{reit}");
    assert_eq!(money(&reit["monthly"]), dec!(10.00));
    assert_eq!(money(&reit["monthly_gross"]), dec!(10.00));
    // Sem histórico longo: sem média e sem marca.
    assert_eq!(reit["usual"], Value::Null);
    assert_eq!(reit["above_usual"], false);

    let reserve = find(&estimate["assets"], "asset_id", &wxyz);
    assert_eq!(reserve["source"], "yahoo", "{reserve}");
    // 2,40 por cota × 100 ÷ 12; o Yahoo não separa o JCP: líquido = bruto.
    assert_eq!(money(&reserve["monthly"]), dec!(20.00));
    assert_eq!(money(&reserve["monthly_gross"]), dec!(20.00));

    assert_eq!(money(&estimate["monthly"]), dec!(48.25));
    assert_eq!(money(&estimate["monthly_gross"]), dec!(50.00));
    assert_eq!(money(&estimate["above_usual_monthly"]), dec!(18.25));
    assert_eq!(estimate["missing"], json!([]));

    // O yield on cost estimado usa o mesmo líquido da B3.
    let yields = get(&me.router, "/api/v1/income/yield-on-cost").await;
    let row = find(&yields.body["assets"], "asset_id", &abcd);
    assert_eq!(row["basis"], "estimate", "{row}");
    assert_eq!(money(&row["income_12m"]), dec!(220.98));

    // O plano do Aportar soma a renda líquida, a mesma da Renda.
    let plan = post(&me.router, "/api/v1/plans", json!({ "amount": 2000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    let item = find(&plan.body["items"], "asset_id", &abcd);
    let bought = Decimal::try_from(item["quantity"].as_f64().unwrap()).unwrap();
    assert!(bought > Decimal::ZERO);
    let expected = (dec!(1.825) * bought / dec!(12)).round_dp(2);
    assert_eq!(money(&item["estimated_monthly_income"]), expected);
    let income = &plan.body["income"];
    assert_eq!(money(&income["before_monthly"]), dec!(48.25));
    assert_eq!(money(&income["after_monthly"]), dec!(48.25) + expected);

    // A B3 fora do ar há dias, com o Yahoo do fundo em cache: vale o Yahoo.
    fetched(&env, "FICT", OffsetDateTime::now_utc() - Duration::days(5)).await;
    let stale = get(&me.router, "/api/v1/income/summary").await;
    let reit = find(&stale.body["estimate"]["assets"], "asset_id", &fict);
    assert_eq!(reit["source"], "yahoo", "{reit}");
    assert_eq!(money(&reit["monthly"]), dec!(9.00));

    // Sem nenhuma das duas fontes, o ativo fica de fora e é dito.
    me.dividends.histories.lock().unwrap().remove("FICT11.SA");
    sqlx::query("delete from dividend_fetches where symbol = 'FICT11.SA'")
        .execute(&env.pool)
        .await
        .unwrap();
    sqlx::query("delete from b3_issuer_fetches where issuer = 'FICT'")
        .execute(&env.pool)
        .await
        .unwrap();
    let gone = get(&me.router, "/api/v1/income/summary").await;
    assert_eq!(gone.body["estimate"]["missing"], json!(["FICT11"]));
}
