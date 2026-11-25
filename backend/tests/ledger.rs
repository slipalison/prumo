//! O livro de lançamentos contra um Postgres de verdade: a migração dos dados
//! de antes do livro, a API `/transactions`, o dono e os casos de borda.
//!
//! Regra que todo cenário confere no fim: a quantidade de cada ativo é a soma
//! do livro, e o cache de custo é o que o domínio calcula do livro inteiro.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::str::FromStr;

use axum::http::StatusCode;
use common::*;
use prumo::domain::ledger::{self, Entry, Kind};
use prumo::store::{Store, TransactionFilter};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

/// Postgres 17 vazio, sem migração nenhuma.
async fn bare_postgres() -> (ContainerAsync<Postgres>, PgPool) {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();
    (container, pool)
}

fn num(value: &Value) -> Decimal {
    Decimal::from_str(&value.to_string()).unwrap_or_else(|_| panic!("numero: {value}"))
}

/// Confere, para todo ativo do banco, que `assets.quantity` é a soma do livro
/// e que o cache de custo e lucro é o que o domínio refaz do livro.
async fn assert_book_matches(store: &Store, pool: &PgPool) {
    let users: Vec<(Uuid,)> = sqlx::query_as("select id from users")
        .fetch_all(pool)
        .await
        .unwrap();
    for (user,) in users {
        let positions = store.positions(user).await.unwrap();
        let all = store
            .transactions(user, TransactionFilter::default())
            .await
            .unwrap();
        for (asset, cached) in positions {
            let entries: Vec<Entry> = all
                .iter()
                .filter(|t| t.asset_id == Some(asset))
                .map(|t| t.entry())
                .collect();
            let signed: Decimal = entries
                .iter()
                .map(|e| e.kind.quantity_delta(e.quantity))
                .sum();
            assert_eq!(cached.quantity, signed, "soma do livro do ativo {asset}");
            let replayed = ledger::position(&entries).unwrap();
            assert_eq!(replayed.quantity, cached.quantity);
            assert_eq!(
                replayed.cost_basis, cached.cost_basis,
                "custo do ativo {asset}"
            );
            assert_eq!(replayed.realized_gain, cached.realized_gain);
        }
    }
}

// ---------------------------------------------------------------- migração --

/// Produção tem ativos e aportes de antes do livro. Monta uma carteira
/// fictícia no esquema 0003, aplica a 0004 e confere o que ela fez.
#[tokio::test]
async fn migration_turns_existing_data_into_a_ledger() {
    let (_container, pool) = bare_postgres().await;
    let migrator = sqlx::migrate!("./migrations");
    migrator.run_to(3, &pool).await.unwrap();

    let user = Uuid::new_v4();
    let other = Uuid::new_v4();
    for (id, email) in [(user, "migra@prumo.test"), (other, "outra@prumo.test")] {
        sqlx::query("insert into users (id, email) values ($1, $2)")
            .bind(id)
            .bind(email)
            .execute(&pool)
            .await
            .unwrap();
    }
    // (rótulo, dono, classe, símbolo, quantidade de hoje, cadastrado em)
    let assets = [
        (
            "knri",
            user,
            "br_reits",
            Some("KNRI11"),
            "15",
            "2026-01-10T15:00:00Z",
        ),
        (
            "wege",
            user,
            "br_stocks",
            Some("WEGE3"),
            "8",
            "2026-01-10T15:00:00Z",
        ),
        (
            "hglg",
            user,
            "br_reits",
            Some("HGLG11"),
            "3",
            "2026-01-10T15:00:00Z",
        ),
        (
            "cdb",
            user,
            "fixed_income",
            None,
            "52000",
            "2026-01-05T15:00:00Z",
        ),
        (
            "btc",
            user,
            "crypto",
            Some("BTC"),
            "0.5",
            "2026-01-12T15:00:00Z",
        ),
        (
            "wish",
            user,
            "br_reits",
            Some("BTLG11"),
            "0",
            "2026-01-12T15:00:00Z",
        ),
        (
            "gone",
            user,
            "fixed_income",
            None,
            "0",
            "2026-01-05T15:00:00Z",
        ),
        // Cadastrado depois do primeiro aporte nele (o relógio da pessoa não manda).
        (
            "late",
            user,
            "br_stocks",
            Some("ITSA4"),
            "100",
            "2026-03-01T15:00:00Z",
        ),
        (
            "theirs",
            other,
            "br_reits",
            Some("KNRI11"),
            "7",
            "2026-01-10T15:00:00Z",
        ),
        (
            "doomed",
            user,
            "br_stocks",
            Some("SUMIU3"),
            "1",
            "2026-01-10T15:00:00Z",
        ),
    ];
    let mut ids: HashMap<&str, Uuid> = HashMap::new();
    for (label, owner, class, symbol, quantity, created) in assets {
        let id = Uuid::new_v4();
        ids.insert(label, id);
        sqlx::query(
            "insert into assets (id, user_id, asset_class, symbol, display_name, quantity, listed_on_b3, created_at)
             values ($1, $2, $3, $4, $5, $6::numeric, $7, $8::timestamptz)",
        )
        .bind(id)
        .bind(owner)
        .bind(class)
        .bind(symbol)
        .bind(format!("Ficticio {label}"))
        .bind(quantity)
        .bind(class == "br_reits" || class == "br_stocks")
        .bind(created)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Dois aportes. O primeiro às 01:00 UTC de 1/2 — em Brasília ainda 31/1.
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    for (id, owner, at) in [
        (first, user, "2026-02-01T01:00:00Z"),
        (second, user, "2026-02-15T14:00:00Z"),
    ] {
        sqlx::query("insert into contributions (id, user_id, amount, created_at) values ($1, $2, 10000, $3::timestamptz)")
            .bind(id)
            .bind(owner)
            .bind(at)
            .execute(&pool)
            .await
            .unwrap();
    }
    // (aporte, ativo, quantidade, preço, valor)
    let items = [
        (first, Some("knri"), "5", "140", "700"),
        (first, Some("wege"), "3", "50", "150"),
        (second, Some("wege"), "5", "52", "260"),
        (first, Some("hglg"), "5", "100", "500"),
        (second, Some("cdb"), "1000", "1", "1000"),
        (second, Some("gone"), "1000", "1", "1000"),
        (first, Some("late"), "10", "11", "110"),
        (second, Some("doomed"), "1", "20", "20"),
        // Item de um ativo já removido: fica só em contribution_items.
        (second, None, "2", "30", "60"),
    ];
    for (contribution, asset, quantity, price, value) in items {
        sqlx::query(
            "insert into contribution_items (id, contribution_id, asset_id, symbol, display_name, quantity, unit_price, value)
             values ($1, $2, $3, 'X', 'Ficticio', $4::numeric, $5::numeric, $6::numeric)",
        )
        .bind(Uuid::new_v4())
        .bind(contribution)
        .bind(asset.map(|a| ids[a]))
        .bind(quantity)
        .bind(price)
        .bind(value)
        .execute(&pool)
        .await
        .unwrap();
    }

    let store = Store::from_pool(pool.clone());
    store.migrate().await.unwrap();

    assert_book_matches(&store, &pool).await;
    let book = |label: &str| {
        let store = store.clone();
        let owner = if label == "theirs" { other } else { user };
        let id = ids[label];
        async move {
            let mut list = store
                .transactions(
                    owner,
                    TransactionFilter {
                        asset_id: Some(id),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            list.reverse(); // do mais antigo para o mais novo
            list
        }
    };
    let positions = store.positions(user).await.unwrap();

    // d > 0: saldo inicial de custo desconhecido, ANTES da compra do aporte.
    let knri = book("knri").await;
    let kinds: Vec<(Kind, Decimal, Option<Decimal>)> = knri
        .iter()
        .map(|t| (t.fields.kind, t.fields.quantity, t.fields.unit_price))
        .collect();
    assert_eq!(
        kinds,
        [
            (Kind::Opening, dec!(10), None),
            (Kind::Buy, dec!(5), Some(dec!(140)))
        ]
    );
    assert_eq!(knri[0].source.as_str(), "migration");
    assert_eq!(knri[0].fields.trade_date.to_string(), "2026-01-10");
    assert_eq!(knri[1].source.as_str(), "plan");
    assert_eq!(knri[1].contribution_id, Some(first));
    // 01:00 UTC de 1/2 é 31/1 em Brasília.
    assert_eq!(knri[1].fields.trade_date.to_string(), "2026-01-31");
    assert_eq!(positions[&ids["knri"]].cost_basis, None);

    // d = 0: tudo dos aportes, custo conhecido, sem saldo inicial.
    let wege = book("wege").await;
    assert!(wege.iter().all(|t| t.fields.kind == Kind::Buy));
    assert_eq!(positions[&ids["wege"]].cost_basis, Some(dec!(410)));
    assert_eq!(positions[&ids["wege"]].average_price(), Some(dec!(51.25)));

    // d < 0: a pessoa baixou a quantidade à mão; ajuste depois da compra, custo mantido.
    let hglg = book("hglg").await;
    assert_eq!(
        hglg.iter()
            .map(|t| (t.fields.kind, t.fields.quantity))
            .collect::<Vec<_>>(),
        [(Kind::Buy, dec!(5)), (Kind::Adjustment, dec!(-2))]
    );
    assert_eq!(positions[&ids["hglg"]].cost_basis, Some(dec!(500)));
    // Zerou à mão: custo zero.
    assert_eq!(positions[&ids["gone"]].quantity, dec!(0));
    assert_eq!(positions[&ids["gone"]].cost_basis, Some(dec!(0)));

    // Renda fixa por valor: a compra é o valor, a preço 1; o resto é saldo inicial.
    let cdb = book("cdb").await;
    assert_eq!(
        cdb.iter()
            .map(|t| (t.fields.kind, t.fields.quantity, t.fields.unit_price))
            .collect::<Vec<_>>(),
        [
            (Kind::Opening, dec!(51000), None),
            (Kind::Buy, dec!(1000), Some(dec!(1)))
        ]
    );

    // Sem aporte: só o saldo inicial. Sem quantidade: livro vazio, custo zero.
    assert_eq!(book("btc").await.len(), 1);
    assert!(book("wish").await.is_empty());
    assert_eq!(positions[&ids["wish"]].cost_basis, Some(dec!(0)));
    // Aporte antes do cadastro: o saldo inicial vem na data do aporte, antes da compra.
    let late = book("late").await;
    assert_eq!(late[0].fields.kind, Kind::Opening);
    assert_eq!(late[0].fields.trade_date, late[1].fields.trade_date);
    assert!(late[0].seq < late[1].seq);

    // Cada pessoa só tem o próprio livro; o item órfão não virou lançamento.
    assert_eq!(book("theirs").await.len(), 1);
    let mine = store
        .transactions(user, TransactionFilter::default())
        .await
        .unwrap();
    assert!(mine.iter().all(|t| t.asset_id != Some(ids["theirs"])));
    let (orphans,): (i64,) =
        sqlx::query_as("select count(*) from transactions where asset_id is null")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(orphans, 0);

    // Apagar o ativo deixa o livro dele como histórico.
    sqlx::query("delete from assets where id = $1")
        .bind(ids["doomed"])
        .execute(&pool)
        .await
        .unwrap();
    let history = store
        .transactions(user, TransactionFilter::default())
        .await
        .unwrap();
    assert!(history
        .iter()
        .any(|t| t.asset_id.is_none() && t.symbol.as_deref() == Some("SUMIU3")));
    assert_book_matches(&store, &pool).await;
}

/// A migração confere o próprio resultado: se o livro não bater com a
/// quantidade, ela falha e nada muda. Aqui roda só o esquema e a conferência,
/// sem a parte que monta o livro, para provar que a trava existe.
#[tokio::test]
async fn migration_refuses_to_finish_with_a_wrong_book() {
    let (_container, pool) = bare_postgres().await;
    let migrator = sqlx::migrate!("./migrations");
    migrator.run_to(3, &pool).await.unwrap();
    let sql = include_str!("../migrations/0004_ledger.sql");
    let check = &sql[sql.find("do $$").unwrap()..];
    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, 'trava@prumo.test')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    // O esquema novo sem os dados migrados: o ativo tem 5 e o livro, nada.
    let schema = &sql[..sql.find("create temporary table").unwrap()];
    sqlx::raw_sql(schema).execute(&pool).await.unwrap();
    sqlx::query(
        "insert into assets (id, user_id, asset_class, symbol, display_name, quantity, listed_on_b3)
         values ($1, $2, 'br_stocks', 'WEGE3', 'Ficticio', 5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let error = sqlx::raw_sql(check).execute(&pool).await.unwrap_err();
    assert!(error.to_string().contains("livro nao bate"), "{error}");
}

// ---------------------------------------------------------------------- API --

fn asset_of<'a>(portfolio: &'a Value, id: &str) -> &'a Value {
    find(&portfolio["assets"], "id", id)
}

async fn portfolio_asset(router: &axum::Router, id: &str) -> Value {
    asset_of(&get(router, "/api/v1/portfolio").await.body, id).clone()
}

#[tokio::test]
async fn transactions_api() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "livro@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &app.router;
    // Hoje em Brasília, como o servidor data o que grava sozinho.
    let today = (time::OffsetDateTime::now_utc() - time::Duration::hours(3))
        .date()
        .to_string();

    // Cadastro com preço médio: saldo inicial com custo. KNRI11 cotado a 142.
    let knri = post(r, "/api/v1/assets", json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10, "average_price": 100 })).await;
    assert_eq!(knri.status, StatusCode::CREATED, "{}", knri.text);
    assert_eq!(knri.body["quantity"], 10.0);
    assert_eq!(knri.body["average_price"], 100.0);
    assert_eq!(knri.body["cost_basis"], 1000.0);
    assert_eq!(knri.body["unrealized_gain"], 420.0);
    assert_eq!(knri.body["return_percent"], 42.0);
    assert_eq!(knri.body["realized_gain"], 0.0);
    let knri_id = knri.body["id"].as_str().unwrap().to_owned();
    let opening = get(r, &format!("/api/v1/transactions?asset_id={knri_id}")).await;
    // O saldo inicial nasce datado hoje; aqui ele vai para o começo do ano,
    // antes do histórico que vem a seguir.
    let opening_id = opening.body[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(opening.body[0]["trade_date"], today);
    let moved = patch(
        r,
        &format!("/api/v1/transactions/{opening_id}"),
        json!({ "trade_date": "2026-01-02" }),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text);
    let opening = get(r, &format!("/api/v1/transactions?asset_id={knri_id}")).await;
    assert_eq!(opening.status, StatusCode::OK);
    assert_eq!(opening.body.as_array().unwrap().len(), 1);
    let opening = &opening.body[0];
    assert_eq!(opening["kind"], "opening");
    assert_eq!(opening["unit_price"], 100.0);
    assert_eq!(opening["source"], "manual");
    assert_eq!(opening["currency"], "BRL");
    assert_eq!(opening["fx_rate"], Value::Null);
    assert_eq!(opening["symbol"], "KNRI11");
    assert_eq!(opening["trade_date"], "2026-01-02");

    // Sem preço médio: custo desconhecido, PM e rentabilidade nulos (não zero).
    let hglg = post(
        r,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "HGLG11", "quantity": 4 }),
    )
    .await;
    assert_eq!(hglg.body["average_price"], Value::Null);
    assert_eq!(hglg.body["cost_basis"], Value::Null);
    assert_eq!(hglg.body["unrealized_gain"], Value::Null);
    assert_eq!(hglg.body["return_percent"], Value::Null);
    let hglg_id = hglg.body["id"].as_str().unwrap().to_owned();
    // Sem quantidade: livro vazio, custo zero, sem rentabilidade.
    let wish = post(r, "/api/v1/assets", json!({ "asset_class": "br_reits", "symbol": "BTLG11", "quantity": 0, "score_override": 5 })).await;
    assert_eq!(wish.body["cost_basis"], 0.0);
    assert_eq!(wish.body["return_percent"], Value::Null);
    // Renda fixa: aplicou 50 mil, vale 52 mil — saldo inicial + ajuste do rendimento.
    let cdb = post(r, "/api/v1/assets", json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 52000, "invested": 50000, "score_override": 6 })).await;
    assert_eq!(cdb.status, StatusCode::CREATED, "{}", cdb.text);
    assert_eq!(cdb.body["current_value"], 52000.0);
    assert_eq!(cdb.body["cost_basis"], 50000.0);
    assert_eq!(
        cdb.body["average_price"],
        Value::Null,
        "renda fixa por valor nao tem PM"
    );
    assert_eq!(cdb.body["return_percent"], 4.0);
    let cdb_id = cdb.body["id"].as_str().unwrap().to_owned();
    let cdb_book = get(r, &format!("/api/v1/transactions?asset_id={cdb_id}"))
        .await
        .body;
    let kinds: Vec<&str> = cdb_book
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["adjustment", "opening"],
        "do mais novo para o mais antigo"
    );
    for (body, why) in [
        (
            json!({ "asset_class": "br_reits", "symbol": "MXRF11", "quantity": 0, "average_price": 10 }),
            "PM sem quantidade",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "MXRF11", "quantity": 1, "average_price": 0 }),
            "PM zero",
        ),
        (
            json!({ "asset_class": "br_reits", "symbol": "MXRF11", "quantity": 1, "invested": 10 }),
            "aplicado em ativo cotado",
        ),
        (
            json!({ "asset_class": "fixed_income", "display_name": "X", "amount": 10, "average_price": 1 }),
            "PM em renda fixa",
        ),
        (
            json!({ "asset_class": "fixed_income", "display_name": "X", "amount": 0, "invested": 1 }),
            "aplicado sem valor",
        ),
    ] {
        let reply = post(r, "/api/v1/assets", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
    }

    // Compra com taxa: PM ponderado com a taxa.
    let buy = post(r, "/api/v1/transactions", json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "2026-03-02", "quantity": 10, "unit_price": 120, "fees": 10 })).await;
    assert_eq!(buy.status, StatusCode::CREATED, "{}", buy.text);
    let buy_id = buy.body["id"].as_str().unwrap().to_owned();
    let asset = portfolio_asset(r, &knri_id).await;
    assert_eq!(asset["quantity"], 20.0);
    assert_eq!(asset["cost_basis"], 2210.0);
    assert_eq!(asset["average_price"], 110.5);
    // Venda: o PM não muda, o lucro realiza. 15 × 150 − 5 − 15 × 110,50.
    let sell = post(r, "/api/v1/transactions", json!({ "asset_id": knri_id, "kind": "sell", "trade_date": "2026-03-10", "quantity": 15, "unit_price": 150, "fees": 5 })).await;
    assert_eq!(sell.status, StatusCode::CREATED, "{}", sell.text);
    let sell_id = sell.body["id"].as_str().unwrap().to_owned();
    let asset = portfolio_asset(r, &knri_id).await;
    assert_eq!(asset["quantity"], 5.0);
    assert_eq!(asset["average_price"], 110.5);
    assert_eq!(asset["realized_gain"], 587.5);
    assert_eq!(asset["cost_basis"], 552.5);

    // Venda maior que a posição: recusada, nada muda.
    let oversold = post(r, "/api/v1/transactions", json!({ "asset_id": knri_id, "kind": "sell", "trade_date": "2026-03-11", "quantity": 6, "unit_price": 150 })).await;
    assert_eq!(oversold.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(oversold.body["code"], "exceeds_position");
    assert!(
        oversold
            .text
            .contains("A venda de 2026-03-11 (6) passa da posicao de 5"),
        "{}",
        oversold.text
    );
    // Venda antes da compra que a sustenta: a data importa, não só o total.
    let early = post(r, "/api/v1/transactions", json!({ "asset_id": knri_id, "kind": "sell", "trade_date": "2026-03-01", "quantity": 12, "unit_price": 150 })).await;
    assert_eq!(early.body["code"], "exceeds_position", "{}", early.text);
    // Apagar ou encolher a compra antiga que sustenta a venda: recusado.
    let gone = delete(r, &format!("/api/v1/transactions/{buy_id}")).await;
    assert_eq!(gone.body["code"], "exceeds_position", "{}", gone.text);
    let shrink = patch(
        r,
        &format!("/api/v1/transactions/{buy_id}"),
        json!({ "quantity": 1 }),
    )
    .await;
    assert_eq!(shrink.body["code"], "exceeds_position", "{}", shrink.text);
    // Mover a compra para depois da venda também.
    let later = patch(
        r,
        &format!("/api/v1/transactions/{buy_id}"),
        json!({ "trade_date": "2026-03-20" }),
    )
    .await;
    assert_eq!(later.body["code"], "exceeds_position", "{}", later.text);
    assert_eq!(portfolio_asset(r, &knri_id).await["quantity"], 5.0);

    // Editar a compra antiga muda tudo depois dela: PM e lucro da venda.
    let edited = patch(
        r,
        &format!("/api/v1/transactions/{buy_id}"),
        json!({ "unit_price": 100, "fees": 0 }),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);
    assert_eq!(edited.body["unit_price"], 100.0);
    let asset = portfolio_asset(r, &knri_id).await;
    assert_eq!(asset["average_price"], 100.0);
    // 15 × 150 − 5 − 15 × 100
    assert_eq!(asset["realized_gain"], 745.0);
    // Apagar a venda devolve as cotas.
    assert_eq!(
        delete(r, &format!("/api/v1/transactions/{sell_id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(portfolio_asset(r, &knri_id).await["quantity"], 20.0);
    assert_eq!(
        delete(r, &format!("/api/v1/transactions/{sell_id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Eventos: desdobramento preserva o custo; bonificação com custo atribuído.
    let split = post(
        r,
        "/api/v1/transactions",
        json!({ "asset_id": knri_id, "kind": "split", "trade_date": "2026-03-12", "quantity": 20 }),
    )
    .await;
    assert_eq!(split.status, StatusCode::CREATED, "{}", split.text);
    let asset = portfolio_asset(r, &knri_id).await;
    assert_eq!(
        (asset["quantity"].clone(), asset["cost_basis"].clone()),
        (json!(40.0), json!(2000.0))
    );
    assert_eq!(asset["average_price"], 50.0);
    let bonus = post(r, "/api/v1/transactions", json!({ "asset_id": knri_id, "kind": "bonus", "trade_date": "2026-03-13", "quantity": 4, "unit_price": 10 })).await;
    assert_eq!(bonus.status, StatusCode::CREATED, "{}", bonus.text);
    assert_eq!(portfolio_asset(r, &knri_id).await["cost_basis"], 2040.0);

    // "Informe o preço médio": o saldo inicial sem custo ganha preço e tudo acende.
    let hglg_book = get(r, &format!("/api/v1/transactions?asset_id={hglg_id}"))
        .await
        .body;
    let hglg_opening = hglg_book[0]["id"].as_str().unwrap().to_owned();
    let priced = patch(
        r,
        &format!("/api/v1/transactions/{hglg_opening}"),
        json!({ "unit_price": 130 }),
    )
    .await;
    assert_eq!(priced.status, StatusCode::OK, "{}", priced.text);
    let asset = portfolio_asset(r, &hglg_id).await;
    assert_eq!(asset["average_price"], 130.0);
    // 4 × 140,80 − 520
    assert_eq!(asset["unrealized_gain"], 43.2);
    assert_eq!(asset["return_percent"], 8.31);
    // E volta a desconhecido.
    patch(
        r,
        &format!("/api/v1/transactions/{hglg_opening}"),
        json!({ "unit_price": null }),
    )
    .await;
    assert_eq!(
        portfolio_asset(r, &hglg_id).await["average_price"],
        Value::Null
    );

    // Editar a quantidade na folha do ativo grava um ajuste, com o custo mantido.
    let edited = patch(
        r,
        &format!("/api/v1/assets/{knri_id}"),
        json!({ "quantity": 50 }),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);
    assert_eq!(edited.body["quantity"], 50.0);
    assert_eq!(edited.body["cost_basis"], 2040.0);
    let newest = &get(r, &format!("/api/v1/transactions?asset_id={knri_id}"))
        .await
        .body[0];
    assert_eq!(
        (newest["kind"].clone(), newest["quantity"].clone()),
        (json!("adjustment"), json!(6.0))
    );
    // Mesma quantidade: nenhum ajuste.
    patch(
        r,
        &format!("/api/v1/assets/{knri_id}"),
        json!({ "quantity": 50, "display_name": "Kinea" }),
    )
    .await;
    let count = get(r, &format!("/api/v1/transactions?asset_id={knri_id}"))
        .await
        .body
        .as_array()
        .unwrap()
        .len();
    assert_eq!(count, 5);
    // Renda fixa: atualizar o valor é ajuste, e a rentabilidade acompanha.
    let cdb_up = patch(
        r,
        &format!("/api/v1/assets/{cdb_id}"),
        json!({ "amount": 55000 }),
    )
    .await;
    assert_eq!(cdb_up.body["return_percent"], 10.0);

    // "Já comprei": cada item vira compra ligada ao aporte.
    let contribution = post(
        r,
        "/api/v1/contributions",
        json!({ "amount": 2000, "items": [
        { "asset_id": hglg_id, "quantity": 2, "unit_price": 140.8 },
        { "asset_id": cdb_id, "quantity": 1000, "unit_price": 1 }
    ] }),
    )
    .await;
    assert_eq!(
        contribution.status,
        StatusCode::CREATED,
        "{}",
        contribution.text
    );
    let contribution_id = contribution.body["id"].clone();
    let hglg_book = get(r, &format!("/api/v1/transactions?asset_id={hglg_id}"))
        .await
        .body;
    assert_eq!(hglg_book[0]["kind"], "buy");
    assert_eq!(hglg_book[0]["source"], "plan");
    assert_eq!(hglg_book[0]["contribution_id"], contribution_id);
    assert_eq!(portfolio_asset(r, &hglg_id).await["quantity"], 6.0);
    let cdb_buy = &get(r, &format!("/api/v1/transactions?asset_id={cdb_id}"))
        .await
        .body[0];
    assert_eq!(
        (cdb_buy["quantity"].clone(), cdb_buy["unit_price"].clone()),
        (json!(1000.0), json!(1.0))
    );
    assert_eq!(portfolio_asset(r, &cdb_id).await["cost_basis"], 51000.0);

    // Renda fixa: resgate realiza o rendimento proporcional.
    let redeem = post(
        r,
        "/api/v1/transactions",
        json!({ "asset_id": cdb_id, "kind": "redemption", "trade_date": today, "quantity": 11200 }),
    )
    .await;
    assert_eq!(redeem.status, StatusCode::CREATED, "{}", redeem.text);
    let asset = portfolio_asset(r, &cdb_id).await;
    // Tinha 56000 de saldo com custo 51000: saem 11200 de saldo e 10200 de custo.
    assert_eq!(asset["current_value"], 44800.0);
    assert_eq!(asset["cost_basis"], 40800.0);
    assert_eq!(asset["realized_gain"], 1000.0);

    // Ativo em dólar: preço e câmbio do dia; custo e PM em reais.
    let aapl = post(
        r,
        "/api/v1/assets",
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 0 }),
    )
    .await;
    let aapl_id = aapl.body["id"].as_str().unwrap().to_owned();
    let usd = post(r, "/api/v1/transactions", json!({ "asset_id": aapl_id, "kind": "buy", "trade_date": "2026-03-03", "quantity": 2, "unit_price": 180, "fees": 1, "currency": "usd", "fx_rate": 5 })).await;
    assert_eq!(usd.status, StatusCode::CREATED, "{}", usd.text);
    assert_eq!(usd.body["currency"], "USD");
    assert_eq!(usd.body["fx_rate"], 5.0);
    let asset = portfolio_asset(r, &aapl_id).await;
    // (2 × 180 + 1) × 5 = 1805; hoje 2 × 200 × 5,50 = 2200.
    assert_eq!(asset["cost_basis"], 1805.0);
    assert_eq!(asset["average_price"], 902.5);
    assert_eq!(asset["unrealized_gain"], 395.0);

    // Validação.
    for (body, why) in [
        (
            json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "2026-03-02", "quantity": 1.5, "unit_price": 1 }),
            "fracao em cota",
        ),
        (
            json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "2026-03-02", "quantity": 1 }),
            "compra sem preco",
        ),
        (
            json!({ "asset_id": knri_id, "kind": "split", "trade_date": "2026-03-02", "quantity": 1, "unit_price": 3 }),
            "evento com preco",
        ),
        (
            json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "02/03/2026", "quantity": 1, "unit_price": 1 }),
            "data em outro formato",
        ),
        (
            json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "2999-01-01", "quantity": 1, "unit_price": 1 }),
            "data no futuro",
        ),
        (
            json!({ "asset_id": knri_id, "kind": "swap", "trade_date": "2026-03-02", "quantity": 1 }),
            "tipo desconhecido",
        ),
        (
            json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "2026-03-02", "quantity": 1, "unit_price": 1, "x": 1 }),
            "campo desconhecido",
        ),
        (
            json!({ "asset_id": aapl_id, "kind": "buy", "trade_date": "2026-03-02", "quantity": 1, "unit_price": 1, "currency": "USD" }),
            "dolar sem cambio",
        ),
        (
            json!({ "asset_id": cdb_id, "kind": "amortization", "trade_date": "2026-03-02", "quantity": 1, "unit_price": 1 }),
            "amortizacao em renda fixa por valor",
        ),
    ] {
        let reply = post(r, "/api/v1/transactions", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
    }
    let ghost = post(r, "/api/v1/transactions", json!({ "asset_id": Uuid::new_v4(), "kind": "buy", "trade_date": "2026-03-02", "quantity": 1, "unit_price": 1 })).await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    assert_eq!(
        patch(
            r,
            &format!("/api/v1/transactions/{}", Uuid::new_v4()),
            json!({ "quantity": 1 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(
            r,
            "/api/v1/transactions/nao-e-uuid",
            json!({ "quantity": 1 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(
            r,
            &format!("/api/v1/transactions/{buy_id}"),
            json!({ "kind": "split" })
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Filtros: ativo e período (pontas incluídas).
    let period = get(r, "/api/v1/transactions?from=2026-03-12&to=2026-03-13").await;
    assert_eq!(period.status, StatusCode::OK);
    let kinds: Vec<&str> = period
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["bonus", "split"]);
    assert_eq!(
        get(r, "/api/v1/transactions?from=2026-03-13&to=2026-03-12")
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        get(r, "/api/v1/transactions?from=ontem").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let all = get(r, "/api/v1/transactions").await.body;
    let dates: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["trade_date"].as_str().unwrap())
        .collect();
    assert!(
        dates.windows(2).all(|w| w[0] >= w[1]),
        "do mais novo para o mais antigo: {dates:?}"
    );

    // Dono: outra pessoa não vê, não edita, não apaga e não lança no ativo alheio.
    let intruder = common::app(
        &db.store,
        "intrusa@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let ir = &intruder.router;
    assert_eq!(get(ir, "/api/v1/transactions").await.body, json!([]));
    assert_eq!(
        get(ir, &format!("/api/v1/transactions?asset_id={knri_id}"))
            .await
            .body,
        json!([])
    );
    assert_eq!(
        patch(
            ir,
            &format!("/api/v1/transactions/{buy_id}"),
            json!({ "quantity": 99 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(ir, &format!("/api/v1/transactions/{buy_id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let foreign = post(ir, "/api/v1/transactions", json!({ "asset_id": knri_id, "kind": "buy", "trade_date": "2026-03-02", "quantity": 1, "unit_price": 1 })).await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!(portfolio_asset(r, &knri_id).await["quantity"], 50.0);

    // Ativo removido: o livro dele fica de histórico; só dá para apagar.
    assert_eq!(
        delete(r, &format!("/api/v1/assets/{aapl_id}")).await.status,
        StatusCode::NO_CONTENT
    );
    let orphan = get(r, "/api/v1/transactions")
        .await
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["symbol"] == "AAPL")
        .cloned()
        .expect("lancamento de historico");
    assert_eq!(orphan["asset_id"], Value::Null);
    let orphan_id = orphan["id"].as_str().unwrap();
    assert_eq!(
        patch(
            r,
            &format!("/api/v1/transactions/{orphan_id}"),
            json!({ "quantity": 1 })
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        delete(r, &format!("/api/v1/transactions/{orphan_id}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    // No fim, a soma do livro é a quantidade de todo ativo.
    let pool = sqlx_pool(&db).await;
    assert_book_matches(&db.store, &pool).await;
    let knri_qty = num(&portfolio_asset(r, &knri_id).await["quantity"]);
    assert_eq!(knri_qty, dec!(50));
}

/// O pool do banco do teste (o `Store` não expõe o dele).
async fn sqlx_pool(db: &Database) -> PgPool {
    PgPool::connect(&db.url).await.unwrap()
}

/// Muitas escritas fora de ordem, inclusive recusadas: o cache sempre bate
/// com o livro refeito do zero.
#[tokio::test]
async fn the_cache_always_matches_a_full_replay() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "cache@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &app.router;
    let asset = post(r, "/api/v1/assets", json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 1, "average_price": 300000, "score_override": 5 })).await;
    let id = asset.body["id"].as_str().unwrap().to_owned();
    let mut accepted = 0;
    let mut refused = 0;
    let mut ids = Vec::new();
    for i in 0..60u32 {
        let day = 1 + (i * 7) % 28;
        let date = format!("2026-02-{day:02}");
        let body = match i % 5 {
            0 | 1 => {
                json!({ "asset_id": id, "kind": "buy", "trade_date": date, "quantity": format!("0.{:03}", 100 + i), "unit_price": 300000 + i * 1000, "fees": 1 })
            }
            2 => {
                json!({ "asset_id": id, "kind": "sell", "trade_date": date, "quantity": "0.35", "unit_price": 350000 })
            }
            3 => {
                json!({ "asset_id": id, "kind": "adjustment", "trade_date": date, "quantity": "-0.05" })
            }
            _ => json!({ "asset_id": id, "kind": "bonus", "trade_date": date, "quantity": "0.01" }),
        };
        let reply = post(r, "/api/v1/transactions", body).await;
        match reply.status {
            StatusCode::CREATED => {
                accepted += 1;
                ids.push(reply.body["id"].as_str().unwrap().to_owned());
            }
            StatusCode::UNPROCESSABLE_ENTITY => {
                assert_eq!(reply.body["code"], "exceeds_position", "{}", reply.text);
                refused += 1;
            }
            other => panic!("{other}: {}", reply.text),
        }
        // De vez em quando apaga um antigo (pode ser recusado, se sustenta uma venda).
        if i % 9 == 8 {
            let victim = ids.remove(0);
            let gone = delete(r, &format!("/api/v1/transactions/{victim}")).await;
            if gone.status != StatusCode::NO_CONTENT {
                assert_eq!(gone.body["code"], "exceeds_position");
                ids.push(victim);
            }
        }
    }
    assert!(
        accepted > 20 && refused > 0,
        "{accepted} aceitos, {refused} recusados"
    );
    let pool = sqlx_pool(&db).await;
    assert_book_matches(&db.store, &pool).await;
}
