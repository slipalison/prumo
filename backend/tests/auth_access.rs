//! O verificador do Cloudflare Access contra um JWKS falso.
//!
//! A chave RSA nasce no próprio teste: nenhuma chave privada mora no
//! repositório (e o Gitleaks não tem o que reclamar).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use prumo::auth::{AccessAuthenticator, Authenticator};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const AUD: &str = "aud-do-prumo";

struct Key {
    kid: String,
    encoding: EncodingKey,
    jwk: Value,
}

fn key(kid: &str) -> Key {
    let private = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let der = private.to_pkcs1_der().unwrap();
    let public = private.to_public_key();
    Key {
        kid: kid.to_owned(),
        encoding: EncodingKey::from_rsa_der(der.as_bytes()),
        jwk: json!({
            "kid": kid,
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        }),
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn sign(key: &Key, claims: Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    encode(&header, &claims, &key.encoding).unwrap()
}

fn claims(server: &MockServer) -> Value {
    json!({
        "aud": [AUD],
        "iss": server.uri(),
        "email": "  Eu@Exemplo.COM ",
        "exp": now() + 600,
        "iat": now(),
        "sub": "123",
    })
}

async fn jwks(keys: &[&Key], calls: u64) -> MockServer {
    let server = MockServer::start().await;
    let body = json!({ "keys": keys.iter().map(|k| k.jwk.clone()).collect::<Vec<_>>(), "public_cert": {} });
    Mock::given(method("GET"))
        .and(path("/cdn-cgi/access/certs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(calls)
        .mount(&server)
        .await;
    server
}

fn authenticator(server: &MockServer) -> AccessAuthenticator {
    AccessAuthenticator::with_endpoints(
        &server.uri(),
        &format!("{}/cdn-cgi/access/certs", server.uri()),
        AUD,
        reqwest::Client::new(),
    )
}

fn with_header(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "cf-access-jwt-assertion",
        HeaderValue::from_str(token).unwrap(),
    );
    headers
}

async fn rejected(auth: &AccessAuthenticator, headers: &HeaderMap) -> String {
    let error = auth.authenticate(headers).await.expect_err("devia recusar");
    assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    assert_eq!(error.code, "unauthenticated");
    error.detail
}

#[tokio::test]
async fn a_valid_token_yields_the_normalized_email_and_keys_are_cached() {
    let key = key("k1");
    let server = jwks(&[&key], 1).await;
    let auth = authenticator(&server);
    let token = sign(&key, claims(&server));

    let identity = auth.authenticate(&with_header(&token)).await.unwrap();
    assert_eq!(identity.email, "eu@exemplo.com");

    let mut cookie = HeaderMap::new();
    cookie.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("x=1; CF_Authorization={token}")).unwrap(),
    );
    assert_eq!(
        auth.authenticate(&cookie).await.unwrap().email,
        "eu@exemplo.com"
    );
}

#[tokio::test]
async fn tokens_for_another_app_issuer_or_time_are_refused() {
    let key = key("k1");
    let server = jwks(&[&key], 1).await;
    let auth = authenticator(&server);

    let mut wrong_aud = claims(&server);
    wrong_aud["aud"] = json!(["aud-de-outro-painel"]);
    rejected(&auth, &with_header(&sign(&key, wrong_aud))).await;

    let mut wrong_iss = claims(&server);
    wrong_iss["iss"] = json!("https://outro-time.cloudflareaccess.com");
    rejected(&auth, &with_header(&sign(&key, wrong_iss))).await;

    let mut expired = claims(&server);
    expired["exp"] = json!(now() - 3600);
    rejected(&auth, &with_header(&sign(&key, expired))).await;

    let mut no_email = claims(&server);
    no_email.as_object_mut().unwrap().remove("email");
    assert_eq!(
        rejected(&auth, &with_header(&sign(&key, no_email))).await,
        "Credencial sem e-mail."
    );

    let mut bad_email = claims(&server);
    bad_email["email"] = json!("token-de-servico");
    rejected(&auth, &with_header(&sign(&key, bad_email))).await;
}

#[tokio::test]
async fn missing_or_malformed_tokens_never_reach_the_jwks() {
    let server = jwks(&[], 0).await;
    let auth = authenticator(&server);
    assert_eq!(
        rejected(&auth, &HeaderMap::new()).await,
        "Sem credencial do Access."
    );
    assert_eq!(
        rejected(&auth, &with_header("isso.nao.e-um-jwt")).await,
        "Credencial malformada."
    );

    // Assinado por alguém, mas sem `kid`: não há como escolher a chave.
    let key = key("k1");
    let token = encode(
        &Header::new(Algorithm::RS256),
        &claims(&server),
        &key.encoding,
    )
    .unwrap();
    assert_eq!(
        rejected(&auth, &with_header(&token)).await,
        "Credencial malformada."
    );
}

#[tokio::test]
async fn an_unknown_key_fetches_once_and_then_respects_the_cooldown() {
    let known = key("conhecida");
    let stranger = key("estranha");
    let server = jwks(&[&known], 1).await;
    let auth = authenticator(&server);
    let token = sign(&stranger, claims(&server));
    assert_eq!(
        rejected(&auth, &with_header(&token)).await,
        "Chave da credencial desconhecida."
    );
    // Logo em seguida não busca de novo (o `expect(1)` do mock confere no fim).
    rejected(&auth, &with_header(&token)).await;
    // A chave conhecida já está em cache.
    assert!(auth
        .authenticate(&with_header(&sign(&known, claims(&server))))
        .await
        .is_ok());
}

#[tokio::test]
async fn a_jwks_outage_refuses_without_hammering_the_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/cdn-cgi/access/certs"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let auth = authenticator(&server);
    let key = key("k1");
    let token = sign(&key, claims(&server));
    rejected(&auth, &with_header(&token)).await;
    rejected(&auth, &with_header(&token)).await;
}

#[test]
fn the_team_domain_builds_issuer_and_certs_url() {
    // Só constrói: a URL real é https e o teste não sai para a internet.
    let _ = AccessAuthenticator::new("time.cloudflareaccess.com", AUD, reqwest::Client::new());
}
