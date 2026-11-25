//! A leitura da tabela do Tesouro Transparente contra o servidor de mentira:
//! o CSV é um recorte do arquivo real (conferido em 25/11/2026) e o
//! `package_show` tem o formato que o CKAN devolve.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prumo::domain::treasury::{Bond, BondKind};
use prumo::treasury::{
    parse_csv, FeedError, TransparenteFeed, TreasuryFeed, DATASET_ID, KNOWN_CSV_PATH,
};
use rust_decimal_macros::dec;
use serde_json::json;
use time::macros::date;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAMPLE: &str = include_str!("data/precotaxatesourodireto_amostra.csv");
const TODAY: time::Date = date!(2026 - 11 - 25);

fn package_show(csv_url: &str) -> serde_json::Value {
    json!({
        "help": "https://www.tesourotransparente.gov.br/ckan/api/3/action/help_show?name=package_show",
        "success": true,
        "result": {
            "id": DATASET_ID,
            "resources": [
                { "name": "Metadados", "format": "PDF",
                  "url": "https://www.tesourotransparente.gov.br/ckan/dataset/x/resource/m/download/taxa.pdf" },
                { "name": "Taxas dos Titulos Ofertados pelo Tesouro Direto", "format": "CSV", "url": csv_url },
            ]
        }
    })
}

fn csv_response(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/csv")
        .set_body_string(body)
}

async fn feed() -> (MockServer, TransparenteFeed) {
    let server = MockServer::start().await;
    let feed = TransparenteFeed::with_base(reqwest::Client::new(), &format!("{}/", server.uri()));
    (server, feed)
}

async fn mount_package(server: &MockServer, csv_url: &str) {
    Mock::given(method("GET"))
        .and(path("/ckan/api/3/action/package_show"))
        .and(query_param("id", DATASET_ID))
        .respond_with(ResponseTemplate::new(200).set_body_json(package_show(csv_url)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn follows_the_csv_address_the_ckan_gives() {
    let (server, feed) = feed().await;
    // O CKAN fala do servidor de produção; o Prumo só aproveita o caminho e
    // continua no servidor configurado.
    mount_package(
        &server,
        "https://www.tesourotransparente.gov.br/ckan/dataset/x/resource/y/download/novoarquivo.csv",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/ckan/dataset/x/resource/y/download/novoarquivo.csv"))
        .respond_with(csv_response(SAMPLE))
        .expect(1)
        .mount(&server)
        .await;

    let prices = feed.latest_prices(TODAY).await.unwrap();

    // 12 títulos vigentes; os de 2008, 2020 e 2021 já venceram.
    assert_eq!(prices.len(), 12);
    let of = |kind, maturity| {
        prices
            .iter()
            .find(|p| p.bond == Bond::new(kind, maturity))
            .map(|p| (p.redemption_price, p.base_date))
    };
    assert_eq!(
        of(BondKind::IpcaPlus, date!(2035 - 05 - 15)),
        Some((dec!(2500.95), date!(2026 - 11 - 19)))
    );
    assert_eq!(
        of(BondKind::Selic, date!(2029 - 03 - 01)),
        Some((dec!(20305.65), date!(2026 - 11 - 19)))
    );
    assert_eq!(
        of(BondKind::RendaPlus, date!(2084 - 12 - 15)),
        Some((dec!(152.95), date!(2026 - 11 - 19)))
    );
    assert!(of(BondKind::PrefixedWithCoupons, date!(2008 - 01 - 01)).is_none());
}

#[tokio::test]
async fn falls_back_to_the_known_address_when_the_ckan_fails() {
    let (server, feed) = feed().await;
    Mock::given(method("GET"))
        .and(path("/ckan/api/3/action/package_show"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(KNOWN_CSV_PATH))
        .respond_with(csv_response(SAMPLE))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(feed.latest_prices(TODAY).await.unwrap().len(), 12);
}

#[tokio::test]
async fn falls_back_when_the_ckan_lists_no_csv() {
    let (server, feed) = feed().await;
    Mock::given(method("GET"))
        .and(path("/ckan/api/3/action/package_show"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "resources": [{ "format": "PDF", "url": "https://x.test/a.pdf" }] }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(KNOWN_CSV_PATH))
        .respond_with(csv_response(SAMPLE))
        .mount(&server)
        .await;
    assert_eq!(feed.latest_prices(TODAY).await.unwrap().len(), 12);
}

#[tokio::test]
async fn a_download_that_fails_or_is_not_the_table_is_an_error() {
    let (server, feed) = feed().await;
    mount_package(
        &server,
        "https://x.test/ckan/dataset/x/resource/y/download/a.csv",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/ckan/dataset/x/resource/y/download/a.csv"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert!(matches!(
        feed.latest_prices(TODAY).await,
        Err(FeedError::Unavailable(_))
    ));

    let (server, feed) = self::feed().await;
    mount_package(
        &server,
        "https://x.test/ckan/dataset/x/resource/y/download/a.csv",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/ckan/dataset/x/resource/y/download/a.csv"))
        .respond_with(csv_response("<html><body>Manutencao</body></html>"))
        .mount(&server)
        .await;
    assert!(matches!(
        feed.latest_prices(TODAY).await,
        Err(FeedError::Malformed(_))
    ));

    let offline = TransparenteFeed::with_base(reqwest::Client::new(), "http://127.0.0.1:1");
    assert!(matches!(
        offline.latest_prices(TODAY).await,
        Err(FeedError::Unavailable(_))
    ));
}

/// O arquivo real tem 15 MB e chega em pedaços que cortam linhas ao meio: um
/// arquivo grande o bastante para isso, lido em fluxo, dá o mesmo resultado
/// que o texto lido de uma vez.
#[tokio::test]
async fn a_big_file_read_as_a_stream_gives_the_same_prices_as_the_whole_text() {
    let mut big = String::from(SAMPLE.lines().next().unwrap());
    big.push('\n');
    let history = SAMPLE.lines().skip(1);
    for day in 1..=28u32 {
        for row in history.clone() {
            // Linhas antigas do mesmo título: nunca vencem o preço de 19/11.
            let mut fields: Vec<String> = row.split(';').map(str::to_owned).collect();
            fields[2] = format!("{day:02}/10/2026");
            for _ in 0..40 {
                big.push_str(&fields.join(";"));
                big.push('\n');
            }
        }
    }
    big.push_str(
        SAMPLE
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .as_str(),
    );
    big.push_str("\r\n");
    assert!(big.len() > 1_000_000, "{} bytes", big.len());

    let (server, feed) = feed().await;
    mount_package(
        &server,
        "https://x.test/ckan/dataset/x/resource/y/download/a.csv",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/ckan/dataset/x/resource/y/download/a.csv"))
        .respond_with(csv_response(&big))
        .mount(&server)
        .await;
    let streamed = feed.latest_prices(TODAY).await.unwrap();
    let whole = parse_csv(&big, TODAY).unwrap().prices;
    assert_eq!(streamed, whole);
    assert_eq!(streamed.len(), 12);
}
