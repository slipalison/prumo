//! Preço ao vivo, de ponta a ponta: o streamer contra um servidor WebSocket
//! local que fala o protocolo do Yahoo, o hub gravando no Postgres e o stream
//! SSE da carteira.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use common::*;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use prumo::auth::{Authenticator, Identity};
use prumo::error::ApiError;
use prumo::live::yahoo::YahooStreamer;
use prumo::live::{LiveQuotes, Tick};
use prumo::quotes::Quote;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

// Mensagens reais do streamer do Yahoo, gravadas em 25/11/2026.
const PETR4: &str = r#"{"type":"pricing","message":"CghQRVRSNC5TQRWPwiRCGLCwy8bCaCoDU0FPMAg4AUWiBco/SLCF2jNlANcjP7AByAHYAQT1AgDXIz/9AqIFyj8="}"#;
const DOLLAR: &str = r#"{"type":"pricing","message":"CgVCUkw9WBXsUbpAGKCPucfCaCoDQ0NZMA44AUXbupm/ZYD/kL3YAQj1AoD/kL39Atu6mb8="}"#;

const WAIT: Duration = Duration::from_secs(5);

async fn within<T>(what: &str, future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(WAIT, future)
        .await
        .unwrap_or_else(|_| panic!("demorou demais: {what}"))
}

fn set(symbols: &[&str]) -> BTreeSet<String> {
    symbols.iter().map(|s| (*s).to_owned()).collect()
}

/// Um servidor WebSocket local. Cada conexão aceita vira um par de canais:
/// o que o cliente mandou em texto, e o que o teste quer mandar de volta
/// (`None` fecha a conexão).
struct FakeYahoo {
    url: String,
    connections: mpsc::Receiver<(mpsc::Receiver<Value>, mpsc::Sender<Option<String>>)>,
}

async fn fake_yahoo(answer_pings: bool) -> FakeYahoo {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (conn_tx, connections) = mpsc::channel(4);
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            let (received_tx, received_rx) = mpsc::channel(16);
            let (send_tx, mut send_rx) = mpsc::channel::<Option<String>>(16);
            if conn_tx.send((received_rx, send_tx)).await.is_err() {
                return;
            }
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        outgoing = send_rx.recv() => match outgoing {
                            Some(Some(text)) => ws.send(Message::text(text)).await.unwrap(),
                            _ => {
                                let _ = ws.close(None).await;
                                return;
                            }
                        },
                        incoming = ws.next(), if answer_pings => match incoming {
                            Some(Ok(Message::Text(text))) => {
                                let value: Value = serde_json::from_str(text.as_str()).unwrap();
                                let _ = received_tx.send(value).await;
                            }
                            Some(Ok(_)) => {}
                            _ => return,
                        },
                    }
                }
            });
        }
    });
    FakeYahoo { url, connections }
}

#[tokio::test]
async fn the_streamer_subscribes_relays_prices_and_reconnects() {
    let mut yahoo = fake_yahoo(true).await;
    let (symbols_tx, symbols_rx) = watch::channel(set(&["PETR4.SA", "USDBRL=X"]));
    let (ticks_tx, mut ticks) = mpsc::channel(16);
    let streamer = tokio::spawn(YahooStreamer::with_url(&yahoo.url).run(symbols_rx, ticks_tx));

    let (mut received, send) = within("primeira conexao", yahoo.connections.recv())
        .await
        .unwrap();
    // O dólar do `chart` (`USDBRL=X`) se chama `BRL=X` no streamer.
    assert_eq!(
        within("assinatura", received.recv()).await.unwrap(),
        json!({ "subscribe": ["BRL=X", "PETR4.SA"] })
    );

    send.send(Some(PETR4.to_owned())).await.unwrap();
    send.send(Some(DOLLAR.to_owned())).await.unwrap();
    send.send(Some(r#"{"type":"pricing","message":"lixo"}"#.to_owned()))
        .await
        .unwrap();
    let petr4 = within("preco da PETR4", ticks.recv()).await.unwrap();
    assert_eq!(
        (petr4.symbol.as_str(), petr4.price),
        ("PETR4.SA", dec!(41.19))
    );
    assert_eq!(petr4.traded_at.unix_timestamp(), 1_795_638_783);
    let dollar = within("preco do dolar", ticks.recv()).await.unwrap();
    assert_eq!(
        (dollar.symbol.as_str(), dollar.price),
        ("USDBRL=X", dec!(5.8225))
    );

    // A carteira mudou: sai o que não tem mais, entra o que é novo.
    symbols_tx.send_replace(set(&["USDBRL=X", "BTC-USD"]));
    assert_eq!(
        within("cancelamento", received.recv()).await.unwrap(),
        json!({ "unsubscribe": ["PETR4.SA"] })
    );
    assert_eq!(
        within("assinatura nova", received.recv()).await.unwrap(),
        json!({ "subscribe": ["BTC-USD"] })
    );

    // O servidor derruba a conexão: o streamer volta e assina tudo de novo.
    send.send(None).await.unwrap();
    let (mut received, _send) = within("reconexao", yahoo.connections.recv()).await.unwrap();
    assert_eq!(
        within("assinatura depois de reconectar", received.recv())
            .await
            .unwrap(),
        json!({ "subscribe": ["BRL=X", "BTC-USD"] })
    );

    // Sem quem mande símbolos, o streamer encerra.
    drop(symbols_tx);
    within("streamer encerrando", streamer).await.unwrap();
}

#[tokio::test]
async fn a_silent_server_is_dropped_and_reconnected() {
    // Este servidor não lê o socket: o `ping` do cliente fica sem `pong`.
    let mut yahoo = fake_yahoo(false).await;
    let (_symbols_tx, symbols_rx) = watch::channel(set(&["PETR4.SA"]));
    let (ticks_tx, _ticks) = mpsc::channel(16);
    tokio::spawn(
        YahooStreamer::with_url(&yahoo.url)
            .with_heartbeat(Duration::from_millis(50))
            .run(symbols_rx, ticks_tx),
    );
    let _first = within("primeira conexao", yahoo.connections.recv()).await;
    let _second = within("reconexao por silencio", yahoo.connections.recv()).await;
}

#[tokio::test]
async fn the_streamer_waits_for_something_to_subscribe() {
    let mut yahoo = fake_yahoo(true).await;
    let (symbols_tx, symbols_rx) = watch::channel(BTreeSet::new());
    let (ticks_tx, _ticks) = mpsc::channel(16);
    tokio::spawn(YahooStreamer::with_url(&yahoo.url).run(symbols_rx, ticks_tx));
    // Lista vazia: nenhuma conexão aberta.
    assert!(
        tokio::time::timeout(Duration::from_millis(200), yahoo.connections.recv())
            .await
            .is_err()
    );
    symbols_tx.send_replace(set(&["BTC-USD"]));
    let (mut received, _send) = within("conexao", yahoo.connections.recv()).await.unwrap();
    assert_eq!(
        within("assinatura", received.recv()).await.unwrap(),
        json!({ "subscribe": ["BTC-USD"] })
    );
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_795_600_000 + seconds).unwrap()
}

fn tick(symbol: &str, price: Decimal, seconds: i64) -> Tick {
    Tick {
        symbol: symbol.to_owned(),
        price,
        traded_at: at(seconds),
    }
}

fn quote(symbol: &str, price: Decimal, currency: &str, traded: Option<i64>) -> Quote {
    Quote {
        symbol: symbol.to_owned(),
        price,
        currency: currency.to_owned(),
        name: Some(format!("{symbol} nome")),
        traded_at: traded.map(at),
    }
}

#[tokio::test]
async fn the_newest_trade_wins_whichever_way_it_came() {
    let db = database().await;
    let store = &db.store;
    let now = OffsetDateTime::now_utc();
    store
        .upsert_quote(&quote("PETR4.SA", dec!(40), "BRL", Some(10)), now)
        .await
        .unwrap();

    // Do streamer: negócio mais novo muda o preço e diz quem mudou.
    let moved = store
        .apply_ticks(&[tick("PETR4.SA", dec!(41.19), 20)], now)
        .await
        .unwrap();
    assert_eq!(moved, set(&["PETR4.SA"]));
    let stored = &store.quotes(&["PETR4.SA".into()]).await.unwrap()["PETR4.SA"];
    assert_eq!(
        (stored.price, stored.traded_at),
        (dec!(41.19), Some(at(20)))
    );

    // Negócio mais velho, chegado depois: ignorado.
    let moved = store
        .apply_ticks(&[tick("PETR4.SA", dec!(1), 15)], now)
        .await
        .unwrap();
    assert!(moved.is_empty());
    // Mesmo preço, negócio mais novo: grava a hora, mas não é mudança de preço.
    let moved = store
        .apply_ticks(&[tick("PETR4.SA", dec!(41.19), 30)], now)
        .await
        .unwrap();
    assert!(moved.is_empty());
    // Símbolo sem cotação do `chart` (sem moeda): o streamer não cria.
    let moved = store
        .apply_ticks(&[tick("NADA3.SA", dec!(9), 30)], now)
        .await
        .unwrap();
    assert!(moved.is_empty());
    assert!(store.quotes(&["NADA3.SA".into()]).await.unwrap().is_empty());
    assert!(store.apply_ticks(&[], now).await.unwrap().is_empty());

    let stored = &store.quotes(&["PETR4.SA".into()]).await.unwrap()["PETR4.SA"];
    assert_eq!(
        (stored.price, stored.traded_at),
        (dec!(41.19), Some(at(30)))
    );

    // Do `chart`, com negócio mais velho que o do streamer: o preço fica, a
    // moeda e o nome atualizam.
    let mut older = quote("PETR4.SA", dec!(39), "BRL", Some(25));
    older.name = Some("Petrobras PN".into());
    store.upsert_quote(&older, now).await.unwrap();
    let stored = &store.quotes(&["PETR4.SA".into()]).await.unwrap()["PETR4.SA"];
    assert_eq!(
        (stored.price, stored.traded_at, stored.name.as_deref()),
        (dec!(41.19), Some(at(30)), Some("Petrobras PN"))
    );
    // Do `chart`, mais novo (o fechamento, por exemplo): vence.
    store
        .upsert_quote(&quote("PETR4.SA", dec!(41.5), "BRL", Some(40)), now)
        .await
        .unwrap();
    // Sem hora do negócio, o `chart` vence como sempre venceu.
    store
        .upsert_quote(&quote("KNRI11.SA", dec!(140), "BRL", Some(50)), now)
        .await
        .unwrap();
    store
        .upsert_quote(&quote("KNRI11.SA", dec!(142), "BRL", None), now)
        .await
        .unwrap();
    let stored = store
        .quotes(&["PETR4.SA".into(), "KNRI11.SA".into()])
        .await
        .unwrap();
    assert_eq!(stored["PETR4.SA"].price, dec!(41.5));
    assert_eq!(
        (stored["KNRI11.SA"].price, stored["KNRI11.SA"].traded_at),
        (dec!(142), None)
    );
}

#[tokio::test]
async fn the_hub_batches_ticks_and_follows_the_assets() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let app = app(
        &db.store,
        "hub@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let live = Arc::clone(app.state.quotes.live());
    let mut symbols = live.symbols();
    let mut changes = live.subscribe();

    // Ativo cotado na criação entra na assinatura, com o câmbio da moeda dele.
    for body in [
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 1 }),
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 1 }),
    ] {
        assert_eq!(
            post(&app.router, "/api/v1/assets", body).await.status,
            StatusCode::CREATED
        );
    }
    within(
        "assinatura com os ativos",
        symbols.wait_for(|s| *s == set(&["AAPL", "KNRI11.SA", "USDBRL=X"])),
    )
    .await
    .unwrap();

    // Vários preços do mesmo símbolo no mesmo lote: um aviso, o último preço.
    let ticks = live.ticks();
    for (price, second) in [(dec!(143), 1), (dec!(144), 2), (dec!(145), 3)] {
        ticks
            .send(Tick {
                symbol: "KNRI11.SA".into(),
                price,
                traded_at: OffsetDateTime::now_utc() + time::Duration::seconds(second),
            })
            .await
            .unwrap();
    }
    loop {
        let changed = within("aviso do lote", changes.recv()).await.unwrap();
        // O `chart` da criação também avisa; o que interessa é o do lote.
        let stored = db.store.quotes(&["KNRI11.SA".into()]).await.unwrap();
        if changed.contains("KNRI11.SA") && stored["KNRI11.SA"].price == dec!(145) {
            break;
        }
    }

    // Ativo apagado sai da assinatura na hora.
    let portfolio = get(&app.router, "/api/v1/portfolio").await.body;
    let aapl = find(&portfolio["assets"], "symbol", "AAPL")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        delete(&app.router, &format!("/api/v1/assets/{aapl}"))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    within(
        "assinatura sem o ativo apagado",
        symbols.wait_for(|s| *s == set(&["KNRI11.SA"])),
    )
    .await
    .unwrap();
}

/// Lê eventos de uma resposta SSE.
struct Events {
    body: Body,
    buffer: String,
}

#[derive(Debug, Default)]
struct Event {
    name: String,
    data: String,
    retry: Option<u64>,
}

impl Events {
    fn new(body: Body) -> Self {
        Self {
            body,
            buffer: String::new(),
        }
    }

    async fn next(&mut self) -> Option<Event> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..end + 2).collect();
                let mut event = Event::default();
                for line in block.lines() {
                    if let Some(value) = line.strip_prefix("event:") {
                        event.name = value.trim_start().to_owned();
                    } else if let Some(value) = line.strip_prefix("data:") {
                        event.data.push_str(value.trim_start());
                    } else if let Some(value) = line.strip_prefix("retry:") {
                        event.retry = value.trim().parse().ok();
                    }
                }
                if event.name.is_empty() && event.data.is_empty() {
                    continue;
                }
                return Some(event);
            }
            let frame = self.body.frame().await?.ok()?;
            if let Ok(data) = frame.into_data() {
                self.buffer.push_str(&String::from_utf8_lossy(&data));
            }
        }
    }

    async fn prices(&mut self) -> Value {
        let event = within("evento prices", self.next())
            .await
            .expect("o stream acabou antes do evento");
        assert_eq!(event.name, "prices");
        serde_json::from_str(&event.data).unwrap()
    }

    async fn ends(&mut self) {
        let end = within("fim do stream", async {
            while self.next().await.is_some() {}
        });
        end.await;
    }
}

async fn open_stream(router: &axum::Router) -> (StatusCode, HeaderMap, Events) {
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/v1/portfolio/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    (status, headers, Events::new(response.into_body()))
}

#[tokio::test]
async fn the_portfolio_streams_new_prices_as_they_arrive() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let quotes = FakeQuotes::with_market();
    let app = app(
        &db.store,
        "aovivo@prumo.test",
        Arc::clone(&quotes),
        &dir.path(),
    );
    for body in [
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 2 }),
    ] {
        post(&app.router, "/api/v1/assets", body).await;
    }

    let (status, headers, mut events) = open_stream(&app.router).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "text/event-stream");
    assert_eq!(headers["x-accel-buffering"], "no");
    assert_eq!(headers["cache-control"], "no-store");

    // Primeiro evento na hora, com o pedido de reconexão em 3 s.
    let first = within("primeiro evento", events.next()).await.unwrap();
    assert_eq!(first.retry, Some(3000));
    let prices: Value = serde_json::from_str(&first.data).unwrap();
    // 10 × 142 + 2 × 200 × 5,50
    assert_eq!(prices["total_value"], 3620.0);
    let asset = |prices: &Value, value: f64| {
        prices["assets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["current_value"] == value)
    };
    assert!(asset(&prices, 1420.0) && asset(&prices, 2200.0));
    assert!(prices.get("checklists").is_none(), "o evento leva so preco");

    // Preço do streamer: a carteira chega recalculada.
    let ticks = app.state.quotes.live().ticks();
    let soon = || OffsetDateTime::now_utc() + time::Duration::seconds(5);
    ticks
        .send(Tick {
            symbol: "KNRI11.SA".into(),
            price: dec!(150),
            traded_at: soon(),
        })
        .await
        .unwrap();
    let prices = events.prices().await;
    assert_eq!(prices["total_value"], 3700.0);
    let knri = prices["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["unit_price"] == 150.0)
        .expect("KNRI11 com o preco novo");
    assert_eq!(knri["current_value"], 1500.0);
    assert_eq!(knri["price_source"], "quote");
    let br_reits = find(&prices["classes"], "asset_class", "br_reits");
    assert_eq!(br_reits["current_value"], 1500.0);

    // O câmbio também é preço: o dólar sobe, a ação americana sobe junto.
    ticks
        .send(Tick {
            symbol: "USDBRL=X".into(),
            price: dec!(6),
            traded_at: soon(),
        })
        .await
        .unwrap();
    assert_eq!(events.prices().await["total_value"], 3900.0);

    // A cotação do `chart` (o botão de atualizar) avisa pelo mesmo canal.
    *quotes.traded_at.lock().unwrap() =
        Some(OffsetDateTime::now_utc() + time::Duration::minutes(1));
    quotes
        .prices
        .lock()
        .unwrap()
        .insert("AAPL".into(), (dec!(210), "USD", "Apple Inc."));
    let refreshed = post(&app.router, "/api/v1/quotes/refresh", json!({})).await;
    assert_eq!(refreshed.status, StatusCode::OK);
    // O `chart` cota tudo com negócio mais novo que o do streamer, e vence:
    // 10 × 142 + 2 × 210 × 5,50 (o câmbio do `chart` falso é 5,50).
    assert_eq!(events.prices().await["total_value"], 3730.0);

    // O processo começa a encerrar: o stream termina e não segura o desligamento.
    app.state.stop();
    events.ends().await;
}

/// Credencial que vence logo, como um token do Access no fim da sessão.
struct ShortLived {
    lasts: time::Duration,
    refuse: bool,
}

#[async_trait]
impl Authenticator for ShortLived {
    async fn authenticate(&self, _headers: &HeaderMap) -> Result<Identity, ApiError> {
        if self.refuse {
            return Err(ApiError::unauthenticated("Sem credencial do Access."));
        }
        Ok(Identity {
            email: "curta@prumo.test".into(),
            expires_at: Some(OffsetDateTime::now_utc() + self.lasts),
        })
    }
}

#[tokio::test]
async fn the_stream_ends_when_the_credential_expires() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let auth = Arc::new(ShortLived {
        lasts: time::Duration::milliseconds(300),
        refuse: false,
    });
    let app = app_with_auth(&db.store, auth, FakeQuotes::with_market(), &dir.path());
    let (status, _, mut events) = open_stream(&app.router).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events.prices().await["total_value"], 0.0);
    // Vencida a credencial, o servidor fecha; o navegador reconecta pelo Access.
    events.ends().await;

    let refused = app_with_auth(
        &db.store,
        Arc::new(ShortLived {
            lasts: time::Duration::hours(1),
            refuse: true,
        }),
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let reply = get(&refused.router, "/api/v1/portfolio/stream").await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.body["code"], "unauthenticated");
}

#[tokio::test]
async fn a_live_hub_ignores_a_second_start() {
    let db = database().await;
    let live = LiveQuotes::new(db.store.clone());
    live.spawn(TEST_FLUSH, Duration::from_millis(50));
    // A segunda chamada só avisa no log: não sobe outro gravador.
    live.spawn(TEST_FLUSH, Duration::from_millis(50));
    let mut changes = live.subscribe();
    live.announce(BTreeSet::new());
    live.announce(set(&["KNRI11.SA"]));
    assert_eq!(
        *within("aviso", changes.recv()).await.unwrap(),
        set(&["KNRI11.SA"])
    );
}
