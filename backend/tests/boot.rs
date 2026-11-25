//! O arranque inteiro, como no pod: conecta, migra, abre as duas portas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use prumo::config::Config;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_ok(url: &str) -> reqwest::Response {
    for _ in 0..100 {
        if let Ok(response) = reqwest::get(url).await {
            return response;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{url} nunca respondeu");
}

#[tokio::test]
async fn the_binary_boots_migrates_and_serves_both_ports() {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );
    let (public, operational) = (free_port(), free_port());
    let env = [
        ("DATABASE_URL", url),
        ("AUTH_MODE", "dev".to_owned()),
        ("DEV_USER_EMAIL", "boot@prumo.test".to_owned()),
        ("PORT", public.to_string()),
        ("OPERATIONAL_PORT", operational.to_string()),
        ("QUOTES_ENABLED", "false".to_owned()),
        ("APP_VERSION", "1.2.3".to_owned()),
        ("GIT_SHA", "deadbee".to_owned()),
    ];
    let config =
        Config::from_lookup(|name| env.iter().find(|(k, _)| *k == name).map(|(_, v)| v.clone()))
            .unwrap();
    let server = tokio::spawn(prumo::run(config));

    let info: serde_json::Value = wait_ok(&format!("http://127.0.0.1:{public}/api/v1/info"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(info["version"], "1.2.3");
    assert_eq!(info["commit"], "deadbee");

    let me: serde_json::Value = reqwest::get(format!("http://127.0.0.1:{public}/api/v1/me"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["email"], "boot@prumo.test");

    // Cotações desligadas: a busca responde 502, e o resto segue de pé.
    let search = reqwest::get(format!(
        "http://127.0.0.1:{public}/api/v1/symbols?query=knri"
    ))
    .await
    .unwrap();
    assert_eq!(search.status(), 502);

    let ready = wait_ok(&format!("http://127.0.0.1:{operational}/readyz")).await;
    assert_eq!(ready.status(), 200);
    let metrics = reqwest::get(format!("http://127.0.0.1:{operational}/metrics"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        metrics.contains("http_requests_total"),
        "metricas: {metrics}"
    );
    let hidden = reqwest::get(format!("http://127.0.0.1:{public}/readyz"))
        .await
        .unwrap();
    assert_eq!(hidden.status(), 404);

    server.abort();
}
