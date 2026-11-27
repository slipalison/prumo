//! Os clientes do BCB (SGS), do `chart` do Yahoo e das estatísticas de índice
//! da B3 contra respostas gravadas com `curl` (`tests/fixtures/history/`; ver
//! o README de lá). `yahoo_aapl_split.json` é `AAPL` de 26/08 a 04/09/2020 com
//! `events=split`: o desdobro de 4 para 1 em 31/08/2020, com os fechamentos de
//! antes já divididos por 4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use prumo::history::b3::{self, B3Indexes};
use prumo::history::bcb::BcbClient;
use prumo::history::yahoo::YahooHistory;
use prumo::history::{HistoryError, HistoryProvider, IndexCode, PublicHistory};
use rust_decimal_macros::dec;
use time::macros::{date, datetime};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CDI: &str = include_str!("fixtures/history/bcb_12_cdi.json");
const IPCA: &str = include_str!("fixtures/history/bcb_433_ipca.json");
const EMPTY: &str = include_str!("fixtures/history/bcb_404_empty.json");
const TOO_WIDE: &str = include_str!("fixtures/history/bcb_406_window.json");
const BVSP: &str = include_str!("fixtures/history/yahoo_bvsp.json");
const KNRI11: &str = include_str!("fixtures/history/yahoo_knri11.json");
const UNKNOWN: &str = include_str!("fixtures/history/yahoo_404.json");
const AAPL_SPLIT: &str = include_str!("fixtures/history/yahoo_aapl_split.json");
const IFIX_2010: &str = include_str!("fixtures/history/b3_ifix_2010.json");
const IFIX_2025: &str = include_str!("fixtures/history/b3_ifix_2025.json");
const IFIX_2026: &str = include_str!("fixtures/history/b3_ifix_2026.json");
const IFIX_EMPTY: &str = include_str!("fixtures/history/b3_ifix_empty.json");

fn json(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.as_bytes().to_vec(), "application/json")
}

#[tokio::test]
async fn bcb_reads_daily_and_monthly_series() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.12/dados"))
        .and(query_param("formato", "json"))
        .and(query_param("dataInicial", "17/11/2026"))
        .and(query_param("dataFinal", "25/11/2026"))
        .respond_with(json(200, CDI))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.433/dados"))
        .respond_with(json(200, IPCA))
        .mount(&server)
        .await;
    let bcb = BcbClient::with_base(reqwest::Client::new(), &server.uri());
    let cdi = bcb
        .series(12, date!(2026 - 11 - 17), date!(2026 - 11 - 25))
        .await
        .unwrap();
    // 20/11 é feriado; 25/11 ainda não foi publicado.
    assert_eq!(cdi.len(), 5);
    assert_eq!(cdi[0], (date!(2026 - 11 - 17), dec!(0.050788)));
    assert_eq!(cdi[3].0, date!(2026 - 11 - 23));
    let ipca = bcb
        .series(433, date!(2026 - 06 - 01), date!(2026 - 11 - 25))
        .await
        .unwrap();
    assert_eq!(ipca.len(), 5);
    assert_eq!(ipca[2], (date!(2026 - 08 - 01), dec!(-0.32)));
}

#[tokio::test]
async fn bcb_window_without_data_is_empty_and_errors_are_unavailable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.433/dados"))
        .respond_with(json(404, EMPTY))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.12/dados"))
        .respond_with(json(406, TOO_WIDE))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.11/dados"))
        .respond_with(json(200, r#"[{"data":"31/02/2026","valor":"0.05"}]"#))
        .mount(&server)
        .await;
    let bcb = BcbClient::with_base(reqwest::Client::new(), &server.uri());
    let d = date!(2026 - 11 - 25);
    assert!(bcb.series(433, d, d).await.unwrap().is_empty());
    assert!(matches!(
        bcb.series(12, d, d).await,
        Err(HistoryError::Unavailable(_))
    ));
    assert!(matches!(
        bcb.series(11, d, d).await,
        Err(HistoryError::Unavailable(_))
    ));
}

/// A API recusa janela diária de mais de 10 anos: o cliente pede em pedaços.
#[tokio::test]
async fn bcb_pages_long_windows() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.12/dados"))
        .and(query_param("dataInicial", "01/01/2006"))
        .respond_with(json(200, r#"[{"data":"02/01/2006","valor":"0.06"}]"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.12/dados"))
        .and(query_param("dataFinal", "25/11/2026"))
        .respond_with(json(200, CDI))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.12/dados"))
        .respond_with(json(200, "[]"))
        .mount(&server)
        .await;
    let bcb = BcbClient::with_base(reqwest::Client::new(), &server.uri());
    let all = bcb
        .series(12, date!(2006 - 01 - 01), date!(2026 - 11 - 25))
        .await
        .unwrap();
    assert_eq!(all.first().map(|r| r.0), Some(date!(2006 - 01 - 02)));
    assert_eq!(all.last().map(|r| r.0), Some(date!(2026 - 11 - 24)));
    let requests = server.received_requests().await.unwrap();
    // 20 anos em janelas de 3.600 dias: três pedidos.
    assert_eq!(requests.len(), 3);
}

#[tokio::test]
async fn yahoo_reads_closes_in_the_exchange_day() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/%5EBVSP"))
        .and(query_param("interval", "1d"))
        .respond_with(json(200, BVSP))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/KNRI11.SA"))
        .respond_with(json(200, KNRI11))
        .mount(&server)
        .await;
    let yahoo = YahooHistory::with_base(reqwest::Client::new(), &server.uri());
    // Depois do fechamento de 25/11 (o pregão regular acaba às 20:00 UTC).
    let closed = yahoo
        .closes_at(
            "^BVSP",
            date!(2026 - 11 - 17),
            date!(2026 - 11 - 25),
            datetime!(2026-11-25 22:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(closed.len(), 6);
    assert_eq!(closed[0], (date!(2026 - 11 - 17), dec!(169817)));
    assert_eq!(closed[3], (date!(2026 - 11 - 23), dec!(164796)));
    // Com o pregão aberto, o dia de hoje ainda não é fechamento.
    let open = yahoo
        .closes_at(
            "^BVSP",
            date!(2026 - 11 - 17),
            date!(2026 - 11 - 25),
            datetime!(2026-11-25 15:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(open.last().map(|r| r.0), Some(date!(2026 - 11 - 24)));
    // A janela corta as pontas.
    let knri = yahoo
        .closes_at(
            "KNRI11.SA",
            date!(2026 - 11 - 18),
            date!(2026 - 11 - 23),
            datetime!(2026-11-26 12:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(
        knri,
        vec![
            (date!(2026 - 11 - 18), dec!(144.71000671)),
            (date!(2026 - 11 - 19), dec!(143.11000061)),
            (date!(2026 - 11 - 23), dec!(140.5)),
        ]
    );
}

#[tokio::test]
async fn yahoo_unknown_symbol_and_null_closes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/NAOEXISTE123.SA"))
        .respond_with(json(404, UNKNOWN))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/USDBRL%3DX"))
        .respond_with(json(
            200,
            r#"{"chart":{"result":[{"meta":{"gmtoffset":0},"timestamp":[1795392000,1795478400],
                "indicators":{"quote":[{"close":[null,5.9284]}]}}],"error":null}}"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/FORA"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let yahoo = YahooHistory::with_base(reqwest::Client::new(), &server.uri());
    let (from, to) = (date!(2026 - 11 - 20), date!(2026 - 11 - 25));
    assert_eq!(
        yahoo.closes("NAOEXISTE123.SA", from, to).await,
        Err(HistoryError::NotFound)
    );
    assert_eq!(
        yahoo.closes("USDBRL=X", from, to).await.unwrap(),
        vec![(date!(2026 - 11 - 24), dec!(5.9284))]
    );
    assert!(matches!(
        yahoo.closes("FORA", from, to).await,
        Err(HistoryError::Unavailable(_))
    ));
}

/// O Yahoo divide o passado pelo fator de cada desdobramento; o cliente pede
/// os eventos até hoje (só assim vem o desdobro que caiu depois da janela) e
/// devolve o fechamento real de cada dia.
#[tokio::test]
async fn yahoo_undoes_the_split_adjustment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/AAPL"))
        .and(query_param("events", "split"))
        .respond_with(json(200, AAPL_SPLIT))
        .mount(&server)
        .await;
    let yahoo = YahooHistory::with_base(reqwest::Client::new(), &server.uri());
    let now = datetime!(2026-11-25 22:00 UTC);
    let closes = yahoo
        .closes_at("AAPL", date!(2020 - 08 - 27), date!(2020 - 08 - 31), now)
        .await
        .unwrap();
    assert_eq!(
        closes,
        vec![
            // 125,01 e 124,8075 ajustados: 500,04 e 499,23 no dia.
            (date!(2020 - 08 - 27), dec!(500.04000856)),
            (date!(2020 - 08 - 28), dec!(499.230011)),
            // No dia do desdobro o preço já é o novo.
            (date!(2020 - 08 - 31), dec!(129.03999329)),
        ]
    );
    let requests = server.received_requests().await.unwrap();
    let period2: i64 = requests[0]
        .url
        .query_pairs()
        .find(|(k, _)| k == "period2")
        .map(|(_, v)| v.parse().unwrap())
        .unwrap();
    assert!(period2 > now.unix_timestamp(), "{period2}");
}

/// O comparador guarda os desdobramentos do mesmo pedido: a data do evento e o
/// fator (4 para 1 = 4), os mesmos que desfazem o ajuste dos fechamentos.
#[tokio::test]
async fn yahoo_returns_the_splits_with_the_closes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/AAPL"))
        .and(query_param("events", "split"))
        .respond_with(json(200, AAPL_SPLIT))
        .mount(&server)
        .await;
    let provider = PublicHistory::with_bases(reqwest::Client::new(), &server.uri(), &server.uri());
    let (closes, splits) = provider
        .closes_and_splits("AAPL", date!(2020 - 08 - 27), date!(2020 - 08 - 31))
        .await
        .unwrap();
    assert_eq!(closes.len(), 3);
    assert_eq!(splits, vec![(date!(2020 - 08 - 31), dec!(4))]);
}

/// O provedor público junta os dois: índice do SGS e Ibovespa do Yahoo.
#[tokio::test]
async fn public_history_routes_each_index() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.433/dados"))
        .respond_with(json(200, IPCA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/%5EBVSP"))
        .respond_with(json(200, BVSP))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v8/finance/chart/KNRI11.SA"))
        .respond_with(json(200, KNRI11))
        .mount(&server)
        .await;
    let provider = PublicHistory::with_bases(reqwest::Client::new(), &server.uri(), &server.uri());
    let (from, to) = (date!(2026 - 06 - 01), date!(2026 - 11 - 24));
    let ipca = provider.index(IndexCode::Ipca, from, to).await.unwrap();
    assert_eq!(ipca.len(), 5);
    let ibov = provider.index(IndexCode::Ibov, from, to).await.unwrap();
    assert_eq!(ibov.last(), Some(&(date!(2026 - 11 - 24), dec!(164090))));
    let knri = provider.closes("KNRI11.SA", from, to).await.unwrap();
    assert_eq!(knri.len(), 5);
}

fn ifix_path(year: i32) -> String {
    format!(
        "/indexStatisticsProxy/IndexCall/GetPortfolioDay/{}",
        b3::query(b3::IFIX, year)
    )
}

async fn ifix_server() -> MockServer {
    let server = MockServer::start().await;
    for (year, body) in [(2010, IFIX_2010), (2025, IFIX_2025), (2026, IFIX_2026)] {
        Mock::given(method("GET"))
            .and(path(ifix_path(year)))
            .respond_with(json(200, body))
            .mount(&server)
            .await;
    }
    server
}

/// O IFIX das estatísticas da B3: um pedido por ano, os fechamentos em texto
/// brasileiro viram número, e a janela corta o que pediu.
#[tokio::test]
async fn b3_reads_ifix_closes_year_by_year() {
    let server = ifix_server().await;
    let b3 = B3Indexes::with_base(reqwest::Client::new(), &server.uri())
        .with_pause(std::time::Duration::ZERO);
    let turn = b3
        .levels(b3::IFIX, date!(2025 - 12 - 26), date!(2026 - 01 - 05))
        .await
        .unwrap();
    // 26, 29 e 30/12 (31/12 sem pregão), 02 e 05/01.
    assert_eq!(
        turn,
        vec![
            (date!(2025 - 12 - 26), dec!(3775.56)),
            (date!(2025 - 12 - 29), dec!(3772.30)),
            (date!(2025 - 12 - 30), dec!(3775.31)),
            (date!(2026 - 01 - 02), dec!(3778.89)),
            (date!(2026 - 01 - 05), dec!(3782.93)),
        ]
    );
    let year = b3
        .levels(b3::IFIX, date!(2025 - 01 - 01), date!(2025 - 12 - 31))
        .await
        .unwrap();
    assert_eq!(year.len(), 250);
    // O pregão de hoje (27/11/2026, às 14h) ainda não estava na tabela.
    let november = b3
        .levels(b3::IFIX, date!(2026 - 11 - 20), date!(2026 - 11 - 27))
        .await
        .unwrap();
    assert_eq!(
        november.last(),
        Some(&(date!(2026 - 11 - 26), dec!(3578.98)))
    );
    assert_eq!(november.len(), 4);
}

/// Antes de 2010 o IFIX não existe: o ano nem é pedido. Ano sem dado é lista
/// vazia; resposta que não é a tabela é erro, e a tarefa segue sem o índice.
#[tokio::test]
async fn b3_skips_years_before_the_index_and_reports_failures() {
    let server = ifix_server().await;
    Mock::given(method("GET"))
        .and(path(ifix_path(2009)))
        .respond_with(json(200, IFIX_EMPTY))
        .expect(0)
        .mount(&server)
        .await;
    let b3 = B3Indexes::with_base(reqwest::Client::new(), &server.uri())
        .with_pause(std::time::Duration::ZERO);
    let start = b3
        .levels(b3::IFIX, date!(2009 - 06 - 01), date!(2010 - 12 - 31))
        .await
        .unwrap();
    assert_eq!(start, vec![(date!(2010 - 12 - 30), dec!(1000))]);

    let broken = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ifix_path(2026)))
        .respond_with(ResponseTemplate::new(503))
        .mount(&broken)
        .await;
    Mock::given(method("GET"))
        .and(path(ifix_path(2025)))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>manutencao</html>"))
        .mount(&broken)
        .await;
    let b3 = B3Indexes::with_base(reqwest::Client::new(), &broken.uri())
        .with_pause(std::time::Duration::ZERO);
    let d = date!(2026 - 11 - 25);
    assert!(matches!(
        b3.levels(b3::IFIX, d, d).await,
        Err(HistoryError::Unavailable(_))
    ));
    assert!(matches!(
        b3.levels(b3::IFIX, date!(2025 - 03 - 01), date!(2025 - 03 - 31))
            .await,
        Err(HistoryError::Unavailable(_))
    ));
}

/// O provedor público pede o IFIX à B3, com o resto das séries.
#[tokio::test]
async fn public_history_asks_the_b3_for_ifix() {
    let server = ifix_server().await;
    let provider = PublicHistory::with_bases(reqwest::Client::new(), &server.uri(), &server.uri())
        .with_b3(
            B3Indexes::with_base(reqwest::Client::new(), &server.uri())
                .with_pause(std::time::Duration::ZERO),
        );
    let ifix = provider
        .index(
            IndexCode::Ifix,
            date!(2026 - 11 - 23),
            date!(2026 - 11 - 26),
        )
        .await
        .unwrap();
    assert_eq!(ifix.first(), Some(&(date!(2026 - 11 - 23), dec!(3440.08))));
    assert_eq!(ifix.len(), 4);
}
