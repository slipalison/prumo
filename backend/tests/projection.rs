//! Projeção por cenários contra um Postgres de verdade: a busca diária do
//! Focus (uma Olinda falsa), a renda projetada por tipo de ativo e a migração
//! que troca a premissa nominal pela real. Carteira, fundos e números
//! FICTÍCIOS.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::auth::DevAuthenticator;
use prumo::dividends::{DividendError, DividendEvent, DividendHistory, DividendProvider};
use prumo::domain::income::{today_in_brazil, YearMonth};
use prumo::domain::performance::months_before;
use prumo::domain::projection::rates::{first_day, Expectation, Indicator, Period};
use prumo::domain::sector::SectorSource;
use prumo::focus::{FocusSync, OlindaFocus, MAX_AGE};
use prumo::history::IndexCode;
use prumo::http::{AppState, BuildInfo};
use prumo::income::IncomeService;
use prumo::live::LiveQuotes;
use prumo::sectors::SectorEntry;
use prumo::service::QuoteService;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use time::macros::date;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

const SERVICE: &str = "/olinda/servico/Expectativas/versao/v1/odata";

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

/// A consulta vai com `%20` no lugar do espaço, nunca com `+`: a Olinda
/// recusa `+`.
struct SpacesAsPercent20;

impl Match for SpacesAsPercent20 {
    fn matches(&self, request: &Request) -> bool {
        let query = request.url.query().unwrap_or("");
        query.contains("%20") && !query.contains('+')
    }
}

fn olinda_row(indicator: &str, survey: &str, reference: &str, median: f64) -> Value {
    json!({ "Indicador": indicator, "Data": survey, "DataReferencia": reference, "Mediana": median })
}

async fn olinda(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("{SERVICE}/ExpectativasMercadoAnuais")))
        .and(query_param("$format", "json"))
        .and(SpacesAsPercent20)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [
            // A pesquisa da véspera não fica.
            olinda_row("IPCA", "2026-11-18", "2027", 9.99),
            olinda_row("IPCA", "2026-11-19", "2026", 5.1),
            olinda_row("IPCA", "2026-11-19", "2027", 4.5),
            olinda_row("IPCA", "2026-11-19", "2028", 4.0),
            olinda_row("Selic", "2026-11-19", "2026", 14.0),
            olinda_row("Selic", "2026-11-19", "2027", 12.0),
        ] })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{SERVICE}/ExpectativaMercadoMensais")))
        .and(SpacesAsPercent20)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [
            olinda_row("IPCA", "2026-11-19", "12/2026", 0.4),
            olinda_row("IPCA", "2026-11-19", "01/2027", 0.3),
        ] })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn focus_is_fetched_once_a_day_and_kept_when_it_fails() {
    let db = database().await;
    let server = MockServer::start().await;
    olinda(&server).await;
    let http = reqwest::Client::new();
    let sync = |base: &str| {
        FocusSync::new(
            db.store.clone(),
            Arc::new(OlindaFocus::with_base(http.clone(), base)),
        )
    };
    let good = sync(&server.uri());
    let now = OffsetDateTime::now_utc();

    assert_eq!(good.sync_if_stale(now).await.unwrap(), Some(7));
    let rows = db.store.focus_expectations().await.unwrap();
    assert_eq!(rows.len(), 7);
    assert!(rows.iter().all(|r| r.survey_date == date!(2026 - 11 - 19)));
    let ipca_2027 = rows
        .iter()
        .find(|r| {
            r.indicator == Indicator::Ipca
                && r.period == Period::Annual
                && r.reference.year() == 2027
        })
        .unwrap();
    assert_eq!(ipca_2027.median, dec!(4.5));
    assert!(rows
        .iter()
        .any(|r| r.period == Period::Monthly && r.reference == date!(2027 - 01 - 01)));
    assert!(db.store.focus_fetched_at().await.unwrap().is_some());

    // No mesmo dia, não busca de novo.
    assert_eq!(
        good.sync_if_stale(now + Duration::hours(2)).await.unwrap(),
        None
    );

    // Vencido, duas réplicas ao mesmo tempo: uma busca, a outra pula.
    let later = now + MAX_AGE + Duration::minutes(1);
    let other = sync(&server.uri());
    let (a, b) = tokio::join!(good.sync_if_stale(later), other.sync_if_stale(later));
    let fetched: Vec<Option<usize>> = vec![a.unwrap(), b.unwrap()];
    assert_eq!(
        fetched.iter().filter(|f| f.is_some()).count(),
        1,
        "{fetched:?}"
    );

    // A fonte fora do ar: erro, e o Focus de ontem continua no banco.
    let broken_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&broken_server)
        .await;
    let broken = sync(&broken_server.uri());
    let much_later = later + MAX_AGE + Duration::minutes(1);
    assert!(broken.sync_if_stale(much_later).await.is_err());
    assert_eq!(db.store.focus_expectations().await.unwrap().len(), 7);
    // A vez voltou para a fila: a próxima conferência já tenta de novo.
    assert_eq!(good.sync_if_stale(much_later).await.unwrap(), Some(7));
}

/// O histórico do Yahoo por símbolo.
#[derive(Default)]
struct FakeDividends {
    histories: Mutex<HashMap<String, DividendHistory>>,
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

/// Um pagamento de `amount` por mês, no dia de hoje de cada um dos 12 meses
/// que terminam hoje: um por mês do calendário, todos na janela de 365 dias.
fn monthly(amount: Decimal) -> Vec<DividendEvent> {
    (0..12)
        .map(|i| DividendEvent {
            ex_date: months_before(today(), i),
            amount,
        })
        .collect()
}

fn money(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("nao e numero: {value}"))
}

#[tokio::test]
async fn income_projection_by_type_and_scenario() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let dividends = Arc::new(FakeDividends::default());
    {
        let mut histories = dividends.histories.lock().unwrap();
        for (symbol, amount) in [
            ("PAPL11.SA", dec!(1.00)),
            ("TIJL11.SA", dec!(0.80)),
            ("ACAO3.SA", dec!(0.25)),
        ] {
            histories.insert(
                symbol.to_owned(),
                DividendHistory {
                    currency: "BRL".to_owned(),
                    events: monthly(amount),
                },
            );
        }
    }
    let quotes = FakeQuotes::with_market();
    {
        let mut prices = quotes.prices.lock().unwrap();
        prices.insert(
            "PAPL11.SA".to_owned(),
            (dec!(100), "BRL", "Fundo Papel Ficticio"),
        );
        prices.insert(
            "TIJL11.SA".to_owned(),
            (dec!(90), "BRL", "Fundo Tijolo Ficticio"),
        );
        prices.insert(
            "ACAO3.SA".to_owned(),
            (dec!(20), "BRL", "Empresa Ficticia ON"),
        );
    }
    let state = AppState::new(
        db.store.clone(),
        Arc::new(DevAuthenticator::new("projecao-renda@prumo.test")),
        Arc::new(QuoteService::new(
            db.store.clone(),
            quotes,
            LiveQuotes::new(db.store.clone()),
        )),
        BuildInfo {
            version: "9.9.9".into(),
            commit: "abc1234".into(),
        },
        None,
    )
    .with_dividend_provider(dividends);
    let income: Arc<IncomeService> = state.income.clone();
    let router = prumo::http::public_router(state, &dir.path());

    // O tipo de cada FII, como a fonte da CVM grava.
    db.store
        .replace_sector_reference(
            SectorSource::CvmReits,
            &[
                SectorEntry {
                    code: "PAPL".into(),
                    sector: "Papel".into(),
                    subsector: None,
                    segment: None,
                    composition: None,
                },
                SectorEntry {
                    code: "TIJL".into(),
                    sector: "Tijolo".into(),
                    subsector: None,
                    segment: None,
                    composition: None,
                },
            ],
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    for (class, symbol, quantity) in [
        ("br_reits", "PAPL11", 100),
        ("br_reits", "TIJL11", 50),
        ("br_stocks", "ACAO3", 200),
    ] {
        let reply = post(
            &router,
            "/api/v1/assets",
            json!({ "asset_class": class, "symbol": symbol, "quantity": quantity }),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    }
    income.settle().await;
    // O CDI dos últimos 12 meses (0,05% por dia corrido, para o teste) e o Focus.
    let cdi: Vec<(Date, Decimal)> = (1..=365)
        .map(|i| (today() - Duration::days(i), dec!(0.05)))
        .rev()
        .collect();
    db.store.upsert_index(IndexCode::Cdi, &cdi).await.unwrap();
    let survey = date!(2026 - 11 - 19);
    let focus = [
        (
            Indicator::Ipca,
            Period::Annual,
            today().year() + 1,
            dec!(4.5),
        ),
        (Indicator::Selic, Period::Annual, today().year(), dec!(14.0)),
        (
            Indicator::Selic,
            Period::Annual,
            today().year() + 1,
            dec!(12.0),
        ),
    ]
    .map(|(indicator, period, year, median)| Expectation {
        indicator,
        period,
        reference: Date::from_calendar_date(year, time::Month::January, 1).unwrap(),
        median,
        survey_date: survey,
    });
    db.store
        .save_focus(&focus, OffsetDateTime::now_utc())
        .await
        .unwrap();

    // O padrão: o Focus do banco, 12 meses, amplitude de 2 pontos.
    let reply = get(&router, "/api/v1/income/projection").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let body = &reply.body;
    assert_eq!(body["months"].as_array().unwrap().len(), 12);
    assert_eq!(body["premises"]["horizon_months"], 12);
    assert_eq!(body["premises"]["scenario_spread_percent"], 2.0);
    assert_eq!(body["premises"]["cdi"]["source"], "focus");
    assert_eq!(body["premises"]["ipca"]["source"], "focus");
    assert_eq!(body["premises"]["focus"]["survey_date"], "2026-11-19");
    // A partida é a estimativa da Renda: a mesma conta.
    let summary = get(&router, "/api/v1/income/summary").await;
    assert_eq!(
        body["estimate_monthly"],
        summary.body["estimate"]["monthly"]
    );
    for month in body["months"].as_array().unwrap() {
        assert!(money(&month["low"]) <= money(&month["base"]));
        assert!(money(&month["base"]) <= money(&month["high"]));
    }
    let groups: Vec<&str> = body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["group"].as_str().unwrap())
        .collect();
    assert_eq!(groups, ["paper_reits", "brick_reits", "br_stocks"]);

    // CDI e IPCA trocados pela pessoa: a conta fica fechada.
    let reply = get(&router, "/api/v1/income/projection?cdi=10&ipca=6&spread=0").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let body = &reply.body;
    assert_eq!(body["premises"]["cdi"]["source"], "informed");
    assert_eq!(body["premises"]["ipca"]["source"], "informed");
    let trailing = money(&body["premises"]["cdi"]["trailing_percent"]);
    let asset = |symbol: &str| {
        body["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["symbol"] == symbol)
            .unwrap()
            .clone()
    };
    // Papel: 100 cotas × 1,00 por mês = 100; vezes 10% ÷ o CDI dos 12 meses.
    let paper = asset("PAPL11");
    assert_eq!(paper["driver"], "cdi");
    assert_eq!(money(&paper["monthly_start"]), 100.0);
    let expected = 100.0 * 12.0 * 10.0 / trailing;
    assert!(
        (money(&paper["totals"]["base"]) - expected).abs() < 0.5,
        "{paper} contra {expected}"
    );
    // Tijolo: 50 × 0,80 = 40 por mês, com o IPCA de 6% ao ano acumulado mês a mês.
    let brick = asset("TIJL11");
    assert_eq!(brick["driver"], "ipca");
    let expected: f64 = (1..=12)
        .map(|k| 40.0 * 1.06f64.powf(f64::from(k) / 12.0))
        .sum();
    assert!(
        (money(&brick["totals"]["base"]) - expected).abs() < 0.05,
        "{brick} contra {expected}"
    );
    // Ação: a média, parada (o histórico tem um ano só: os 12 meses).
    let stock = asset("ACAO3");
    assert_eq!(stock["driver"], "flat");
    assert_eq!(stock["basis"], "trailing_12m");
    assert!((money(&stock["totals"]["base"]) - 600.0).abs() < 0.01);
    // Sem amplitude, os três cenários são o mesmo.
    assert_eq!(body["totals"]["low"], body["totals"]["base"]);
    assert_eq!(body["totals"]["high"], body["totals"]["base"]);

    // 24 meses.
    let long = get(&router, "/api/v1/income/projection?months=24").await;
    assert_eq!(long.body["months"].as_array().unwrap().len(), 24);

    for uri in [
        "/api/v1/income/projection?months=6",
        "/api/v1/income/projection?spread=21",
        "/api/v1/income/projection?cdi=-1",
        "/api/v1/income/projection?ipca=51",
        "/api/v1/income/projection?outro=1",
    ] {
        assert_eq!(
            get(&router, uri).await.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{uri}"
        );
    }

    // Outra pessoa, sem carteira: nada a projetar, sem erro.
    let other = app(
        &db.store,
        "projecao-vazia@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let empty = get(&other.router, "/api/v1/income/projection").await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text);
    assert_eq!(empty.body["assets"], json!([]));
    assert_eq!(empty.body["totals"]["base"], 0.0);
}

/// Sobe um Postgres vazio, sem migração.
async fn empty_database() -> (sqlx::PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    (PgPoolOptions::new().connect(&url).await.unwrap(), container)
}

/// As migrações até a de antes desta frente.
fn before_projection() -> Migrator {
    let all = sqlx::migrate!("./migrations");
    let ours = all
        .migrations
        .iter()
        .find(|m| m.description == "projection")
        .map(|m| m.version)
        .unwrap();
    Migrator::with_migrations(
        all.migrations
            .iter()
            .filter(|m| m.version < ours)
            .cloned()
            .collect(),
    )
}

async fn objective_with_nominal_return(pool: &sqlx::PgPool, nominal: Option<Decimal>) -> Uuid {
    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, $2)")
        .bind(user)
        .bind(format!("{user}@prumo.test"))
        .execute(pool)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    sqlx::query("insert into objectives (id, user_id, kind, target, expected_return) values ($1, $2, 'net_worth', 100000, $3)")
        .bind(id)
        .bind(user)
        .bind(nominal)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn real_return(pool: &sqlx::PgPool, id: Uuid) -> Option<Decimal> {
    let (value,): (Option<Decimal>,) =
        sqlx::query_as("select real_return from objectives where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    value
}

#[tokio::test]
async fn the_nominal_premise_becomes_real_on_migration() {
    // Com 12 meses de IPCA no banco: desconta o IPCA.
    let (pool, _container) = empty_database().await;
    before_projection().run(&pool).await.unwrap();
    let informed = objective_with_nominal_return(&pool, Some(dec!(10))).await;
    let default = objective_with_nominal_return(&pool, None).await;
    for i in 0..12 {
        let month = first_day(
            YearMonth {
                year: 2025,
                month: 11,
            }
            .shift(i),
        )
        .unwrap();
        sqlx::query("insert into index_history (code, date, value) values ('ipca', $1, 0.5)")
            .bind(month)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let expected = (1.10 / 1.005f64.powi(12) - 1.0) * 100.0;
    let real = real_return(&pool, informed).await.unwrap();
    assert!(
        (real - Decimal::try_from(expected).unwrap()).abs() < dec!(0.0001),
        "{real} contra {expected}"
    );
    assert_eq!(real_return(&pool, default).await, None);
    // A coluna nominal saiu.
    let (exists,): (bool,) = sqlx::query_as(
        "select exists (select 1 from information_schema.columns where table_name = 'objectives' and column_name = 'expected_return')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!exists);

    // Sem IPCA no banco não há como converter: volta ao padrão.
    let (pool, _container) = empty_database().await;
    before_projection().run(&pool).await.unwrap();
    let informed = objective_with_nominal_return(&pool, Some(dec!(10))).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    assert_eq!(real_return(&pool, informed).await, None);
}
