//! Desempenho contra um Postgres de verdade: a tarefa diária (idempotente,
//! uma réplica por vez), o fluxo dos aportes, as rotas e o dono. Carteira
//! fictícia.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::history::{HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::{local_date, Performance, JOB_LOCK_KEY};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::{Date, Duration, OffsetDateTime};

/// Séries em memória; guarda cada pedido para o teste conferir o incremental.
#[derive(Default)]
struct FakeHistory {
    closes: Mutex<HashMap<String, Series>>,
    indexes: Mutex<HashMap<IndexCode, Series>>,
    requests: Mutex<Vec<(String, Date, Date)>>,
}

impl FakeHistory {
    fn set_close(&self, symbol: &str, date: Date, close: Decimal) {
        let mut closes = self.closes.lock().unwrap();
        let series = closes.entry(symbol.to_owned()).or_default();
        series.retain(|(d, _)| *d != date);
        series.push((date, close));
        series.sort_by_key(|(d, _)| *d);
    }
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
        self.requests
            .lock()
            .unwrap()
            .push((code.as_str().to_owned(), from, to));
        Ok(window(self.indexes.lock().unwrap().get(&code), from, to))
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        self.requests
            .lock()
            .unwrap()
            .push((symbol.to_owned(), from, to));
        Ok(window(self.closes.lock().unwrap().get(symbol), from, to))
    }
}

async fn snapshot_rows(db: &Database, email: &str) -> Vec<(Date, Decimal, Decimal, Decimal)> {
    let user = db.store.ensure_user(email).await.unwrap();
    db.store
        .snapshot_points(user)
        .await
        .unwrap()
        .into_iter()
        .map(|p| (p.date, p.market_value, p.net_flow, p.quota_value))
        .collect()
}

#[tokio::test]
async fn performance_end_to_end() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();

    daily_job_is_idempotent_and_separates_flow_from_return(&db, &dir).await;
    one_replica_at_a_time(&db, &dir).await;
    today_is_partial_until_the_close(&db, &dir).await;
    each_person_sees_only_their_own_series(&db, &dir).await;
    rejects_unknown_period_and_index(&db, &dir).await;
}

async fn daily_job_is_idempotent_and_separates_flow_from_return(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let email = "cotas@exemplo.com";
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let knri = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    assert_eq!(knri.status, StatusCode::CREATED, "{}", knri.text);
    let cdb = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 1000 }),
    )
    .await;
    assert_eq!(cdb.status, StatusCode::CREATED, "{}", cdb.text);

    // Dois fechamentos: ontem (dia 1) e agora (dia 2).
    let now = OffsetDateTime::now_utc();
    let day1_at = now - Duration::days(1);
    let day1 = local_date(day1_at);
    let history = Arc::new(FakeHistory::default());
    history.set_close("KNRI11.SA", day1, dec!(142));
    history.indexes.lock().unwrap().insert(
        IndexCode::Cdi,
        (0..20)
            .map(|n| (day1 + Duration::days(1) - Duration::days(n), dec!(0.050788)))
            .collect(),
    );
    let job = Performance::new(db.store.clone(), history.clone());

    let first = job.run_daily(day1_at).await.unwrap();
    assert!(first.ran);
    assert_eq!(first.snapshots, 1);
    // Rodar de novo no mesmo dia refaz a foto, não cria outra.
    let again = job.run_daily(day1_at + Duration::minutes(5)).await.unwrap();
    assert!(again.ran);
    let rows = snapshot_rows(db, email).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, day1);
    // A primeira foto entra inteira como aplicado, com a cota em 1.
    assert_eq!(
        (rows[0].1, rows[0].2, rows[0].3),
        (dec!(2420), dec!(2420), dec!(1))
    );

    // Dia 2: KNRI11 fecha a 150; a pessoa registra 5 cotas pagas a 148 e
    // informa que o CDB rendeu R$ 10.
    let knri_id = knri.body["id"].as_str().unwrap();
    let contribution = post(
        &app.router,
        "/api/v1/contributions",
        json!({ "amount": 740, "items": [{ "asset_id": knri_id, "quantity": 5, "unit_price": 148 }] }),
    )
    .await;
    assert_eq!(
        contribution.status,
        StatusCode::CREATED,
        "{}",
        contribution.text
    );
    let cdb_id = cdb.body["id"].as_str().unwrap();
    let edited = patch(
        &app.router,
        &format!("/api/v1/assets/{cdb_id}"),
        json!({ "amount": 1010 }),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);

    // Depois do aporte: ele cai na janela desta foto.
    let at = OffsetDateTime::now_utc();
    let day2 = local_date(at);
    history.set_close("KNRI11.SA", day2, dec!(150));
    job.run_daily(at).await.unwrap();
    job.run_daily(at + Duration::seconds(1)).await.unwrap();
    let rows = snapshot_rows(db, email).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].0, day2);
    // 15 × 150 + 1010; o fluxo é só o aporte (740), o resto é rendimento.
    assert_eq!((rows[1].1, rows[1].2), (dec!(3260), dec!(740)));
    let expected = (dec!(2520) / dec!(2420)).round_dp(12);
    assert_eq!(rows[1].3, expected);

    // Incremental: o segundo dia só pediu o que faltava de KNRI11.
    let requests = history.requests.lock().unwrap().clone();
    let knri_requests: Vec<_> = requests.iter().filter(|r| r.0 == "KNRI11.SA").collect();
    // A segunda rodada do dia 1 e a do dia 2 já não pedem nada.
    assert_eq!(knri_requests.len(), 2, "{knri_requests:?}");
    assert_eq!(knri_requests[0].1, day1 - Duration::days(7));
    assert_eq!(knri_requests[1].1, day1 + Duration::days(1));
    let symbol = ["KNRI11.SA".to_owned()];
    assert_eq!(
        db.store.closes_on(&symbol, day1).await.unwrap()["KNRI11.SA"],
        dec!(142)
    );
    assert_eq!(
        db.store.closes_on(&symbol, day2).await.unwrap()["KNRI11.SA"],
        dec!(150)
    );
    assert_eq!(
        db.store.last_price_date("KNRI11.SA").await.unwrap(),
        Some(day2)
    );
    let (cdi_from, cdi_to) = db
        .store
        .index_bounds(IndexCode::Cdi)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cdi_from, day1 - Duration::days(10));
    assert_eq!(cdi_to, day1 + Duration::days(1));
    assert!(db
        .store
        .index_bounds(IndexCode::Ipca)
        .await
        .unwrap()
        .is_none());

    // A tela: patrimônio (aplicado × ganho) e a rentabilidade contra o CDI.
    let worth = get(&app.router, "/api/v1/history/net-worth?period=all").await;
    assert_eq!(worth.status, StatusCode::OK, "{}", worth.text);
    let points = worth.body["points"].as_array().unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[1]["market_value"], 3260.0);
    assert_eq!(points[1]["invested"], 3160.0);
    assert_eq!(points[1]["gain"], 100.0);
    assert_eq!(points[1]["partial"], false);
    assert_eq!(worth.body["starts_on"], Value::String(day1.to_string()));

    let perf = get(
        &app.router,
        "/api/v1/performance?period=all&benchmarks=cdi,ipca,ibov",
    )
    .await;
    assert_eq!(perf.status, StatusCode::OK, "{}", perf.text);
    let ret = perf.body["return_percent"].as_f64().unwrap();
    assert!((ret - 4.1322).abs() < 0.0001, "{ret}");
    let benchmarks = perf.body["benchmarks"].as_array().unwrap();
    assert_eq!(benchmarks.len(), 3);
    assert_eq!(benchmarks[0]["code"], "cdi");
    assert!(benchmarks[0]["return_percent"].as_f64().unwrap() > 0.0);
    // Sem IPCA e Ibovespa no falso: sem número, em vez de zero.
    assert_eq!(benchmarks[1]["return_percent"], Value::Null);
    assert_eq!(benchmarks[2]["as_of"], Value::Null);
    let since = &perf.body["summary"]["since_start"];
    assert_eq!(since["full_window"], true);
    assert!(since["excess_over_cdi"].as_f64().unwrap() > 3.0);
    assert!(since["percent_of_cdi"].as_f64().unwrap() > 100.0);
    assert_eq!(perf.body["summary"]["twelve_months"]["full_window"], false);
    let years = perf.body["years"].as_array().unwrap();
    assert!(!years.is_empty());
    assert_eq!(years[0]["months"].as_array().unwrap().len(), 12);
    let curve = perf.body["points"].as_array().unwrap();
    assert_eq!(curve.len(), 2);
    assert_eq!(curve[0]["portfolio"], 0.0);
    assert_eq!(curve[0]["benchmarks"]["cdi"], 0.0);
}

/// Duas réplicas: quem não pega o lock pula; rodando juntas, uma foto só.
async fn one_replica_at_a_time(db: &Database, dir: &tempfile_dir::Dir) {
    let email = "replicas@exemplo.com";
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 4 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let history: Arc<FakeHistory> = Arc::new(FakeHistory::default());
    let job = Performance::new(db.store.clone(), history.clone());

    let held = db.store.try_job_lock(JOB_LOCK_KEY).await.unwrap().unwrap();
    assert!(db.store.try_job_lock(JOB_LOCK_KEY).await.unwrap().is_none());
    let skipped = job.run_daily(OffsetDateTime::now_utc()).await.unwrap();
    assert!(!skipped.ran);
    held.release().await;

    let other = Performance::new(db.store.clone(), history);
    let now = OffsetDateTime::now_utc();
    let (a, b) = tokio::join!(job.run_daily(now), other.run_daily(now));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(a.ran || b.ran);
    assert_eq!(snapshot_rows(db, email).await.len(), 1);
    // O lock foi solto: a próxima rodada entra.
    assert!(job.run_daily(now).await.unwrap().ran);
    // Lock abandonado (sem `release`) não fica preso numa conexão do pool.
    drop(db.store.try_job_lock(JOB_LOCK_KEY).await.unwrap().unwrap());
    assert!(job.run_daily(now).await.unwrap().ran);
}

async fn today_is_partial_until_the_close(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "parcial@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let empty = get(&app.router, "/api/v1/performance").await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text);
    assert_eq!(empty.body["starts_on"], Value::Null);
    assert_eq!(empty.body["return_percent"], Value::Null);
    assert_eq!(empty.body["points"], json!([]));
    assert_eq!(empty.body["summary"]["since_start"], Value::Null);

    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "MXRF11", "quantity": 100 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let worth = get(&app.router, "/api/v1/history/net-worth").await;
    assert_eq!(worth.body["period"], "12m");
    let points = worth.body["points"].as_array().unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0]["partial"], true);
    assert_eq!(points[0]["market_value"], 837.0);
    assert_eq!(points[0]["invested"], 837.0);
    let perf = get(&app.router, "/api/v1/performance?period=month").await;
    assert_eq!(perf.body["return_percent"], 0.0);
    assert_eq!(perf.body["window"]["from"], perf.body["window"]["to"]);
    // A foto parcial não fica gravada.
    assert!(snapshot_rows(db, "parcial@exemplo.com").await.is_empty());
}

async fn each_person_sees_only_their_own_series(db: &Database, dir: &tempfile_dir::Dir) {
    let other = app(
        &db.store,
        "outra@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let worth = get(&other.router, "/api/v1/history/net-worth?period=all").await;
    assert_eq!(worth.status, StatusCode::OK);
    assert_eq!(worth.body["starts_on"], Value::Null);
    assert_eq!(worth.body["points"], json!([]));
    let perf = get(&other.router, "/api/v1/performance?period=all").await;
    assert_eq!(perf.body["years"], json!([]));
}

async fn rejects_unknown_period_and_index(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "valida@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let bad_period = get(&app.router, "/api/v1/performance?period=5y").await;
    assert_eq!(bad_period.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_period.body["code"], "validation_failed");
    let bad_index = get(&app.router, "/api/v1/performance?benchmarks=cdi,ifix").await;
    assert_eq!(bad_index.status, StatusCode::UNPROCESSABLE_ENTITY);
    let bad_worth = get(&app.router, "/api/v1/history/net-worth?period=semana").await;
    assert_eq!(bad_worth.status, StatusCode::UNPROCESSABLE_ENTITY);
}
