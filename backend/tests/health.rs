//! Saúde da carteira e setor, contra um Postgres de verdade: a referência de
//! setor (busca, prazo, falha), o setor no ativo, os avisos e a configuração
//! das regras. Um contêiner para o arquivo inteiro; cada cenário tem seu e-mail.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::domain::fund_issuer::FundClaim;
use prumo::domain::sector::{FundComposition, SectorSource};
use prumo::sectors::{SectorEntry, SectorError, SectorFeed, SectorService};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use time::{Duration, OffsetDateTime};

/// Fonte de setor em memória: devolve o que o teste mandar e conta as buscas.
struct FakeFeed {
    source: SectorSource,
    answer: Mutex<Result<Vec<SectorEntry>, SectorError>>,
    calls: AtomicUsize,
}

impl FakeFeed {
    fn new(source: SectorSource, entries: Vec<SectorEntry>) -> Arc<Self> {
        Arc::new(Self {
            source,
            answer: Mutex::new(Ok(entries)),
            calls: AtomicUsize::new(0),
        })
    }

    fn answer(&self, answer: Result<Vec<SectorEntry>, SectorError>) {
        *self.answer.lock().unwrap() = answer;
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SectorFeed for FakeFeed {
    fn source(&self) -> SectorSource {
        self.source
    }

    async fn fetch(&self) -> Result<Vec<SectorEntry>, SectorError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer.lock().unwrap().clone()
    }
}

fn entry(code: &str, sector: &str, subsector: Option<&str>, segment: Option<&str>) -> SectorEntry {
    SectorEntry {
        code: code.to_owned(),
        sector: sector.to_owned(),
        subsector: subsector.map(str::to_owned),
        segment: segment.map(str::to_owned),
        composition: None,
        fund: None,
    }
}

fn b3_entries() -> Vec<SectorEntry> {
    vec![
        entry(
            "ITUB",
            "Financeiro",
            Some("Intermediários Financeiros"),
            Some("Bancos"),
        ),
        entry(
            "BBAS",
            "Financeiro",
            Some("Intermediários Financeiros"),
            Some("Bancos"),
        ),
        entry(
            "WEGE",
            "Bens Industriais",
            Some("Máquinas e Equipamentos"),
            Some("Motores, Compressores e Outros"),
        ),
    ]
}

/// Um FII como a fonte da CVM de hoje grava: o tipo, a composição da carteira
/// e o fundo de origem (CNPJ fictício).
fn fund(code: &str, sector: &str, segment: &str) -> SectorEntry {
    let (brick, paper) = if sector == "Papel" { (5, 95) } else { (90, 10) };
    let n: u32 = code.bytes().map(u32::from).sum::<u32>() % 100;
    SectorEntry {
        composition: FundComposition::from_amounts(
            Decimal::from(brick),
            Decimal::from(paper),
            Decimal::ZERO,
        ),
        fund: Some(FundClaim {
            cnpj: format!("{n:02}.000.000/0001-{n:02}"),
            first_reported: time::macros::date!(2020 - 01 - 01),
            last_reported: time::macros::date!(2026 - 10 - 01),
        }),
        ..entry(code, sector, None, Some(segment))
    }
}

fn cvm_entries() -> Vec<SectorEntry> {
    vec![
        fund("KNRI", "Tijolo", "Multicategoria"),
        fund("HGLG", "Tijolo", "Multicategoria"),
        fund("MXRF", "Papel", "Logística"),
    ]
}

fn quotes() -> Arc<FakeQuotes> {
    let quotes = FakeQuotes::with_market();
    {
        let mut prices = quotes.prices.lock().unwrap();
        prices.insert(
            "ITUB4.SA".into(),
            ("30.00".parse().unwrap(), "BRL", "Itau Unibanco"),
        );
        prices.insert(
            "BBAS3.SA".into(),
            ("20.00".parse().unwrap(), "BRL", "Banco do Brasil"),
        );
    }
    quotes
}

fn static_dir() -> tempfile_dir::Dir {
    tempfile_dir::Dir::new()
}

#[tokio::test]
async fn health_and_sector_contract() {
    let db = database().await;
    let dir = static_dir();
    let b3 = FakeFeed::new(SectorSource::B3Stocks, b3_entries());
    let cvm = FakeFeed::new(SectorSource::CvmReits, cvm_entries());
    let service = SectorService::new(
        db.store.clone(),
        vec![
            Arc::clone(&b3) as Arc<dyn SectorFeed>,
            Arc::clone(&cvm) as Arc<dyn SectorFeed>,
        ],
    );

    an_empty_portfolio_has_nothing_to_warn_about(&db, &dir).await;
    the_reference_is_fetched_once_and_kept_until_it_is_due(&db, &service, &b3, &cvm).await;
    assets_carry_their_sector(&db, &dir).await;
    warnings_name_the_number_the_limit_and_the_fix(&db, &dir).await;
    rules_can_be_turned_off_and_tuned(&db, &dir).await;
    rules_are_validated(&db, &dir).await;
    each_person_has_their_own_rules(&db, &dir).await;
    a_source_that_fails_keeps_the_last_good_reference(&db, &dir, &service, &b3).await;
}

async fn build_portfolio(app: &TestApp) {
    for (class, symbol, quantity) in [
        ("br_stocks", "ITUB4", 100),
        ("br_stocks", "BBAS3", 100),
        ("br_stocks", "WEGE3", 20),
        ("br_reits", "KNRI11", 10),
        ("br_reits", "HGLG11", 10),
        ("br_reits", "MXRF11", 100),
    ] {
        let created = post(
            &app.router,
            "/api/v1/assets",
            json!({ "asset_class": class, "symbol": symbol, "quantity": quantity }),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{symbol}");
    }
    let goals = put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 50, "br_reits": 40, "crypto": 10 } }),
    )
    .await;
    assert_eq!(goals.status, StatusCode::OK);
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

async fn an_empty_portfolio_has_nothing_to_warn_about(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(&db.store, "vazia@prumo.test", quotes(), &dir.path());
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(health.body["total_value"], 0.0);
    assert_eq!(health.body["warnings"], json!([]));
    assert_eq!(health.body["unclassified_count"], 0);
    let rules = health.body["rules"].as_array().unwrap();
    let ids: Vec<&str> = rules.iter().map(|r| r["rule"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [
            "max_asset",
            "top_five",
            "max_sector",
            "min_assets",
            "empty_class",
            "income_every_month",
            "max_payer"
        ]
    );
    assert!(rules.iter().all(|r| r["status"] == "not_applicable"));
    assert!(rules.iter().all(|r| r["enabled"] == true));
    // Nenhuma fonte foi buscada ainda: setor vazio, sem falha.
    let sources = health.body["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0]["source"], "b3_stocks");
    assert_eq!(sources[0]["asset_class"], "br_stocks");
    assert_eq!(sources[0]["updated_at"], Value::Null);
    assert_eq!(sources[0]["failing"], false);
    assert_eq!(sources[1]["source"], "cvm_reits");

    let defaults = get(&app.router, "/api/v1/health/rules").await;
    assert_eq!(defaults.status, StatusCode::OK);
    let rules = &defaults.body["rules"];
    assert_eq!(find(rules, "rule", "max_asset")["threshold"], 15.0);
    assert_eq!(find(rules, "rule", "top_five")["threshold"], 60.0);
    assert_eq!(find(rules, "rule", "max_sector")["threshold"], 40.0);
    assert_eq!(find(rules, "rule", "min_assets")["threshold"], 3.0);
    assert_eq!(find(rules, "rule", "min_assets")["unit"], "count");
    assert_eq!(find(rules, "rule", "empty_class")["threshold"], Value::Null);
    assert_eq!(find(rules, "rule", "empty_class")["unit"], Value::Null);
}

async fn the_reference_is_fetched_once_and_kept_until_it_is_due(
    db: &Database,
    service: &SectorService,
    b3: &Arc<FakeFeed>,
    cvm: &Arc<FakeFeed>,
) {
    let now = OffsetDateTime::now_utc();
    let first = service.refresh_due(now).await;
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|o| o.result.is_ok()));
    assert_eq!(first[0].result, Ok(3));
    assert_eq!((b3.calls(), cvm.calls()), (1, 1));

    // Um minuto depois nada vence; a fonte não é incomodada.
    let again = service.refresh_due(now + Duration::minutes(1)).await;
    assert!(again.is_empty());
    assert_eq!((b3.calls(), cvm.calls()), (1, 1));

    let state = db
        .store
        .sector_sync_state(SectorSource::B3Stocks)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.entries, 3);
    assert!(state.last_success_at.is_some());
    assert_eq!(state.last_error, None);

    // Passado o mês, busca de novo e troca a referência (inclusive tirando o
    // que a fonte deixou de listar).
    cvm.answer(Ok(vec![
        fund("KNRI", "Papel", "Multicategoria"),
        fund("HGLG", "Tijolo", "Multicategoria"),
    ]));
    let later = service.refresh_due(now + Duration::days(31)).await;
    assert_eq!(later.len(), 2);
    assert_eq!(b3.calls(), 2);
    assert_eq!(
        db.store
            .sector_reference(&[
                (SectorSource::CvmReits, "KNRI".into()),
                (SectorSource::CvmReits, "MXRF".into()),
            ])
            .await
            .unwrap()
            .into_iter()
            .map(|((_, code), info)| (code, info.sector))
            .collect::<Vec<_>>(),
        vec![("KNRI".to_owned(), "Papel".to_owned())]
    );
    // Volta ao que o resto dos cenários espera.
    cvm.answer(Ok(cvm_entries()));
    let refreshed = service.refresh_due(now + Duration::days(62)).await;
    assert_eq!(refreshed.len(), 2);

    // Uma referência gravada por um código de antes da composição (um pod
    // antigo durante o deploy) é lida de novo mesmo em dia, só até ficar
    // completa: a condição é o dado, não uma marca da migração.
    let old_code: Vec<SectorEntry> = cvm_entries()
        .into_iter()
        .map(|e| SectorEntry {
            composition: None,
            ..e
        })
        .collect();
    db.store
        .replace_sector_reference(SectorSource::CvmReits, &old_code, now + Duration::days(62))
        .await
        .unwrap();
    assert!(db
        .store
        .sector_reference_incomplete(SectorSource::CvmReits)
        .await
        .unwrap());
    let calls = (b3.calls(), cvm.calls());
    let backfill = service
        .refresh_due(now + Duration::days(62) + Duration::minutes(5))
        .await;
    assert_eq!(backfill.len(), 1, "{backfill:?}");
    assert_eq!(backfill[0].source, SectorSource::CvmReits);
    assert_eq!((b3.calls(), cvm.calls()), (calls.0, calls.1 + 1));
    assert!(!db
        .store
        .sector_reference_incomplete(SectorSource::CvmReits)
        .await
        .unwrap());
    let done = service
        .refresh_due(now + Duration::days(62) + Duration::minutes(10))
        .await;
    assert!(done.is_empty(), "{done:?}");
}

async fn assets_carry_their_sector(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(&db.store, "setor@prumo.test", quotes(), &dir.path());
    build_portfolio(&app).await;
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let assets = &portfolio.body["assets"];

    let itub = find(assets, "symbol", "ITUB4");
    assert_eq!(itub["sector"], "Financeiro");
    assert_eq!(itub["subsector"], "Intermediários Financeiros");
    assert_eq!(itub["segment"], "Bancos");
    let knri = find(assets, "symbol", "KNRI11");
    assert_eq!(knri["sector"], "Tijolo");
    assert_eq!(knri["subsector"], Value::Null);
    assert_eq!(knri["segment"], "Multicategoria");
    assert_eq!(find(assets, "symbol", "MXRF11")["sector"], "Papel");

    // ITUB3 e ITUB4 dividem o código do emissor: os dois acham o setor.
    let sibling = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "ITUB3", "quantity": 0 }),
    )
    .await;
    assert_eq!(sibling.body["sector"], "Financeiro");

    // Sem fonte (EUA) e fora da referência: nulos, sem erro.
    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 2 }),
    )
    .await;
    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "TRXF11", "quantity": 5 }),
    )
    .await;
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let aapl = find(&portfolio.body["assets"], "symbol", "AAPL");
    assert_eq!(aapl["sector"], Value::Null);
    assert_eq!(aapl["segment"], Value::Null);
    let trxf = find(&portfolio.body["assets"], "symbol", "TRXF11");
    assert_eq!(trxf["sector"], Value::Null);

    // O ativo avulso (PATCH) volta com o setor também.
    let id = itub["id"].as_str().unwrap();
    let patched = patch(
        &app.router,
        &format!("/api/v1/assets/{id}"),
        json!({ "quantity": 100 }),
    )
    .await;
    assert_eq!(patched.body["sector"], "Financeiro");

    // O setor é de todos: outra pessoa com o mesmo papel também o recebe.
    let other = common::app(&db.store, "outra@prumo.test", quotes(), &dir.path());
    post(
        &other.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "BBAS3", "quantity": 1 }),
    )
    .await;
    let theirs = get(&other.router, "/api/v1/portfolio").await;
    assert_eq!(theirs.body["assets"][0]["sector"], "Financeiro");
}

async fn warnings_name_the_number_the_limit_and_the_fix(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(&db.store, "avisos@prumo.test", quotes(), &dir.path());
    build_portfolio(&app).await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(health.status, StatusCode::OK);
    let body = &health.body;
    // 3000 + 2000 + 1000 + 1420 + 1408 + 837 = 9665
    assert_eq!(body["total_value"], 9665.0);

    let max_asset = warnings_of(body, "max_asset");
    let labels: Vec<&str> = max_asset
        .iter()
        .map(|w| w["subject"]["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["ITUB4", "BBAS3"]);
    let itub = max_asset[0];
    assert_eq!(itub["subject"]["kind"], "asset");
    assert_eq!(itub["unit"], "percent");
    assert_eq!(itub["current"], 31.04);
    assert_eq!(itub["limit"], 15.0);
    // 3000 / (9665 + x) = 0,15  =>  x = 10335
    assert_eq!(itub["needed_contribution"], 10335.0);

    let top = warnings_of(body, "top_five");
    assert_eq!(top.len(), 1);
    assert_eq!(top[0]["subject"]["kind"], "top_assets");
    assert_eq!(top[0]["subject"]["labels"].as_array().unwrap().len(), 5);
    assert_eq!(top[0]["current"], 91.34);

    let sectors = warnings_of(body, "max_sector");
    assert_eq!(sectors.len(), 2);
    assert_eq!(sectors[0]["subject"]["kind"], "sector");
    assert_eq!(sectors[0]["subject"]["asset_class"], "br_stocks");
    assert_eq!(sectors[0]["subject"]["sector"], "Financeiro");
    assert_eq!(sectors[0]["current"], 83.33);
    assert_eq!(sectors[1]["subject"]["sector"], "Tijolo");

    // Cada classe com meta tem três ativos: `min_assets` fica quieta.
    assert!(warnings_of(body, "min_assets").is_empty());
    assert_eq!(rule_status(body, "min_assets"), "ok");
    let empty = warnings_of(body, "empty_class");
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0]["subject"]["asset_class"], "crypto");
    assert_eq!(empty[0]["current"], 0.0);
    assert_eq!(empty[0]["limit"], 1.0);
    assert_eq!(empty[0]["needed_contribution"], Value::Null);
    assert_eq!(rule_status(body, "max_asset"), "warning");
    assert_eq!(body["unclassified_count"], 0);

    // FII fora da referência conta como sem setor.
    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "TRXF11", "quantity": 5 }),
    )
    .await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(health.body["unclassified_count"], 1);

    // As duas fontes buscadas: a data de cada uma aparece.
    let sources = health.body["sources"].as_array().unwrap();
    assert!(sources.iter().all(|s| s["updated_at"].is_string()));
    assert!(sources.iter().all(|s| s["failing"] == false));
}

async fn rules_can_be_turned_off_and_tuned(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(&db.store, "regras@prumo.test", quotes(), &dir.path());
    build_portfolio(&app).await;

    let saved = put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": {
            "max_asset": { "enabled": false },
            "top_five": { "threshold": 95 },
            "max_sector": { "enabled": true, "threshold": 90.5 },
            "min_assets": { "threshold": 4 },
            "empty_class": { "enabled": false },
        }}),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK);
    let rules = &saved.body["rules"];
    assert_eq!(find(rules, "rule", "max_asset")["enabled"], false);
    // Só `enabled`: o limite continua o de antes.
    assert_eq!(find(rules, "rule", "max_asset")["threshold"], 15.0);
    assert_eq!(find(rules, "rule", "top_five")["threshold"], 95.0);
    assert_eq!(find(rules, "rule", "top_five")["enabled"], true);
    assert_eq!(find(rules, "rule", "max_sector")["threshold"], 90.5);
    assert_eq!(find(rules, "rule", "min_assets")["threshold"], 4.0);
    assert_eq!(find(rules, "rule", "empty_class")["enabled"], false);
    // O padrão de fábrica continua à vista para a tela oferecer "restaurar".
    assert_eq!(find(rules, "rule", "top_five")["default_threshold"], 60.0);

    let again = get(&app.router, "/api/v1/health/rules").await;
    assert_eq!(again.body, saved.body);

    let health = get(&app.router, "/api/v1/health").await;
    let body = &health.body;
    assert_eq!(rule_status(body, "max_asset"), "off");
    assert!(warnings_of(body, "max_asset").is_empty());
    // 91,34% < 95: dentro do novo limite.
    assert_eq!(rule_status(body, "top_five"), "ok");
    // Financeiro 83,33% e Tijolo 77,16% < 90,5.
    assert_eq!(rule_status(body, "max_sector"), "ok");
    // Com mínimo 4, as duas classes de três ativos avisam.
    let shallow = warnings_of(body, "min_assets");
    assert_eq!(shallow.len(), 2);
    assert_eq!(shallow[0]["current"], 3.0);
    assert_eq!(shallow[0]["limit"], 4.0);
    assert_eq!(shallow[0]["unit"], "count");
    assert_eq!(rule_status(body, "empty_class"), "off");

    // Religar uma regra desligada.
    let back = put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "max_asset": { "enabled": true } } }),
    )
    .await;
    assert_eq!(
        find(&back.body["rules"], "rule", "max_asset")["enabled"],
        true
    );
    // O que não veio no PUT não mudou.
    assert_eq!(
        find(&back.body["rules"], "rule", "top_five")["threshold"],
        95.0
    );
}

async fn rules_are_validated(db: &Database, dir: &tempfile_dir::Dir) {
    let app = app(&db.store, "valida@prumo.test", quotes(), &dir.path());
    let bad = [
        json!({ "rules": {} }),
        json!({ "rules": { "max_asset": {} } }),
        json!({ "rules": { "max_asset": { "threshold": 0 } } }),
        json!({ "rules": { "max_asset": { "threshold": 101 } } }),
        json!({ "rules": { "max_asset": { "threshold": -5 } } }),
        json!({ "rules": { "max_asset": { "threshold": 12.345 } } }),
        json!({ "rules": { "min_assets": { "threshold": 2.5 } } }),
        json!({ "rules": { "min_assets": { "threshold": 0 } } }),
        json!({ "rules": { "empty_class": { "threshold": 1 } } }),
        json!({ "rules": { "nota_final": { "enabled": true } } }),
        json!({ "rules": { "max_asset": { "enabled": "sim" } } }),
        json!({ "rules": { "max_asset": { "enabled": true, "peso": 3 } } }),
        json!({ "regras": {} }),
        json!([1]),
    ];
    for body in bad {
        let reply = put(&app.router, "/api/v1/health/rules", body.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(reply.body["code"], "validation_failed", "{body}");
    }
    // Um pedido com uma regra boa e uma ruim não grava nenhuma.
    let mixed = put(
        &app.router,
        "/api/v1/health/rules",
        json!({ "rules": {
            "top_five": { "threshold": 70 },
            "max_asset": { "threshold": 0 },
        }}),
    )
    .await;
    assert_eq!(mixed.status, StatusCode::UNPROCESSABLE_ENTITY);
    let rules = get(&app.router, "/api/v1/health/rules").await;
    assert_eq!(
        find(&rules.body["rules"], "rule", "top_five")["threshold"],
        60.0
    );
}

async fn each_person_has_their_own_rules(db: &Database, dir: &tempfile_dir::Dir) {
    let mine = app(&db.store, "minha@prumo.test", quotes(), &dir.path());
    let theirs = app(&db.store, "dela@prumo.test", quotes(), &dir.path());
    put(
        &mine.router,
        "/api/v1/health/rules",
        json!({ "rules": { "top_five": { "threshold": 99, "enabled": false } } }),
    )
    .await;
    let other = get(&theirs.router, "/api/v1/health/rules").await;
    let top = find(&other.body["rules"], "rule", "top_five");
    assert_eq!(top["threshold"], 60.0);
    assert_eq!(top["enabled"], true);
    // Os avisos de uma carteira não aparecem na outra.
    build_portfolio(&mine).await;
    let health = get(&theirs.router, "/api/v1/health").await;
    assert_eq!(health.body["total_value"], 0.0);
    assert_eq!(health.body["warnings"], json!([]));
}

async fn a_source_that_fails_keeps_the_last_good_reference(
    db: &Database,
    dir: &tempfile_dir::Dir,
    service: &SectorService,
    b3: &Arc<FakeFeed>,
) {
    let app = app(&db.store, "fora@prumo.test", quotes(), &dir.path());
    post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "ITUB4", "quantity": 1 }),
    )
    .await;
    let now = OffsetDateTime::now_utc() + Duration::days(100);

    b3.answer(Err(SectorError::Unavailable("status 503".into())));
    let outcomes = service.refresh_due(now).await;
    let b3_outcome = outcomes
        .iter()
        .find(|o| o.source == SectorSource::B3Stocks)
        .unwrap();
    assert!(b3_outcome.result.as_ref().unwrap_err().contains("503"));

    // O setor que já estava gravado continua; a tela só fica sabendo que a
    // fonte está falhando.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(portfolio.body["assets"][0]["sector"], "Financeiro");
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(health.body["sources"][0]["failing"], true);
    assert!(health.body["sources"][0]["updated_at"].is_string());
    assert_eq!(health.body["sources"][1]["failing"], false);

    // Logo depois da falha, não tenta de novo.
    let calls = b3.calls();
    assert!(service
        .refresh_due(now + Duration::minutes(10))
        .await
        .is_empty());
    assert_eq!(b3.calls(), calls);

    // Resposta vazia e queda brusca de linhas não trocam uma referência boa.
    b3.answer(Ok(vec![]));
    let later = now + Duration::hours(7);
    let empty = service.refresh_due(later).await;
    assert!(empty
        .iter()
        .any(|o| o.source == SectorSource::B3Stocks && o.result.is_err()));
    let before = db
        .store
        .sector_sync_state(SectorSource::B3Stocks)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.entries, 3);
    assert!(before.last_error.is_some());

    let mut big = b3_entries();
    for i in 0..3 {
        big.push(entry(&format!("ZZ{i}A"), "Outros", None, None));
    }
    // A referência boa tem 6 linhas agora; uma resposta de 1 é suspeita.
    b3.answer(Ok(big));
    let ok = service.refresh_due(later + Duration::hours(7)).await;
    assert!(ok
        .iter()
        .any(|o| o.source == SectorSource::B3Stocks && o.result == Ok(6)));
    b3.answer(Ok(vec![entry("ITUB", "Financeiro", None, None)]));
    let drop = service.refresh_due(later + Duration::days(40)).await;
    let outcome = drop
        .iter()
        .find(|o| o.source == SectorSource::B3Stocks)
        .unwrap();
    assert!(outcome
        .result
        .as_ref()
        .unwrap_err()
        .contains("queda suspeita"));
    let kept = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(kept.body["assets"][0]["segment"], "Bancos");

    // A fonte voltou: tudo normal e sem marca de falha.
    b3.answer(Ok(b3_entries()));
    service.refresh_due(later + Duration::days(80)).await;
    let health = get(&app.router, "/api/v1/health").await;
    assert_eq!(health.body["sources"][0]["failing"], false);
}
