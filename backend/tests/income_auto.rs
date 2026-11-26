//! Proventos automáticos contra a amostra real da B3 (wiremock) e um Postgres
//! de verdade: a rodada idempotente, duas réplicas ao mesmo tempo, o provento
//! lançado à mão e o importado que vencem, o livro que muda no passado, a
//! chave por pessoa, o dono, o caixa, o alerta e a migração 0018.
//!
//! As páginas em `fixtures/b3_announcements/` são respostas reais da B3,
//! gravadas em 26/11/2026 (ITSA e KNRI: dados públicos de emissores). As
//! carteiras são fictícias: quantidades e datas de compra inventadas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::alerts::AlertService;
use prumo::domain::income_auto::{CashAnnouncement, StockAnnouncement, StockEventKind, Venue};
use prumo::income_auto::{
    detail_query, encoded, supplement_query, AnnouncementSource, B3Listed, IncomeAutoService,
    Listing, SourceError, Supplement,
};
use prumo::performance::BRASILIA;
use prumo::store::Store;
use prumo::telegram::{TelegramApi, TelegramError, Update};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::PgPool;
use time::macros::{date, time};
use time::{Date, OffsetDateTime};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ------------------------------------------------------------- amostras --

fn fixture(name: &str) -> Value {
    let text = match name {
        "itsa_supplement" => include_str!("fixtures/b3_announcements/itsa_supplement.json"),
        "itsa_detail" => include_str!("fixtures/b3_announcements/itsa_detail.json"),
        "knri_supplement" => include_str!("fixtures/b3_announcements/knri_supplement.json"),
        other => panic!("amostra desconhecida: {other}"),
    };
    serde_json::from_str(text).unwrap()
}

const COMPANY: &str = "/listedCompaniesProxy/CompanyCall/GetListedSupplementCompany/";
const FUND: &str = "/fundsProxy/fundsCall/GetListedSupplementFunds/";
const DETAIL: &str = "/listedCompaniesProxy/CompanyCall/GetDetail/";

/// A B3 de mentira: as páginas reais de ITSA e KNRI e o cadastro de ITSA.
async fn b3() -> MockServer {
    let server = MockServer::start().await;
    for (prefix, query, body) in [
        (
            COMPANY,
            supplement_query("ITSA", Venue::Company),
            fixture("itsa_supplement"),
        ),
        (
            FUND,
            supplement_query("KNRI", Venue::Fund),
            fixture("knri_supplement"),
        ),
        (DETAIL, detail_query("7617"), fixture("itsa_detail")),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("{prefix}{}", encoded(&query))))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    server
}

fn listed(server: &MockServer) -> B3Listed {
    B3Listed::with_base(reqwest::Client::new(), &format!("{}/", server.uri()))
}

// ------------------------------------------------------------- o cliente --

#[tokio::test]
async fn the_client_reads_the_recorded_b3_pages() {
    let server = b3().await;
    let source = listed(&server);

    let itsa = source.supplement("ITSA", Venue::Company).await.unwrap();
    assert_eq!(itsa.code_cvm.as_deref(), Some("7617"));
    assert_eq!(itsa.cash.len(), 16);
    let jcp = itsa
        .cash
        .iter()
        .find(|c| c.isin == "BRITSAACNPR7" && c.record_date == date!(2026 - 08 - 31))
        .unwrap();
    assert_eq!(jcp.label, "JRS CAP PROPRIO");
    assert_eq!(jcp.rate, dec!(0.0242425));
    assert_eq!(jcp.pay_date, Some(date!(2026 - 10 - 01)));
    assert_eq!(jcp.approved_on, Some(date!(2026 - 02 - 09)));
    assert_eq!(jcp.related_to.as_deref(), Some("3º Trimestre/2026"));
    let dividend = itsa
        .cash
        .iter()
        .find(|c| c.isin == "BRITSAACNPR7" && c.label == "DIVIDENDO")
        .unwrap();
    assert_eq!(dividend.rate, dec!(0.775364));
    assert_eq!(dividend.record_date, date!(2025 - 12 - 09));
    // Eventos em ações: as duas bonificações e os dois resgates antigos.
    assert_eq!(itsa.stock.len(), 4);
    let bonus = itsa
        .stock
        .iter()
        .find(|s| s.isin == "BRITSAACNPR7")
        .unwrap();
    assert_eq!(
        (bonus.label.as_str(), bonus.factor, bonus.record_date),
        ("BONIFICACAO", dec!(2), date!(2025 - 12 - 18))
    );

    let listings = source.listings("7617").await.unwrap();
    assert_eq!(
        listings,
        vec![
            Listing {
                ticker: "ITSA3".into(),
                isin: "BRITSAACNOR0".into()
            },
            Listing {
                ticker: "ITSA4".into(),
                isin: "BRITSAACNPR7".into()
            },
        ]
    );

    let knri = source.supplement("KNRI", Venue::Fund).await.unwrap();
    assert_eq!(knri.code_cvm, None);
    assert_eq!(knri.cash.len(), 12);
    assert!(knri
        .cash
        .iter()
        .all(|c| c.isin == "BRKNRICTF007" && c.label == "RENDIMENTO"));
    let november = knri
        .cash
        .iter()
        .find(|c| c.record_date == date!(2026 - 10 - 30))
        .unwrap();
    assert_eq!(november.rate, dec!(1.1));
    assert_eq!(november.pay_date, Some(date!(2026 - 11 - 16)));

    // A página que não existe (404 do mock) e o emissor inválido.
    assert!(matches!(
        source.supplement("ZZZZ", Venue::Company).await,
        Err(SourceError::Unavailable(_))
    ));
    assert_eq!(
        source.supplement("itsa", Venue::Company).await,
        Err(SourceError::InvalidIssuer)
    );
    assert_eq!(source.listings("").await, Ok(Vec::new()));
}

// ------------------------------------------------------------ montagem --

/// Uma fonte em memória, para datas que o teste controla.
#[derive(Default)]
struct FakeSource {
    pages: Mutex<HashMap<String, Supplement>>,
    calls: AtomicUsize,
}

impl FakeSource {
    fn set(&self, issuer: &str, cash: Vec<CashAnnouncement>, stock: Vec<StockAnnouncement>) {
        self.pages.lock().unwrap().insert(
            issuer.to_owned(),
            Supplement {
                code_cvm: None,
                cash,
                stock,
            },
        );
    }
}

#[async_trait]
impl AnnouncementSource for FakeSource {
    async fn supplement(&self, issuer: &str, _venue: Venue) -> Result<Supplement, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // Um pouco de espera: duas réplicas se cruzam no meio da busca.
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.pages
            .lock()
            .unwrap()
            .get(issuer)
            .cloned()
            .ok_or_else(|| SourceError::Unavailable("sem pagina".into()))
    }

    async fn listings(&self, _code_cvm: &str) -> Result<Vec<Listing>, SourceError> {
        Ok(Vec::new())
    }
}

const FICT_PN: &str = "BRFICTACNPR1";

fn cash(record: Date, label: &str, rate: Decimal, pay: Option<Date>) -> CashAnnouncement {
    CashAnnouncement {
        issuer: "FICT".into(),
        isin: FICT_PN.into(),
        label: label.into(),
        record_date: record,
        rate,
        pay_date: pay,
        approved_on: None,
        related_to: None,
    }
}

fn service(store: &Store, source: Arc<dyn AnnouncementSource>) -> IncomeAutoService {
    IncomeAutoService::new(store.clone(), source).with_pause(Duration::ZERO)
}

fn now() -> OffsetDateTime {
    date!(2026 - 06 - 15)
        .with_time(time!(12:00))
        .assume_offset(BRASILIA)
}

struct Person {
    app: TestApp,
    user: Uuid,
    _dir: tempfile_dir::Dir,
}

async fn person(db: &Database, email: &str) -> Person {
    let dir = tempfile_dir::Dir::new();
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let user = db.store.ensure_user(email).await.unwrap();
    Person {
        app,
        user,
        _dir: dir,
    }
}

async fn add_asset(p: &Person, class: &str, symbol: &str) -> String {
    let reply = post(
        &p.app.router,
        "/api/v1/assets",
        json!({ "asset_class": class, "symbol": symbol, "quantity": 0 }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

async fn trade(p: &Person, asset: &str, kind: &str, day: Date, quantity: u32) -> String {
    let reply = post(
        &p.app.router,
        "/api/v1/transactions",
        json!({ "asset_id": asset, "kind": kind, "trade_date": day.to_string(), "quantity": quantity, "unit_price": 10 }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

async fn income_list(p: &Person) -> Vec<Value> {
    let reply = get(&p.app.router, "/api/v1/income?limit=1000").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply.body.as_array().unwrap().clone()
}

fn automatic(list: &[Value]) -> Vec<&Value> {
    list.iter()
        .filter(|e| e["source"] == "b3_announcement")
        .collect()
}

fn num(value: &Value) -> Decimal {
    value.as_f64().map_or(Decimal::ZERO, |v| {
        Decimal::try_from(v).unwrap().round_dp(8).normalize()
    })
}

// --------------------------------------------- a rodada, de ponta a ponta --

#[tokio::test]
async fn a_round_fetches_once_writes_once_and_is_idempotent() {
    let db = database().await;
    let server = b3().await;
    let ana = person(&db, "auto-ana@prumo.test").await;
    let itsa = add_asset(&ana, "br_stocks", "ITSA4").await;
    let knri = add_asset(&ana, "br_reits", "KNRI11").await;
    trade(&ana, &itsa, "buy", date!(2025 - 11 - 03), 100).await;
    trade(&ana, &knri, "buy", date!(2025 - 11 - 03), 10).await;
    // Vendeu tudo de KNRI11 em 15/06/2026: data com a partir daí, sem cota.
    trade(&ana, &knri, "sell", date!(2026 - 06 - 15), 10).await;

    let auto = service(&db.store, Arc::new(listed(&server)));
    let first = auto.round(now()).await;
    assert_eq!((first.fetched, first.failed), (2, 0));
    // ITSA4: os 8 proventos da PN (todos depois da compra). KNRI11: as 7 datas com
    // de 28/11/2025 a 29/05/2026.
    assert_eq!(first.inserted, 15, "{first:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    let list = income_list(&ana).await;
    let autos = automatic(&list);
    assert_eq!(autos.len(), 15);
    let jcp = autos
        .iter()
        .find(|e| e["external_ref"] == "b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425")
        .unwrap();
    assert_eq!(jcp["kind"], "jcp");
    assert_eq!(jcp["symbol"], "ITSA4");
    assert_eq!(jcp["ex_date"], "2026-09-01");
    assert_eq!(jcp["pay_date"], "2026-10-01");
    assert_eq!(num(&jcp["quantity"]), dec!(100));
    // 0,0242425 × 100 = 2,42; JCP pago em 2026: 17,5% retido.
    assert_eq!(
        (
            num(&jcp["gross"]),
            num(&jcp["withholding"]),
            num(&jcp["net"])
        ),
        (dec!(2.42), dec!(0.42), dec!(2))
    );
    let dividend = autos.iter().find(|e| e["kind"] == "dividend").unwrap();
    assert_eq!(dividend["pay_date"], "2025-12-19");
    assert_eq!(num(&dividend["gross"]), dec!(77.54));
    assert_eq!(num(&dividend["withholding"]), Decimal::ZERO);
    let rents: Vec<&&Value> = autos.iter().filter(|e| e["symbol"] == "KNRI11").collect();
    assert_eq!(rents.len(), 7);
    assert!(rents
        .iter()
        .all(|e| e["kind"] == "reit_income" && num(&e["quantity"]) == dec!(10)));

    // De novo: a B3 não é chamada (busca diária) e nada muda.
    let second = auto.round(now() + time::Duration::hours(1)).await;
    assert_eq!(
        (
            second.fetched,
            second.inserted,
            second.updated,
            second.deleted
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert_eq!(automatic(&income_list(&ana).await).len(), 15);
    // No dia seguinte a busca volta à B3, e a mesma página não muda nada.
    let next_day = auto.round(now() + time::Duration::hours(25)).await;
    assert_eq!((next_day.fetched, next_day.inserted), (2, 0));

    // A tela de ajustes: ligado, os dois cobertos, e a bonificação de 2% de
    // ITSA4 (data com 18/12/2025) como pendência.
    let screen = get(&ana.app.router, "/api/v1/income/auto").await;
    assert_eq!(screen.status, StatusCode::OK, "{}", screen.text);
    assert_eq!(screen.body["enabled"], true);
    assert_eq!(screen.body["covered"], json!(["ITSA4", "KNRI11"]));
    assert_eq!(screen.body["unmatched"], json!([]));
    assert!(screen.body["fetched_at"].is_string());
    let state = db
        .store
        .income_auto_state(ana.user, date!(2026 - 06 - 15))
        .await
        .unwrap();
    assert_eq!(state.pending.len(), 1);
    let pending = &state.pending[0];
    assert_eq!(pending.symbol, "ITSA4");
    assert_eq!(pending.kind, StockEventKind::Bonus);
    assert_eq!(pending.factor, dec!(2));
    assert_eq!(pending.quantity, dec!(100));
    assert_eq!(pending.record_date, date!(2025 - 12 - 18));
    // Lançou a bonificação no livro: a pendência some.
    let bonus = post(
        &ana.app.router,
        "/api/v1/transactions",
        json!({ "asset_id": itsa, "kind": "bonus", "trade_date": "2025-12-22", "quantity": 2 }),
    )
    .await;
    assert_eq!(bonus.status, StatusCode::CREATED, "{}", bonus.text);
    let state = db
        .store
        .income_auto_state(ana.user, date!(2026 - 06 - 15))
        .await
        .unwrap();
    assert!(state.pending.is_empty());
    // A bonificação mudou o livro antes de várias datas com: a rodada refaz
    // a quantidade (102 a partir de 22/12/2025).
    let after = auto.round(now() + time::Duration::hours(2)).await;
    assert_eq!(after.updated, 5, "{after:?}");
    let list = income_list(&ana).await;
    let jcp = automatic(&list)
        .into_iter()
        .find(|e| e["external_ref"] == jcp["external_ref"])
        .unwrap();
    assert_eq!(num(&jcp["quantity"]), dec!(102));
}

#[tokio::test]
async fn two_replicas_at_once_fetch_once_and_write_once() {
    let db = database().await;
    let bia = person(&db, "auto-bia@prumo.test").await;
    let fict = add_asset(&bia, "br_stocks", "FICT4").await;
    trade(&bia, &fict, "buy", date!(2026 - 01 - 05), 40).await;
    let source = Arc::new(FakeSource::default());
    source.set(
        "FICT",
        vec![
            cash(
                date!(2026 - 02 - 02),
                "DIVIDENDO",
                dec!(0.5),
                Some(date!(2026 - 02 - 20)),
            ),
            cash(
                date!(2026 - 03 - 02),
                "JRS CAP PROPRIO",
                dec!(0.3),
                Some(date!(2026 - 03 - 30)),
            ),
            // Sem data de pagamento: fica guardado, sem provento.
            cash(date!(2026 - 04 - 01), "DIVIDENDO", dec!(0.7), None),
        ],
        Vec::new(),
    );
    let first = service(&db.store, source.clone());
    let second = service(&db.store, source.clone());
    let (a, b) = tokio::join!(first.round(now()), second.round(now()));
    assert_eq!(a.fetched + b.fetched, 1, "{a:?} {b:?}");
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert_eq!(a.inserted + b.inserted, 2, "{a:?} {b:?}");

    // A mesma pessoa refeita duas vezes ao mesmo tempo: o lock a põe em fila.
    let (x, y) = tokio::join!(
        db.store.regenerate_income_auto(bia.user),
        db.store.regenerate_income_auto(bia.user)
    );
    let (x, y) = (x.unwrap(), y.unwrap());
    assert_eq!(
        x.inserted + y.inserted + x.updated + y.updated + x.deleted + y.deleted,
        0
    );
    assert_eq!(automatic(&income_list(&bia).await).len(), 2);

    // A data de pagamento saiu: o anúncio guardado vira provento.
    source.set(
        "FICT",
        vec![
            cash(
                date!(2026 - 02 - 02),
                "DIVIDENDO",
                dec!(0.5),
                Some(date!(2026 - 02 - 20)),
            ),
            cash(
                date!(2026 - 03 - 02),
                "JRS CAP PROPRIO",
                dec!(0.3),
                Some(date!(2026 - 03 - 30)),
            ),
            cash(
                date!(2026 - 04 - 01),
                "DIVIDENDO",
                dec!(0.7),
                Some(date!(2026 - 04 - 28)),
            ),
        ],
        Vec::new(),
    );
    let later = first.round(now() + time::Duration::days(1)).await;
    assert_eq!((later.fetched, later.inserted), (1, 1));
    let list = income_list(&bia).await;
    let paid = automatic(&list)
        .into_iter()
        .find(|e| e["pay_date"] == "2026-04-28")
        .unwrap();
    assert_eq!(num(&paid["gross"]), dec!(28));

    // A B3 corrigiu o JCP (outro valor): o velho sai, o novo entra.
    source.set(
        "FICT",
        vec![
            cash(
                date!(2026 - 02 - 02),
                "DIVIDENDO",
                dec!(0.5),
                Some(date!(2026 - 02 - 20)),
            ),
            cash(
                date!(2026 - 03 - 02),
                "JRS CAP PROPRIO",
                dec!(0.31),
                Some(date!(2026 - 03 - 30)),
            ),
            cash(
                date!(2026 - 04 - 01),
                "DIVIDENDO",
                dec!(0.7),
                Some(date!(2026 - 04 - 28)),
            ),
        ],
        Vec::new(),
    );
    let fixed = first.round(now() + time::Duration::days(2)).await;
    assert_eq!((fixed.inserted, fixed.deleted), (1, 1), "{fixed:?}");
    let list = income_list(&bia).await;
    let jcp: Vec<&Value> = automatic(&list)
        .into_iter()
        .filter(|e| e["kind"] == "jcp")
        .collect();
    assert_eq!(jcp.len(), 1);
    assert_eq!(num(&jcp[0]["amount_per_unit"]), dec!(0.31));
}

// --------------------------------------------- o que a pessoa lançou vence --

#[tokio::test]
async fn manual_and_imported_income_always_win() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let caio = person(&db, "auto-caio@prumo.test").await;
    let fict = add_asset(&caio, "br_stocks", "FICT4").await;
    trade(&caio, &fict, "buy", date!(2026 - 01 - 05), 100).await;
    let source = Arc::new(FakeSource::default());
    source.set(
        "FICT",
        vec![
            cash(
                date!(2026 - 02 - 02),
                "DIVIDENDO",
                dec!(0.5),
                Some(date!(2026 - 02 - 20)),
            ),
            cash(
                date!(2026 - 03 - 02),
                "DIVIDENDO",
                dec!(0.25),
                Some(date!(2026 - 03 - 20)),
            ),
            cash(
                date!(2026 - 04 - 01),
                "JRS CAP PROPRIO",
                dec!(0.2),
                Some(date!(2026 - 04 - 20)),
            ),
        ],
        Vec::new(),
    );
    // Lançado à mão antes (de outra ferramenta): o mesmo dividendo de
    // fevereiro, com 2 casas e sem data ex; e março com outro valor.
    let same = post(
        &caio.app.router,
        "/api/v1/income",
        json!({ "asset_id": fict, "kind": "dividend", "pay_date": "2026-02-20", "amount_per_unit": 0.50, "quantity": 100 }),
    )
    .await;
    assert_eq!(same.status, StatusCode::CREATED, "{}", same.text);
    let other = post(
        &caio.app.router,
        "/api/v1/income",
        json!({ "asset_id": fict, "kind": "dividend", "pay_date": "2026-03-20", "amount_per_unit": 0.40, "quantity": 100 }),
    )
    .await;
    assert_eq!(other.status, StatusCode::CREATED, "{}", other.text);

    let auto = service(&db.store, source.clone());
    let report = auto.round(now()).await;
    // Fevereiro não duplica; março (valor diferente) e o JCP entram.
    assert_eq!(report.inserted, 2, "{report:?}");
    let list = income_list(&caio).await;
    assert_eq!(list.len(), 4);
    let refs: Vec<&str> = automatic(&list)
        .iter()
        .map(|e| e["pay_date"].as_str().unwrap())
        .collect();
    assert!(refs.contains(&"2026-03-20") && refs.contains(&"2026-04-20"));
    assert!(!refs.contains(&"2026-02-20"));

    // O extrato da B3 chega depois com o JCP (líquido por cota): o
    // importado vence e o automático sai.
    sqlx::query(
        "insert into income_events (id, user_id, asset_id, symbol, display_name, kind, pay_date,
             amount_per_unit, quantity, gross, withholding, net, source, external_ref)
         values ($1, $2, $3, 'FICT4', 'FICT4', 'jcp', '2026-04-20', 0.165, 100, 20, 3.5, 16.5,
                 'b3_import', 'b3:v1:income:fict')",
    )
    .bind(Uuid::new_v4())
    .bind(caio.user)
    .bind(Uuid::parse_str(&fict).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    let outcome = db.store.regenerate_income_auto(caio.user).await.unwrap();
    assert_eq!(
        (outcome.inserted, outcome.deleted, outcome.covered),
        (0, 1, 2)
    );
    let list = income_list(&caio).await;
    assert_eq!(list.len(), 4);
    assert_eq!(automatic(&list).len(), 1);

    // Os da pessoa e o importado nunca mudam: nem com a chave desligada.
    let off = put(
        &caio.app.router,
        "/api/v1/income/auto",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text);
    assert_eq!(off.body["enabled"], false);
    let list = income_list(&caio).await;
    assert_eq!(list.len(), 3);
    assert!(automatic(&list).is_empty());
    // Desligado, a rodada não recria.
    let report = auto.round(now() + time::Duration::days(1)).await;
    assert_eq!(report.inserted, 0);
    assert!(automatic(&income_list(&caio).await).is_empty());
    // Ligar refaz na hora, sem ir à B3.
    let calls = source.calls.load(Ordering::SeqCst);
    let on = put(
        &caio.app.router,
        "/api/v1/income/auto",
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text);
    assert_eq!(on.body["enabled"], true);
    assert_eq!(automatic(&income_list(&caio).await).len(), 1);
    assert_eq!(source.calls.load(Ordering::SeqCst), calls);
    let refused = put(
        &caio.app.router,
        "/api/v1/income/auto",
        json!({ "enabled": "sim" }),
    )
    .await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.text
    );
}

// --------------------------------------------------- o livro muda no passado --

#[tokio::test]
async fn the_book_changes_and_the_automatic_follows() {
    let db = database().await;
    let duda = person(&db, "auto-duda@prumo.test").await;
    let fict = add_asset(&duda, "br_stocks", "FICT4").await;
    let buy = trade(&duda, &fict, "buy", date!(2026 - 01 - 05), 10).await;
    let source = Arc::new(FakeSource::default());
    source.set(
        "FICT",
        vec![cash(
            date!(2026 - 03 - 02),
            "DIVIDENDO",
            dec!(1),
            Some(date!(2026 - 03 - 20)),
        )],
        Vec::new(),
    );
    let auto = service(&db.store, source);
    assert_eq!(auto.round(now()).await.inserted, 1);

    // Compra retroativa antes da data com: 10 → 25.
    let late = trade(&duda, &fict, "buy", date!(2026 - 02 - 10), 15).await;
    // Compra na data ex (03/03): não dá direito.
    trade(&duda, &fict, "buy", date!(2026 - 03 - 03), 5).await;
    let report = db.store.regenerate_income_auto(duda.user).await.unwrap();
    assert_eq!(report.updated, 1);
    let list = income_list(&duda).await;
    assert_eq!(num(&automatic(&list)[0]["quantity"]), dec!(25));
    assert_eq!(num(&automatic(&list)[0]["net"]), dec!(25));

    // Editar a compra retroativa para 20.
    let edited = patch(
        &duda.app.router,
        &format!("/api/v1/transactions/{late}"),
        json!({ "quantity": 20 }),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);
    db.store.regenerate_income_auto(duda.user).await.unwrap();
    assert_eq!(
        num(&automatic(&income_list(&duda).await)[0]["quantity"]),
        dec!(30)
    );

    // Apagar as compras antes da data com: zerou, o automático some.
    for id in [&late, &buy] {
        let gone = delete(&duda.app.router, &format!("/api/v1/transactions/{id}")).await;
        assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.text);
    }
    let report = db.store.regenerate_income_auto(duda.user).await.unwrap();
    assert_eq!(report.deleted, 1);
    assert!(automatic(&income_list(&duda).await).is_empty());
}

// ------------------------------------------------------- dono e apagados --

#[tokio::test]
async fn each_person_has_their_own_switch_and_their_own_automatics() {
    let db = database().await;
    let eva = person(&db, "auto-eva@prumo.test").await;
    let fabio = person(&db, "auto-fabio@prumo.test").await;
    for p in [&eva, &fabio] {
        let fict = add_asset(p, "br_stocks", "FICT4").await;
        trade(p, &fict, "buy", date!(2026 - 01 - 05), 10).await;
    }
    let source = Arc::new(FakeSource::default());
    source.set(
        "FICT",
        vec![
            cash(
                date!(2026 - 03 - 02),
                "DIVIDENDO",
                dec!(1),
                Some(date!(2026 - 03 - 20)),
            ),
            cash(
                date!(2026 - 04 - 01),
                "DIVIDENDO",
                dec!(2),
                Some(date!(2026 - 04 - 20)),
            ),
        ],
        Vec::new(),
    );
    let auto = service(&db.store, source);
    let report = auto.round(now()).await;
    assert_eq!((report.fetched, report.inserted), (1, 4));

    // Eva desliga: só os dela somem.
    let off = put(
        &eva.app.router,
        "/api/v1/income/auto",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK);
    assert!(automatic(&income_list(&eva).await).is_empty());
    assert_eq!(automatic(&income_list(&fabio).await).len(), 2);
    assert_eq!(
        get(&fabio.app.router, "/api/v1/income/auto").await.body["enabled"],
        true
    );

    // Fábio apaga um automático: não volta na próxima rodada.
    let list = income_list(&fabio).await;
    let first = automatic(&list)[0]["id"].as_str().unwrap().to_owned();
    // De outra pessoa é como não existir.
    let not_hers = delete(&eva.app.router, &format!("/api/v1/income/{first}")).await;
    assert_eq!(not_hers.status, StatusCode::NOT_FOUND);
    // Automático não se edita: é o que a B3 e o livro dizem.
    let edit = patch(
        &fabio.app.router,
        &format!("/api/v1/income/{first}"),
        json!({ "amount_per_unit": 3 }),
    )
    .await;
    assert_eq!(
        edit.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        edit.text
    );
    let gone = delete(&fabio.app.router, &format!("/api/v1/income/{first}")).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let report = auto.round(now() + time::Duration::hours(1)).await;
    assert_eq!(report.inserted, 0);
    assert_eq!(automatic(&income_list(&fabio).await).len(), 1);
}

// ------------------------------------------------- caixa e alerta leem igual --

struct SilentTelegram {
    sent: Mutex<Vec<String>>,
}

#[async_trait]
impl TelegramApi for SilentTelegram {
    async fn username(&self) -> Result<String, TelegramError> {
        Ok("prumo_teste_bot".into())
    }

    async fn updates(
        &self,
        _offset: i64,
        _timeout: Duration,
    ) -> Result<Vec<Update>, TelegramError> {
        Ok(Vec::new())
    }

    async fn send(&self, _chat_id: i64, text: &str) -> Result<(), TelegramError> {
        self.sent.lock().unwrap().push(text.to_owned());
        Ok(())
    }
}

#[tokio::test]
async fn cash_and_the_daily_alert_read_the_automatic_income() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let gil = person(&db, "auto-gil@prumo.test").await;
    let fict = add_asset(&gil, "br_stocks", "FICT4").await;
    trade(&gil, &fict, "buy", date!(2026 - 01 - 05), 10).await;
    sqlx::query("update users set cash_since = '2026-01-01' where id = $1")
        .bind(gil.user)
        .execute(&pool)
        .await
        .unwrap();
    let source = Arc::new(FakeSource::default());
    source.set(
        "FICT",
        vec![
            // Recebido em março: entra no caixa.
            cash(
                date!(2026 - 03 - 02),
                "DIVIDENDO",
                dec!(1.5),
                Some(date!(2026 - 03 - 20)),
            ),
            // Cai amanhã do dia do alerta (16/06/2026).
            cash(
                date!(2026 - 06 - 01),
                "DIVIDENDO",
                dec!(0.8),
                Some(date!(2026 - 06 - 16)),
            ),
        ],
        Vec::new(),
    );
    service(&db.store, source).round(now()).await;

    let cash = get(&gil.app.router, "/api/v1/cash").await;
    assert_eq!(cash.status, StatusCode::OK, "{}", cash.text);
    // Os dois já foram pagos no relógio de verdade (depois de 16/06/2026).
    let today = prumo::domain::income::today_in_brazil(OffsetDateTime::now_utc());
    let expected: Decimal = automatic(&income_list(&gil).await)
        .iter()
        .filter(|e| e["pay_date"].as_str().unwrap() <= today.to_string().as_str())
        .map(|e| num(&e["net"]))
        .sum();
    assert_eq!(num(&cash.body["income"]["amount"]), expected);
    assert!(expected >= dec!(15));

    let telegram = Arc::new(SilentTelegram {
        sent: Mutex::new(Vec::new()),
    });
    let alerts = AlertService::new(
        db.store.clone(),
        Some(Arc::clone(&telegram) as Arc<dyn TelegramApi>),
    )
    .with_clock(Arc::new(now));
    db.store
        .link_alert_chat(gil.user, 777, now())
        .await
        .unwrap();
    let report = alerts.run_digest(now()).await.unwrap();
    assert_eq!(report.sent, 1, "{report:?}");
    let sent = telegram.sent.lock().unwrap().clone();
    assert!(
        sent[0].contains("FICT4 paga amanhã: R$ 8,00 líquidos."),
        "{}",
        sent[0]
    );
}

// ------------------------------------------------------------- migração --

#[tokio::test]
async fn migration_0018_keeps_income_and_turns_the_switch_on() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    sqlx::query("create schema antes")
        .execute(&pool)
        .await
        .unwrap();
    let url = format!("{}?options=-csearch_path%3Dantes", db.url);
    let old = PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations")
        .run_to(17, &old)
        .await
        .unwrap();
    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, 'antes-auto@prumo.test')")
        .bind(user)
        .execute(&old)
        .await
        .unwrap();
    for source in ["manual", "b3_import"] {
        sqlx::query(
            "insert into income_events (id, user_id, symbol, display_name, kind, pay_date,
                 amount_per_unit, quantity, gross, withholding, net, source)
             values ($1, $2, 'FICT4', 'Ficticia', 'dividend', '2025-01-10', 1, 10, 10, 0, 10, $3)",
        )
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(source)
        .execute(&old)
        .await
        .unwrap();
    }
    Store::from_pool(old.clone()).migrate().await.unwrap();
    let (count,): (i64,) = sqlx::query_as("select count(*) from income_events")
        .fetch_one(&old)
        .await
        .unwrap();
    assert_eq!(count, 2);
    let (on,): (bool,) = sqlx::query_as("select income_auto from users where id = $1")
        .bind(user)
        .fetch_one(&old)
        .await
        .unwrap();
    assert!(on);
    let insert = |source: &'static str| {
        sqlx::query(
            "insert into income_events (id, user_id, symbol, display_name, kind, pay_date,
                 amount_per_unit, quantity, gross, withholding, net, source, external_ref)
             values ($1, $2, 'FICT4', 'Ficticia', 'dividend', '2025-02-10', 1, 10, 10, 0, 10, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(source)
        .bind(format!("b3a:{source}"))
    };
    assert!(insert("b3_announcement").execute(&old).await.is_ok());
    assert!(insert("outra").execute(&old).await.is_err());
}
