//! O cliente da Bot API do Telegram contra respostas no formato real
//! (conferido na documentação oficial, `core.telegram.org/bots/api`): `getMe`,
//! `getUpdates` com `offset`, `sendMessage`, e os erros 429, 403 e 409. O
//! servidor é um wiremock; nenhuma chamada sai para `api.telegram.org`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use prumo::config::Secret;
use prumo::telegram::{BotApi, Chat, Message, TelegramApi, TelegramError, Update};
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "123456:token-de-mentira";

async fn bot() -> (MockServer, BotApi) {
    let server = MockServer::start().await;
    let api = BotApi::with_base(reqwest::Client::new(), Secret::new(TOKEN), &server.uri());
    (server, api)
}

fn ok(result: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "result": result }))
}

#[tokio::test]
async fn get_me_returns_the_bot_username() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/getMe")))
        .respond_with(ok(json!({
            "id": 123456, "is_bot": true, "first_name": "Prumo", "username": "prumo_exemplo_bot"
        })))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(api.username().await.unwrap(), "prumo_exemplo_bot");
}

#[tokio::test]
async fn get_updates_sends_the_offset_and_reads_private_messages() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/getUpdates")))
        .and(body_json(json!({
            "offset": 42, "timeout": 0, "allowed_updates": ["message"]
        })))
        .respond_with(ok(json!([
            {
                "update_id": 42,
                "message": {
                    "message_id": 7,
                    "from": { "id": 555, "is_bot": false, "first_name": "Pessoa" },
                    "chat": { "id": 555, "type": "private", "first_name": "Pessoa" },
                    "date": 1_767_000_000,
                    "text": "/start K7M2QX9PTA"
                }
            },
            // Sem `message` (por exemplo, uma edição): o cliente aceita e segue.
            { "update_id": 43 },
            {
                "update_id": 44,
                "message": {
                    "message_id": 8,
                    "chat": { "id": -100200, "type": "supergroup", "title": "Grupo" },
                    "date": 1_767_000_001
                }
            }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let updates = api.updates(42, Duration::ZERO).await.unwrap();
    assert_eq!(
        updates,
        [
            Update {
                update_id: 42,
                message: Some(Message {
                    chat: Chat {
                        id: 555,
                        kind: "private".into()
                    },
                    text: Some("/start K7M2QX9PTA".into()),
                }),
            },
            Update {
                update_id: 43,
                message: None
            },
            Update {
                update_id: 44,
                message: Some(Message {
                    chat: Chat {
                        id: -100_200,
                        kind: "supergroup".into()
                    },
                    text: None,
                }),
            },
        ]
    );
}

#[tokio::test]
async fn the_long_poll_timeout_goes_in_seconds() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/getUpdates")))
        .and(body_json(json!({
            "offset": 0, "timeout": 25, "allowed_updates": ["message"]
        })))
        .respond_with(ok(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        api.updates(0, Duration::from_secs(25)).await.unwrap(),
        Vec::<Update>::new()
    );
}

#[tokio::test]
async fn send_message_posts_the_chat_and_the_text() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/sendMessage")))
        .and(body_json(json!({
            "chat_id": 555,
            "text": "Prumo - resumo de 25/11",
            "disable_web_page_preview": true
        })))
        .respond_with(ok(
            json!({ "message_id": 9, "chat": { "id": 555, "type": "private" } }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    api.send(555, "Prumo - resumo de 25/11").await.unwrap();
}

#[tokio::test]
async fn too_many_requests_carries_retry_after() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/sendMessage")))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "ok": false,
            "error_code": 429,
            "description": "Too Many Requests: retry after 7",
            "parameters": { "retry_after": 7 }
        })))
        .mount(&server)
        .await;
    assert_eq!(
        api.send(555, "oi").await,
        Err(TelegramError::RateLimited(Duration::from_secs(7)))
    );
}

#[tokio::test]
async fn forbidden_and_conflict_are_told_apart_from_other_errors() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/sendMessage")))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "ok": false, "error_code": 403,
            "description": "Forbidden: bot was blocked by the user"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/getUpdates")))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "ok": false, "error_code": 409,
            "description": "Conflict: terminated by other getUpdates request"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{TOKEN}/getMe")))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "ok": false, "error_code": 401, "description": "Unauthorized"
        })))
        .mount(&server)
        .await;

    assert_eq!(
        api.send(555, "oi").await,
        Err(TelegramError::Forbidden(
            "Forbidden: bot was blocked by the user".into()
        ))
    );
    assert_eq!(
        api.updates(0, Duration::ZERO).await,
        Err(TelegramError::Conflict)
    );
    assert_eq!(
        api.username().await,
        Err(TelegramError::Api {
            status: 401,
            description: "Unauthorized".into()
        })
    );
}

#[tokio::test]
async fn a_429_without_retry_after_waits_one_second() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "ok": false, "error_code": 429, "description": "Too Many Requests"
        })))
        .mount(&server)
        .await;
    assert_eq!(
        api.send(1, "oi").await,
        Err(TelegramError::RateLimited(Duration::from_secs(1)))
    );
}

#[tokio::test]
async fn a_reply_that_is_not_the_bot_api_is_an_error_not_a_panic() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"))
        .mount(&server)
        .await;
    let error = api.send(1, "oi").await.unwrap_err();
    assert!(
        matches!(error, TelegramError::Api { status: 502, .. }),
        "{error:?}"
    );

    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .mount(&server)
        .await;
    assert!(matches!(
        api.username().await,
        Err(TelegramError::Api { status: 200, .. })
    ));
}

#[tokio::test]
async fn a_bot_without_username_is_an_error() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .respond_with(ok(json!({ "id": 1, "is_bot": true, "first_name": "X" })))
        .mount(&server)
        .await;
    assert!(matches!(
        api.username().await,
        Err(TelegramError::Api { .. })
    ));
}

#[tokio::test]
async fn the_token_never_appears_in_an_error() {
    // Porta em que ninguém escuta: o erro de rede do reqwest cita a URL, que
    // aqui leva o token. O cliente tem de tirá-la.
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let api = BotApi::with_base(
        reqwest::Client::new(),
        Secret::new(TOKEN),
        &format!("http://127.0.0.1:{closed}"),
    );
    let error = api.send(1, "oi").await.unwrap_err();
    assert!(matches!(error, TelegramError::Network(_)), "{error:?}");
    let shown = format!("{error} / {error:?}");
    assert!(!shown.contains("token-de-mentira"), "{shown}");
    assert!(!shown.contains(TOKEN), "{shown}");
}

#[tokio::test]
async fn a_description_that_repeats_the_token_is_scrubbed() {
    let (server, api) = bot().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "ok": false, "error_code": 400,
            "description": format!("Bad Request: wrong token {TOKEN}")
        })))
        .mount(&server)
        .await;
    let error = api.send(1, "oi").await.unwrap_err();
    let shown = format!("{error}");
    assert!(!shown.contains("token-de-mentira"), "{shown}");
    assert!(shown.contains("***"), "{shown}");
}
