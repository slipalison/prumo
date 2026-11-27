//! Desempenho contra um Postgres de verdade: a tarefa diária (idempotente,
//! uma réplica por vez), o fluxo dos aportes, as rotas e o dono. A série sai do
//! livro: o saldo inicial do cadastro é datado no dia simulado. Carteira
//! fictícia.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::domain::risk::history_start;
use prumo::domain::treasury::Bond;
use prumo::history::{HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::{local_date, Performance, JOB_LOCK_KEY};
use prumo::quotes::Quote;
use prumo::treasury::history::TreasuryHistory;
use prumo::treasury::FeedError;
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

/// Histórico do Tesouro em memória: guarda o que foi pedido.
#[derive(Default)]
struct FakeBonds {
    asked: Mutex<Vec<(Bond, Date)>>,
}

#[async_trait]
impl TreasuryHistory for FakeBonds {
    async fn history(
        &self,
        wanted: &std::collections::BTreeMap<Bond, Date>,
    ) -> Result<std::collections::BTreeMap<Bond, Series>, FeedError> {
        let mut asked = self.asked.lock().unwrap();
        asked.extend(wanted.iter().map(|(b, d)| (*b, *d)));
        Ok(wanted.keys().map(|b| (*b, Series::new())).collect())
    }
}

/// Leva todos os lançamentos do ativo para `date`: o cadastro de hoje vira um
/// saldo inicial no dia que o teste simula.
async fn backdate(router: &axum::Router, asset_id: &str, date: Date) {
    let list = get(router, &format!("/api/v1/transactions?asset_id={asset_id}")).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text);
    for item in list.body.as_array().unwrap() {
        let id = item["id"].as_str().unwrap();
        let moved = patch(
            router,
            &format!("/api/v1/transactions/{id}"),
            json!({ "trade_date": date.to_string() }),
        )
        .await;
        assert_eq!(moved.status, StatusCode::OK, "{}", moved.text);
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
    treasury_title_is_valued_by_its_daily_quote(&db, &dir).await;
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
    // O IFIX, em pontos: +1% do dia 1 para o dia 2.
    history.indexes.lock().unwrap().insert(
        IndexCode::Ifix,
        vec![
            (day1 - Duration::days(3), dec!(3480)),
            (day1, dec!(3500)),
            (day1 + Duration::days(1), dec!(3535)),
        ],
    );
    let job = Performance::new(db.store.clone(), history.clone());
    backdate(&app.router, knri.body["id"].as_str().unwrap(), day1).await;
    backdate(&app.router, cdb.body["id"].as_str().unwrap(), day1).await;

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

    // Incremental: o primeiro pedido começa no começo da janela mais longa
    // do risco (antes do lançamento mais antigo); a segunda rodada do dia 1
    // não pede nada, e a do dia 2 só o dia novo.
    let requests = history.requests.lock().unwrap().clone();
    let knri_requests: Vec<_> = requests.iter().filter(|r| r.0 == "KNRI11.SA").collect();
    assert_eq!(knri_requests.len(), 2, "{knri_requests:?}");
    assert_eq!(knri_requests[0].1, history_start(day1));
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
    // O CDI é pedido desde 400 dias antes de hoje (os 12 meses dele são o alvo
    // do FII de papel), mesmo com a primeira foto de ontem; o falso só tem 20
    // dias, que entram todos.
    assert_eq!(cdi_from, day1 + Duration::days(1) - Duration::days(19));
    assert_eq!(cdi_to, day1 + Duration::days(1));
    // O Ibovespa é pedido desde o começo da janela mais longa do risco.
    let ibov_from = requests
        .iter()
        .filter(|r| r.0 == "ibov")
        .map(|r| r.1)
        .min()
        .unwrap();
    assert_eq!(ibov_from, history_start(day1));
    let first_cdi = requests
        .iter()
        .find(|r| r.0 == "cdi")
        .expect("o CDI foi pedido");
    assert!(first_cdi.1 <= day1 - Duration::days(365), "{first_cdi:?}");
    assert!(db
        .store
        .index_bounds(IndexCode::Ipca)
        .await
        .unwrap()
        .is_none());
    // O IFIX anda com os outros índices e cabe na tabela (migração 0027).
    assert!(requests.iter().any(|r| r.0 == "ifix"), "{requests:?}");
    assert_eq!(
        db.store.index_bounds(IndexCode::Ifix).await.unwrap(),
        Some((day1 - Duration::days(3), day1 + Duration::days(1)))
    );

    // A tela: patrimônio (aplicado × ganho) e a rentabilidade contra o CDI.
    let worth = get(&app.router, "/api/v1/history/net-worth?period=all").await;
    assert_eq!(worth.status, StatusCode::OK, "{}", worth.text);
    let points = worth.body["points"].as_array().unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[1]["market_value"], 3260.0);
    assert_eq!(points[1]["invested"], 3160.0);
    assert_eq!(points[1]["gain"], 100.0);
    assert_eq!(points[1]["income"], 0.0);
    assert_eq!(points[1]["partial"], false);
    assert_eq!(worth.body["starts_on"], Value::String(day1.to_string()));
    assert_eq!(worth.body["rebuilding_since"], Value::Null);
    // Patrimônio × aportado: desde o começo, tudo é aporte ou mercado.
    let bridge = &worth.body["summary"];
    assert_eq!(bridge["from"], Value::Null);
    assert_eq!(bridge["start_value"], 0.0);
    assert_eq!(bridge["contributions"], 3160.0);
    assert_eq!(bridge["market"], 100.0);
    assert_eq!(bridge["income"], 0.0);
    assert_eq!(bridge["end_value"], 3260.0);
    assert_eq!(
        (bridge["invested"].clone(), bridge["gain"].clone()),
        (json!(3160.0), json!(100.0))
    );
    // Série mais nova que o período: a janela também começa do zero.
    let month = get(&app.router, "/api/v1/history/net-worth?period=2y").await;
    assert_eq!(month.status, StatusCode::OK, "{}", month.text);
    assert_eq!(month.body["period"], "2y");
    assert_eq!(month.body["summary"]["from"], Value::Null);

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
    // Ano a ano: um mês medido (dia 1 → dia 2, caia ou não na virada do mês),
    // o acumulado é a série inteira e o CDI fica abaixo.
    let last_year = years.last().unwrap();
    let accumulated = last_year["accumulated_percent"].as_f64().unwrap();
    assert!((accumulated - 4.1322).abs() < 0.0001, "{accumulated}");
    assert_eq!(last_year["accumulated"].as_array().unwrap().len(), 12);
    let cdi_year = &last_year["benchmarks"]["cdi"];
    assert!(cdi_year["return_percent"].as_f64().unwrap() > 0.0);
    assert_eq!(cdi_year["months"].as_array().unwrap().len(), 12);
    assert_eq!(
        last_year["benchmarks"]["ipca"]["return_percent"],
        Value::Null
    );
    let summary = &perf.body["months_summary"];
    assert_eq!(
        (summary["positive"].clone(), summary["negative"].clone()),
        (json!(1), json!(0))
    );
    assert_eq!(summary["best"], summary["worst"]);
    assert!((summary["best"]["return_percent"].as_f64().unwrap() - 4.1322).abs() < 0.0001);
    assert_eq!(
        summary["versus"][0],
        json!({ "code": "cdi", "label": "CDI", "above": 1, "compared": 1 })
    );
    assert_eq!(summary["versus"][1]["compared"], 0);

    // Por classe: cada uma pelo método de cotas, sem o aporte no meio.
    let classes = get(&app.router, "/api/v1/performance/classes?period=all").await;
    assert_eq!(classes.status, StatusCode::OK, "{}", classes.text);
    assert_eq!(
        classes.body["window"]["from"],
        Value::String(day1.to_string())
    );
    let rows = classes.body["classes"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{rows:?}");
    // FIIs: (2.250 − 740) / 1.420 = +6,338%; peso 2.250 / 3.260.
    assert_eq!(rows[0]["asset_class"], "br_reits");
    assert_eq!(rows[0]["market_value"], 2250.0);
    assert_eq!(rows[0]["return_percent"], 6.338);
    assert_eq!(rows[0]["weight_percent"], 69.0184);
    assert_eq!(rows[0]["full_window"], true);
    assert_eq!(rows[0]["benchmark"]["code"], "ifix");
    assert_eq!(rows[0]["benchmark"]["return_percent"], 1.0);
    // Renda Fixa: o CDB rendeu 10 sobre 1.000 (o ajuste é rendimento).
    assert_eq!(rows[1]["asset_class"], "fixed_income");
    assert_eq!(rows[1]["return_percent"], 1.0);
    assert_eq!(rows[1]["weight_percent"], 30.9816);
    assert_eq!(rows[1]["benchmark"]["code"], "cdi");
    assert!(rows[1]["benchmark"]["return_percent"].as_f64().unwrap() > 0.0);
    let ifix = get(
        &app.router,
        "/api/v1/performance?period=all&benchmarks=ifix",
    )
    .await;
    assert_eq!(ifix.body["benchmarks"][0]["return_percent"], 1.0);
    assert_eq!(ifix.body["benchmarks"][0]["label"], "IFIX");
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
    assert_eq!(perf.body["months_summary"]["best"], Value::Null);
    assert_eq!(worth.body["summary"], Value::Null);
    let classes = get(&other.router, "/api/v1/performance/classes?period=all").await;
    assert_eq!(classes.status, StatusCode::OK, "{}", classes.text);
    assert_eq!(classes.body["window"], Value::Null);
    assert_eq!(classes.body["classes"], json!([]));
}

async fn rejects_unknown_period_and_index(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(
        &db.store,
        "valida@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let bad_period = get(&app.router, "/api/v1/performance?period=10y").await;
    assert_eq!(bad_period.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_period.body["code"], "validation_failed");
    let bad_index = get(&app.router, "/api/v1/performance?benchmarks=cdi,ivvb11").await;
    assert_eq!(bad_index.status, StatusCode::UNPROCESSABLE_ENTITY);
    let bad_classes = get(&app.router, "/api/v1/performance/classes?period=semana").await;
    assert_eq!(bad_classes.status, StatusCode::UNPROCESSABLE_ENTITY);
    for period in ["2y", "5y"] {
        let ok = get(&app.router, &format!("/api/v1/performance?period={period}")).await;
        assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);
        assert_eq!(ok.body["period"], period);
    }
    let bad_worth = get(&app.router, "/api/v1/history/net-worth?period=semana").await;
    assert_eq!(bad_worth.status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Título do Tesouro (`TD-...`) não existe no Yahoo: o job não pede histórico
/// dele lá. Hoje vale o preço de resgate do dia guardado em `quotes`; o
/// passado vem do histórico do Tesouro, pedido uma vez só, e, sem ele, do que
/// a foto daquele dia guardou. O aporte nele é fluxo pela quantidade, não por
/// valor.
async fn treasury_title_is_valued_by_its_daily_quote(db: &Database, dir: &tempfile_dir::Dir) {
    let email = "tesouro-desempenho@exemplo.com";
    let symbol = "TD-IPCA-20350515";
    let quote = |price: Decimal| Quote {
        symbol: symbol.to_owned(),
        price,
        currency: "BRL".to_owned(),
        name: Some("Tesouro IPCA+ 2035".to_owned()),
        traded_at: None,
    };
    db.store
        .upsert_quote(&quote(dec!(3000)), OffsetDateTime::now_utc())
        .await
        .unwrap();
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let bond = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": symbol, "treasury": true, "quantity": 1.5, "score_override": 5 }),
    )
    .await;
    assert_eq!(bond.status, StatusCode::CREATED, "{}", bond.text);

    let history = Arc::new(FakeHistory::default());
    let bonds = Arc::new(FakeBonds::default());
    let job =
        Performance::new(db.store.clone(), history.clone()).with_treasury_history(bonds.clone());
    let day1_at = OffsetDateTime::now_utc() - Duration::days(1);
    let day1 = local_date(day1_at);
    backdate(&app.router, bond.body["id"].as_str().unwrap(), day1).await;
    job.run_daily(day1_at).await.unwrap();
    // O título sem cupom entra no risco por ativo: a tabela é pedida já no
    // dia 1, desde o começo da janela mais longa.
    let bond_key = Bond::from_symbol(symbol).unwrap();
    assert_eq!(
        bonds.asked.lock().unwrap().clone(),
        vec![(bond_key, history_start(day1))]
    );

    let id = bond.body["id"].as_str().unwrap();
    let contribution = post(
        &app.router,
        "/api/v1/contributions",
        json!({ "amount": 1500, "items": [{ "asset_id": id, "quantity": 0.5, "unit_price": 3000 }] }),
    )
    .await;
    assert_eq!(
        contribution.status,
        StatusCode::CREATED,
        "{}",
        contribution.text
    );
    db.store
        .upsert_quote(&quote(dec!(3010)), OffsetDateTime::now_utc())
        .await
        .unwrap();
    job.run_daily(OffsetDateTime::now_utc()).await.unwrap();

    let rows = snapshot_rows(db, email).await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!((rows[0].1, rows[0].2), (dec!(4500), dec!(4500)));
    // 2 × 3.010; o fluxo é o aporte (0,5 × 3.000); o resto é a alta do título.
    assert_eq!((rows[1].1, rows[1].2), (dec!(6020), dec!(1500)));
    assert_eq!(rows[1].3, (dec!(4520) / dec!(4500)).round_dp(12));

    let asked = history.requests.lock().unwrap().clone();
    assert!(
        asked.iter().all(|(s, _, _)| !s.starts_with("TD-")),
        "{asked:?}"
    );
    // Uma vez só: a fonte não tinha nada, e a marca de cobertura evita pedir
    // de novo.
    assert_eq!(bonds.asked.lock().unwrap().len(), 1);
    job.run_daily(OffsetDateTime::now_utc()).await.unwrap();
    assert_eq!(bonds.asked.lock().unwrap().len(), 1);
    assert_eq!(snapshot_rows(db, email).await.len(), 2);
}
