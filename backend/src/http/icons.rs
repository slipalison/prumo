//! Logos dos ativos: `GET /api/v1/icons` (quais símbolos têm logo) e
//! `GET /api/v1/icons/{symbol}` (o logo, do cache do banco). A busca na brapi é
//! da tarefa em segundo plano (`crate::icons`); aqui só se lê.
//!
//! O SVG é de terceiro e sai do domínio do Prumo, então a resposta o trata como
//! conteúdo hostil, mesmo já saneado (`domain::icon`): CSP que proíbe tudo e
//! sandbox, `nosniff`, `Content-Disposition: inline` (a tela só o usa em
//! `<img>`, onde o navegador não executa script de SVG de qualquer jeito) e
//! `Cross-Origin-Resource-Policy: same-origin`.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use super::{ApiPath, AppState, CurrentUser};
use crate::domain::icon::icon_symbol;
use crate::error::{ApiError, ApiResult};

/// Nada roda, nada carrega, nada é enquadrado: um logo é só um desenho.
const ICON_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; sandbox";
/// Um dia no navegador; depois confere pelo `ETag` (e a tela troca a URL, com
/// `?v=<versão>`, quando o logo muda).
const ICON_CACHE: &str = "private, max-age=86400";

/// As duas rotas, montadas fora do `/api/v1` comum (ver `public_router`).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/icons", get(index))
        .route("/api/v1/icons/{symbol}", get(show))
}

#[derive(Debug, Serialize)]
pub struct IconIndex {
    pub icons: Vec<IconEntry>,
}

#[derive(Debug, Serialize)]
pub struct IconEntry {
    pub symbol: String,
    /// A versão do logo (o `ETag` sem aspas): vai na URL como `?v=` e faz o
    /// navegador buscar de novo quando o logo muda.
    pub version: String,
}

/// Os símbolos que têm logo, sem os bytes. A tela usa a lista para não pedir
/// logo que não existe.
pub async fn index(State(state): State<AppState>, _user: CurrentUser) -> ApiResult<Response> {
    let icons = state
        .store
        .icon_versions()
        .await?
        .into_iter()
        .map(|(symbol, version)| IconEntry { symbol, version })
        .collect();
    let mut response = Json(IconIndex { icons }).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub async fn show(
    State(state): State<AppState>,
    _user: CurrentUser,
    ApiPath(raw): ApiPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(icon) = (match icon_symbol(&raw) {
        Some(symbol) => state.store.icon(&symbol).await?,
        None => None,
    }) else {
        return Ok(not_found());
    };
    let etag = format!("\"{}\"", icon.etag);
    let fresh = matches_etag(&headers, &etag);
    let mut response = if fresh {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (StatusCode::OK, icon.bytes).into_response()
    };
    let out = response.headers_mut();
    if !fresh {
        out.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&icon.content_type).map_err(|_| ApiError::internal())?,
        );
    }
    out.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).map_err(|_| ApiError::internal())?,
    );
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(ICON_CACHE));
    out.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(ICON_CSP),
    );
    out.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );
    out.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    out.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    Ok(response)
}

/// 404 que o navegador não guarda: o logo pode chegar a qualquer momento.
fn not_found() -> Response {
    let mut response = ApiError::not_found().into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Se o `If-None-Match` da requisição cobre o `ETag` (aceita `*`, lista e a
/// forma fraca `W/"..."`).
fn matches_etag(headers: &HeaderMap, etag: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    value.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn if_none_match_is_read_like_the_spec() {
        let etag = "\"abc\"";
        assert!(matches_etag(&headers("\"abc\""), etag));
        assert!(matches_etag(&headers("W/\"abc\""), etag));
        assert!(matches_etag(&headers("\"x\", \"abc\""), etag));
        assert!(matches_etag(&headers("*"), etag));
        assert!(!matches_etag(&headers("\"x\""), etag));
        assert!(!matches_etag(&HeaderMap::new(), etag));
    }
}
