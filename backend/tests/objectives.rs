//! Objetivos com prazo (patrimônio e valor numa classe), rota por rota, contra
//! um Postgres de verdade: CRUD, dono, `achieved_at`, a média de aporte do
//! livro, a premissa de rentabilidade e o efeito de um aporte.
//!
//! Um contêiner para o arquivo inteiro; cada cenário tem a própria pessoa, e
//! como tudo é filtrado pelo dono, um cenário não enxerga o dado do outro.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::domain::asset_class::AssetClass;
use prumo::domain::income::today_in_brazil;
use prumo::domain::objectives::Target;
use prumo::domain::performance::Quota;
use prumo::history::IndexCode;
use prumo::history::{HistoryError, HistoryProvider, Series};
use prumo::objectives::{record_achieved, record_achieved_for};
use prumo::performance::Performance;
use prumo::store::NewObjective;
use prumo::store::Snapshot;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

fn iso(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap()
}

fn days_ago(days: i64) -> String {
    iso(today() - Duration::days(days))
}

struct Person {
    router: axum::Router,
    email: String,
}

fn person(db: &Database, email: &str, dir: &tempfile_dir::Dir) -> Person {
    Person {
        router: app(&db.store, email, FakeQuotes::with_market(), &dir.path()).router,
        email: email.to_owned(),
    }
}

async fn add_asset(p: &Person, body: Value) -> String {
    let reply = post(&p.router, "/api/v1/assets", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

async fn buy(p: &Person, asset: &str, days: i64, quantity: u32, price: u32) {
    let reply = post(
        &p.router,
        "/api/v1/transactions",
        json!({
            "asset_id": asset, "kind": "buy", "trade_date": days_ago(days),
            "quantity": quantity, "unit_price": price,
        }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
}

async fn net_worth(p: &Person) -> f64 {
    get(&p.router, "/api/v1/portfolio").await.body["total_value"]
        .as_f64()
        .unwrap()
}

fn objective<'a>(list: &'a Value, id: &str) -> &'a Value {
    find(&list["objectives"], "id", id)
}

#[tokio::test]
async fn objectives_contract() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();

    crud_validation_and_owner(&db, &dir).await;
    income_goal_stays_out(&db, &dir).await;
    achieved_is_recorded_once_and_kept(&db, &dir).await;
    contribution_comes_from_the_ledger(&db, &dir).await;
    return_premise_comes_from_the_best_source(&db, &dir).await;
    impact_of_a_contribution(&db, &dir).await;
    background_job_records_every_kind(&db, &dir).await;
}

async fn crud_validation_and_owner(db: &Database, dir: &tempfile_dir::Dir) {
    let me = person(db, "obj-crud@prumo.test", dir);
    let other = person(db, "obj-outra@prumo.test", dir);

    let created = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": 1000000, "monthly_contribution": 2500, "expected_return": 9.5 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let id = created.body["id"].as_str().unwrap().to_owned();
    assert_eq!(created.body["kind"], "net_worth");
    assert_eq!(created.body["asset_class"], Value::Null);
    assert_eq!(created.body["target"], 1000000.0);
    assert_eq!(created.body["monthly_contribution"], 2500.0);
    assert_eq!(created.body["expected_return"], 9.5);
    assert_eq!(created.body["status"], "active");
    assert_eq!(created.body["achieved_at"], Value::Null);
    assert_eq!(
        created.body["assumptions"]["contribution_source"],
        "informed"
    );
    assert_eq!(created.body["assumptions"]["return_source"], "informed");
    assert_eq!(created.body["assumptions"]["annual_return_percent"], 9.5);

    let by_class = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "class_value", "asset_class": "br_reits", "target": 100000 }),
    )
    .await;
    assert_eq!(by_class.status, StatusCode::CREATED, "{}", by_class.text);
    let class_id = by_class.body["id"].as_str().unwrap().to_owned();
    assert_eq!(by_class.body["asset_class"], "br_reits");
    assert_eq!(by_class.body["monthly_contribution"], Value::Null);
    assert_eq!(
        by_class.body["assumptions"]["contribution_source"],
        "ledger"
    );

    // Validação.
    for (name, body) in [
        (
            "classe sem asset_class",
            json!({ "kind": "class_value", "target": 10 }),
        ),
        (
            "patrimonio com classe",
            json!({ "kind": "net_worth", "asset_class": "crypto", "target": 10 }),
        ),
        (
            "meta de renda mora em Renda",
            json!({ "kind": "monthly_income", "target": 10 }),
        ),
        ("meta zero", json!({ "kind": "net_worth", "target": 0 })),
        (
            "meta negativa",
            json!({ "kind": "net_worth", "target": -5 }),
        ),
        (
            "aporte negativo",
            json!({ "kind": "net_worth", "target": 10, "monthly_contribution": -1 }),
        ),
        (
            "rentabilidade alta demais",
            json!({ "kind": "net_worth", "target": 10, "expected_return": 101 }),
        ),
        (
            "rentabilidade baixa demais",
            json!({ "kind": "net_worth", "target": 10, "expected_return": -51 }),
        ),
        (
            "tipo desconhecido",
            json!({ "kind": "goals", "target": 10 }),
        ),
        (
            "campo desconhecido",
            json!({ "kind": "net_worth", "target": 10, "name": "x" }),
        ),
    ] {
        let reply = post(&me.router, "/api/v1/objectives", body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{name}");
    }

    // Lista, na ordem de criação, com o padrão de rentabilidade.
    let list = get(&me.router, "/api/v1/objectives").await;
    assert_eq!(list.status, StatusCode::OK);
    let ids: Vec<&str> = list.body["objectives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [id.as_str(), class_id.as_str()]);
    assert!(list.body["defaults"]["annual_return_percent"].is_number());

    // Troca parcial: a meta, o aporte (e limpa a rentabilidade com null).
    let patched = patch(
        &me.router,
        &format!("/api/v1/objectives/{id}"),
        json!({ "target": 2000000, "monthly_contribution": 3000, "expected_return": null }),
    )
    .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text);
    assert_eq!(patched.body["target"], 2000000.0);
    assert_eq!(patched.body["monthly_contribution"], 3000.0);
    assert_eq!(patched.body["expected_return"], Value::Null);
    assert_ne!(patched.body["assumptions"]["return_source"], "informed");

    // Só o aporte: a rentabilidade e a meta ficam.
    let again = patch(
        &me.router,
        &format!("/api/v1/objectives/{id}"),
        json!({ "monthly_contribution": null }),
    )
    .await;
    assert_eq!(again.body["target"], 2000000.0);
    assert_eq!(again.body["monthly_contribution"], Value::Null);
    assert_eq!(again.body["assumptions"]["contribution_source"], "ledger");

    // O tipo e a classe não mudam.
    for body in [
        json!({ "kind": "class_value" }),
        json!({ "asset_class": "crypto" }),
        json!({ "target": 0 }),
        json!({ "expected_return": 500 }),
    ] {
        let reply = patch(&me.router, &format!("/api/v1/objectives/{id}"), body).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    // A outra pessoa não vê, não troca e não apaga.
    let theirs = get(&other.router, "/api/v1/objectives").await;
    assert_eq!(theirs.body["objectives"], json!([]));
    let uri = format!("/api/v1/objectives/{id}");
    assert_eq!(
        patch(&other.router, &uri, json!({ "target": 1 }))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&other.router, &uri).await.status,
        StatusCode::NOT_FOUND
    );
    let still = get(&me.router, "/api/v1/objectives").await;
    assert_eq!(still.body["objectives"].as_array().unwrap().len(), 2);

    // Apagar.
    assert_eq!(
        delete(&me.router, &uri).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(delete(&me.router, &uri).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        patch(&me.router, &uri, json!({ "target": 1 })).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&me.router, "/api/v1/objectives").await.body["objectives"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Um id que nem é um UUID também é 404.
    assert_eq!(
        delete(&me.router, "/api/v1/objectives/nao-e-id")
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

async fn income_goal_stays_out(db: &Database, dir: &tempfile_dir::Dir) {
    let me = person(db, "obj-renda@prumo.test", dir);
    let goal = put(&me.router, "/api/v1/income/goal", json!({ "target": 3000 })).await;
    assert_eq!(goal.status, StatusCode::OK, "{}", goal.text);
    let goal_id = goal.body["id"].as_str().unwrap().to_owned();

    // A meta de renda continua onde estava: não entra na lista e não se
    // mexe nela por aqui.
    let list = get(&me.router, "/api/v1/objectives").await;
    assert_eq!(list.body["objectives"], json!([]));
    let uri = format!("/api/v1/objectives/{goal_id}");
    assert_eq!(
        patch(&me.router, &uri, json!({ "target": 1 })).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(delete(&me.router, &uri).await.status, StatusCode::NOT_FOUND);
    let still = get(&me.router, "/api/v1/income/goal").await;
    assert_eq!(still.body["target"], 3000.0);
}

async fn achieved_is_recorded_once_and_kept(db: &Database, dir: &tempfile_dir::Dir) {
    let me = person(db, "obj-atingida@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    let worth = net_worth(&me).await;
    assert!(worth > 0.0, "a carteira precisa ter valor");

    // Já nasce atingida: a data fica gravada na criação.
    let done = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": 100 }),
    )
    .await;
    assert_eq!(done.status, StatusCode::CREATED, "{}", done.text);
    assert_eq!(done.body["status"], "achieved");
    assert_eq!(done.body["percent"], 100.0);
    assert_eq!(done.body["projection"]["state"], "reached");
    assert_eq!(done.body["assumptions"], Value::Null);
    let achieved_at = done.body["achieved_at"].as_str().unwrap().to_owned();
    let done_id = done.body["id"].as_str().unwrap().to_owned();

    // Um objetivo de classe que a carteira ainda não cobre.
    let open = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "class_value", "asset_class": "crypto", "target": 5000 }),
    )
    .await;
    assert_eq!(open.body["status"], "active");
    assert_eq!(open.body["current"], 0.0);
    assert_eq!(open.body["percent"], 0.0);
    assert_eq!(open.body["achieved_at"], Value::Null);

    // A carteira encolhe abaixo da meta: a conquista fica, com a mesma data.
    assert_eq!(
        delete(&me.router, &format!("/api/v1/assets/{knri}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let list = get(&me.router, "/api/v1/objectives").await;
    let kept = objective(&list.body, &done_id);
    assert_eq!(kept["status"], "achieved");
    assert_eq!(kept["achieved_at"], achieved_at);
    assert_eq!(kept["current"], 0.0);

    // Meta nova, conquista nova: subir a meta apaga a data.
    let raised = patch(
        &me.router,
        &format!("/api/v1/objectives/{done_id}"),
        json!({ "target": 500000 }),
    )
    .await;
    assert_eq!(raised.body["status"], "active");
    assert_eq!(raised.body["achieved_at"], Value::Null);

    // Trocar só o aporte de uma meta atingida não apaga a data.
    let cheap = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": 0.01 }),
    )
    .await;
    // Com a carteira vazia o valor é zero: ainda ativa.
    assert_eq!(cheap.body["status"], "active");
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    assert!(!knri.is_empty());
    let cheap_id = cheap.body["id"].as_str().unwrap();
    let list = get(&me.router, "/api/v1/objectives").await;
    let reached = objective(&list.body, cheap_id);
    assert_eq!(reached["status"], "achieved");
    let first_date = reached["achieved_at"].as_str().unwrap().to_owned();
    let touched = patch(
        &me.router,
        &format!("/api/v1/objectives/{cheap_id}"),
        json!({ "monthly_contribution": 100 }),
    )
    .await;
    assert_eq!(touched.body["status"], "achieved");
    assert_eq!(touched.body["achieved_at"], first_date);
}

async fn contribution_comes_from_the_ledger(db: &Database, dir: &tempfile_dir::Dir) {
    let me = person(db, "obj-livro@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    // A primeira compra faz 40 dias (2 meses de 30 dias): (1.000 + 500) / 2.
    buy(&me, &knri, 40, 10, 100).await;
    buy(&me, &knri, 10, 5, 100).await;

    let worth = net_worth(&me).await;
    let reits = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "class_value", "asset_class": "br_reits", "target": worth + 2250.0, "expected_return": 0 }),
    )
    .await;
    assert_eq!(reits.status, StatusCode::CREATED, "{}", reits.text);
    assert_eq!(reits.body["assumptions"]["contribution_source"], "ledger");
    assert_eq!(reits.body["assumptions"]["monthly_contribution"], 750.0);
    // Faltam 2.250 a 750 por mês, sem rendimento: 3 meses.
    assert_eq!(reits.body["projection"]["state"], "eta");
    assert_eq!(reits.body["projection"]["months"], 3);
    let expected = {
        let (year, month) = prumo::domain::objectives::month_after(today(), 3);
        format!("{year:04}-{:02}", u8::from(month))
    };
    assert_eq!(reits.body["projection"]["month"], expected);

    // O mesmo valor no patrimônio: a média é de todas as classes (igual aqui).
    let total = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": worth + 7500.0, "expected_return": 0 }),
    )
    .await;
    assert_eq!(total.body["assumptions"]["monthly_contribution"], 750.0);
    assert_eq!(total.body["projection"]["months"], 10);

    // Uma classe sem nenhuma compra não tem aporte nem rendimento: não chega.
    let crypto = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "class_value", "asset_class": "crypto", "target": 5000, "expected_return": 0 }),
    )
    .await;
    assert_eq!(crypto.body["assumptions"]["monthly_contribution"], 0.0);
    assert_eq!(crypto.body["projection"]["state"], "unreachable");
    assert_eq!(crypto.body["projection"]["months"], Value::Null);
    assert_eq!(crypto.body["projection"]["month"], Value::Null);

    // O aporte informado vale no lugar da média.
    let informed = patch(
        &me.router,
        &format!("/api/v1/objectives/{}", reits.body["id"].as_str().unwrap()),
        json!({ "monthly_contribution": 1125 }),
    )
    .await;
    assert_eq!(
        informed.body["assumptions"]["contribution_source"],
        "informed"
    );
    assert_eq!(informed.body["projection"]["months"], 2);

    // Com rentabilidade positiva a data não atrasa.
    let faster = patch(
        &me.router,
        &format!("/api/v1/objectives/{}", reits.body["id"].as_str().unwrap()),
        json!({ "monthly_contribution": null, "expected_return": 12 }),
    )
    .await;
    assert!(faster.body["projection"]["months"].as_u64().unwrap() <= 3);
}

async fn return_premise_comes_from_the_best_source(db: &Database, dir: &tempfile_dir::Dir) {
    // Sem série da carteira nem CDI no banco: o padrão fixo, escrito na resposta.
    let me = person(db, "obj-premissa@prumo.test", dir);
    let first = get(&me.router, "/api/v1/objectives").await;
    assert_eq!(first.body["defaults"]["return_source"], "fallback");
    assert_eq!(first.body["defaults"]["annual_return_percent"], 8.0);

    // Com o CDI de hoje: o CDI líquido do IR (15%).
    let day = today() - Duration::days(2);
    db.store
        .upsert_index(IndexCode::Cdi, &vec![(day, dec!(0.0551))])
        .await
        .unwrap();
    let cdi = get(&me.router, "/api/v1/objectives").await;
    assert_eq!(cdi.body["defaults"]["return_source"], "cdi_net");
    let percent = cdi.body["defaults"]["annual_return_percent"]
        .as_f64()
        .unwrap();
    let gross = (1.0f64 + 0.0551 / 100.0).powf(252.0) - 1.0;
    assert!(
        (percent - gross * 0.85 * 100.0).abs() < 0.02,
        "{percent} contra {}",
        gross * 85.0
    );

    // Com seis meses de série da carteira: a rentabilidade dela, anualizada.
    let id = db.store.ensure_user(&me.email).await.unwrap();
    let start = today() - Duration::days(400);
    let end = today() - Duration::days(1);
    let days: Vec<Snapshot> = [(start, dec!(100)), (end, dec!(112))]
        .into_iter()
        .map(|(date, value)| Snapshot {
            date,
            market_value: dec!(1000),
            net_flow: dec!(0),
            income: dec!(0),
            quota: Quota {
                quotas: dec!(10),
                value,
            },
            positions: Vec::new(),
            taken_at: OffsetDateTime::now_utc(),
        })
        .collect();
    db.store
        .replace_snapshots(id, None, start, &days)
        .await
        .unwrap();
    let measured = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": 100000, "monthly_contribution": 100 }),
    )
    .await;
    assert_eq!(measured.body["assumptions"]["return_source"], "performance");
    let percent = measured.body["assumptions"]["annual_return_percent"]
        .as_f64()
        .unwrap();
    let expected = (1.12f64.powf(365.0 / 399.0) - 1.0) * 100.0;
    assert!(
        (percent - expected).abs() < 0.02,
        "{percent} contra {expected}"
    );

    // A que a pessoa informa vale mais que as duas.
    let informed = patch(
        &me.router,
        &format!(
            "/api/v1/objectives/{}",
            measured.body["id"].as_str().unwrap()
        ),
        json!({ "expected_return": 6.25 }),
    )
    .await;
    assert_eq!(informed.body["assumptions"]["return_source"], "informed");
    assert_eq!(informed.body["assumptions"]["annual_return_percent"], 6.25);
}

async fn impact_of_a_contribution(db: &Database, dir: &tempfile_dir::Dir) {
    let me = person(db, "obj-impacto@prumo.test", dir);
    let other = person(db, "obj-impacto-outra@prumo.test", dir);
    add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    let worth = net_worth(&me).await;

    let total = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": worth + 4000.0, "monthly_contribution": 1000, "expected_return": 0 }),
    )
    .await;
    let reits = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "class_value", "asset_class": "br_reits", "target": worth + 3000.0, "monthly_contribution": 1000, "expected_return": 0 }),
    )
    .await;
    let stocks = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "class_value", "asset_class": "br_stocks", "target": 10000, "monthly_contribution": 1000, "expected_return": 0 }),
    )
    .await;
    let done = post(
        &me.router,
        "/api/v1/objectives",
        json!({ "kind": "net_worth", "target": 1 }),
    )
    .await;
    assert_eq!(done.body["status"], "achieved");

    let reply = post(
        &me.router,
        "/api/v1/objectives/impact",
        json!({ "items": [
            { "asset_class": "br_reits", "value": 1000 },
            { "asset_class": "br_reits", "value": 500 },
            { "asset_class": "br_stocks", "value": 500 },
        ] }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let entries = &reply.body["objectives"];
    // A concluída não entra.
    assert_eq!(entries.as_array().unwrap().len(), 3);

    let by_id = |created: &Reply| find(entries, "id", created.body["id"].as_str().unwrap()).clone();
    let total_entry = by_id(&total);
    assert_eq!(total_entry["current"].as_f64().unwrap(), worth);
    assert_eq!(total_entry["after"].as_f64().unwrap(), worth + 2000.0);
    // Faltam 2.000 depois do aporte, a 1.000 por mês: 2 meses.
    assert_eq!(total_entry["projection"]["state"], "eta");
    assert_eq!(total_entry["projection"]["months"], 2);
    assert!(
        total_entry["percent_after"].as_f64().unwrap()
            > total_entry["percent_before"].as_f64().unwrap()
    );
    assert_eq!(total_entry["assumptions"]["monthly_contribution"], 1000.0);

    // Só o que foi para FIIs conta na meta de FIIs.
    let reits_entry = by_id(&reits);
    assert_eq!(reits_entry["after"].as_f64().unwrap(), worth + 1500.0);
    assert_eq!(reits_entry["projection"]["months"], 2);

    let stocks_entry = by_id(&stocks);
    assert_eq!(stocks_entry["current"], 0.0);
    assert_eq!(stocks_entry["after"], 500.0);
    assert_eq!(stocks_entry["percent_after"], 5.0);
    assert_eq!(stocks_entry["projection"]["months"], 10);

    // Um aporte que fecha a meta: a conta diz que chega.
    let big = post(
        &me.router,
        "/api/v1/objectives/impact",
        json!({ "items": [{ "asset_class": "br_reits", "value": 5000 }] }),
    )
    .await;
    let closed = find(
        &big.body["objectives"],
        "id",
        total.body["id"].as_str().unwrap(),
    );
    assert_eq!(closed["projection"]["state"], "reached");
    assert_eq!(closed["percent_after"], 100.0);

    // Sem itens, o valor fica como está.
    let none = post(
        &me.router,
        "/api/v1/objectives/impact",
        json!({ "items": [] }),
    )
    .await;
    assert_eq!(none.status, StatusCode::OK);
    let same = find(
        &none.body["objectives"],
        "id",
        total.body["id"].as_str().unwrap(),
    );
    assert_eq!(same["after"], same["current"]);

    // Validação e dono.
    for body in [
        json!({ "items": [{ "asset_class": "br_reits", "value": -1 }] }),
        json!({ "items": [{ "asset_class": "goals", "value": 1 }] }),
        json!({ "items": [{ "asset_class": "br_reits" }] }),
        json!({}),
    ] {
        assert_eq!(
            post(&me.router, "/api/v1/objectives/impact", body)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let theirs = post(
        &other.router,
        "/api/v1/objectives/impact",
        json!({ "items": [{ "asset_class": "br_reits", "value": 100 }] }),
    )
    .await;
    assert_eq!(theirs.body["objectives"], json!([]));
}

/// Sem dado de fora: o teste mexe só no que a tarefa faz com os objetivos.
struct NoHistory;

#[async_trait]
impl HistoryProvider for NoHistory {
    async fn index(
        &self,
        _code: IndexCode,
        _from: Date,
        _to: Date,
    ) -> Result<Series, HistoryError> {
        Ok(Vec::new())
    }

    async fn closes(&self, _symbol: &str, _from: Date, _to: Date) -> Result<Series, HistoryError> {
        Ok(Vec::new())
    }
}

fn new_objective(kind: Target, asset_class: Option<AssetClass>, target: u32) -> NewObjective {
    NewObjective {
        target_kind: kind,
        asset_class,
        target: target.into(),
        monthly_contribution: None,
        expected_return: None,
    }
}

async fn background_job_records_every_kind(db: &Database, dir: &tempfile_dir::Dir) {
    let me = person(db, "obj-tarefa@prumo.test", dir);
    let other = person(db, "obj-tarefa-outra@prumo.test", dir);
    let knri = add_asset(
        &me,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    let user = db.store.ensure_user(&me.email).await.unwrap();

    // A meta de renda existe antes de haver provento: ainda aberta.
    let goal = put(&me.router, "/api/v1/income/goal", json!({ "target": 50 })).await;
    assert_eq!(goal.status, StatusCode::OK, "{}", goal.text);
    assert_eq!(goal.body["achieved_at"], Value::Null);
    let income = post(
        &me.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": days_ago(10), "amount_per_unit": 120 }),
    )
    .await;
    assert_eq!(income.status, StatusCode::CREATED, "{}", income.text);

    // Criados direto no banco: nenhuma rota os avalia.
    let reached_total = db
        .store
        .insert_objective(user, &new_objective(Target::NetWorth, None, 100))
        .await
        .unwrap();
    let reached_class = db
        .store
        .insert_objective(
            user,
            &new_objective(Target::ClassValue, Some(AssetClass::BrReits), 100),
        )
        .await
        .unwrap();
    let open_class = db
        .store
        .insert_objective(
            user,
            &new_objective(Target::ClassValue, Some(AssetClass::Crypto), 5000),
        )
        .await
        .unwrap();
    let theirs = db
        .store
        .insert_objective(
            db.store.ensure_user(&other.email).await.unwrap(),
            &new_objective(Target::NetWorth, None, 1_000_000_000),
        )
        .await
        .unwrap();

    // Duas réplicas ao mesmo tempo: cada objetivo é gravado uma vez só.
    let now = OffsetDateTime::now_utc();
    let (first, second) = tokio::join!(
        record_achieved_for(&db.store, user, now),
        record_achieved_for(&db.store, user, now),
    );
    assert_eq!(first.unwrap() + second.unwrap(), 3);

    let open = db.store.open_objectives(user).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, open_class.id);
    // Idempotente: outra rodada não grava nem mexe na data.
    assert_eq!(
        record_achieved_for(&db.store, user, now + Duration::hours(1))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        record_achieved(&db.store, now + Duration::hours(1))
            .await
            .unwrap(),
        0
    );

    // As telas mostram a data que a tarefa gravou, sem mudá-la.
    let list = get(&me.router, "/api/v1/objectives").await;
    for id in [reached_total.id, reached_class.id] {
        let found = objective(&list.body, &id.to_string());
        assert_eq!(found["status"], "achieved");
        assert!(found["achieved_at"].as_str().is_some());
    }
    assert_eq!(
        objective(&list.body, &open_class.id.to_string())["status"],
        "active"
    );
    let goal = get(&me.router, "/api/v1/income/goal").await;
    assert!(goal.body["achieved_at"].as_str().is_some(), "{}", goal.text);
    assert_eq!(goal.body["reached"], true);

    // O objetivo de outra pessoa não foi tocado.
    let theirs_now = db
        .store
        .open_objectives(db.store.ensure_user(&other.email).await.unwrap())
        .await
        .unwrap();
    assert_eq!(theirs_now.len(), 1);
    assert_eq!(theirs_now[0].id, theirs.id);

    // A tarefa diária faz a apuração sozinha, com o lock de job.
    let later = db
        .store
        .insert_objective(user, &new_objective(Target::NetWorth, None, 200))
        .await
        .unwrap();
    let job = Performance::new(db.store.clone(), std::sync::Arc::new(NoHistory));
    let report = job.run_daily(OffsetDateTime::now_utc()).await.unwrap();
    assert!(report.ran);
    assert!(report.objectives >= 1, "{report:?}");
    assert!(!db
        .store
        .open_objectives(user)
        .await
        .unwrap()
        .iter()
        .any(|o| o.id == later.id));
}
