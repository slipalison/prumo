//! O streamer de preços do Yahoo Finance.
//!
//! É o WebSocket que a própria página do Yahoo usa
//! (`streamer.finance.yahoo.com`, `version=2`): o cliente manda
//! `{"subscribe": [...]}` e recebe `{"type": "pricing", "message": "<base64>"}`,
//! em que a mensagem é um protobuf `PricingData`. Medido em 25/11/2026:
//!
//! - B3: um preço por segundo, com **15 min de atraso** — o dado gratuito da B3
//!   sai atrasado; em tempo real só com licença de distribuidor.
//! - Ações dos EUA e câmbio: ~1–2 s de atraso. Cripto: a cada ~5 s.
//! - O dólar só sai como `BRL=X`. `USDBRL=X`, o símbolo do `chart`, não
//!   recebe nada pelo streamer.
//!
//! O streamer não diz moeda nem nome das ações: ele só atualiza o preço de um
//! símbolo que o `chart` já cotou (moeda e nome vêm de lá).

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::time::Duration;

use base64::Engine;
use futures_util::{Sink, SinkExt, StreamExt};
use prost::Message as _;
use rust_decimal::Decimal;
use serde::Deserialize;
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::{self, Bytes, Message};

use super::Tick;

pub const YAHOO_STREAMER: &str = "wss://streamer.finance.yahoo.com/?version=2";

/// Espera antes de reconectar: dobra a cada queda, até o teto.
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Uma conexão que durou isto volta a reconectar rápido na próxima queda.
const STABLE: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Os campos de `PricingData` que o Prumo usa. A mensagem tem mais de 30; o
/// protobuf pula os que não estão aqui.
#[derive(Clone, PartialEq, prost::Message)]
struct PricingData {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(float, tag = "2")]
    price: f32,
    /// Hora do negócio, em milissegundos desde 1970.
    #[prost(sint64, tag = "3")]
    time: i64,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

/// O nome com que o streamer conhece um símbolo do `chart`.
pub fn stream_name(provider_symbol: &str) -> &str {
    match provider_symbol {
        "USDBRL=X" => "BRL=X",
        other => other,
    }
}

/// Uma mensagem de texto do streamer → `(nome no streamer, preço, hora)`.
fn decode(text: &str) -> Option<(String, Decimal, OffsetDateTime)> {
    let envelope: Envelope = serde_json::from_str(text).ok()?;
    if envelope.kind != "pricing" {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(envelope.message)
        .ok()?;
    let data = PricingData::decode(bytes.as_slice()).ok()?;
    if data.id.is_empty() || data.time <= 0 {
        return None;
    }
    let price = price_from(data.price)?;
    let traded_at =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(data.time) * 1_000_000).ok()?;
    Some((data.id, price, traded_at))
}

/// `f32` → o decimal mais curto que volta ao mesmo `f32`: `41.26`, e não
/// `41.2599983215332`. É o que o `Display` do Rust escreve.
fn price_from(value: f32) -> Option<Decimal> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Decimal::from_str(&value.to_string())
        .ok()
        .map(|d| d.round_dp(8))
        .filter(|d| *d > Decimal::ZERO)
}

/// O que mandar para sair de `subscribed` e chegar em `wanted`:
/// `(cancelar, assinar)`, pelos nomes do streamer.
fn diff(
    subscribed: &BTreeSet<String>,
    wanted: &BTreeMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let gone = subscribed
        .iter()
        .filter(|name| !wanted.contains_key(*name))
        .cloned()
        .collect();
    let new = wanted
        .keys()
        .filter(|name| !subscribed.contains(*name))
        .cloned()
        .collect();
    (gone, new)
}

enum SessionEnd {
    /// Quem ouvia foi embora: o processo está encerrando.
    Stopped,
    /// A conexão caiu; vale tentar de novo.
    Dropped(String),
}

/// Conexão com o streamer, com reconexão.
pub struct YahooStreamer {
    url: String,
    heartbeat: Duration,
}

impl Default for YahooStreamer {
    fn default() -> Self {
        Self::with_url(YAHOO_STREAMER)
    }
}

impl YahooStreamer {
    pub fn with_url(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            heartbeat: Duration::from_secs(30),
        }
    }

    /// De quanto em quanto tempo mandar `ping`. Três intervalos sem nenhum
    /// quadro do servidor (nem o `pong`) derrubam a conexão.
    pub fn with_heartbeat(mut self, every: Duration) -> Self {
        self.heartbeat = every;
        self
    }

    /// Roda até `symbols` ou `ticks` fecharem: conecta, assina o que
    /// `symbols` pede (e acompanha as mudanças), repassa cada preço e, quando a
    /// conexão cai, reconecta com espera crescente.
    pub async fn run(
        self,
        mut symbols: watch::Receiver<BTreeSet<String>>,
        ticks: mpsc::Sender<Tick>,
    ) {
        let mut wait = MIN_BACKOFF;
        loop {
            if symbols.borrow_and_update().is_empty() {
                // Nada para assinar: espera aparecer, sem conexão aberta.
                if symbols.changed().await.is_err() {
                    return;
                }
                continue;
            }
            let started = Instant::now();
            match self.session(&mut symbols, &ticks).await {
                SessionEnd::Stopped => return,
                SessionEnd::Dropped(reason) => {
                    metrics::counter!("quote_stream_reconnects_total").increment(1);
                    tracing::warn!(reason = %reason, "streamer de precos caiu, reconectando");
                }
            }
            if started.elapsed() >= STABLE {
                wait = MIN_BACKOFF;
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(MAX_BACKOFF);
        }
    }

    async fn session(
        &self,
        symbols: &mut watch::Receiver<BTreeSet<String>>,
        ticks: &mpsc::Sender<Tick>,
    ) -> SessionEnd {
        let connect = tokio_tungstenite::connect_async(self.url.as_str());
        let mut ws = match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
            Ok(Ok((ws, _))) => ws,
            Ok(Err(error)) => return SessionEnd::Dropped(error.to_string()),
            Err(_) => return SessionEnd::Dropped("conexao demorou demais".into()),
        };
        let mut subscribed = BTreeSet::new();
        let mut names = BTreeMap::new();
        let wanted = symbols.borrow_and_update().clone();
        if let Err(error) = resubscribe(&mut ws, &mut subscribed, &mut names, &wanted).await {
            return SessionEnd::Dropped(error.to_string());
        }
        tracing::info!(symbols = subscribed.len(), "streamer de precos conectado");

        let mut heartbeat =
            tokio::time::interval_at(Instant::now() + self.heartbeat, self.heartbeat);
        let mut last_seen = Instant::now();
        loop {
            tokio::select! {
                frame = ws.next() => {
                    let frame = match frame {
                        None => return SessionEnd::Dropped("conexao encerrada".into()),
                        Some(Err(error)) => return SessionEnd::Dropped(error.to_string()),
                        Some(Ok(frame)) => frame,
                    };
                    last_seen = Instant::now();
                    match frame {
                        Message::Text(text) => {
                            let Some((name, price, traded_at)) = decode(text.as_str()) else {
                                continue;
                            };
                            let Some(symbol) = names.get(&name) else {
                                continue;
                            };
                            let tick = Tick { symbol: symbol.clone(), price, traded_at };
                            if ticks.send(tick).await.is_err() {
                                return SessionEnd::Stopped;
                            }
                        }
                        Message::Close(_) => {
                            return SessionEnd::Dropped("o servidor fechou a conexao".into());
                        }
                        // `ping` o tungstenite responde sozinho; `pong` e
                        // binário só contam como sinal de vida.
                        _ => {}
                    }
                }
                changed = symbols.changed() => {
                    if changed.is_err() {
                        let _ = ws.close(None).await;
                        return SessionEnd::Stopped;
                    }
                    let wanted = symbols.borrow_and_update().clone();
                    if let Err(error) = resubscribe(&mut ws, &mut subscribed, &mut names, &wanted).await {
                        return SessionEnd::Dropped(error.to_string());
                    }
                }
                _ = heartbeat.tick() => {
                    if last_seen.elapsed() > self.heartbeat * 3 {
                        return SessionEnd::Dropped("servidor sem resposta".into());
                    }
                    if let Err(error) = ws.send(Message::Ping(Bytes::new())).await {
                        return SessionEnd::Dropped(error.to_string());
                    }
                }
            }
        }
    }
}

/// Leva a assinatura de `subscribed` para `wanted` (símbolos do `chart`).
/// `names` fica com o caminho de volta: nome no streamer → símbolo do `chart`.
async fn resubscribe<S>(
    ws: &mut S,
    subscribed: &mut BTreeSet<String>,
    names: &mut BTreeMap<String, String>,
    wanted: &BTreeSet<String>,
) -> Result<(), tungstenite::Error>
where
    S: Sink<Message, Error = tungstenite::Error> + Unpin,
{
    let wanted: BTreeMap<String, String> = wanted
        .iter()
        .map(|symbol| (stream_name(symbol).to_owned(), symbol.clone()))
        .collect();
    let (gone, new) = diff(subscribed, &wanted);
    if !gone.is_empty() {
        let body = serde_json::json!({ "unsubscribe": gone }).to_string();
        ws.send(Message::text(body)).await?;
    }
    if !new.is_empty() {
        let body = serde_json::json!({ "subscribe": new }).to_string();
        ws.send(Message::text(body)).await?;
    }
    *subscribed = wanted.keys().cloned().collect();
    *names = wanted;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // Mensagens reais do streamer, gravadas em 25/11/2026.
    const PETR4: &str = r#"{"type":"pricing","message":"CghQRVRSNC5TQRWPwiRCGLCwy8bCaCoDU0FPMAg4AUWiBco/SLCF2jNlANcjP7AByAHYAQT1AgDXIz/9AqIFyj8="}"#;
    const DOLLAR: &str = r#"{"type":"pricing","message":"CgVCUkw9WBXsUbpAGKCPucfCaCoDQ0NZMA44AUXbupm/ZYD/kL3YAQj1AoD/kL39Atu6mb8="}"#;
    const BITCOIN: &str = r#"{"type":"pricing","message":"CgdCVEMtVVNEFfZUakcYoI+5x8JoIgNVU0QqA0NDQzApOAFFFEv9PkiAgNHvoAJVfRRrR10vzl9HZQCok0N9p2FpR7ABgIDR76AC2AEE4AGAgNHvoALoAYCA0e+gAvIBA0JUQ4ECAAAAwHYvc0GJAgAADsBii3FC9QIAqJND/QIUS/0+"}"#;

    #[test]
    fn decodes_recorded_messages() {
        let (name, price, at) = decode(PETR4).unwrap();
        assert_eq!((name.as_str(), price), ("PETR4.SA", dec!(41.19)));
        assert_eq!(at.unix_timestamp(), 1_795_638_783);

        let (name, price, _) = decode(DOLLAR).unwrap();
        assert_eq!((name.as_str(), price), ("BRL=X", dec!(5.8225)));

        let (name, price, _) = decode(BITCOIN).unwrap();
        assert_eq!((name.as_str(), price), ("BTC-USD", dec!(59988.96)));
    }

    #[test]
    fn ignores_what_is_not_a_price() {
        assert_eq!(decode("nada"), None);
        assert_eq!(decode(r#"{"type":"heartbeat","message":""}"#), None);
        assert_eq!(decode(r#"{"type":"pricing","message":"%%%"}"#), None);
        // Protobuf válido sem símbolo nem hora.
        let empty = base64::engine::general_purpose::STANDARD.encode(
            PricingData {
                id: String::new(),
                price: 1.0,
                time: 0,
            }
            .encode_to_vec(),
        );
        assert_eq!(
            decode(&format!(r#"{{"type":"pricing","message":"{empty}"}}"#)),
            None
        );
    }

    #[test]
    fn prices_keep_only_the_digits_the_float_carries() {
        assert_eq!(price_from(41.26), Some(dec!(41.26)));
        assert_eq!(price_from(5.8102), Some(dec!(5.8102)));
        assert_eq!(price_from(59_791.59), Some(dec!(59791.59)));
        assert_eq!(price_from(0.000_012_34), Some(dec!(0.00001234)));
        assert_eq!(price_from(0.0), None);
        assert_eq!(price_from(-3.0), None);
        assert_eq!(price_from(f32::NAN), None);
        assert_eq!(price_from(f32::INFINITY), None);
        assert_eq!(price_from(1e-10), None);
    }

    #[test]
    fn the_dollar_has_another_name_on_the_streamer() {
        assert_eq!(stream_name("USDBRL=X"), "BRL=X");
        assert_eq!(stream_name("KNRI11.SA"), "KNRI11.SA");
        assert_eq!(stream_name("BTC-USD"), "BTC-USD");
    }

    #[test]
    fn subscription_changes_only_what_differs() {
        let subscribed: BTreeSet<String> =
            ["BRL=X", "KNRI11.SA", "PETR4.SA"].map(str::to_owned).into();
        let wanted: BTreeMap<String, String> = [
            ("BRL=X", "USDBRL=X"),
            ("PETR4.SA", "PETR4.SA"),
            ("BTC-USD", "BTC-USD"),
        ]
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .into();
        let (gone, new) = diff(&subscribed, &wanted);
        assert_eq!(gone, vec!["KNRI11.SA"]);
        assert_eq!(new, vec!["BTC-USD"]);
        assert_eq!(diff(&BTreeSet::new(), &BTreeMap::new()), (vec![], vec![]));
    }
}
