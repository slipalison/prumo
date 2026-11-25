//! O Tesouro Direto de ponta a ponta: a tabela (servidor de mentira com o
//! recorte do CSV real) vira cotação no Postgres, a busca sugere o título, o
//! título entra na carteira com quantidade fracionária e preço automático, e o
//! plano compra fração dele.
//!
//! Um contêiner para o arquivo inteiro, como `api.rs`; cada cenário usa um
//! e-mail próprio.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use common::*;
use prumo::live::LiveQuotes;
use prumo::treasury::{TransparenteFeed, TreasurySync, MAX_AGE};
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::macros::datetime;
use time::OffsetDateTime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAMPLE: &str = include_str!("data/precotaxatesourodireto_amostra.csv");
const CSV_PATH: &str = "/ckan/dataset/x/resource/y/download/tabela.csv";

/// A hora em que a rodada "roda" nos testes: depois da data-base do recorte
/// (19/11/2026), com os títulos do recorte ainda vigentes.
const NOW: OffsetDateTime = datetime!(2026-11-25 15:00 UTC);

/// Um servidor do Tesouro de mentira servindo `csv`, e a rodada que o lê.
async fn sync_for(db: &Database, csv: &str) -> (MockServer, TreasurySync) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ckan/api/3/action/package_show"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "resources": [{ "format": "CSV", "url": format!("https://x.test{CSV_PATH}") }] }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(CSV_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(csv))
        .mount(&server)
        .await;
    let feed = TransparenteFeed::with_base(reqwest::Client::new(), &server.uri());
    let sync = TreasurySync::new(
        db.store.clone(),
        Arc::new(feed),
        LiveQuotes::new(db.store.clone()),
    );
    (server, sync)
}

fn symbols_of(reply: &Reply) -> Vec<&str> {
    reply
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["symbol"].as_str().unwrap())
        .collect()
}

fn treasury_symbols(reply: &Reply) -> Vec<&str> {
    reply
        .body
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["treasury"] == true)
        .map(|s| s["symbol"].as_str().unwrap())
        .collect()
}

async fn search(app: &TestApp, query: &str) -> Reply {
    let query = query.replace('+', "%2B").replace(' ', "%20");
    get(&app.router, &format!("/api/v1/symbols?query={query}")).await
}

#[tokio::test]
async fn treasury_bonds_from_the_table_to_the_plan() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let quotes = FakeQuotes::with_market();
    let app = app(
        &db.store,
        "tesouro@prumo.test",
        Arc::clone(&quotes),
        &dir.path(),
    );
    let (server, sync) = sync_for(&db, SAMPLE).await;

    // Antes da primeira rodada não há título para sugerir, e a busca segue
    // valendo só com o provedor.
    let before = search(&app, "tesouro").await;
    assert_eq!(before.status, StatusCode::OK);
    assert_eq!(symbols_of(&before), ["TESOURO"]);

    // A rodada grava 12 títulos; a segunda, dentro do prazo, não baixa nada.
    let report = sync.sync(NOW).await.unwrap();
    assert_eq!((report.bonds, report.pruned), (12, 0));
    let requests_after_first = server.received_requests().await.unwrap().len();
    assert_eq!(requests_after_first, 2, "package_show e o csv");
    assert!(sync
        .sync_if_stale(NOW + Duration::from_secs(3600), MAX_AGE)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        requests_after_first,
        "em dia: nao baixa de novo"
    );
    // Passado o prazo, baixa de novo e o resultado é o mesmo.
    let again = sync
        .sync_if_stale(NOW + MAX_AGE + Duration::from_secs(60), MAX_AGE)
        .await
        .unwrap()
        .expect("velha: baixa");
    assert_eq!((again.bonds, again.pruned), (12, 0));

    search_suggests_the_titles(&app).await;
    a_title_is_registered_priced_and_validated(&app, &quotes).await;
    the_plan_buys_a_fraction_and_the_contribution_records_it(&db, &dir).await;
    the_streamer_and_the_provider_leave_the_title_alone(&db, &dir, &quotes).await;
    leaving_the_table_keeps_what_a_portfolio_holds(&db, &app).await;
}

async fn search_suggests_the_titles(app: &TestApp) {
    // "tesouro" casa com todos; a lista corta em 10 e vai do tipo mais simples
    // ao mais longo, com o resultado do provedor depois.
    let all = search(app, "tesouro").await;
    assert_eq!(all.status, StatusCode::OK);
    let list = all.body.as_array().unwrap();
    assert_eq!(list.len(), 11, "10 titulos e 1 do provedor");
    assert_eq!(list[0]["symbol"], "TD-SELIC-20290301");
    assert_eq!(list[0]["display_name"], "Tesouro Selic 2029");
    assert_eq!(list[0]["exchange"], "Tesouro Direto");
    assert_eq!(list[0]["suggested_class"], "fixed_income");
    assert_eq!(list[0]["listed_on_b3"], false);
    assert_eq!(list[0]["treasury"], true);
    assert_eq!(list[1]["symbol"], "TD-SELIC-20310301");
    assert_eq!(list[10]["symbol"], "TESOURO");
    assert_eq!(list[10]["treasury"], false);

    // Um título só: o nome diz tipo e ano.
    let ipca = search(app, "ipca 2035").await;
    assert_eq!(symbols_of(&ipca)[0], "TD-IPCA-20350515");
    assert_eq!(ipca.body[0]["display_name"], "Tesouro IPCA+ 2035");
    assert_eq!(treasury_symbols(&ipca), ["TD-IPCA-20350515"]);
    // Os dois Renda+, do vencimento mais próximo ao mais distante.
    let renda = search(app, "renda+").await;
    assert_eq!(
        treasury_symbols(&renda),
        ["TD-RENDA-20491215", "TD-RENDA-20841215"]
    );
    assert_eq!(
        renda.body[0]["display_name"],
        "Tesouro Renda+ Aposentadoria Extra 2049"
    );
    assert_eq!(
        treasury_symbols(&search(app, "educa+").await),
        ["TD-EDUCA-20371215"]
    );
    assert!(treasury_symbols(&search(app, "prefixado").await).contains(&"TD-PREJS-20350101"));
    assert_eq!(
        treasury_symbols(&search(app, "selic").await),
        ["TD-SELIC-20290301", "TD-SELIC-20310301"]
    );
    // O que não é título não sugere título, e busca curta demais também não.
    assert!(treasury_symbols(&search(app, "knri").await).is_empty());
    assert!(treasury_symbols(&search(app, "te").await).is_empty());
}

async fn a_title_is_registered_priced_and_validated(app: &TestApp, quotes: &Arc<FakeQuotes>) {
    let calls_before = *quotes.calls.lock().unwrap();
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({
            "asset_class": "fixed_income",
            "symbol": "TD-IPCA-20350515",
            "treasury": true,
            "quantity": 1.5,
            "score_override": 6
        }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let bond = &created.body;
    assert_eq!(bond["symbol"], "TD-IPCA-20350515");
    assert_eq!(bond["display_name"], "Tesouro IPCA+ 2035");
    assert_eq!(bond["treasury"], true);
    assert_eq!(bond["listed_on_b3"], false);
    assert_eq!(bond["quantity"], 1.5);
    // O preço de resgate de 19/11/2026, sem digitar nada.
    assert_eq!(bond["unit_price"], 2500.95);
    assert_eq!(bond["price_source"], "quote");
    assert_eq!(bond["currency"], "BRL");
    assert_eq!(bond["quote_updated_at"], "2026-11-19T03:00:00Z");
    assert_eq!(bond["current_value"], 3751.43);
    assert_eq!(bond["score"], 6);
    assert_eq!(
        *quotes.calls.lock().unwrap(),
        calls_before,
        "o titulo nao passa pelo provedor"
    );

    // A classe aparece com o valor do título.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    assert_eq!(portfolio.body["total_value"], 3751.43);
    assert_eq!(
        find(&portfolio.body["classes"], "asset_class", "fixed_income")["current_value"],
        3751.43
    );

    // Dois cadastros do mesmo título são conflito.
    let twice = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "td-ipca-20350515", "treasury": true, "quantity": 1, "score_override": 5 }),
    )
    .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);

    // Só a chave que a busca devolve vale; outra classe ou a marca da B3, não.
    for (why, body) in [
        (
            "ticker no lugar da chave",
            json!({ "asset_class": "fixed_income", "symbol": "KNRI11", "treasury": true, "quantity": 1 }),
        ),
        (
            "tipo desconhecido",
            json!({ "asset_class": "fixed_income", "symbol": "TD-NADA-20350515", "treasury": true, "quantity": 1 }),
        ),
        (
            "data que nao existe",
            json!({ "asset_class": "fixed_income", "symbol": "TD-IPCA-20350230", "treasury": true, "quantity": 1 }),
        ),
        (
            "classe errada",
            json!({ "asset_class": "crypto", "symbol": "TD-IPCA-20400815", "treasury": true, "quantity": 1 }),
        ),
        (
            "renda fixa internacional",
            json!({ "asset_class": "intl_fixed_income", "symbol": "TD-IPCA-20400815", "treasury": true, "quantity": 1 }),
        ),
        (
            "da B3",
            json!({ "asset_class": "fixed_income", "symbol": "TD-IPCA-20400815", "treasury": true, "listed_on_b3": true, "quantity": 1 }),
        ),
        (
            "sem simbolo",
            json!({ "asset_class": "fixed_income", "treasury": true, "quantity": 1 }),
        ),
        (
            "quantidade negativa",
            json!({ "asset_class": "fixed_income", "symbol": "TD-IPCA-20400815", "treasury": true, "quantity": -1 }),
        ),
    ] {
        let reply = post(&app.router, "/api/v1/assets", body).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
    }

    // Sem a marca, a chave é só um ticker estranho da B3: não vira título por
    // acidente e não ganha o preço do Tesouro.
    let plain = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "TD-IPCA-20400815", "listed_on_b3": true, "quantity": 1, "score_override": 5 }),
    )
    .await;
    assert_eq!(plain.status, StatusCode::CREATED, "{}", plain.text);
    assert_eq!(plain.body["treasury"], false);
    assert_eq!(plain.body["price_source"], "missing");
    let plain_id = plain.body["id"].as_str().unwrap();
    delete(&app.router, &format!("/api/v1/assets/{plain_id}")).await;

    // Um título que o Tesouro ainda não cotou nasce sem preço e com o nome do
    // tipo e do ano, sem quebrar.
    let unlisted = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "TD-EDUCA-20991215", "treasury": true, "quantity": 0.25, "score_override": 5 }),
    )
    .await;
    assert_eq!(unlisted.status, StatusCode::CREATED);
    assert_eq!(unlisted.body["display_name"], "Tesouro Educa+ 2099");
    assert_eq!(unlisted.body["price_source"], "missing");
    assert_eq!(unlisted.body["unit_price"], Value::Null);
    let unlisted_id = unlisted.body["id"].as_str().unwrap();
    delete(&app.router, &format!("/api/v1/assets/{unlisted_id}")).await;

    // Editar: quantidade fracionária vale; valor (`amount`) e marca da B3, não.
    let id = bond["id"].as_str().unwrap();
    let url = format!("/api/v1/assets/{id}");
    let edited = patch(&app.router, &url, json!({ "quantity": 2.25 })).await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text);
    assert_eq!(edited.body["quantity"], 2.25);
    assert_eq!(edited.body["current_value"], 5627.14);
    assert_eq!(edited.body["treasury"], true);
    for body in [json!({ "amount": 100 }), json!({ "listed_on_b3": true })] {
        assert_eq!(
            patch(&app.router, &url, body).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    // Tirar a marca da B3 de quem nunca teve é inofensivo.
    assert_eq!(
        patch(&app.router, &url, json!({ "listed_on_b3": false }))
            .await
            .status,
        StatusCode::OK
    );
    // O preço manual vence a tabela, como nos outros ativos.
    let manual = patch(&app.router, &url, json!({ "manual_price": 2600 })).await;
    assert_eq!(manual.body["unit_price"], 2600.0);
    assert_eq!(manual.body["price_source"], "manual");
    let back = patch(&app.router, &url, json!({ "manual_price": null })).await;
    assert_eq!(back.body["unit_price"], 2500.95);
    assert_eq!(back.body["price_source"], "quote");

    // A renda fixa por valor continua como era.
    let cdb = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "display_name": "CDB Banco Exemplo", "amount": 1000, "score_override": 5 }),
    )
    .await;
    assert_eq!(cdb.status, StatusCode::CREATED);
    assert_eq!(cdb.body["treasury"], false);
    assert_eq!(cdb.body["symbol"], Value::Null);
    assert_eq!(cdb.body["price_source"], "manual");
    assert_eq!(cdb.body["current_value"], 1000.0);
    let cdb_id = cdb.body["id"].as_str().unwrap();
    delete(&app.router, &format!("/api/v1/assets/{cdb_id}")).await;
    delete(&app.router, &url).await;
}

async fn the_plan_buys_a_fraction_and_the_contribution_records_it(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let app = app(
        &db.store,
        "plano-tesouro@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    put(
        &app.router,
        "/api/v1/goals",
        json!({ "targets": { "fixed_income": 100 } }),
    )
    .await;
    // Ainda não comprou: quantidade 0, só a nota.
    let bond = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "TD-IPCA-20350515", "treasury": true, "quantity": 0, "score_override": 6 }),
    )
    .await;
    assert_eq!(bond.status, StatusCode::CREATED, "{}", bond.text);
    let id = bond.body["id"].as_str().unwrap().to_owned();

    let plan = post(&app.router, "/api/v1/plans", json!({ "amount": 1000 })).await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.text);
    let items = plan.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["asset_id"], id.as_str());
    assert_eq!(item["treasury"], true);
    assert_eq!(item["divisible"], true);
    // R$ 1.000 a R$ 2.500,95: 0,39 título (0,3998… para baixo, em centésimos).
    assert_eq!(item["quantity"], 0.39);
    assert_eq!(item["unit_price"], 2500.95);
    assert_eq!(item["value"], 975.37);
    assert_eq!(plan.body["change"], 24.63);
    assert_eq!(plan.body["unallocated"], 0.0);

    let recorded = post(
        &app.router,
        "/api/v1/contributions",
        json!({ "amount": 1000, "items": [{ "asset_id": id, "quantity": 0.39, "unit_price": 2500.95 }] }),
    )
    .await;
    assert_eq!(recorded.status, StatusCode::CREATED, "{}", recorded.text);
    assert_eq!(recorded.body["items"][0]["quantity"], 0.39);
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let asset = find(&portfolio.body["assets"], "id", &id);
    assert_eq!(asset["quantity"], 0.39);
    assert_eq!(asset["current_value"], 975.37);
}

async fn the_streamer_and_the_provider_leave_the_title_alone(
    db: &Database,
    dir: &tempfile_dir::Dir,
    quotes: &Arc<FakeQuotes>,
) {
    let app = app(
        &db.store,
        "ao-vivo-tesouro@prumo.test",
        Arc::clone(quotes),
        &dir.path(),
    );
    for body in [
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 1 }),
        json!({ "asset_class": "fixed_income", "symbol": "TD-SELIC-20290301", "treasury": true, "quantity": 0.5, "score_override": 5 }),
    ] {
        assert_eq!(
            post(&app.router, "/api/v1/assets", body).await.status,
            StatusCode::CREATED
        );
    }
    let live = Arc::clone(app.state.quotes.live());
    let mut subscribed = live.symbols();
    let seen = tokio::time::timeout(
        Duration::from_secs(10),
        subscribed.wait_for(|s| s.contains("KNRI11.SA")),
    )
    .await
    .expect("assinatura a tempo")
    .unwrap()
    .clone();
    // Todas as carteiras do banco entram na conta; nenhuma chave de título.
    assert!(seen.iter().all(|s| !s.starts_with("TD-")), "{seen:?}");

    // A atualização manual e a periódica não perguntam ao provedor pelo título
    // e não o contam como falha.
    let refreshed = post(&app.router, "/api/v1/quotes/refresh", json!({})).await;
    assert_eq!(refreshed.body["refreshed"], 1);
    assert_eq!(refreshed.body["failed"], json!([]));
    let report = app.state.quotes.refresh_all().await;
    assert!(
        report.failed.iter().all(|s| !s.starts_with("TD-")),
        "{report:?}"
    );
    let stored = db
        .store
        .quotes(&["TD-SELIC-20290301".to_owned()])
        .await
        .unwrap();
    assert_eq!(stored["TD-SELIC-20290301"].price, dec!(20305.65));
}

async fn leaving_the_table_keeps_what_a_portfolio_holds(db: &Database, app: &TestApp) {
    // A mesma pessoa de antes guarda o IPCA+ 2035.
    let held = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "TD-IPCA-20350515", "treasury": true, "quantity": 2, "score_override": 6 }),
    )
    .await;
    assert_eq!(held.status, StatusCode::CREATED);

    // A tabela nova, de duas semanas depois, só traz Selic 2029 e Educa+ 2037:
    // o IPCA+ de 2040 (que ninguém guarda) some; o de 2035 (guardado) fica com
    // o último preço e a data dele; os que vieram ganham o preço novo.
    let newer = "Tipo Titulo;Data Vencimento;Data Base;Taxa Compra Manha;Taxa Venda Manha;PU Compra Manha;PU Venda Manha;PU Base Manha\n\
                 Tesouro Selic;01/03/2029;03/12/2026;0,05;0,06;20400,00;20390,00;20390,00\n\
                 Tesouro Educa+;15/12/2037;03/12/2026;7,80;7,92;2600,00;2580,00;2580,00\n";
    let (_server, second) = sync_for(db, newer).await;
    let report = second
        .sync(NOW + Duration::from_secs(14 * 86400))
        .await
        .unwrap();
    assert_eq!(report.bonds, 2);
    // 12 gravados antes, 2 vieram de novo e 1 está numa carteira. A conta vale
    // para todas as carteiras do banco: o Selic 2029 e o IPCA+ 2035 dos outros
    // cenários já estavam na lista de quem guarda.
    assert_eq!(report.pruned, 12 - 2 - 1);

    let quotes = db
        .store
        .quotes(&[
            "TD-SELIC-20290301".to_owned(),
            "TD-IPCA-20400815".to_owned(),
            "TD-IPCA-20350515".to_owned(),
        ])
        .await
        .unwrap();
    assert_eq!(quotes["TD-SELIC-20290301"].price, dec!(20390));
    assert!(!quotes.contains_key("TD-IPCA-20400815"));
    assert_eq!(quotes["TD-IPCA-20350515"].price, dec!(2500.95));

    // O guardado continua na carteira com o preço de 19/11, datado.
    let portfolio = get(&app.router, "/api/v1/portfolio").await;
    let asset = find(
        &portfolio.body["assets"],
        "id",
        held.body["id"].as_str().unwrap(),
    );
    assert_eq!(asset["unit_price"], 2500.95);
    assert_eq!(asset["quote_updated_at"], "2026-11-19T03:00:00Z");
    // Mas não é mais sugestão para quem cadastra: saiu da oferta.
    let ipca = search(app, "ipca").await;
    assert!(treasury_symbols(&ipca).is_empty(), "{}", ipca.text);
    let selic = search(app, "selic").await;
    assert_eq!(treasury_symbols(&selic), ["TD-SELIC-20290301"]);

    // Uma tabela com defeito não apaga nada.
    let (_broken_server, broken) = sync_for(db, "<html>fora do ar</html>").await;
    assert!(broken
        .sync(NOW + Duration::from_secs(15 * 86400))
        .await
        .is_err());
    let still = db
        .store
        .quotes(&["TD-SELIC-20290301".to_owned()])
        .await
        .unwrap();
    assert_eq!(still["TD-SELIC-20290301"].price, dec!(20390));
}
