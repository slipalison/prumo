//! A gravação da importação da B3: prévia com estados, decisões da pessoa,
//! `commit` tudo ou nada, reimportação, provisionado que vira recebido,
//! eventos, saldo inicial, dono e o que fica coerente depois.
//!
//! As planilhas são FICTÍCIAS, montadas aqui no formato da Área do Investidor
//! (cabeçalhos iguais): tickers públicos, quantidades e valores inventados.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::*;
use http_body_util::BodyExt;
use prumo::domain::asset_class::AssetClass;
use prumo::domain::ledger::Kind;
use prumo::store::{NewAsset, NewContributionItem, Source, Store, TransactionFilter};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use rust_xlsxwriter::Workbook;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

// --- planilhas fictícias -------------------------------------------------------

enum Cell {
    Text(String),
    Num(f64),
}

fn t(text: &str) -> Cell {
    Cell::Text(text.to_owned())
}

fn n(number: f64) -> Cell {
    Cell::Num(number)
}

fn workbook(name: &str, header: &[&str], rows: Vec<Vec<Cell>>) -> Vec<u8> {
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name(name).unwrap();
    for (c, title) in header.iter().enumerate() {
        sheet
            .write_string(0, u16::try_from(c).unwrap(), *title)
            .unwrap();
    }
    for (r, row) in rows.iter().enumerate() {
        let r = u32::try_from(r + 1).unwrap();
        for (c, cell) in row.iter().enumerate() {
            let c = u16::try_from(c).unwrap();
            match cell {
                Cell::Text(text) => {
                    sheet.write_string(r, c, text).unwrap();
                }
                Cell::Num(number) => {
                    sheet.write_number(r, c, *number).unwrap();
                }
            }
        }
    }
    book.save_to_buffer().unwrap()
}

const BROKER: &str = "CORRETORA FICTICIA S/A";

/// Negociação: (data, Compra/Venda, código, quantidade, preço).
fn trades(rows: &[(&str, &str, &str, f64, f64)]) -> Vec<u8> {
    workbook(
        "Negociação",
        &[
            "Data do Negócio",
            "Tipo de Movimentação",
            "Mercado",
            "Prazo/Vencimento",
            "Instituição",
            "Código de Negociação",
            "Quantidade",
            "Preço",
            "Valor",
        ],
        rows.iter()
            .map(|(date, side, code, quantity, price)| {
                vec![
                    t(date),
                    t(side),
                    t("Mercado à Vista"),
                    t("-"),
                    t(BROKER),
                    t(code),
                    n(*quantity),
                    n(*price),
                    n(quantity * price),
                ]
            })
            .collect(),
    )
}

/// Uma linha da Movimentação: (Credito/Debito, data, movimentação, produto,
/// quantidade, preço, valor).
type Movement<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    f64,
    Option<f64>,
    Option<f64>,
);

fn movements(rows: &[Movement<'_>]) -> Vec<u8> {
    workbook(
        "Movimentação",
        &[
            "Entrada/Saída",
            "Data",
            "Movimentação",
            "Produto",
            "Instituição",
            "Quantidade",
            "Preço unitário",
            "Valor da Operação",
        ],
        rows.iter()
            .map(
                |(direction, date, movement, product, quantity, price, value)| {
                    let opt = |v: &Option<f64>| v.map_or_else(|| t("-"), n);
                    vec![
                        t(direction),
                        t(date),
                        t(movement),
                        t(product),
                        t(BROKER),
                        n(*quantity),
                        opt(price),
                        opt(value),
                    ]
                },
            )
            .collect(),
    )
}

/// Proventos recebidos: (produto, pagamento, evento, quantidade, preço, líquido).
fn received(rows: &[(&str, &str, &str, f64, f64, f64)]) -> Vec<u8> {
    workbook(
        "Proventos Recebidos",
        &[
            "Produto",
            "Pagamento",
            "Tipo de Evento",
            "Instituição",
            "Quantidade",
            "Preço unitário",
            "Valor líquido",
        ],
        rows.iter()
            .map(|(product, date, kind, quantity, price, net)| {
                vec![
                    t(product),
                    t(date),
                    t(kind),
                    t(BROKER),
                    n(*quantity),
                    n(*price),
                    n(*net),
                ]
            })
            .collect(),
    )
}

/// Proventos a receber: (produto, previsão, evento, quantidade, preço, líquido).
fn provisioned(rows: &[(&str, &str, &str, f64, f64, f64)]) -> Vec<u8> {
    workbook(
        "Proventos a Receber",
        &[
            "Produto",
            "Tipo",
            "Tipo de Evento",
            "Previsão de pagamento",
            "Instituição",
            "Conta",
            "Quantidade",
            "Preço unitário",
            "Valor líquido",
        ],
        rows.iter()
            .map(|(product, date, kind, quantity, price, net)| {
                vec![
                    t(product),
                    t("ON"),
                    t(kind),
                    t(date),
                    t(BROKER),
                    n(12345.0),
                    n(*quantity),
                    n(*price),
                    n(*net),
                ]
            })
            .collect(),
    )
}

const WEG: &str = "WEGE3 - WEG S.A.";
const HGLG: &str = "HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII";

/// O envio do cenário principal.
fn main_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "negociacao.xlsx",
            trades(&[
                ("10/03/2020", "Compra", "WEGE3", 100.0, 20.0),
                ("15/06/2021", "Venda", "WEGE3", 30.0, 35.0),
                // KNRI11 só pelo código: a classe pede confirmação.
                ("05/01/2022", "Compra", "KNRI11", 10.0, 150.0),
                // Venda sem compra no extrato: comprado antes de 2019.
                ("02/02/2023", "Venda", "HGLG11", 5.0, 160.0),
            ]),
        ),
        (
            "movimentacao.xlsx",
            movements(&[
                ("Credito", "10/04/2021", "Desdobro", WEG, 100.0, None, None),
                (
                    "Credito",
                    "15/01/2023",
                    "Rendimento",
                    HGLG,
                    5.0,
                    Some(1.1),
                    Some(5.5),
                ),
                (
                    "Credito",
                    "02/04/2024",
                    "COMPRA / VENDA",
                    "Tesouro Selic 2029",
                    1.5,
                    Some(14000.0),
                    Some(21000.0),
                ),
                (
                    "Credito",
                    "03/05/2024",
                    "Juros",
                    "Tesouro Selic 2029",
                    0.0,
                    None,
                    Some(10.0),
                ),
                (
                    "Credito",
                    "20/07/2021",
                    "Fração em Ativos",
                    WEG,
                    0.5,
                    None,
                    None,
                ),
            ]),
        ),
        (
            "proventos-recebidos.xlsx",
            received(&[(
                WEG,
                "20/05/2022",
                "Juros Sobre Capital Próprio",
                170.0,
                0.5,
                72.25,
            )]),
        ),
        (
            "proventos-a-receber.xlsx",
            provisioned(&[
                (WEG, "20/12/2026", "Dividendo", 170.0, 1.0, 170.0),
                (WEG, "-", "Dividendo", 170.0, 0.3, 51.0),
            ]),
        ),
    ]
}

// --- envio ---------------------------------------------------------------------

const BOUNDARY: &str = "----prumo-teste-commit";

fn multipart(files: &[(&str, Vec<u8>)], decisions: Option<&Value>) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(decisions) = decisions {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"decisions\"\r\nContent-Type: application/json\r\n\r\n{decisions}\r\n"
            )
            .as_bytes(),
        );
    }
    for (name, bytes) in files {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{name}\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn send(
    router: &axum::Router,
    route: &str,
    files: &[(&str, Vec<u8>)],
    decisions: Option<&Value>,
) -> Reply {
    let body = multipart(files, decisions);
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/imports/b3/{route}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .header(header::CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn preview(router: &axum::Router, files: &[(&str, Vec<u8>)], decisions: &Value) -> Reply {
    let reply = send(router, "preview", files, Some(decisions)).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply
}

/// Prévia e gravação com o `digest` dela.
async fn import(
    router: &axum::Router,
    files: &[(&str, Vec<u8>)],
    decisions: Value,
) -> (Reply, Reply) {
    let seen = preview(router, files, &decisions).await;
    let mut with_digest = decisions;
    with_digest["digest"] = seen.body["plan"]["digest"].clone();
    let done = send(router, "commit", files, Some(&with_digest)).await;
    (seen, done)
}

fn line<'a>(reply: &'a Reply, asset: &str, group: &str, index: usize) -> &'a Value {
    let external_ref = find(&reply.body["assets"], "key", asset)[group][index]["external_ref"]
        .as_str()
        .unwrap()
        .to_owned();
    &reply.body["plan"]["lines"][external_ref]
}

fn plan_of<'a>(reply: &'a Reply, key: &str) -> &'a Value {
    find(&reply.body["plan"]["assets"], "key", key)
}

fn num(value: &Value) -> Decimal {
    let text = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    Decimal::from_str(&text).unwrap_or_else(|_| panic!("numero: {value}"))
}

/// Quantas linhas a pessoa tem em cada tabela que a importação toca.
async fn counts(pool: &PgPool, user: Uuid) -> (i64, i64, i64) {
    let (assets,): (i64,) = sqlx::query_as("select count(*) from assets where user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap();
    let (transactions,): (i64,) =
        sqlx::query_as("select count(*) from transactions where user_id = $1")
            .bind(user)
            .fetch_one(pool)
            .await
            .unwrap();
    let (income,): (i64,) = sqlx::query_as("select count(*) from income_events where user_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap();
    (assets, transactions, income)
}

async fn asset_by_symbol(store: &Store, user: Uuid, symbol: &str) -> prumo::store::Asset {
    store
        .assets(user)
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.symbol.as_deref() == Some(symbol))
        .unwrap_or_else(|| panic!("{symbol} nao esta na carteira"))
}

/// A tabela do Tesouro conhece o Selic 2029.
async fn seed_treasury(pool: &PgPool) {
    sqlx::query(
        "insert into quotes (symbol, price, currency, name, fetched_at, traded_at)
         values ('TD-SELIC-20290301', 15000, 'BRL', 'Tesouro Selic 2029', now(), now())
         on conflict do nothing",
    )
    .execute(pool)
    .await
    .unwrap();
}

// --- o fluxo inteiro --------------------------------------------------------------

#[tokio::test]
async fn preview_marks_lines_and_commit_writes_everything_once() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    seed_treasury(&pool).await;
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "importa@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("importa@prumo.test").await.unwrap();
    let files = main_files();

    // 1. A prévia: tudo novo, KNRI11 por confirmar, saldo inicial do HGLG11.
    let first = preview(&app.router, &files, &json!({})).await;
    let plan = &first.body["plan"];
    assert_eq!(plan["ready"], false);
    assert_eq!(plan["summary"]["needs_confirmation"], 1);
    assert_eq!(plan_of(&first, "KNRI11")["class_needs_confirmation"], true);
    assert_eq!(plan_of(&first, "KNRI11")["asset_class"], "br_reits");
    assert_eq!(plan_of(&first, "WEGE3")["class_needs_confirmation"], false);
    assert_eq!(line(&first, "WEGE3", "trades", 0)["status"], "new");
    let hglg = plan_of(&first, "HGLG11");
    assert_eq!(hglg["opening"]["quantity"], 5.0);
    assert_eq!(hglg["opening"]["basis"], "covers_sales");
    assert_eq!(hglg["opening"]["date"], "2023-02-01");
    let bond = plan_of(&first, "TESOURO SELIC 2029");
    assert_eq!(bond["symbol"], "TD-SELIC-20290301");
    assert_eq!(bond["action"], "create");
    // Juros do Tesouro, fração em ativos e provisionado sem data ficam de fora.
    let reasons: Vec<&str> = plan["lines"]
        .as_object()
        .unwrap()
        .values()
        .filter(|l| l["status"] == "not_imported")
        .map(|l| l["reason_code"].as_str().unwrap())
        .collect();
    for reason in [
        "treasury_interest",
        "fraction_in_kind",
        "provisioned_without_date",
    ] {
        assert!(reasons.contains(&reason), "{reason}: {reasons:?}");
    }

    // 2. Gravar sem confirmar a classe: recusado, nada gravado.
    let blocked = send(
        &app.router,
        "commit",
        &files,
        Some(&json!({ "digest": plan["digest"] })),
    )
    .await;
    assert_eq!(
        blocked.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        blocked.text
    );
    assert_eq!(blocked.body["code"], "import_blocked");
    assert_eq!(counts(&pool, user).await, (0, 0, 0));
    // Sem o digest, nem tenta.
    let no_digest = send(&app.router, "commit", &files, Some(&json!({}))).await;
    assert_eq!(no_digest.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(no_digest.body["detail"]
        .as_str()
        .unwrap()
        .contains("digest"));

    // 3. Confirma a classe e escolhe o preço médio do saldo inicial.
    let decisions = json!({
        "assets": {
            "KNRI11": { "asset_class": "br_reits" },
            "HGLG11": { "opening": { "quantity": "5", "unit_price": "100" } }
        }
    });
    let (seen, done) = import(&app.router, &files, decisions.clone()).await;
    assert_eq!(seen.body["plan"]["ready"], true);
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    assert_eq!(done.headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    let summary = &done.body["summary"];
    assert_eq!(summary["assets_created"], 4);
    // WEGE3: compra, venda, desdobro; KNRI11: compra; HGLG11: venda; Tesouro: compra.
    assert_eq!(summary["transactions_new"], 6);
    assert_eq!(summary["openings"], 1);
    // JCP, rendimento do HGLG11 e o dividendo a receber.
    assert_eq!(summary["income_new"], 3);
    let left_out: Vec<&str> = done.body["not_imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["reason_code"].as_str().unwrap())
        .collect();
    assert_eq!(left_out.len(), 3, "{left_out:?}");

    // 4. O livro e o cache da carteira.
    let wege = asset_by_symbol(&db.store, user, "WEGE3").await;
    assert_eq!(wege.asset_class, AssetClass::BrStocks);
    assert_eq!(wege.quantity, dec!(170));
    let positions = db.store.positions(user).await.unwrap();
    // 2000 de custo; a venda de 30 das 200 (depois do desdobro) tira 300.
    assert_eq!(positions[&wege.id].cost_basis, Some(dec!(1700)));
    assert_eq!(positions[&wege.id].realized_gain, Some(dec!(750)));
    let knri = asset_by_symbol(&db.store, user, "KNRI11").await;
    assert_eq!(
        (knri.asset_class, knri.quantity),
        (AssetClass::BrReits, dec!(10))
    );
    let hglg = asset_by_symbol(&db.store, user, "HGLG11").await;
    assert_eq!(hglg.quantity, dec!(0));
    assert_eq!(positions[&hglg.id].realized_gain, Some(dec!(300)));
    let bond = asset_by_symbol(&db.store, user, "TD-SELIC-20290301").await;
    assert!(bond.treasury);
    assert_eq!(bond.quantity, dec!(1.5));
    let ledger = db
        .store
        .transactions(user, TransactionFilter::default())
        .await
        .unwrap();
    assert_eq!(ledger.len(), 7);
    assert!(ledger.iter().all(|t| t.source == Source::B3Import));
    assert_eq!(
        ledger.iter().filter(|t| t.external_ref.is_none()).count(),
        1
    );
    let opening = ledger
        .iter()
        .find(|t| t.fields.kind == Kind::Opening)
        .unwrap();
    assert_eq!(opening.fields.unit_price, Some(dec!(100)));
    let income = db.store.income_ledger(user).await.unwrap();
    let jcp = income.iter().find(|i| i.kind.as_str() == "jcp").unwrap();
    assert_eq!(
        (jcp.gross, jcp.withholding, jcp.net),
        (dec!(85), dec!(12.75), dec!(72.25))
    );
    assert_eq!(jcp.asset_id, Some(wege.id));
    let (provisional,): (i64,) =
        sqlx::query_as("select count(*) from income_events where user_id = $1 and provisional")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(provisional, 1);

    // 5. Reimportar os mesmos arquivos: tudo já existe, nada muda.
    let before = counts(&pool, user).await;
    let (again, redone) = import(&app.router, &files, decisions).await;
    assert_eq!(line(&again, "WEGE3", "trades", 0)["status"], "existing");
    // O provento sem data vem primeiro (fica de fora); o JCP e o a receber, depois.
    assert_eq!(line(&again, "WEGE3", "income", 1)["status"], "existing");
    assert_eq!(line(&again, "WEGE3", "income", 2)["status"], "existing");
    assert_eq!(again.body["plan"]["summary"]["transactions_new"], 0);
    assert_eq!(again.body["plan"]["ready"], true);
    assert_eq!(redone.status, StatusCode::OK, "{}", redone.text);
    assert_eq!(redone.body["summary"]["transactions_new"], 0);
    assert_eq!(redone.body["summary"]["income_new"], 0);
    assert_eq!(counts(&pool, user).await, before);
    assert_eq!(
        asset_by_symbol(&db.store, user, "WEGE3").await.quantity,
        dec!(170)
    );
    // Uma parte do período outra vez (arquivo menor, sobreposto): também nada.
    let part = vec![(
        "negociacao-2020.xlsx",
        trades(&[("10/03/2020", "Compra", "WEGE3", 100.0, 20.0)]),
    )];
    let (_, partial) = import(&app.router, &part, json!({})).await;
    assert_eq!(partial.body["summary"]["transactions_new"], 0);
    assert_eq!(counts(&pool, user).await, before);

    // 6. A carteira lê o novo estado.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let wege_view = find(&portfolio.body["assets"], "symbol", "WEGE3");
    assert_eq!(num(&wege_view["quantity"]), dec!(170));
}

// --- prévia vencida, conflito e tudo ou nada ----------------------------------------

#[tokio::test]
async fn nothing_is_written_when_the_preview_is_stale_blocked_or_fails() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "tudo@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("tudo@prumo.test").await.unwrap();
    let files = vec![
        (
            "negociacao.xlsx",
            trades(&[
                ("10/03/2020", "Compra", "WEGE3", 100.0, 20.0),
                ("11/03/2020", "Compra", "VALE3", 10.0, 60.0),
                ("12/03/2021", "Venda", "VALE3", 30.0, 70.0),
            ]),
        ),
        (
            "proventos.xlsx",
            received(&[(
                "VALE3 - VALE S.A.",
                "10/09/2021",
                "Dividendo",
                10.0,
                2.0,
                20.0,
            )]),
        ),
    ];

    // Prévia vencida: a carteira mudou entre a prévia e a gravação.
    let seen = preview(&app.router, &files, &json!({})).await;
    assert_eq!(plan_of(&seen, "VALE3")["opening"]["quantity"], 20.0);
    db.store
        .insert_asset(
            user,
            &NewAsset {
                asset_class: AssetClass::BrStocks,
                symbol: Some("WEGE3".into()),
                display_name: "WEG".into(),
                quantity: dec!(5),
                manual_price: None,
                score_override: None,
                listed_on_b3: true,
                treasury: false,
                max_price: None,
            },
        )
        .await
        .unwrap();
    let stale = send(
        &app.router,
        "commit",
        &files,
        Some(&json!({ "digest": seen.body["plan"]["digest"] })),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.text);
    assert_eq!(stale.body["code"], "preview_stale");
    assert_eq!(counts(&pool, user).await, (1, 1, 0));

    // Conflito: o saldo inicial escolhido não cobre a venda.
    let small = json!({ "assets": { "VALE3": { "opening": { "quantity": 10 } } } });
    let seen = preview(&app.router, &files, &small).await;
    assert_eq!(seen.body["plan"]["ready"], false);
    assert_eq!(
        plan_of(&seen, "VALE3")["conflict"]["reason_code"],
        "exceeds_position"
    );
    assert_eq!(line(&seen, "VALE3", "trades", 1)["status"], "conflict");
    let mut with_digest = small.clone();
    with_digest["digest"] = seen.body["plan"]["digest"].clone();
    let blocked = send(&app.router, "commit", &files, Some(&with_digest)).await;
    assert_eq!(blocked.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(blocked.body["code"], "import_blocked");
    assert_eq!(counts(&pool, user).await, (1, 1, 0));

    // Uma falha no meio da gravação (o banco recusa o provento da VALE3,
    // gravado depois dos lançamentos): nada fica, nem os lançamentos de antes.
    sqlx::query(
        "create function refuse_vale() returns trigger language plpgsql as $$
         begin
             if new.symbol = 'VALE3' then raise exception 'recusado pelo teste'; end if;
             return new;
         end $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "create trigger refuse_vale before insert on income_events
         for each row execute function refuse_vale()",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (seen, failed) = import(&app.router, &files, json!({})).await;
    assert_eq!(seen.body["plan"]["ready"], true);
    assert_eq!(
        failed.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "{}",
        failed.text
    );
    assert_eq!(counts(&pool, user).await, (1, 1, 0));
    let wege = asset_by_symbol(&db.store, user, "WEGE3").await;
    assert_eq!(wege.quantity, dec!(5));

    // Sem a falha, a mesma importação entra inteira.
    sqlx::query("drop trigger refuse_vale on income_events")
        .execute(&pool)
        .await
        .unwrap();
    let (_, done) = import(&app.router, &files, json!({})).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    // O saldo digitado (5) é a quantidade na data de hoje; as 100 de 2020
    // explicam mais do que isso, e a B3 vence.
    assert_eq!(
        asset_by_symbol(&db.store, user, "WEGE3").await.quantity,
        dec!(100)
    );
    assert_eq!(
        asset_by_symbol(&db.store, user, "VALE3").await.quantity,
        dec!(0)
    );
}

// --- provisionado → recebido ------------------------------------------------------

#[tokio::test]
async fn the_received_payment_replaces_the_provisioned_one_without_adding() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "renda@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("renda@prumo.test").await.unwrap();

    let announced = vec![(
        "a-receber.xlsx",
        provisioned(&[(WEG, "10/06/2025", "Dividendo", 100.0, 1.0, 100.0)]),
    )];
    let (_, done) = import(&app.router, &announced, json!({})).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    assert_eq!(done.body["summary"]["income_new"], 1);

    // Pago dois dias depois e com outro líquido: substitui, não soma.
    let paid = vec![(
        "recebidos.xlsx",
        received(&[(WEG, "12/06/2025", "Dividendo", 100.0, 1.0, 99.5)]),
    )];
    let (seen, done) = import(&app.router, &paid, json!({})).await;
    assert_eq!(
        line(&seen, "WEGE3", "income", 0)["reason_code"],
        "replaces_provisioned"
    );
    assert_eq!(done.body["summary"]["income_replaced"], 1);
    let income = db.store.income_ledger(user).await.unwrap();
    assert_eq!(income.len(), 1);
    assert_eq!(income[0].net, dec!(99.5));
    assert_eq!(income[0].pay_date.to_string(), "2025-06-12");
    let (provisional,): (bool,) =
        sqlx::query_as("select provisional from income_events where user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!provisional);

    // O "a receber" velho de novo, depois do recebido: já existe.
    let (seen, done) = import(&app.router, &announced, json!({})).await;
    assert_eq!(line(&seen, "WEGE3", "income", 0)["status"], "existing");
    assert_eq!(done.body["summary"]["income_new"], 0);
    assert_eq!(db.store.income_ledger(user).await.unwrap().len(), 1);
    // Renda lê o livro: um pagamento só.
    let list = get(&app.router, "/api/v1/income").await;
    assert_eq!(list.body.as_array().unwrap().len(), 1);
}

// --- eventos e o livro que já existia -------------------------------------------------

#[tokio::test]
async fn corporate_events_and_the_existing_book() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "livro@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("livro@prumo.test").await.unwrap();

    // A carteira de antes: 100 WEGE3 digitadas no cadastro (saldo inicial de
    // hoje, custo desconhecido) e um "Já comprei" de 10 hoje a R$ 50.
    let wege = db
        .store
        .insert_asset(
            user,
            &NewAsset {
                asset_class: AssetClass::BrStocks,
                symbol: Some("WEGE3".into()),
                display_name: "WEG".into(),
                quantity: dec!(100),
                manual_price: None,
                score_override: None,
                listed_on_b3: true,
                treasury: false,
                max_price: None,
            },
        )
        .await
        .unwrap();
    db.store
        .record_contribution(
            user,
            dec!(500),
            Decimal::ZERO,
            &[NewContributionItem {
                asset_id: wege.id,
                quantity: dec!(10),
                unit_price: dec!(50),
            }],
        )
        .await
        .unwrap();
    let today = prumo::performance::local_date(time::OffsetDateTime::now_utc());
    let today_text = format!(
        "{:02}/{:02}/{}",
        today.day(),
        u8::from(today.month()),
        today.year()
    );
    // A B3: as 100 vieram de 2020 (50) e de um desdobro em 2021 (+50); hoje,
    // as mesmas 10 do aporte a R$ 50,40.
    let files = vec![
        (
            "negociacao.xlsx",
            trades(&[
                ("10/03/2020", "Compra", "WEGE3", 50.0, 40.0),
                (&today_text, "Compra", "WEGE3", 10.0, 50.4),
            ]),
        ),
        (
            "movimentacao.xlsx",
            movements(&[("Credito", "10/04/2021", "Desdobro", WEG, 50.0, None, None)]),
        ),
    ];
    let (seen, done) = import(&app.router, &files, json!({})).await;
    let plan = plan_of(&seen, "WEGE3");
    assert_eq!(plan["action"], "update");
    assert_eq!(plan["opening"], Value::Null);
    assert_eq!(plan["replaced"][0]["kind"], "opening");
    assert_eq!(num(&plan["final_quantity"]), dec!(110));
    assert_eq!(line(&seen, "WEGE3", "trades", 1)["status"], "existing");
    assert_eq!(
        line(&seen, "WEGE3", "trades", 1)["reason_code"],
        "same_entry"
    );
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);

    let ledger = db
        .store
        .transactions(user, TransactionFilter::default())
        .await
        .unwrap();
    assert!(ledger.iter().all(|t| t.fields.kind != Kind::Opening));
    let plan_buy = ledger.iter().find(|t| t.source == Source::Plan).unwrap();
    assert!(plan_buy
        .external_ref
        .as_deref()
        .unwrap()
        .starts_with("b3:v1:trade:"));
    let position = db.store.positions(user).await.unwrap()[&wege.id];
    assert_eq!(position.quantity, dec!(110));
    // O custo agora é conhecido: 50 × 40 + 10 × 50 (o desdobro não muda o custo).
    assert_eq!(position.cost_basis, Some(dec!(2500)));
    let (still_one,): (i64,) = sqlx::query_as(
        "select count(*) from transactions where user_id = $1 and kind = 'buy' and trade_date = $2",
    )
    .bind(user)
    .bind(today)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(still_one, 1, "a compra do aporte nao entra em dobro");

    // Grupamento em que a B3 só credita as cotas novas: 110 → 11.
    let grouped = vec![(
        "movimentacao-2.xlsx",
        movements(&[("Credito", &today_text, "Grupamento", WEG, 11.0, None, None)]),
    )];
    let (_, done) = import(&app.router, &grouped, json!({})).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    let position = db.store.positions(user).await.unwrap()[&wege.id];
    assert_eq!(
        (position.quantity, position.cost_basis),
        (dec!(11), Some(dec!(2500)))
    );
}

// --- dono -------------------------------------------------------------------------

#[tokio::test]
async fn each_person_imports_into_their_own_book() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let alice = app(
        &db.store,
        "alice@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let bruno = app(
        &db.store,
        "bruno@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let alice_id = db.store.ensure_user("alice@prumo.test").await.unwrap();
    let bruno_id = db.store.ensure_user("bruno@prumo.test").await.unwrap();
    let files = vec![
        (
            "negociacao.xlsx",
            trades(&[("10/03/2020", "Compra", "WEGE3", 100.0, 20.0)]),
        ),
        (
            "proventos.xlsx",
            received(&[(WEG, "20/05/2022", "Dividendo", 100.0, 1.0, 100.0)]),
        ),
    ];
    let (_, done) = import(&alice.router, &files, json!({})).await;
    assert_eq!(done.status, StatusCode::OK);

    // Para o Bruno, o mesmo arquivo é todo novo: o `external_ref` é por pessoa.
    let seen = preview(&bruno.router, &files, &json!({})).await;
    assert_eq!(line(&seen, "WEGE3", "trades", 0)["status"], "new");
    assert_eq!(line(&seen, "WEGE3", "income", 0)["status"], "new");
    assert_eq!(plan_of(&seen, "WEGE3")["action"], "create");
    assert_eq!(plan_of(&seen, "WEGE3")["asset_id"], Value::Null);
    let (_, done) = import(&bruno.router, &files, json!({})).await;
    assert_eq!(done.status, StatusCode::OK);
    assert_eq!(counts(&pool, alice_id).await, (1, 1, 1));
    assert_eq!(counts(&pool, bruno_id).await, (1, 1, 1));
    let alice_asset = asset_by_symbol(&db.store, alice_id, "WEGE3").await;
    let bruno_asset = asset_by_symbol(&db.store, bruno_id, "WEGE3").await;
    assert_ne!(alice_asset.id, bruno_asset.id);
    let bruno_income = db.store.income_ledger(bruno_id).await.unwrap();
    assert_eq!(bruno_income[0].asset_id, Some(bruno_asset.id));
}

// --- o desempenho de hoje e o log --------------------------------------------------

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogCapture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn old_history_marks_the_whole_series_dirty_and_the_log_stays_clean() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "foto@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("foto@prumo.test").await.unwrap();
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    // A série já estava em dia (a tarefa refez e apagou a marca).
    sqlx::query("delete from performance_rebuilds where user_id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    assert!(db.store.dirty_since(user).await.unwrap().is_none());

    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(capture.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    // As 10 cadastradas hoje vieram de 2020; mais 40 em 2021, e um provento em 2022.
    let files = vec![
        (
            "negociacao-sigilosa.xlsx",
            trades(&[
                ("10/03/2020", "Compra", "WEGE3", 10.0, 20.0),
                ("10/03/2021", "Compra", "WEGE3", 40.0, 30.0),
                ("10/03/2021", "Compra", "HGLG11", 7.0, 155.0),
            ]),
        ),
        (
            "proventos-sigilosos.xlsx",
            received(&[(WEG, "20/05/2022", "Dividendo", 50.0, 1.0, 50.0)]),
        ),
    ];
    let decisions = json!({ "assets": { "HGLG11": { "asset_class": "br_reits" } } });
    let (_, done) = import(&app.router, &files, decisions).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    drop(guard);
    let logged = String::from_utf8_lossy(&capture.0.lock().unwrap()).into_owned();
    for secret in [
        "WEGE3",
        "HGLG11",
        "CORRETORA",
        "FICTICIA",
        "sigilosa",
        "sigilosos",
    ] {
        assert!(!logged.contains(secret), "o log tem {secret}: {logged}");
    }

    // Os gatilhos da série marcaram a pessoa como suja desde o lançamento mais
    // antigo da importação: a tarefa refaz a série inteira, não só hoje.
    let dirty = db.store.dirty_since(user).await.unwrap().unwrap();
    assert_eq!(dirty.since.to_string(), "2020-03-10");
    assert_eq!(
        asset_by_symbol(&db.store, user, "WEGE3").await.quantity,
        dec!(50)
    );
}

#[tokio::test]
async fn importing_years_of_history_does_not_fill_the_cash() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "caixa@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("caixa@prumo.test").await.unwrap();
    // O caixa conta a partir de 01/01/2025.
    sqlx::query("update users set cash_since = '2025-01-01' where id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let files = vec![
        (
            "negociacao.xlsx",
            trades(&[
                ("10/03/2020", "Compra", "WEGE3", 100.0, 20.0),
                ("10/03/2022", "Venda", "WEGE3", 30.0, 40.0),
                ("10/07/2025", "Venda", "WEGE3", 10.0, 50.0),
            ]),
        ),
        (
            "proventos.xlsx",
            received(&[
                (WEG, "20/05/2021", "Dividendo", 100.0, 1.0, 100.0),
                (WEG, "20/05/2023", "Dividendo", 70.0, 1.0, 70.0),
                (WEG, "20/05/2025", "Dividendo", 70.0, 0.5, 35.0),
            ]),
        ),
    ];
    let (_, done) = import(&app.router, &files, json!({})).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    assert_eq!(done.body["summary"]["income_new"], 3);

    // Só o que é de 2025 em diante: o dividendo de maio e a venda de julho.
    let cash = get(&app.router, "/api/v1/cash").await;
    assert_eq!(cash.status, StatusCode::OK, "{}", cash.text);
    assert_eq!(num(&cash.body["income"]["amount"]), dec!(35));
    assert_eq!(cash.body["income"]["count"], 1);
    assert_eq!(num(&cash.body["sales"]["amount"]), dec!(500));
    assert_eq!(cash.body["sales"]["count"], 1);
    assert_eq!(num(&cash.body["balance"]), dec!(535));

    // Quem chega hoje (o caixa nasce hoje) importa anos e o caixa fica zerado.
    let fresh = app_for(&db, "nova@prumo.test", &dir);
    let (_, done) = import(&fresh.router, &files, json!({})).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    let cash = get(&fresh.router, "/api/v1/cash").await;
    assert_eq!(num(&cash.body["balance"]), Decimal::ZERO);
    assert_eq!(cash.body["income"]["count"], 0);
    assert_eq!(cash.body["sales"]["count"], 0);
}

fn app_for(db: &Database, email: &str, dir: &tempfile_dir::Dir) -> TestApp {
    app(&db.store, email, FakeQuotes::with_market(), &dir.path())
}

// --- a migração 0011 sobre dados que já existem -------------------------------------

#[tokio::test]
async fn migration_0011_keeps_existing_income_as_not_provisional() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    // Um banco novo, montado até a 0010, com um provento fictício.
    sqlx::query("create schema antes")
        .execute(&pool)
        .await
        .unwrap();
    let url = format!("{}?options=-csearch_path%3Dantes", db.url);
    let old = PgPool::connect(&url).await.unwrap();
    let migrator = sqlx::migrate!("./migrations");
    migrator.run_to(10, &old).await.unwrap();
    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, 'antes@prumo.test')")
        .bind(user)
        .execute(&old)
        .await
        .unwrap();
    sqlx::query(
        "insert into income_events (id, user_id, symbol, display_name, kind, pay_date,
             amount_per_unit, quantity, gross, withholding, net, source)
         values ($1, $2, 'WEGE3', 'WEG', 'dividend', '2025-01-10', 1, 10, 10, 0, 10, 'manual')",
    )
    .bind(Uuid::new_v4())
    .bind(user)
    .execute(&old)
    .await
    .unwrap();
    Store::from_pool(old.clone()).migrate().await.unwrap();
    let (provisional,): (bool,) = sqlx::query_as("select provisional from income_events")
        .fetch_one(&old)
        .await
        .unwrap();
    assert!(!provisional);
    // Só a importação marca provisório.
    let refused = sqlx::query("update income_events set provisional = true")
        .execute(&old)
        .await;
    assert!(refused.is_err());
}

// --- importado → automático -------------------------------------------------------

#[tokio::test]
async fn the_imported_payment_replaces_the_automatic_one() {
    let db = database().await;
    let pool = PgPool::connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "automatico@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let user = db.store.ensure_user("automatico@prumo.test").await.unwrap();
    let asset = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 100 }),
    )
    .await;
    assert_eq!(asset.status, StatusCode::CREATED, "{}", asset.text);
    let asset_id = Uuid::from_str(asset.body["id"].as_str().unwrap()).unwrap();
    // O automático (anúncio × livro): JCP de 0,50 bruto, pago em 12/06/2025.
    let automatic = Uuid::new_v4();
    sqlx::query(
        "insert into income_events (id, user_id, asset_id, symbol, display_name, kind, ex_date,
             pay_date, amount_per_unit, quantity, gross, withholding, net, source, external_ref)
         values ($1, $2, $3, 'WEGE3', 'WEG', 'jcp', '2025-05-21', '2025-06-12', 0.5, 100, 50,
                 7.5, 42.5, 'b3_announcement', 'b3a:BRWEGEACNOR0:JRS-CAP-PROPRIO:2025-05-20:0.5')",
    )
    .bind(automatic)
    .bind(user)
    .bind(asset_id)
    .execute(&pool)
    .await
    .unwrap();

    // O extrato traz o mesmo JCP, líquido: toma o lugar do automático.
    let paid = vec![(
        "recebidos.xlsx",
        received(&[(
            WEG,
            "12/06/2025",
            "Juros Sobre Capital Próprio",
            100.0,
            0.425,
            42.5,
        )]),
    )];
    let (seen, done) = import(&app.router, &paid, json!({})).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text);
    assert_eq!(
        line(&seen, "WEGE3", "income", 0)["reason_code"],
        "replaces_automatic"
    );
    assert_eq!(done.body["summary"]["income_replaced"], 1);
    let income = db.store.income_ledger(user).await.unwrap();
    assert_eq!(income.len(), 1);
    assert_eq!(income[0].id, automatic);
    assert_eq!(income[0].source.as_str(), "b3_import");
    assert_eq!(income[0].net, dec!(42.5));
    assert!(income[0]
        .external_ref
        .as_deref()
        .unwrap()
        .starts_with("b3:v1:"));

    // A rodada seguinte nunca mexe no importado.
    let outcome = db.store.regenerate_income_auto(user).await.unwrap();
    assert_eq!(outcome.inserted, 0);
    assert_eq!(db.store.income_ledger(user).await.unwrap().len(), 1);
}
