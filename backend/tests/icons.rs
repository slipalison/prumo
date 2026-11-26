//! Logos dos ativos contra um Postgres de verdade: o serviço (uma busca por
//! símbolo, revalidação lenta, falha que não apaga o logo, parada no limite), a
//! reserva entre réplicas, o que a carteira manda buscar e a rota que serve o
//! logo do domínio do Prumo. A brapi é um falso em memória (o cliente real tem
//! o próprio teste em `tests/brapi_logos.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use common::*;
use http_body_util::BodyExt;
use prumo::auth::{Authenticator, Identity};
use prumo::brapi::{FetchedLogo, LogoError, LogoSource};
use prumo::domain::icon::sanitize_svg;
use prumo::error::ApiError;
use prumo::icons::{IconService, CLAIM_TTL, LOCK_KEY, RETRY_FAILED_AFTER, REVALIDATE_AFTER};
use prumo::store::icons::IconOutcome;
use prumo::store::Store;
use serde_json::json;
use time::macros::datetime;
use time::OffsetDateTime;
use tower::ServiceExt;

const LOGO: &[u8] = include_bytes!("fixtures/logo_exemplo.svg");
const EVIL: &[u8] = include_bytes!("fixtures/logo_malicioso.svg");
const T0: OffsetDateTime = datetime!(2026-11-26 12:00 UTC);

/// A brapi em memória: cada símbolo tem uma resposta fixa (padrão: sem logo) e
/// toda chamada fica registrada.
#[derive(Default)]
struct FakeLogos {
    replies: Mutex<HashMap<String, Result<FetchedLogo, LogoError>>>,
    calls: Mutex<Vec<String>>,
    /// Demora de cada chamada, para duas réplicas se atropelarem.
    delay: Duration,
}

impl FakeLogos {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn slow(delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            delay,
            ..Self::default()
        })
    }

    fn logo(&self, symbol: &str, bytes: &[u8]) {
        self.set(
            symbol,
            Ok(FetchedLogo {
                url: format!("https://icons.brapi.dev/icons/{symbol}.svg"),
                bytes: bytes.to_vec(),
            }),
        );
    }

    fn set(&self, symbol: &str, reply: Result<FetchedLogo, LogoError>) {
        self.replies
            .lock()
            .unwrap()
            .insert(symbol.to_owned(), reply);
    }

    fn calls(&self) -> Vec<String> {
        let mut calls = self.calls.lock().unwrap().clone();
        calls.sort();
        calls
    }

    fn calls_to(&self, symbol: &str) -> usize {
        self.calls().iter().filter(|s| *s == symbol).count()
    }
}

#[async_trait]
impl LogoSource for FakeLogos {
    async fn fetch(&self, symbol: &str) -> Result<FetchedLogo, LogoError> {
        self.calls.lock().unwrap().push(symbol.to_owned());
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        self.replies
            .lock()
            .unwrap()
            .get(symbol)
            .cloned()
            .unwrap_or(Err(LogoError::NotFound))
    }
}

fn service(store: &Store, source: &Arc<FakeLogos>) -> Arc<IconService> {
    Arc::new(IconService::new(
        store.clone(),
        Some(Arc::clone(source) as Arc<dyn LogoSource>),
    ))
}

fn names(symbols: &[&str]) -> Vec<String> {
    symbols.iter().map(|s| (*s).to_owned()).collect()
}

fn clean(svg: &[u8]) -> Vec<u8> {
    sanitize_svg(svg).unwrap()
}

// ---------------------------------------------------------------- serviço --

#[tokio::test]
async fn each_symbol_is_fetched_once_and_revalidated_slowly() {
    let db = database().await;
    let source = FakeLogos::new();
    source.logo("ITSA4", LOGO);
    source.logo("EVIL3", EVIL);
    source.set("DOWN3", Err(LogoError::Unavailable("fora do ar".into())));
    let svc = service(&db.store, &source);
    let all = names(&["ITSA4", "EVIL3", "NONE3", "DOWN3"]);

    let first = svc.fill(&all, T0).await;
    assert_eq!(
        (first.claimed, first.found, first.missing, first.failed),
        (4, 1, 2, 1)
    );
    assert_eq!(first.stopped, None);
    assert_eq!(source.calls().len(), 4);

    // O logo bom é guardado já saneado, com o tipo e a versão.
    let stored = db.store.icon("ITSA4").await.unwrap().unwrap();
    assert_eq!(stored.content_type, "image/svg+xml");
    assert_eq!(stored.bytes, clean(LOGO));
    assert_eq!(stored.etag.len(), 32);
    // O malicioso nunca é guardado, e o ativo fica "sem logo".
    assert!(db.store.icon("EVIL3").await.unwrap().is_none());
    assert_eq!(
        db.store.icon_state("EVIL3").await.unwrap().unwrap().outcome,
        IconOutcome::Missing
    );
    assert_eq!(
        db.store.icon_state("NONE3").await.unwrap().unwrap().outcome,
        IconOutcome::Missing
    );
    assert_eq!(
        db.store.icon_state("DOWN3").await.unwrap().unwrap().outcome,
        IconOutcome::Failed
    );

    // Uma hora depois, nada vai à rede: o achado e o "sem logo" esperam o mês e
    // a falha espera algumas horas.
    let again = svc.fill(&all, T0 + time::Duration::hours(1)).await;
    assert_eq!(again.claimed, 0);
    assert_eq!(source.calls().len(), 4);

    // Passadas as horas de descanso, só o que falhou volta.
    source.logo("DOWN3", LOGO);
    let retry = svc
        .fill(&all, T0 + RETRY_FAILED_AFTER + time::Duration::minutes(1))
        .await;
    assert_eq!((retry.claimed, retry.found), (1, 1));
    assert_eq!(source.calls_to("DOWN3"), 2);
    assert_eq!(source.calls_to("ITSA4"), 1);

    // Passado o mês, tudo é conferido de novo.
    let month = T0 + REVALIDATE_AFTER + time::Duration::days(1);
    source.set("ITSA4", Err(LogoError::Unavailable("fora do ar".into())));
    let revalidate = svc.fill(&all, month).await;
    assert_eq!(revalidate.claimed, 4);
    // A revalidação que falhou não apagou o logo que já havia.
    let kept = db.store.icon("ITSA4").await.unwrap().unwrap();
    assert_eq!(kept, stored);
    assert_eq!(
        db.store.icon_state("ITSA4").await.unwrap().unwrap().outcome,
        IconOutcome::Failed
    );

    // E, de volta, o logo novo troca o velho (a versão muda).
    let redesigned = String::from_utf8(LOGO.to_vec())
        .unwrap()
        .replace("#0B5FA5", "#0B5FA6");
    source.logo("ITSA4", redesigned.as_bytes());
    svc.fill(
        &all,
        month + RETRY_FAILED_AFTER + time::Duration::minutes(1),
    )
    .await;
    let renewed = db.store.icon("ITSA4").await.unwrap().unwrap();
    assert_ne!(renewed.etag, stored.etag);
    assert_eq!(
        db.store.icon_state("ITSA4").await.unwrap().unwrap().outcome,
        IconOutcome::Found
    );

    // Quando a fonte passa a dizer que não há logo, o antigo sai.
    source.set("ITSA4", Err(LogoError::NotFound));
    svc.fill(&all, month + REVALIDATE_AFTER + time::Duration::days(2))
        .await;
    assert!(db.store.icon("ITSA4").await.unwrap().is_none());
}

#[tokio::test]
async fn the_provider_limits_stop_the_round_and_pause_the_task() {
    let db = database().await;
    let source = FakeLogos::new();
    for symbol in ["AAAA3", "CCCC3", "DDDD3"] {
        source.logo(symbol, LOGO);
    }
    source.set(
        "BBBB3",
        Err(LogoError::RateLimited(Duration::from_secs(120))),
    );
    let svc = service(&db.store, &source);

    // 429 no segundo: o primeiro fica, o segundo falha, o resto volta a ficar livre.
    let round = svc
        .fill(&names(&["AAAA3", "BBBB3", "CCCC3", "DDDD3"]), T0)
        .await;
    assert_eq!((round.claimed, round.found, round.failed), (4, 1, 1));
    assert!(round.stopped.is_some());
    assert_eq!(source.calls(), names(&["AAAA3", "BBBB3"]));
    for released in ["CCCC3", "DDDD3"] {
        let state = db.store.icon_state(released).await.unwrap().unwrap();
        assert_eq!(state.outcome, IconOutcome::Pending);
        assert_eq!(state.claimed_at, None, "{released}");
    }

    // Dentro do tempo que a brapi pediu, nem tenta.
    let paused = svc
        .fill(&names(&["CCCC3"]), T0 + time::Duration::seconds(60))
        .await;
    assert_eq!(paused.claimed, 0);
    assert_eq!(source.calls().len(), 2);

    // Passado o tempo, segue de onde parou.
    let resumed = svc
        .fill(
            &names(&["CCCC3", "DDDD3"]),
            T0 + time::Duration::seconds(121),
        )
        .await;
    assert_eq!((resumed.claimed, resumed.found), (2, 2));
}

#[tokio::test]
async fn a_refused_token_pauses_for_hours_and_a_dead_provider_ends_the_round() {
    let db = database().await;
    let source = FakeLogos::new();
    source.set("AAAA3", Err(LogoError::Unauthorized));
    let svc = service(&db.store, &source);
    let round = svc.fill(&names(&["AAAA3", "BBBB3"]), T0).await;
    assert_eq!(round.failed, 1);
    assert!(round.stopped.unwrap().contains("token"));
    assert_eq!(source.calls().len(), 1);
    let hour_later = svc
        .fill(&names(&["BBBB3"]), T0 + time::Duration::hours(1))
        .await;
    assert_eq!(hour_later.claimed, 0);
    assert_eq!(source.calls().len(), 1);

    // O plano sem o recurso (403) é igual.
    let forbidden = FakeLogos::new();
    forbidden.set("AAAA3", Err(LogoError::Forbidden));
    let round = service(&db.store, &forbidden)
        .fill(&names(&["AAAA3"]), T0 + time::Duration::days(3))
        .await;
    assert!(round.stopped.is_some());

    // Três falhas de rede seguidas: a brapi está fora, a rodada para.
    let db2 = database().await;
    let down = FakeLogos::new();
    for symbol in ["A1", "A2", "A3", "A4", "A5"] {
        down.set(symbol, Err(LogoError::Unavailable("timeout".into())));
    }
    let round = service(&db2.store, &down)
        .fill(&names(&["A1", "A2", "A3", "A4", "A5"]), T0)
        .await;
    assert_eq!((round.failed, down.calls().len()), (3, 3));
    assert!(round.stopped.is_some());
    // Os dois que sobraram não ficam presos: a próxima rodada os pega.
    assert_eq!(
        db2.store
            .icon_state("A5")
            .await
            .unwrap()
            .unwrap()
            .claimed_at,
        None
    );
}

#[tokio::test]
async fn without_a_source_nothing_is_fetched_and_the_cache_still_serves() {
    let db = database().await;
    db.store
        .save_icon(
            "ITSA4",
            &clean(LOGO),
            "https://icons.brapi.dev/icons/ITSA4.svg",
            T0,
        )
        .await
        .unwrap();
    let svc = IconService::disabled(db.store.clone());
    assert!(!svc.enabled());
    let round = svc.fill(&names(&["ITSA4", "KNRI11"]), T0).await;
    assert_eq!(round.claimed, 0);
    assert!(db.store.icon_state("KNRI11").await.unwrap().is_none());
    assert!(db.store.icon("ITSA4").await.unwrap().is_some());
}

// ---------------------------------------------------------------- réplicas --

#[tokio::test]
async fn two_replicas_never_fetch_the_same_symbol() {
    let db = database().await;
    let source = FakeLogos::slow(Duration::from_millis(120));
    let symbols = names(&["AAAA3", "BBBB3", "CCCC3", "DDDD3", "EEEE3", "FFFF3"]);
    for symbol in &symbols {
        source.logo(symbol, LOGO);
    }
    // Duas réplicas = dois serviços com a mesma fonte e o mesmo banco.
    let (a, b) = (service(&db.store, &source), service(&db.store, &source));
    let (ra, rb) = tokio::join!(a.fill(&symbols, T0), b.fill(&symbols, T0));
    for symbol in &symbols {
        assert_eq!(source.calls_to(symbol), 1, "{symbol}");
        assert!(db.store.icon(symbol).await.unwrap().is_some());
    }
    assert_eq!(ra.found + rb.found, symbols.len());

    // Rodar de novo (qualquer réplica) não busca nada.
    let again = tokio::join!(a.fill(&symbols, T0), b.fill(&symbols, T0));
    assert_eq!(again.0.claimed + again.1.claimed, 0);
    assert_eq!(source.calls().len(), symbols.len());
}

#[tokio::test]
async fn a_claim_from_a_crashed_replica_expires() {
    let db = database().await;
    let source = FakeLogos::new();
    source.logo("AAAA3", LOGO);
    let svc = service(&db.store, &source);
    // A outra réplica reservou e caiu antes de gravar.
    let claimed = db
        .store
        .claim_icons(&names(&["AAAA3"]), LOCK_KEY, T0, |_| true)
        .await
        .unwrap();
    assert_eq!(claimed, names(&["AAAA3"]));

    let early = svc
        .fill(
            &names(&["AAAA3"]),
            T0 + CLAIM_TTL - time::Duration::minutes(1),
        )
        .await;
    assert_eq!(early.claimed, 0);
    let late = svc
        .fill(
            &names(&["AAAA3"]),
            T0 + CLAIM_TTL + time::Duration::minutes(1),
        )
        .await;
    assert_eq!((late.claimed, late.found), (1, 1));
}

#[tokio::test]
async fn a_held_lock_skips_the_round_without_holding_a_connection() {
    let db = database().await;
    let source = FakeLogos::new();
    source.logo("AAAA3", LOGO);
    let svc = service(&db.store, &source);
    // Outra transação segura o lock da reserva.
    let pool = sqlx::PgPool::connect(&db.url).await.unwrap();
    let mut holder = pool.begin().await.unwrap();
    let (held,): (bool,) = sqlx::query_as("select pg_try_advisory_xact_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&mut *holder)
        .await
        .unwrap();
    assert!(held);
    let skipped = svc.fill(&names(&["AAAA3"]), T0).await;
    assert_eq!(skipped.claimed, 0);
    assert!(source.calls().is_empty());
    holder.rollback().await.unwrap();

    // Solto o lock, a rodada segue, e o lock da reserva não ficou preso.
    let round = svc.fill(&names(&["AAAA3"]), T0).await;
    assert_eq!(round.found, 1);
    let (free,): (bool,) = sqlx::query_as("select pg_try_advisory_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(free);
}

// ----------------------------------------------- carteira, rota e cabeçalhos --

struct Refuse;

#[async_trait]
impl Authenticator for Refuse {
    async fn authenticate(&self, _headers: &HeaderMap) -> Result<Identity, ApiError> {
        Err(ApiError::unauthenticated("sem credencial"))
    }
}

struct Raw {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

async fn fetch(router: &axum::Router, uri: &str, extra: &[(&str, &str)]) -> Raw {
    let mut request = Request::builder().method(Method::GET).uri(uri);
    for (name, value) in extra {
        request = request.header(*name, *value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    Raw {
        status: response.status(),
        headers: response.headers().clone(),
        body: response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    }
}

/// Depois do `settle`: a busca disparada pelo cadastro já terminou, sem prazo
/// de relógio (a esteira, com cobertura e vários Postgres, é lenta).
async fn assert_icon(store: &Store, symbol: &str) {
    assert!(
        store.icon(symbol).await.unwrap().is_some(),
        "o logo de {symbol} nao chegou"
    );
}

#[tokio::test]
async fn the_portfolio_decides_what_is_fetched_and_new_assets_fetch_their_own() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let source = FakeLogos::new();
    for symbol in ["WEGE3", "KNRI11", "QBTC11", "MXRF11", "BTC", "AAPL"] {
        source.logo(symbol, LOGO);
    }
    let svc = service(&db.store, &source);
    let base = app(
        &db.store,
        "logos@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let router = prumo::http::public_router(
        base.state.clone().with_icon_service(Arc::clone(&svc)),
        &dir.path(),
    );

    // Ação e FII da B3, cripto de verdade e ação dos EUA.
    for body in [
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 10 }),
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
        json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": "0.5" }),
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 3 }),
        // ETF de bitcoin: cripto na carteira, mas negociado na B3.
        json!({ "asset_class": "crypto", "symbol": "QBTC11", "quantity": 4, "listed_on_b3": true }),
    ] {
        let reply = post(&router, "/api/v1/assets", body.clone()).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{body} {}", reply.text);
    }

    // Cada ativo novo da B3 busca o próprio logo; os outros não vão à brapi.
    svc.settle().await;
    for symbol in ["WEGE3", "KNRI11", "QBTC11"] {
        assert_icon(&db.store, symbol).await;
    }
    assert_eq!(source.calls(), names(&["KNRI11", "QBTC11", "WEGE3"]));

    // A rodada periódica não repete o que já está guardado, e pega o que faltar.
    let round = svc.refresh_all(OffsetDateTime::now_utc()).await;
    assert_eq!(round.claimed, 0);
    sqlx::query("delete from asset_icons where symbol = 'KNRI11'")
        .execute(&sqlx::PgPool::connect(&db.url).await.unwrap())
        .await
        .unwrap();
    let round = svc.refresh_all(OffsetDateTime::now_utc()).await;
    assert_eq!((round.claimed, round.found), (1, 1));
    assert_eq!(source.calls_to("KNRI11"), 2);

    // ETF de renda fixa negociado na B3: também é da B3.
    let moved = post(
        &router,
        "/api/v1/assets",
        json!({ "asset_class": "fixed_income", "symbol": "MXRF11", "quantity": 1, "listed_on_b3": true }),
    )
    .await;
    assert_eq!(moved.status, StatusCode::CREATED, "{}", moved.text);
    svc.settle().await;
    assert_icon(&db.store, "MXRF11").await;
}

#[tokio::test]
async fn the_icon_route_serves_the_cache_with_hostile_content_headers() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let source = FakeLogos::new();
    source.logo("ITSA4", LOGO);
    source.logo("EVIL3", EVIL);
    service(&db.store, &source)
        .fill(&names(&["ITSA4", "EVIL3", "NONE3"]), T0)
        .await;
    let router = app(
        &db.store,
        "rota@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    )
    .router;

    // 200 com o SVG saneado e todos os cabeçalhos de defesa.
    let ok = fetch(&router, "/api/v1/icons/ITSA4", &[]).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body, clean(LOGO));
    let header_of = |raw: &Raw, name: &str| {
        raw.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default()
    };
    assert_eq!(header_of(&ok, "content-type"), "image/svg+xml");
    assert_eq!(
        header_of(&ok, "content-security-policy"),
        "default-src 'none'; style-src 'unsafe-inline'; sandbox"
    );
    assert_eq!(header_of(&ok, "x-content-type-options"), "nosniff");
    assert_eq!(header_of(&ok, "content-disposition"), "inline");
    assert_eq!(
        header_of(&ok, "cross-origin-resource-policy"),
        "same-origin"
    );
    assert_eq!(header_of(&ok, "cache-control"), "private, max-age=86400");
    let etag = header_of(&ok, "etag");
    assert!(
        etag.starts_with('"') && etag.ends_with('"') && etag.len() == 34,
        "{etag}"
    );

    // Letra minúscula vale o mesmo símbolo.
    assert_eq!(
        fetch(&router, "/api/v1/icons/itsa4", &[]).await.body,
        ok.body
    );

    // Validador: o mesmo ETag volta 304 sem corpo e com os mesmos cabeçalhos.
    let cached = fetch(&router, "/api/v1/icons/ITSA4", &[("if-none-match", &etag)]).await;
    assert_eq!(cached.status, StatusCode::NOT_MODIFIED);
    assert!(cached.body.is_empty());
    assert_eq!(header_of(&cached, "etag"), etag);
    assert_eq!(
        header_of(&cached, "cache-control"),
        "private, max-age=86400"
    );
    let stale = fetch(
        &router,
        "/api/v1/icons/ITSA4",
        &[("if-none-match", "\"outra\"")],
    )
    .await;
    assert_eq!(stale.status, StatusCode::OK);

    // O que não tem logo, o que foi recusado e o que não é símbolo: 404.
    for uri in [
        "/api/v1/icons/NONE3",
        "/api/v1/icons/EVIL3",
        "/api/v1/icons/ZZZZ9",
        "/api/v1/icons/TD-IPCA-20350515",
        "/api/v1/icons/..%2F..%2Fetc%2Fpasswd",
        "/api/v1/icons/ABCDEFGHIJKLM",
        "/api/v1/icons/%E3%81%82",
    ] {
        let missing = fetch(&router, uri, &[]).await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(header_of(&missing, "cache-control"), "no-store", "{uri}");
        assert!(
            header_of(&missing, "content-type").contains("json"),
            "{uri}: {}",
            header_of(&missing, "content-type")
        );
    }

    // O malicioso não chegou nem ao banco: nada dele pode ser servido.
    for hostile in [
        "onload",
        "<script",
        "foreignObject",
        "javascript:",
        "exemplo.test",
    ] {
        assert!(!String::from_utf8_lossy(&ok.body).contains(hostile));
    }

    // O resto da API continua sem guardar nada.
    let me = fetch(&router, "/api/v1/me", &[]).await;
    assert_eq!(header_of(&me, "cache-control"), "no-store");
    // E a CSP da página não mudou.
    let page = fetch(&router, "/", &[]).await;
    assert!(header_of(&page, "content-security-policy").contains("img-src 'self' data:"));

    // A lista traz só os símbolos com logo, com a versão do ETag.
    let index = get(&router, "/api/v1/icons").await;
    assert_eq!(index.status, StatusCode::OK);
    assert_eq!(
        index.body,
        json!({ "icons": [{ "symbol": "ITSA4", "version": etag.trim_matches('"') }] })
    );
    assert_eq!(
        header_of(&fetch(&router, "/api/v1/icons", &[]).await, "cache-control"),
        "no-store"
    );
}

#[tokio::test]
async fn the_icon_routes_need_a_signed_in_person() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    db.store
        .save_icon(
            "ITSA4",
            &clean(LOGO),
            "https://icons.brapi.dev/icons/ITSA4.svg",
            T0,
        )
        .await
        .unwrap();
    let refused = app_with_auth(
        &db.store,
        Arc::new(Refuse),
        FakeQuotes::with_market(),
        &dir.path(),
    );
    for uri in ["/api/v1/icons", "/api/v1/icons/ITSA4"] {
        let reply = fetch(&refused.router, uri, &[]).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert!(!String::from_utf8_lossy(&reply.body).contains("<svg"));
    }
}
