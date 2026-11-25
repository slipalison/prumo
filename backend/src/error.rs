//! Erros da API no formato RFC 9457 (`application/problem+json`).

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Um erro que a API devolve. `title` e `detail` são lidos pela pessoa; vão
/// sem acento porque também atravessam o log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub title: &'static str,
    pub detail: String,
}

#[derive(Serialize)]
struct Problem<'a> {
    #[serde(rename = "type")]
    kind: String,
    title: &'a str,
    status: u16,
    detail: &'a str,
    code: &'a str,
}

impl ApiError {
    pub fn unauthenticated(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthenticated",
            title: "Acesso nao autorizado",
            detail: detail.into(),
        }
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "validation_failed",
            title: "Dados invalidos",
            detail: detail.into(),
        }
    }

    pub fn targets_must_sum_100(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "targets_must_sum_100",
            title: "As metas precisam somar 100%",
            detail: detail.into(),
        }
    }

    pub fn asset_already_exists(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "asset_already_exists",
            title: "Esse ativo ja esta na carteira",
            detail: detail.into(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            title: "Nao encontrado",
            detail: "O recurso pedido nao existe.".to_owned(),
        }
    }

    pub fn quote_provider_unavailable() -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "quote_provider_unavailable",
            title: "Provedor de cotacoes indisponivel",
            detail: "Nao foi possivel consultar as cotacoes agora. Tente de novo em instantes."
                .to_owned(),
        }
    }

    pub fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            title: "Erro interno",
            detail: "Algo deu errado do nosso lado.".to_owned(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Problem {
            kind: format!("urn:prumo:error:{}", self.code),
            title: self.title,
            status: self.status.as_u16(),
            detail: &self.detail,
            code: self.code,
        };
        let mut response = (self.status, axum::Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(error = %error, "falha no banco");
        ApiError::internal()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn renders_problem_json() {
        let response = ApiError::targets_must_sum_100("A soma enviada foi 95%.").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["type"], "urn:prumo:error:targets_must_sum_100");
        assert_eq!(json["code"], "targets_must_sum_100");
        assert_eq!(json["status"], 422);
        assert_eq!(json["detail"], "A soma enviada foi 95%.");
    }

    #[test]
    fn every_constructor_has_its_status() {
        assert_eq!(
            ApiError::unauthenticated("x").status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ApiError::validation("x").status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ApiError::asset_already_exists("x").status,
            StatusCode::CONFLICT
        );
        assert_eq!(ApiError::not_found().status, StatusCode::NOT_FOUND);
        assert_eq!(
            ApiError::quote_provider_unavailable().status,
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ApiError::internal().status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(ApiError::from(sqlx::Error::RowNotFound).code, "internal");
    }
}
