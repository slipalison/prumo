//! O cliente da brapi contra um servidor falso, com a resposta gravada no
//! formato real (26/11/2026; ver `tests/fixtures/README.md`): o token vai no
//! cabeçalho e só para a API, os erros viram o erro certo, e nada que vem de
//! fora (host do logo, tipo, tamanho, redirecionamento) é aceito sem conferir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use prumo::brapi::{BrapiLogos, LogoError, LogoSource};
use prumo::config::Secret;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const QUOTE: &str = include_str!("fixtures/brapi_v2_quote_petr4.json");
const LOGO: &[u8] = include_bytes!("fixtures/logo_exemplo.svg");
const TOKEN: &str = "token-secreto-da-brapi";
const QUOTE_PATH: &str = "/api/v2/stocks/quote";
const ICON_PATH: &str = "/icons/PETR4.svg";

fn client(api: &MockServer, icons: &MockServer) -> BrapiLogos {
    // O mesmo cliente do app: sem seguir redirecionamento.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    BrapiLogos::with_bases(http, Secret::new(TOKEN), &api.uri(), &icons.uri())
}

/// A resposta gravada, com o `logourl` apontando para o servidor dos logos.
fn quote_pointing_to(icons: &MockServer) -> String {
    QUOTE.replace("https://icons.brapi.dev", &icons.uri())
}

async fn serve_quote(api: &MockServer, body: String) {
    Mock::given(method("GET"))
        .and(path(QUOTE_PATH))
        .and(query_param("symbols", "PETR4"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(api)
        .await;
}

async fn serve_logo(icons: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(ICON_PATH))
        .respond_with(template)
        .mount(icons)
        .await;
}

fn svg() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(LOGO, "image/svg+xml")
}

#[tokio::test]
async fn fetches_the_logo_with_the_token_in_the_header_only() {
    let (api, icons) = (MockServer::start().await, MockServer::start().await);
    serve_quote(&api, quote_pointing_to(&icons)).await;
    serve_logo(&icons, svg()).await;

    let logo = client(&api, &icons).fetch("PETR4").await.unwrap();
    assert_eq!(logo.bytes, LOGO);
    assert_eq!(logo.url, format!("{}{ICON_PATH}", icons.uri()));

    // A API recebeu o token no cabeçalho, e a URL não o carrega.
    let calls = api.received_requests().await.unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].headers.get("authorization").unwrap(),
        &format!("Bearer {TOKEN}")
    );
    assert!(!calls[0].url.to_string().contains(TOKEN));
    assert!(!calls[0].url.query().unwrap_or_default().contains("token"));
    // O host dos logos não recebe credencial nenhuma.
    let downloads = icons.received_requests().await.unwrap();
    assert_eq!(downloads.len(), 1);
    assert!(downloads[0].headers.get("authorization").is_none());
    assert!(downloads[0].headers.get("cookie").is_none());
}

#[tokio::test]
async fn the_header_matcher_really_checks_the_token() {
    // Contra-prova do teste acima: sem o cabeçalho certo o falso não responde.
    let (api, icons) = (MockServer::start().await, MockServer::start().await);
    Mock::given(method("GET"))
        .and(path(QUOTE_PATH))
        .and(header("authorization", "Bearer outro-token"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(QUOTE, "application/json"))
        .mount(&api)
        .await;
    // Sem casar o cabeçalho, o falso responde 404: o token do Prumo é de fato
    // o que foi enviado, e não outro.
    assert_eq!(
        client(&api, &icons).fetch("PETR4").await,
        Err(LogoError::NotFound)
    );
}

#[tokio::test]
async fn http_errors_become_the_right_error() {
    let cases: [(u16, Option<&str>, LogoError); 7] = [
        (401, None, LogoError::Unauthorized),
        (403, None, LogoError::Forbidden),
        (404, None, LogoError::NotFound),
        (400, None, LogoError::NotFound),
        (
            429,
            Some("30"),
            LogoError::RateLimited(Duration::from_secs(30)),
        ),
        // Sem `Retry-After`, espera um minuto.
        (429, None, LogoError::RateLimited(Duration::from_secs(60))),
        (503, None, LogoError::Unavailable("status 503".to_owned())),
    ];
    for (status, retry_after, expected) in cases {
        let (api, icons) = (MockServer::start().await, MockServer::start().await);
        let mut reply = ResponseTemplate::new(status).set_body_raw(
            // A resposta de erro da brapi repete o que recebeu; o Prumo não a lê.
            format!(r#"{{"error":true,"message":"{TOKEN}","code":"X"}}"#),
            "application/json",
        );
        if let Some(seconds) = retry_after {
            reply = reply.insert_header("retry-after", seconds);
        }
        Mock::given(method("GET"))
            .and(path(QUOTE_PATH))
            .respond_with(reply)
            .mount(&api)
            .await;
        let error = client(&api, &icons).fetch("PETR4").await.unwrap_err();
        assert_eq!(error, expected, "status {status}");
        // O erro (que vai para o log e para o banco) nunca cita o token.
        assert!(!error.to_string().contains(TOKEN), "{error}");
        assert!(!format!("{error:?}").contains(TOKEN));
    }
}

#[tokio::test]
async fn the_missing_token_answer_of_the_provider_is_an_unauthorized() {
    // O corpo real que a brapi devolve sem token (medido em 26/11/2026).
    let (api, icons) = (MockServer::start().await, MockServer::start().await);
    Mock::given(method("GET"))
        .and(path(QUOTE_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_raw(
            r#"{"error":true,"message":"Token de autenticação não fornecido","code":"MISSING_TOKEN"}"#,
            "application/json",
        ))
        .mount(&api)
        .await;
    assert_eq!(
        client(&api, &icons).fetch("ITSA4").await,
        Err(LogoError::Unauthorized)
    );
}

#[tokio::test]
async fn a_response_without_logo_is_not_found() {
    for body in [
        r#"{"results":[]}"#.to_owned(),
        r#"{"results":[{"symbol":"PETR4","data":{"logourl":null}}]}"#.to_owned(),
        r#"{"results":[{"symbol":"PETR4","data":{"logourl":"  "}}]}"#.to_owned(),
        r#"{"results":[{"symbol":"PETR4","data":{}}]}"#.to_owned(),
        r#"{"results":[{"symbol":"PETR4"}]}"#.to_owned(),
    ] {
        let (api, icons) = (MockServer::start().await, MockServer::start().await);
        serve_quote(&api, body.clone()).await;
        assert_eq!(
            client(&api, &icons).fetch("PETR4").await,
            Err(LogoError::NotFound),
            "{body}"
        );
    }
}

#[tokio::test]
async fn garbage_from_the_api_is_unavailable_not_a_panic() {
    for body in ["<html>manutencao</html>", "", r#"{"results":"x"}"#] {
        let (api, icons) = (MockServer::start().await, MockServer::start().await);
        serve_quote(&api, body.to_owned()).await;
        assert!(
            matches!(
                client(&api, &icons).fetch("PETR4").await,
                Err(LogoError::Unavailable(_))
            ),
            "{body}"
        );
    }
}

#[tokio::test]
async fn an_api_that_is_gone_is_unavailable_without_leaking_the_url() {
    // Uma porta que acabou de ser liberada: ninguém atende nela.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let icons = MockServer::start().await;
    let brapi = BrapiLogos::with_bases(
        reqwest::Client::new(),
        Secret::new(TOKEN),
        &address,
        &icons.uri(),
    );
    let error = brapi.fetch("PETR4").await.unwrap_err();
    assert!(matches!(error, LogoError::Unavailable(_)), "{error:?}");
    assert!(!error.to_string().contains(&address), "{error}");
    assert!(!error.to_string().contains(TOKEN));
}

#[tokio::test]
async fn a_logo_url_outside_the_icon_origin_is_never_followed() {
    let (api, icons, evil) = (
        MockServer::start().await,
        MockServer::start().await,
        MockServer::start().await,
    );
    // A API manda baixar de outro lugar (resposta adulterada, ou comprometida).
    serve_quote(&api, QUOTE.replace("https://icons.brapi.dev", &evil.uri())).await;
    serve_logo(&evil, svg()).await;
    assert!(matches!(
        client(&api, &icons).fetch("PETR4").await,
        Err(LogoError::Rejected(_))
    ));
    assert!(evil.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_provider_own_mark_is_not_the_logo_of_an_asset() {
    let (api, icons) = (MockServer::start().await, MockServer::start().await);
    serve_quote(
        &api,
        quote_pointing_to(&icons).replace("PETR4.svg", "BRAPI.svg"),
    )
    .await;
    assert_eq!(
        client(&api, &icons).fetch("PETR4").await,
        Err(LogoError::NotFound)
    );
    assert!(icons.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_logo_that_is_not_svg_or_is_too_big_is_rejected() {
    let big = vec![b' '; 129 * 1024];
    let cases = [
        ResponseTemplate::new(200).set_body_raw("<html></html>", "text/html"),
        ResponseTemplate::new(200).set_body_raw(LOGO, "application/octet-stream"),
        ResponseTemplate::new(200).set_body_raw(big, "image/svg+xml"),
    ];
    for template in cases {
        let (api, icons) = (MockServer::start().await, MockServer::start().await);
        serve_quote(&api, quote_pointing_to(&icons)).await;
        serve_logo(&icons, template).await;
        assert!(matches!(
            client(&api, &icons).fetch("PETR4").await,
            Err(LogoError::Rejected(_))
        ));
    }
}

#[tokio::test]
async fn a_missing_or_failing_logo_file_is_told_apart() {
    for (status, expected) in [
        (404, LogoError::NotFound),
        (
            502,
            LogoError::Unavailable("logo: status 502 Bad Gateway".to_owned()),
        ),
    ] {
        let (api, icons) = (MockServer::start().await, MockServer::start().await);
        serve_quote(&api, quote_pointing_to(&icons)).await;
        serve_logo(&icons, ResponseTemplate::new(status)).await;
        assert_eq!(client(&api, &icons).fetch("PETR4").await, Err(expected));
    }
}

#[tokio::test]
async fn a_redirect_from_the_icon_host_is_not_followed() {
    let (api, icons, elsewhere) = (
        MockServer::start().await,
        MockServer::start().await,
        MockServer::start().await,
    );
    serve_quote(&api, quote_pointing_to(&icons)).await;
    serve_logo(
        &icons,
        ResponseTemplate::new(302).insert_header("location", format!("{}/x.svg", elsewhere.uri())),
    )
    .await;
    Mock::given(method("GET"))
        .respond_with(svg())
        .mount(&elsewhere)
        .await;
    assert!(matches!(
        client(&api, &icons).fetch("PETR4").await,
        Err(LogoError::Unavailable(_))
    ));
    assert!(elsewhere.received_requests().await.unwrap().is_empty());
}
