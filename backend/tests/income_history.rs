//! Histórico de proventos flexível e mapa de calor da Renda, rota por rota,
//! contra um Postgres de verdade. Carteira FICTÍCIA, com datas fixas de 2026
//! (sempre no passado de quem roda).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

async fn add_asset(router: &axum::Router, body: Value) -> String {
    let reply = post(router, "/api/v1/assets", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

async fn buy(router: &axum::Router, asset: &str, date: &str, quantity: f64, price: f64) {
    let reply = post(
        router,
        "/api/v1/transactions",
        json!({ "asset_id": asset, "kind": "buy", "trade_date": date, "quantity": quantity, "unit_price": price }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
}

async fn income(router: &axum::Router, body: Value) {
    let reply = post(router, "/api/v1/income", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
}

async fn fetch(router: &axum::Router, uri: &str) -> Value {
    let reply = get(router, uri).await;
    assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text);
    reply.body
}

/// O valor de um grupo num intervalo (`0` quando o grupo não aparece nele).
fn part(bucket: &Value, key: &str) -> f64 {
    bucket["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"] == key)
        .map_or(0.0, |p| {
            p["received"].as_f64().unwrap() + p["upcoming"].as_f64().unwrap()
        })
}

fn bucket<'a>(history: &'a Value, start: &str) -> &'a Value {
    find(&history["buckets"], "start", start)
}

fn keys(history: &Value) -> Vec<String> {
    history["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["key"].as_str().unwrap().to_owned())
        .collect()
}

/// A classificação pública que a Saúde guarda, fictícia para o teste.
async fn classify(pool: &PgPool) {
    sqlx::query(
        "insert into sector_reference (source, code, sector, subsector, segment) values
            ('b3_stocks', 'WEGE', 'Bens Industriais', 'Maquinas e Equipamentos', 'Motores, Compressores e Outros'),
            ('cvm_reits', 'KNRI', 'Tijolo', null, 'Multicategoria')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Fotos diárias de R$ 5.000 em fevereiro e março de 2026 (a tarefa do
/// desempenho não roda no teste).
async fn snapshots(pool: &PgPool, email: &str) {
    sqlx::query(
        "insert into portfolio_snapshots (user_id, date, market_value, net_flow, quota_value, quotas, taken_at)
         select u.id, d::date, 5000, 0, 1, 5000, now()
           from users u, generate_series('2026-02-01'::date, '2026-03-31'::date, '1 day') d
          where u.email = $1",
    )
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
}

/// Provento de um ativo já apagado da carteira: o evento guarda o símbolo.
async fn removed_income(pool: &PgPool, email: &str) {
    sqlx::query(
        "insert into income_events
            (id, user_id, asset_id, symbol, display_name, kind, pay_date, amount_per_unit, quantity,
             gross, withholding, net, source)
         select gen_random_uuid(), u.id, null, 'OLDX11', 'Fundo Ficticio', 'reit_income', '2026-03-14',
                2, 10, 20, 0, 20, 'manual'
           from users u where u.email = $1",
    )
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn income_history_and_heatmap() {
    let db = database().await;
    let pool = PgPoolOptions::new().connect(&db.url).await.unwrap();
    let dir = tempfile_dir::Dir::new();
    let email = "historico@prumo.test";
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let router = &app.router;

    // Livro vazio: nada para somar.
    let empty = fetch(router, "/api/v1/income/history").await;
    assert_eq!(empty["buckets"], json!([]));
    assert_eq!(empty["interval"], "month");
    assert_eq!(empty["basis"], "payment");
    assert_eq!(empty["group"], "total");
    assert_eq!(empty["value"], "net");
    let empty = fetch(router, "/api/v1/income/heatmap").await;
    assert_eq!(empty["years"], json!([]));

    classify(&pool).await;
    let knri = add_asset(
        router,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11" }),
    )
    .await;
    let wege = add_asset(
        router,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3" }),
    )
    .await;
    // Custo: 10 × 100 + 100 × 40 = R$ 5.000 desde 05/01/2026.
    buy(router, &knri, "2026-01-05", 10.0, 100.0).await;
    buy(router, &wege, "2026-01-05", 100.0, 40.0).await;
    // FII: data com na sexta 30/01, data ex na segunda 02/02, pagamento em 13/02.
    income(
        router,
        json!({ "asset_id": knri, "kind": "reit_income", "ex_date": "2026-02-02", "pay_date": "2026-02-13", "amount_per_unit": 1, "quantity": 10 }),
    )
    .await;
    // JCP de 2026: 17,5% retidos (50 bruto, 41,25 líquido); data com 19/02.
    income(
        router,
        json!({ "asset_id": wege, "kind": "jcp", "ex_date": "2026-02-20", "pay_date": "2026-03-10", "amount_per_unit": 0.5, "quantity": 100 }),
    )
    .await;
    removed_income(&pool, email).await;
    snapshots(&pool, email).await;

    // Por classe, pela data de pagamento, líquido.
    let by_class = fetch(router, "/api/v1/income/history?group=class").await;
    assert_eq!(
        keys(&by_class),
        ["class:br_stocks", "removed", "class:br_reits"]
    );
    let first = &by_class["buckets"][0];
    assert_eq!(first["start"], "2026-02");
    assert_eq!(first["received"], 10.0);
    assert_eq!(part(first, "class:br_reits"), 10.0);
    let march = bucket(&by_class, "2026-03");
    assert_eq!(march["received"], 61.25);
    assert_eq!(part(march, "class:br_stocks"), 41.25);
    assert_eq!(part(march, "removed"), 20.0);
    let classes = by_class["groups"].as_array().unwrap();
    assert_eq!(classes[0]["kind"], "class");
    assert_eq!(classes[0]["asset_class"], "br_stocks");
    assert_eq!(classes[1]["kind"], "removed");
    assert_eq!(classes[1]["asset_class"], Value::Null);
    // Sem buraco até o mês de hoje.
    assert_eq!(bucket(&by_class, "2026-04")["received"], 0.0);

    // Por segmento: o da B3 nas ações, o tipo (do balanço) no FII.
    let by_segment = fetch(router, "/api/v1/income/history?group=segment").await;
    assert_eq!(
        keys(&by_segment),
        [
            "segment:br_stocks:Motores, Compressores e Outros",
            "removed",
            "segment:br_reits:Tijolo"
        ]
    );
    assert_eq!(by_segment["groups"][2]["segment"], "Tijolo");

    // Por ativo: o pagador com o nome e o id.
    let by_asset = fetch(router, "/api/v1/income/history?group=asset").await;
    let top = &by_asset["groups"][0];
    assert_eq!(top["label"], "WEGE3");
    assert_eq!(top["asset_id"], wege.as_str());
    assert_eq!(top["key"], format!("asset:{wege}"));
    assert_eq!(by_asset["groups"][1]["label"], "OLDX11");
    assert_eq!(by_asset["groups"][1]["asset_id"], Value::Null);

    // Pela data com: o rendimento pago em fevereiro é de janeiro.
    let by_record = fetch(router, "/api/v1/income/history?basis=record").await;
    assert_eq!(by_record["buckets"][0]["start"], "2026-01");
    assert_eq!(by_record["buckets"][0]["received"], 10.0);
    assert_eq!(bucket(&by_record, "2026-02")["received"], 41.25);
    assert_eq!(bucket(&by_record, "2026-03")["received"], 20.0);

    // Bruto e por trimestre.
    let gross = fetch(
        router,
        "/api/v1/income/history?value=gross&interval=quarter",
    )
    .await;
    assert_eq!(gross["interval"], "quarter");
    assert_eq!(gross["buckets"][0]["start"], "2026-01");
    assert_eq!(gross["buckets"][0]["received"], 80.0);
    let year = fetch(router, "/api/v1/income/history?interval=year").await;
    assert_eq!(year["buckets"][0]["start"], "2026-01");
    assert_eq!(year["buckets"][0]["received"], 71.25);

    // Escolha que não existe é 422.
    for uri in [
        "/api/v1/income/history?interval=week",
        "/api/v1/income/history?group=sector",
        "/api/v1/income/history?other=1",
        "/api/v1/income/heatmap?value=after_tax",
    ] {
        let reply = get(router, uri).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{uri}: {}",
            reply.text
        );
    }

    // O mapa de calor: DY sobre o patrimônio médio, yield on cost sobre o custo médio.
    let heat = fetch(router, "/api/v1/income/heatmap").await;
    let y2026 = heat["years"]
        .as_array()
        .unwrap()
        .iter()
        .find(|y| y["year"] == 2026)
        .unwrap();
    let february = &y2026["months"][1];
    assert_eq!(february["amount"], 10.0);
    assert_eq!(february["market_value"], 5000.0);
    assert_eq!(february["dy"], 0.2);
    assert_eq!(february["cost"], 5000.0);
    assert_eq!(february["yield_on_cost"], 0.2);
    assert_eq!(february["dy_partial"], false);
    let march = &y2026["months"][2];
    assert_eq!(march["amount"], 61.25);
    // O provento do ativo apagado fica fora (o patrimônio dele saiu junto).
    assert_eq!(march["dy"], 0.825);
    assert_eq!(march["dy_partial"], true);
    assert_eq!(march["yield_on_cost"], 0.825);
    // Janeiro: com custo (posição desde o dia 5) e sem foto do patrimônio.
    let january = &y2026["months"][0];
    assert_eq!(january["cost"], 5000.0);
    assert_eq!(january["yield_on_cost"], 0.0);
    assert_eq!(january["dy"], Value::Null);
    // O ano: 51,25 ÷ 5.000, parcial pelo ativo apagado.
    let total = &y2026["total"];
    assert_eq!(total["amount"], 71.25);
    assert_eq!(total["dy"], 1.025);
    assert_eq!(total["yield_on_cost"], 1.025);
    assert_eq!(total["dy_partial"], true);

    // Bruto e pela data com.
    let gross = fetch(router, "/api/v1/income/heatmap?value=gross&basis=record").await;
    let y2026 = gross["years"]
        .as_array()
        .unwrap()
        .iter()
        .find(|y| y["year"] == 2026)
        .unwrap();
    assert_eq!(y2026["months"][0]["amount"], 10.0);
    assert_eq!(y2026["months"][1]["amount"], 50.0);
    assert_eq!(y2026["months"][1]["dy"], 1.0);

    // Outra pessoa não vê nada disso.
    let other = common::app(
        &db.store,
        "outra@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let theirs = fetch(&other.router, "/api/v1/income/history").await;
    assert_eq!(theirs["buckets"], json!([]));
}
