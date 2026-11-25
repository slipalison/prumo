//! `GET /api/v1/portfolio/stream`: a carteira ao vivo, em Server-Sent Events.
//!
//! O primeiro evento `prices` sai na hora (a tela fica em dia mesmo que tenha
//! carregado há pouco); os seguintes, quando um preço de que a carteira depende
//! muda no banco. Comentário de keep-alive a cada 15 s segura a conexão no
//! Cloudflare (100 s sem byte derrubam) e no Envoy do Istio.
//!
//! A conexão termina sozinha quando a credencial do Access vence — o
//! `EventSource` reconecta e o Access confere a sessão de novo — e quando o
//! processo começa a encerrar, para o desligamento gracioso não esperar por
//! ela.

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderName, HeaderValue};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::Instant;
use uuid::Uuid;

use super::dto::LivePricesDto;
use super::{AppState, CurrentUser};
use crate::error::{ApiError, ApiResult};
use crate::live::Changed;
use crate::service;

const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// Quanto o navegador espera para reconectar depois de uma queda.
const RETRY: Duration = Duration::from_secs(3);

pub async fn portfolio_stream(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Response> {
    // Ouve antes de ler: um preço que mude entre a leitura e o laço não se
    // perde.
    let changes = state.quotes.live().subscribe();
    let view = service::portfolio(&state.store, user.id).await?;
    let first = LivePricesDto::from(&view);
    let event = prices_event(&first)
        .map_err(|_| ApiError::internal())?
        .retry(RETRY);

    let (tx, rx) = mpsc::channel::<Event>(4);
    // Canal novo e vazio: cabe.
    let _ = tx.try_send(event);
    let deadline = user.expires_at.map(deadline_for);
    tokio::spawn(feed(
        state,
        user.id,
        view.quote_symbols(),
        first,
        changes,
        deadline,
        tx,
    ));

    let events = stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|event| (Ok::<_, Infallible>(event), rx))
    });
    let mut response = Sse::new(events)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
        .into_response();
    // Proxy que bufferiza resposta (nginx e parecidos) entrega na hora.
    response.headers_mut().insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    Ok(response)
}

fn prices_event(prices: &LivePricesDto) -> Result<Event, axum::Error> {
    Event::default().event("prices").json_data(prices)
}

/// O instante do relógio do processo em que a credencial vence.
fn deadline_for(expires_at: OffsetDateTime) -> Instant {
    let left = expires_at - OffsetDateTime::now_utc();
    Instant::now() + Duration::try_from(left).unwrap_or(Duration::ZERO)
}

/// Espera o [`AppState::stop`]. A guarda do `watch` fica aqui dentro: ela não
/// é `Send` e não pode atravessar o `select!`.
async fn stopped(stopping: &mut watch::Receiver<bool>) {
    let _ = stopping.wait_for(|stop| *stop).await;
}

async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Mantém a conexão de uma pessoa: a cada aviso que toca um símbolo dela,
/// recalcula a carteira e manda o evento se algum número mudou.
async fn feed(
    state: AppState,
    user: Uuid,
    mut watched: BTreeSet<String>,
    mut last: LivePricesDto,
    mut changes: broadcast::Receiver<Changed>,
    deadline: Option<Instant>,
    tx: mpsc::Sender<Event>,
) {
    let mut stopping = state.stopping();
    metrics::gauge!("live_streams").increment(1.0);
    loop {
        tokio::select! {
            () = tx.closed() => break,
            () = stopped(&mut stopping) => break,
            () = until(deadline) => break,
            message = changes.recv() => {
                match message {
                    Ok(changed) if changed.is_disjoint(&watched) => continue,
                    // Quem se atrasou perdeu avisos: recalcula do mesmo jeito.
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
                // Avisos que chegaram juntos viram uma leitura só.
                while changes.try_recv().is_ok() {}
                let view = match service::portfolio(&state.store, user).await {
                    Ok(view) => view,
                    Err(error) => {
                        tracing::warn!(error = %error, "carteira ao vivo nao recalculada");
                        continue;
                    }
                };
                watched = view.quote_symbols();
                let next = LivePricesDto::from(&view);
                if next == last {
                    continue;
                }
                let Ok(event) = prices_event(&next) else { continue };
                if tx.send(event).await.is_err() {
                    break;
                }
                metrics::counter!("live_events_total").increment(1);
                last = next;
            }
        }
    }
    metrics::gauge!("live_streams").decrement(1.0);
}
