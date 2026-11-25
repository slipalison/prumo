//! Porta interna (9090): `/healthz`, `/readyz` e `/metrics`.

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tower_http::set_header::SetResponseHeaderLayer;

use super::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}

/// 200 enquanto o Postgres responde, 503 quando não.
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    match state.store.ping().await {
        Ok(()) => (StatusCode::OK, "ok"),
        Err(error) => {
            tracing::warn!(error = %error, "banco indisponivel");
            (StatusCode::SERVICE_UNAVAILABLE, "banco indisponivel")
        }
    }
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state
        .metrics
        .as_ref()
        .map(|h| h.render())
        .unwrap_or_default();
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}
