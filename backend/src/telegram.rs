//! Cliente da Bot API do Telegram (`api.telegram.org`, HTTPS): o que os alertas
//! precisam — quem é o bot, as mensagens que chegaram (`getUpdates` com
//! `offset`, em polling, porque o app fica atrás do Cloudflare Access e um
//! webhook seria barrado) e `sendMessage`.
//!
//! O token do bot é segredo do servidor e vai **dentro da URL** da Bot API
//! (`/bot<token>/método`). Por isso nenhum erro daqui carrega a URL nem o
//! token: o erro do `reqwest` é limpo com `without_url`, e o texto que o
//! Telegram devolve passa por [`scrub`].

use std::time::Duration;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::config::Secret;

pub const API_BASE: &str = "https://api.telegram.org";

/// Tempo de espera por resposta além do `timeout` do long polling.
const SLACK: Duration = Duration::from_secs(10);
const SEND_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    pub chat: Chat,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Chat {
    pub id: i64,
    /// `private`, `group`, `supergroup` ou `channel`.
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TelegramError {
    /// 429: o Telegram pede para esperar.
    #[error("telegram pediu para esperar {0:?}")]
    RateLimited(Duration),
    /// 403: o bot foi bloqueado ou removido do chat.
    #[error("telegram recusou: {0}")]
    Forbidden(String),
    /// 409: outro `getUpdates` está em andamento (ou há webhook configurado).
    #[error("telegram: outro getUpdates em andamento")]
    Conflict,
    #[error("telegram respondeu {status}: {description}")]
    Api { status: u16, description: String },
    #[error("rede: {0}")]
    Network(String),
}

#[async_trait]
pub trait TelegramApi: Send + Sync {
    /// O `@usuario` do bot (sem o `@`), para a tela dizer com quem conversar.
    async fn username(&self) -> Result<String, TelegramError>;

    /// As mensagens a partir de `offset` (as anteriores ficam confirmadas).
    /// Com `timeout` > 0 a chamada espera até esse tempo por mensagem nova.
    async fn updates(&self, offset: i64, timeout: Duration) -> Result<Vec<Update>, TelegramError>;

    async fn send(&self, chat_id: i64, text: &str) -> Result<(), TelegramError>;
}

#[derive(Clone)]
pub struct BotApi {
    http: reqwest::Client,
    base: String,
    token: Secret,
}

impl std::fmt::Debug for BotApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BotApi")
            .field("base", &self.base)
            .field("token", &self.token)
            .finish()
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: Option<T>,
    error_code: Option<u16>,
    description: Option<String>,
    parameters: Option<Parameters>,
}

#[derive(Deserialize)]
struct Parameters {
    retry_after: Option<u64>,
}

#[derive(Deserialize)]
struct Me {
    username: Option<String>,
}

#[derive(Serialize)]
struct GetUpdates {
    offset: i64,
    timeout: u64,
    /// Só mensagens: o resto (edição, canal, enquete) não interessa.
    allowed_updates: [&'static str; 1],
}

#[derive(Serialize)]
struct SendMessage<'a> {
    chat_id: i64,
    text: &'a str,
    disable_web_page_preview: bool,
}

impl BotApi {
    pub fn new(http: reqwest::Client, token: Secret) -> Self {
        Self::with_base(http, token, API_BASE)
    }

    pub fn with_base(http: reqwest::Client, token: Secret, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            token,
        }
    }

    /// O texto sem o token (a descrição do Telegram não o traz, mas o que sai
    /// daqui vai para log).
    fn scrub(&self, text: &str) -> String {
        scrub(text, self.token.expose())
    }

    async fn call<B: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        body: &B,
        timeout: Duration,
    ) -> Result<T, TelegramError> {
        let url = format!("{}/bot{}/{method}", self.base, self.token.expose());
        let response = self
            .http
            .post(url)
            .timeout(timeout)
            .json(body)
            .send()
            .await
            .map_err(|e| TelegramError::Network(self.scrub(&e.without_url().to_string())))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| TelegramError::Network(self.scrub(&e.without_url().to_string())))?;
        let envelope: Envelope<T> =
            serde_json::from_slice(&bytes).map_err(|_| TelegramError::Api {
                status,
                description: "resposta que nao e do formato da Bot API".to_owned(),
            })?;
        if envelope.ok {
            return envelope.result.ok_or(TelegramError::Api {
                status,
                description: "resposta ok sem resultado".to_owned(),
            });
        }
        let code = envelope.error_code.unwrap_or(status);
        let description = self.scrub(envelope.description.as_deref().unwrap_or("sem descricao"));
        Err(match code {
            429 => TelegramError::RateLimited(Duration::from_secs(
                envelope.parameters.and_then(|p| p.retry_after).unwrap_or(1),
            )),
            403 => TelegramError::Forbidden(description),
            409 => TelegramError::Conflict,
            other => TelegramError::Api {
                status: other,
                description,
            },
        })
    }
}

/// Troca toda ocorrência de `secret` por `***`.
pub fn scrub(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_owned();
    }
    text.replace(secret, "***")
}

#[async_trait]
impl TelegramApi for BotApi {
    async fn username(&self) -> Result<String, TelegramError> {
        let me: Me = self
            .call("getMe", &serde_json::json!({}), SEND_TIMEOUT)
            .await?;
        me.username.ok_or(TelegramError::Api {
            status: 200,
            description: "o bot nao tem usuario".to_owned(),
        })
    }

    async fn updates(&self, offset: i64, timeout: Duration) -> Result<Vec<Update>, TelegramError> {
        let body = GetUpdates {
            offset,
            timeout: timeout.as_secs(),
            allowed_updates: ["message"],
        };
        self.call("getUpdates", &body, timeout + SLACK).await
    }

    async fn send(&self, chat_id: i64, text: &str) -> Result<(), TelegramError> {
        let body = SendMessage {
            chat_id,
            text,
            disable_web_page_preview: true,
        };
        let _: serde_json::Value = self.call("sendMessage", &body, SEND_TIMEOUT).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_hides_the_secret_everywhere() {
        assert_eq!(
            scrub("erro em /bot123:abc/getMe e 123:abc", "123:abc"),
            "erro em /bot***/getMe e ***"
        );
        assert_eq!(scrub("nada", ""), "nada");
    }

    #[test]
    fn debug_never_shows_the_token() {
        let api = BotApi::new(reqwest::Client::new(), Secret::new("123:segredo"));
        let printed = format!("{api:?}");
        assert!(!printed.contains("segredo"), "{printed}");
    }
}
