//! Os clientes das duas fontes de setor contra respostas gravadas no formato
//! real (baixadas em 25/11/2026; ver `tests/fixtures/README.md`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prumo::domain::sector::SectorSource;
use prumo::sectors::b3::B3Classification;
use prumo::sectors::cvm::{CvmFunds, BRICK, PAPER};
use prumo::sectors::{SectorError, SectorFeed};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const B3_SAMPLE: &[u8] = include_bytes!("fixtures/b3_classificacao_setorial.xlsx");
const CVM_SAMPLE: &[u8] = include_bytes!("fixtures/cvm_inf_mensal_fii_2026.zip");

/// O caminho que a página da B3 chama: o filtro `{"language":"pt-br"}` em base64.
const B3_PATH: &str =
    "/listedCompaniesProxy/CompanyCall/GetDownloadIndustryClassification/eyJsYW5ndWFnZSI6InB0LWJyIn0=";
const CVM_2026: &str = "/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_2026.zip";

fn b3(server: &MockServer) -> B3Classification {
    B3Classification::with_base(
        reqwest::Client::new(),
        &format!("{}/listedCompaniesProxy/CompanyCall", server.uri()),
    )
}

fn cvm(server: &MockServer, years: Vec<i32>) -> CvmFunds {
    CvmFunds::with_base(reqwest::Client::new(), &server.uri(), Some(years))
}

#[tokio::test]
async fn b3_downloads_and_reads_the_sector_workbook() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(B3_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/octet-stream")
                .set_body_bytes(B3_SAMPLE),
        )
        .expect(1)
        .mount(&server)
        .await;

    let feed = b3(&server);
    assert_eq!(feed.source(), SectorSource::B3Stocks);
    let entries = feed.fetch().await.unwrap();
    // A planilha de 25/11/2026 tinha 355 emissores no total.
    assert!(entries.len() > 300, "{} linhas", entries.len());
    let sector_of = |code: &str| {
        entries
            .iter()
            .find(|e| e.code == code)
            .map(|e| (e.sector.as_str(), e.segment.as_deref()))
    };
    assert_eq!(sector_of("ITUB"), Some(("Financeiro", Some("Bancos"))));
    assert_eq!(
        sector_of("VALE"),
        Some(("Materiais Básicos", Some("Minerais Metálicos")))
    );
    assert_eq!(
        sector_of("PETR"),
        Some((
            "Petróleo, Gás e Biocombustíveis",
            Some("Exploração, Refino e Distribuição")
        ))
    );
    // Todo emissor tem setor.
    assert!(entries.iter().all(|e| !e.sector.is_empty()));
}

#[tokio::test]
async fn b3_request_carries_no_credentials() {
    // O download é público: nenhum cookie nem credencial vai junto.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(B3_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(B3_SAMPLE))
        .mount(&server)
        .await;
    b3(&server).fetch().await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].headers.get("cookie").is_none());
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn b3_outage_and_garbage_are_errors_not_panics() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(B3_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert!(matches!(
        b3(&server).fetch().await,
        Err(SectorError::Unavailable(reason)) if reason.contains("503")
    ));

    let garbage = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(B3_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html>Pagina em manutencao</html>"),
        )
        .mount(&garbage)
        .await;
    assert!(matches!(
        b3(&garbage).fetch().await,
        Err(SectorError::Format(_))
    ));

    // Fonte que nem responde.
    let gone = MockServer::start().await;
    let feed = b3(&gone);
    drop(gone);
    assert!(matches!(
        feed.fetch().await,
        Err(SectorError::Unavailable(_))
    ));
}

#[tokio::test]
async fn cvm_reads_type_and_segment_from_the_monthly_report() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(CVM_2026))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/zip")
                .set_body_bytes(CVM_SAMPLE),
        )
        .expect(1)
        .mount(&server)
        .await;

    let feed = cvm(&server, vec![2026]);
    assert_eq!(feed.source(), SectorSource::CvmReits);
    let entries = feed.fetch().await.unwrap();
    let find = |code: &str| entries.iter().find(|e| e.code == code).unwrap();
    assert_eq!(find("KNRI").sector, BRICK);
    assert_eq!(find("KNCR").sector, PAPER);
    assert_eq!(find("XPLG").segment.as_deref(), Some("Logística"));
}

#[tokio::test]
async fn cvm_falls_back_to_the_previous_year_when_the_new_file_is_not_out_yet() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_2027.zip",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(CVM_2026))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(CVM_SAMPLE))
        .expect(1)
        .mount(&server)
        .await;
    let entries = cvm(&server, vec![2027, 2026]).fetch().await.unwrap();
    assert_eq!(entries.len(), 9);
}

#[tokio::test]
async fn cvm_outage_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert!(matches!(
        cvm(&server, vec![2026, 2025]).fetch().await,
        Err(SectorError::Unavailable(_))
    ));
    // Sem ano nenhum para tentar também é erro, não pânico.
    assert!(cvm(&server, vec![]).fetch().await.is_err());

    let garbage = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("nao e um zip"))
        .mount(&garbage)
        .await;
    assert!(matches!(
        cvm(&garbage, vec![2026]).fetch().await,
        Err(SectorError::Format(_))
    ));
}
