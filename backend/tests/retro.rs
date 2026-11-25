//! A série retroativa contra um Postgres de verdade e provedores falsos (o
//! `chart` do Yahoo e o CSV do Tesouro Transparente num servidor de mentira):
//! o caso fictício de `docs/desempenho.md` ("Conferência passo a passo"), o
//! desdobro com o preço ajustado pelo Yahoo, o provento, o recálculo depois de
//! editar ou apagar um lançamento antigo, o histórico do Tesouro baixado uma vez
//! e a migração 0013 sobre um banco com livro. Carteira fictícia.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use prumo::history::PublicHistory;
use prumo::performance::{Performance, JOB_LOCK_KEY};
use prumo::quotes::Quote;
use prumo::store::Store;
use prumo::treasury::TransparenteFeed;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use time::macros::{date, time};
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const D0: Date = date!(2025 - 02 - 28);
const D1: Date = date!(2025 - 03 - 03);
const D2: Date = date!(2025 - 03 - 04);
const D3: Date = date!(2025 - 03 - 05);
const D4: Date = date!(2025 - 03 - 06);
const D5: Date = date!(2025 - 03 - 07);

/// A abertura do pregão (10h em Brasília) como o Yahoo data o dia.
fn session(date: Date) -> i64 {
    date.with_time(time!(13:00)).assume_utc().unix_timestamp()
}

/// Uma resposta do `chart` no formato medido (`tests/fixtures/history/`):
/// fechamentos e desdobramentos `(dia, numerador, denominador)`.
fn chart(closes: &[(Date, f64)], splits: &[(Date, f64, f64)]) -> Value {
    let events: serde_json::Map<String, Value> = splits
        .iter()
        .map(|(d, n, den)| {
            (
                session(*d).to_string(),
                json!({ "date": session(*d), "numerator": n, "denominator": den, "splitRatio": format!("{n}:{den}") }),
            )
        })
        .collect();
    json!({ "chart": { "result": [{
        "meta": { "gmtoffset": -10800, "currentTradingPeriod": { "regular": { "end": 0 } } },
        "timestamp": closes.iter().map(|(d, _)| session(*d)).collect::<Vec<_>>(),
        "events": { "splits": events },
        "indicators": { "quote": [{ "close": closes.iter().map(|(_, c)| *c).collect::<Vec<_>>() }] }
    }], "error": null } })
}

const IPCA_2035: &str = "TD-IPCA-20350515";

/// Recorte fictício do CSV do Tesouro: o título da carteira em vários dias,
/// um dia de antes do que ela precisa e um título que ninguém tem.
const BONDS_CSV: &str = "Tipo Titulo;Data Vencimento;Data Base;Taxa Compra Manha;Taxa Venda Manha;PU Compra Manha;PU Venda Manha;PU Base Manha
Tesouro IPCA+;15/05/2035;07/03/2025;7,10;7,22;2545,00;2530,00;2530,00
Tesouro IPCA+;15/05/2035;06/03/2025;7,10;7,22;2535,00;2520,00;2520,00
Tesouro IPCA+;15/05/2035;05/03/2025;7,10;7,22;2525,00;2510,00;2510,00
Tesouro IPCA+;15/05/2035;04/03/2025;7,10;7,22;2515,00;2500,00;2500,00
Tesouro IPCA+;15/05/2035;03/03/2025;7,10;7,22;2505,00;2490,00;2490,00
Tesouro Prefixado;01/01/2031;07/03/2025;13,00;13,10;600,00;598,00;598,00
";

/// O servidor de mentira com o Yahoo e o Tesouro. O BCB e o `^BVSP` não
/// estão montados: respondem 404 (sem índice, a série segue).
async fn sources() -> MockServer {
    let server = MockServer::start().await;
    // KNRI11: sem evento. WEGE3: desdobro de 1 para 2 no dia 4, e o Yahoo
    // já divide por 2 os fechamentos de antes (41 → 20,50; 42 → 21).
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/KNRI11.SA"))
        .and(query_param("events", "split"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            &[
                (D0, 99.0),
                (D1, 101.0),
                (D2, 102.0),
                (D3, 103.0),
                (D4, 103.0),
                (D5, 104.0),
            ],
            &[],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/WEGE3.SA"))
        .and(query_param("events", "split"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chart(
            &[(D2, 20.5), (D3, 21.0), (D4, 21.5), (D5, 22.0)],
            &[(D4, 2.0, 1.0)],
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ckan/api/3/action/package_show"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "resources": [{ "format": "CSV",
                "url": "https://www.tesourotransparente.gov.br/ckan/dataset/a/resource/b/download/precotaxatesourodireto.csv" }] }
        })))
        .mount(&server)
        .await;
    // Baixado uma vez só: depois, o que já está guardado basta.
    Mock::given(method("GET"))
        .and(path(
            "/ckan/dataset/a/resource/b/download/precotaxatesourodireto.csv",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/csv")
                .set_body_string(BONDS_CSV),
        )
        .expect(1)
        .mount(&server)
        .await;
    server
}

fn job(store: &Store, server: &MockServer) -> Performance {
    let http = reqwest::Client::new();
    Performance::new(
        store.clone(),
        Arc::new(PublicHistory::with_bases(
            http.clone(),
            &server.uri(),
            &server.uri(),
        )),
    )
    .with_treasury_history(Arc::new(TransparenteFeed::with_base(http, &server.uri())))
}

/// Um minuto à frente: a marca de "sujo" já assentou.
fn later() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::minutes(1)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Row {
    market_value: Decimal,
    net_flow: Decimal,
    income: Decimal,
    quota: Decimal,
}

async fn rows(store: &Store, user: Uuid) -> Vec<(Date, Row)> {
    store
        .snapshot_points(user)
        .await
        .unwrap()
        .into_iter()
        .map(|p| {
            (
                p.date,
                Row {
                    market_value: p.market_value,
                    net_flow: p.net_flow,
                    income: p.income,
                    quota: p.quota_value,
                },
            )
        })
        .collect()
}

fn on(rows: &[(Date, Row)], date: Date) -> Row {
    rows.iter()
        .find(|(d, _)| *d == date)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("sem foto em {date}"))
}

fn close_to(actual: Decimal, expected: Decimal) {
    assert!(
        (actual - expected).abs() < dec!(0.000000001),
        "{actual} != {expected}"
    );
}

async fn create(app: &TestApp, body: Value) -> String {
    let created = post(&app.router, "/api/v1/assets", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    created.body["id"].as_str().unwrap().to_owned()
}

async fn trade(app: &TestApp, body: Value) -> String {
    let created = post(&app.router, "/api/v1/transactions", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    created.body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn retroactive_series_end_to_end() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let server = sources().await;
    let job = job(&db.store, &server);

    let user = hand_case_matches_the_document(&db, &dir, &job).await;
    editing_an_old_entry_redoes_only_from_its_date(&db, &dir, &job, user).await;
    entry_before_the_first_moves_the_start(&db, &dir, &job, user).await;
    one_replica_and_settling(&db, &job, user).await;
    treasury_history_is_downloaded_once(&db, &dir, &job).await;
}

/// O caso de `docs/desempenho.md`, passo a passo: compra com taxa, provento,
/// desdobro com preço ajustado, venda com taxa e renda fixa por valor.
async fn hand_case_matches_the_document(
    db: &Database,
    dir: &tempfile_dir::Dir,
    job: &Performance,
) -> Uuid {
    let email = "retro@exemplo.com";
    let quotes = FakeQuotes::with_market();
    {
        // A cotação de agora igual ao último fechamento: o ponto parcial de
        // hoje não mexe na conta.
        let mut prices = quotes.prices.lock().unwrap();
        prices.insert(
            "KNRI11.SA".into(),
            (dec!(104), "BRL", "Kinea Renda Imobiliaria"),
        );
        prices.insert("WEGE3.SA".into(), (dec!(22), "BRL", "WEG"));
    }
    let app = app(&db.store, email, quotes, &dir.path());
    let user = db.store.ensure_user(email).await.unwrap();
    let fii = create(
        &app,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0 }),
    )
    .await;
    let stock = create(
        &app,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 0 }),
    )
    .await;
    let cdb = create(
        &app,
        json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 0 }),
    )
    .await;
    trade(&app, json!({ "asset_id": fii, "kind": "buy", "trade_date": D1.to_string(), "quantity": 10, "unit_price": 100, "fees": 1 })).await;
    trade(&app, json!({ "asset_id": cdb, "kind": "opening", "trade_date": D1.to_string(), "quantity": 1000, "unit_price": 1 })).await;
    trade(&app, json!({ "asset_id": stock, "kind": "buy", "trade_date": D2.to_string(), "quantity": 20, "unit_price": 40 })).await;
    trade(
        &app,
        json!({ "asset_id": stock, "kind": "split", "trade_date": D4.to_string(), "quantity": 20 }),
    )
    .await;
    trade(&app, json!({ "asset_id": cdb, "kind": "adjustment", "trade_date": D4.to_string(), "quantity": 5 })).await;
    trade(&app, json!({ "asset_id": fii, "kind": "sell", "trade_date": D5.to_string(), "quantity": 5, "unit_price": 104, "fees": 0.5 })).await;
    let dividend = post(
        &app.router,
        "/api/v1/income",
        json!({ "asset_id": fii, "kind": "reit_income", "pay_date": D3.to_string(), "amount_per_unit": 1, "quantity": 10 }),
    )
    .await;
    assert_eq!(dividend.status, StatusCode::CREATED, "{}", dividend.text);

    // O livro mudou no passado: a tela sabe que a série vai ser refeita.
    let pending = get(&app.router, "/api/v1/performance?period=all").await;
    assert_eq!(pending.body["rebuilding_since"], D1.to_string());

    let report = job.run_pending(later()).await.unwrap();
    assert!(report.ran);
    assert_eq!((report.users, report.failed), (1, 0), "{report:?}");
    // 5 de KNRI11 e 4 de WEGE3, desde o primeiro lançamento de cada um.
    assert_eq!(report.prices, 9, "{report:?}");

    let series = rows(&db.store, user).await;
    assert_eq!(series.first().map(|r| r.0), Some(D1));
    let row = |d| {
        let r = on(&series, d);
        (r.market_value, r.net_flow, r.income)
    };
    assert_eq!(row(D1), (dec!(2010), dec!(2001), dec!(0)));
    assert_eq!(row(D2), (dec!(2840), dec!(800), dec!(0)));
    assert_eq!(row(D3), (dec!(2870), dec!(0), dec!(10)));
    assert_eq!(row(D4), (dec!(2895), dec!(0), dec!(0)));
    assert_eq!(row(D5), (dec!(2405), dec!(-519.5), dec!(0)));
    let q2 = dec!(2040) / dec!(2010);
    let q3 = q2 * dec!(2880) / dec!(2840);
    let q5 = q3 * dec!(2924.5) / dec!(2870);
    close_to(on(&series, D1).quota, Decimal::ONE);
    close_to(on(&series, D2).quota, q2);
    close_to(on(&series, D3).quota, q3);
    close_to(on(&series, D5).quota, q5);
    // Um dia por data corrida até ontem; o fim de semana repete sexta.
    assert_eq!(row(D5 + Duration::days(2)), (dec!(2405), dec!(0), dec!(0)));
    // O cache guarda o fechamento real: o ajuste do Yahoo foi desfeito.
    let wege = ["WEGE3.SA".to_owned()];
    assert_eq!(
        db.store.closes_on(&wege, D2).await.unwrap()["WEGE3.SA"],
        dec!(41)
    );
    assert_eq!(
        db.store.closes_on(&wege, D4).await.unwrap()["WEGE3.SA"],
        dec!(21.5)
    );
    assert!(db.store.dirty_since(user).await.unwrap().is_none());

    // A tela: desde o início, 12 meses e a tabela mês × ano.
    let all = get(&app.router, "/api/v1/performance?period=all").await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.text);
    assert_eq!(all.body["starts_on"], D1.to_string());
    assert_eq!(all.body["rebuilding_since"], Value::Null);
    assert_eq!(all.body["return_percent"], 4.8765);
    let year = get(&app.router, "/api/v1/performance?period=12m").await;
    assert_eq!(year.body["return_percent"], 0.0);
    assert_eq!(year.body["summary"]["twelve_months"]["full_window"], true);
    let years = all.body["years"].as_array().unwrap();
    assert_eq!(years[0]["year"], 2025);
    let months = years[0]["months"].as_array().unwrap();
    assert_eq!(months[1], Value::Null);
    assert_eq!(months[2], 4.8765);
    assert_eq!(months[3], 0.0);
    assert_eq!(years[0]["return_percent"], 4.8765);
    assert_eq!(years.last().unwrap()["return_percent"], 0.0);
    let worth = get(&app.router, "/api/v1/history/net-worth?period=all").await;
    let points = worth.body["points"].as_array().unwrap();
    let last = points.last().unwrap();
    assert_eq!(last["market_value"], 2405.0);
    assert_eq!(last["invested"], 2281.5);
    assert_eq!(last["gain"], 123.5);
    assert_eq!(last["income"], 10.0);
    // A série inteira desde março de 2025, sem buraco.
    let days = (OffsetDateTime::now_utc()
        .to_offset(prumo::performance::BRASILIA)
        .date()
        - D1)
        .whole_days();
    assert!(points.len() as i64 >= days, "{} < {days}", points.len());

    // Cotação ao vivo (o evento `prices`) não suja a série.
    db.store
        .upsert_quote(
            &Quote {
                symbol: "KNRI11.SA".into(),
                price: dec!(105),
                currency: "BRL".into(),
                name: None,
                traded_at: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    assert!(db.store.dirty_since(user).await.unwrap().is_none());
    user
}

async fn editing_an_old_entry_redoes_only_from_its_date(
    db: &Database,
    dir: &tempfile_dir::Dir,
    job: &Performance,
    user: Uuid,
) {
    let app = app(
        &db.store,
        "retro@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let first_day = db.store.snapshot_before(user, D2).await.unwrap().unwrap();
    let list = get(&app.router, "/api/v1/transactions").await;
    let find = |kind: &str, date: Date| {
        list.body
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["kind"] == kind && t["trade_date"] == date.to_string())
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .unwrap()
    };
    // A compra de WEGE3 do dia 2 foi a 39, não a 40.
    let buy = find("buy", D2);
    let edited = patch(
        &app.router,
        &format!("/api/v1/transactions/{buy}"),
        json!({ "unit_price": 39 }),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);
    let dirty = db.store.dirty_since(user).await.unwrap().unwrap();
    assert_eq!(dirty.since, D2);
    let report = job.run_pending(later()).await.unwrap();
    assert_eq!(report.users, 1, "{report:?}");
    // O dia 1 não foi tocado: a mesma foto, da mesma hora.
    assert_eq!(
        db.store.snapshot_before(user, D2).await.unwrap().unwrap(),
        first_day
    );
    let series = rows(&db.store, user).await;
    assert_eq!(on(&series, D2).net_flow, dec!(780));
    let q2 = dec!(2060) / dec!(2010);
    close_to(on(&series, D2).quota, q2);
    close_to(
        on(&series, D5).quota,
        q2 * dec!(2880) / dec!(2840) * dec!(2924.5) / dec!(2870),
    );

    // Sem o provento, o dia 3 é só preço.
    let income = get(&app.router, "/api/v1/income").await;
    let event = income.body[0]["id"].as_str().unwrap().to_owned();
    let removed = delete(&app.router, &format!("/api/v1/income/{event}")).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.text);
    assert_eq!(db.store.dirty_since(user).await.unwrap().unwrap().since, D3);
    job.run_pending(later()).await.unwrap();
    let series = rows(&db.store, user).await;
    assert_eq!(on(&series, D3).income, dec!(0));
    close_to(on(&series, D5).quota, q2 * dec!(2924.5) / dec!(2840));

    // Apagada a venda do dia 5: as 10 cotas ficam.
    let sell = find("sell", D5);
    let gone = delete(&app.router, &format!("/api/v1/transactions/{sell}")).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.text);
    job.run_pending(later()).await.unwrap();
    let series = rows(&db.store, user).await;
    assert_eq!(
        (on(&series, D5).market_value, on(&series, D5).net_flow),
        (dec!(2925), dec!(0))
    );
}

/// Um lançamento antes do primeiro: a série passa a começar nele (o preço
/// desse dia é baixado para trás). Apagado, ela volta a começar no dia 1 e o
/// dia que sobrou some.
async fn entry_before_the_first_moves_the_start(
    db: &Database,
    dir: &tempfile_dir::Dir,
    job: &Performance,
    user: Uuid,
) {
    let app = app(
        &db.store,
        "retro@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let assets = get(&app.router, "/api/v1/portfolio").await;
    let fii = assets.body["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["symbol"] == "KNRI11")
        .map(|a| a["id"].as_str().unwrap().to_owned())
        .unwrap();
    let early = trade(&app, json!({ "asset_id": fii, "kind": "buy", "trade_date": D0.to_string(), "quantity": 1, "unit_price": 99 })).await;
    job.run_pending(later()).await.unwrap();
    let series = rows(&db.store, user).await;
    assert_eq!(series[0].0, D0);
    assert_eq!(on(&series, D0).market_value, dec!(99));
    let knri = ["KNRI11.SA".to_owned()];
    assert_eq!(
        db.store.closes_on(&knri, D0).await.unwrap()["KNRI11.SA"],
        dec!(99)
    );

    let gone = delete(&app.router, &format!("/api/v1/transactions/{early}")).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.text);
    job.run_pending(later()).await.unwrap();
    let series = rows(&db.store, user).await;
    assert_eq!(series[0].0, D1);
    assert!(series.iter().all(|(d, _)| *d >= D1));
}

/// Duas réplicas: quem não pega o lock pula e a marca fica. Marca recém-feita
/// espera assentar (a importação ainda pode estar gravando).
async fn one_replica_and_settling(db: &Database, job: &Performance, user: Uuid) {
    db.store.mark_dirty(&[user], D4).await.unwrap();
    let fresh = job.run_pending(OffsetDateTime::now_utc()).await.unwrap();
    assert_eq!(fresh.users, 0);
    assert!(db.store.dirty_since(user).await.unwrap().is_some());

    let held = db.store.try_job_lock(JOB_LOCK_KEY).await.unwrap().unwrap();
    let skipped = job.run_pending(later()).await.unwrap();
    assert!(!skipped.ran);
    assert!(db.store.dirty_since(user).await.unwrap().is_some());
    held.release().await;
    let done = job.run_pending(later()).await.unwrap();
    assert!(done.ran && done.users == 1, "{done:?}");
    assert!(db.store.dirty_since(user).await.unwrap().is_none());
}

/// O título do Tesouro: o histórico sai do CSV, uma vez, só do título da
/// carteira e só desde o primeiro lançamento dele.
async fn treasury_history_is_downloaded_once(
    db: &Database,
    dir: &tempfile_dir::Dir,
    job: &Performance,
) {
    let email = "tesouro-retro@exemplo.com";
    db.store
        .upsert_quote(
            &Quote {
                symbol: IPCA_2035.into(),
                price: dec!(2530),
                currency: "BRL".into(),
                name: Some("Tesouro IPCA+ 2035".into()),
                traded_at: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let user = db.store.ensure_user(email).await.unwrap();
    let bond = create(
        &app,
        json!({ "asset_class": "fixed_income", "symbol": IPCA_2035, "treasury": true, "quantity": 0, "score_override": 5 }),
    )
    .await;
    let buy = trade(&app, json!({ "asset_id": bond, "kind": "buy", "trade_date": D2.to_string(), "quantity": 1, "unit_price": 2500 })).await;
    job.run_pending(later()).await.unwrap();
    let series = rows(&db.store, user).await;
    assert_eq!(series[0].0, D2);
    assert_eq!(on(&series, D3).market_value, dec!(2510));
    assert_eq!(on(&series, D5).market_value, dec!(2530));
    let td = [IPCA_2035.to_owned()];
    assert!(db.store.closes_on(&td, D1).await.unwrap().is_empty());
    assert_eq!(
        db.store.closes_on(&td, D2).await.unwrap()[IPCA_2035],
        dec!(2500)
    );
    assert!(db
        .store
        .last_price_date("TD-PRE-20310101")
        .await
        .unwrap()
        .is_none());

    // Mais um título no mesmo dia: o histórico já está guardado, a tabela não
    // é baixada de novo (o servidor confere: um download só).
    let edited = patch(
        &app.router,
        &format!("/api/v1/transactions/{buy}"),
        json!({ "quantity": 2 }),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);
    job.run_pending(later()).await.unwrap();
    assert_eq!(
        on(&rows(&db.store, user).await, D3).market_value,
        dec!(5020)
    );

    // Remover a pessoa leva a série e a marca juntas (o gatilho não tropeça
    // na remoção em cascata).
    let pool = PgPoolOptions::new().connect(&db.url).await.unwrap();
    db.store.mark_dirty(&[user], D2).await.unwrap();
    sqlx::query("delete from users where id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let (left,): (i64,) =
        sqlx::query_as("select count(*) from performance_rebuilds where user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

/// A 0013 sobre um banco com livro (0001 a 0012 aplicadas, carteira
/// fictícia): quem tem lançamento ganha a marca desde o mais antigo, e a foto
/// que já existia ganha provento zero.
#[tokio::test]
async fn migration_marks_existing_ledgers() {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    let mut before = sqlx::migrate!("./migrations");
    before.migrations = before
        .migrations
        .iter()
        .filter(|m| m.version < 13)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    before.run(&pool).await.unwrap();

    let (user, empty, asset) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (id, email) in [(user, "livro@prumo.test"), (empty, "vazio@prumo.test")] {
        sqlx::query("insert into users (id, email) values ($1, $2)")
            .bind(id)
            .bind(email)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "insert into assets (id, user_id, asset_class, symbol, display_name, quantity, listed_on_b3)
         values ($1, $2, 'br_reits', 'KNRI11', 'Ficticio', 15, true)",
    )
    .bind(asset)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    for (date, quantity) in [(D3, 5), (D1, 10)] {
        sqlx::query(
            "insert into transactions (id, user_id, asset_id, symbol, asset_class, display_name, kind, trade_date, quantity, unit_price)
             values ($1, $2, $3, 'KNRI11', 'br_reits', 'Ficticio', 'buy', $4, $5, 100)",
        )
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(asset)
        .bind(date)
        .bind(Decimal::from(quantity))
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "insert into portfolio_snapshots (user_id, date, market_value, net_flow, quota_value, quotas, taken_at)
         values ($1, $2, 1500, 1500, 1, 1500, now())",
    )
    .bind(user)
    .bind(date!(2026 - 11 - 20))
    .execute(&pool)
    .await
    .unwrap();

    let store = Store::from_pool(pool.clone());
    store.migrate().await.unwrap();
    let dirty = store.dirty_since(user).await.unwrap().unwrap();
    assert_eq!(dirty.since, D1);
    assert!(store.dirty_since(empty).await.unwrap().is_none());
    let points = store.snapshot_points(user).await.unwrap();
    assert_eq!(points[0].income, dec!(0));
}
