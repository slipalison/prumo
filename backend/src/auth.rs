//! Quem está chamando.
//!
//! Em produção, o Cloudflare Access autentica na borda e manda o JWT em
//! `Cf-Access-Jwt-Assertion` (e no cookie `CF_Authorization`). O gateway do
//! cluster já confere o token; o app confere de novo — assinatura pelo JWKS do
//! team, `iss`, `aud` e `exp` — porque a política do gateway é de outro
//! repositório e um erro lá não pode abrir a carteira de ninguém.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::http::{header, HeaderMap};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

use crate::error::ApiError;

const ACCESS_HEADER: &str = "cf-access-jwt-assertion";
const ACCESS_COOKIE: &str = "CF_Authorization";
/// Intervalo mínimo entre duas buscas do JWKS provocadas por `kid` desconhecido.
/// Conta a partir do FIM da busca: marcar antes faria um token válido ser
/// recusado durante o intervalo inteiro se a primeira busca falhasse.
const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);

/// A pessoa autenticada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub email: String,
}

#[async_trait]
pub trait Authenticator: Send + Sync {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<Identity, ApiError>;
}

/// Desenvolvimento: toda requisição é a mesma pessoa.
pub struct DevAuthenticator {
    email: String,
}

impl DevAuthenticator {
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
        }
    }
}

#[async_trait]
impl Authenticator for DevAuthenticator {
    async fn authenticate(&self, _headers: &HeaderMap) -> Result<Identity, ApiError> {
        Ok(Identity {
            email: self.email.clone(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccessClaims {
    email: Option<String>,
}

#[derive(Default)]
struct KeyCache {
    keys: HashMap<String, DecodingKey>,
    last_fetch_finished: Option<Instant>,
}

/// Confere o JWT do Cloudflare Access.
pub struct AccessAuthenticator {
    certs_url: String,
    validation: Validation,
    http: reqwest::Client,
    cache: RwLock<KeyCache>,
    fetching: Mutex<()>,
}

impl AccessAuthenticator {
    /// `team_domain` é o domínio do team (`<nome>.cloudflareaccess.com`).
    pub fn new(team_domain: &str, audience: &str, http: reqwest::Client) -> Self {
        let issuer = format!("https://{team_domain}");
        let certs_url = format!("{issuer}/cdn-cgi/access/certs");
        Self::with_endpoints(&issuer, &certs_url, audience, http)
    }

    /// Mesmo que [`AccessAuthenticator::new`], com emissor e JWKS explícitos.
    pub fn with_endpoints(
        issuer: &str,
        certs_url: &str,
        audience: &str,
        http: reqwest::Client,
    ) -> Self {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[audience]);
        validation.set_issuer(&[issuer]);
        validation.leeway = 60;
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        Self {
            certs_url: certs_url.to_owned(),
            validation,
            http,
            cache: RwLock::new(KeyCache::default()),
            fetching: Mutex::new(()),
        }
    }

    async fn key_for(&self, kid: &str) -> Option<DecodingKey> {
        if let Some(key) = self.cache.read().await.keys.get(kid) {
            return Some(key.clone());
        }
        // Uma busca por vez; quem esperou confere de novo antes de buscar.
        let _guard = self.fetching.lock().await;
        {
            let cache = self.cache.read().await;
            if let Some(key) = cache.keys.get(kid) {
                return Some(key.clone());
            }
            if cache
                .last_fetch_finished
                .is_some_and(|at| at.elapsed() < REFRESH_COOLDOWN)
            {
                return None;
            }
        }
        let fetched = self.fetch_keys().await;
        let mut cache = self.cache.write().await;
        cache.last_fetch_finished = Some(Instant::now());
        match fetched {
            Ok(keys) => cache.keys = keys,
            Err(error) => {
                tracing::warn!(error = %error, "nao foi possivel buscar o JWKS do Access")
            }
        }
        cache.keys.get(kid).cloned()
    }

    async fn fetch_keys(&self) -> Result<HashMap<String, DecodingKey>, reqwest::Error> {
        let jwks: Jwks = self
            .http
            .get(&self.certs_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(jwks
            .keys
            .into_iter()
            .filter(|k| k.kty == "RSA")
            .filter_map(|k| {
                let key =
                    DecodingKey::from_rsa_components(k.n.as_deref()?, k.e.as_deref()?).ok()?;
                Some((k.kid, key))
            })
            .collect())
    }
}

#[async_trait]
impl Authenticator for AccessAuthenticator {
    async fn authenticate(&self, headers: &HeaderMap) -> Result<Identity, ApiError> {
        let token = token_from(headers)
            .ok_or_else(|| ApiError::unauthenticated("Sem credencial do Access."))?;
        let kid = decode_header(&token)
            .ok()
            .and_then(|h| h.kid)
            .ok_or_else(|| ApiError::unauthenticated("Credencial malformada."))?;
        let key = self
            .key_for(&kid)
            .await
            .ok_or_else(|| ApiError::unauthenticated("Chave da credencial desconhecida."))?;
        let data = decode::<AccessClaims>(&token, &key, &self.validation).map_err(|error| {
            tracing::info!(error = %error, "token do Access recusado");
            ApiError::unauthenticated("Credencial invalida ou expirada.")
        })?;
        let email = data
            .claims
            .email
            .map(|e| e.trim().to_lowercase())
            .filter(|e| e.contains('@'))
            .ok_or_else(|| ApiError::unauthenticated("Credencial sem e-mail."))?;
        Ok(Identity { email })
    }
}

fn token_from(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(ACCESS_HEADER).and_then(|v| v.to_str().ok()) {
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, value)| *name == ACCESS_COOKIE && !value.is_empty())
        .map(|(_, value)| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn dev_mode_accepts_everyone_as_the_configured_email() {
        let auth = DevAuthenticator::new("eu@exemplo.com");
        let identity = auth.authenticate(&HeaderMap::new()).await.unwrap();
        assert_eq!(identity.email, "eu@exemplo.com");
    }

    #[test]
    fn token_comes_from_header_or_cookie() {
        let mut headers = HeaderMap::new();
        assert_eq!(token_from(&headers), None);
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; CF_Authorization=cookie-token; b=2"),
        );
        assert_eq!(token_from(&headers).as_deref(), Some("cookie-token"));
        headers.insert(ACCESS_HEADER, HeaderValue::from_static("header-token"));
        assert_eq!(token_from(&headers).as_deref(), Some("header-token"));
        headers.insert(ACCESS_HEADER, HeaderValue::from_static("  "));
        assert_eq!(token_from(&headers).as_deref(), Some("cookie-token"));
    }
}
