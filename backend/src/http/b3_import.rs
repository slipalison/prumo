//! `POST /api/v1/imports/b3/preview`: lê as planilhas da Área do Investidor da
//! B3 e devolve a prévia do que entraria na carteira. Só lê: nada é gravado.
//!
//! Os arquivos são sensíveis. Eles ficam na memória do pedido, nunca vão para
//! disco, e nada do conteúdo (nem nome de ativo, nem quantidade) vai para o log
//! nem para mensagem de erro. O log desta rota diz no máximo "falhou".

use axum::extract::multipart::{Multipart, MultipartError, MultipartRejection};
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};

use super::{AppState, CurrentUser};
use crate::b3_xlsx;
use crate::domain::b3_import::{build_preview, read_file, FileOutcome, Preview};
use crate::error::{ApiError, ApiResult};

/// Teto do corpo desta rota (todos os arquivos juntos). O resto da API fica no
/// teto de 64 KiB de `http/mod.rs`.
pub const MAX_BODY: usize = 10 * 1024 * 1024;
/// Arquivos por envio: os três extratos e os proventos, com folga.
pub const MAX_FILES: usize = 10;
const MAX_NAME_CHARS: usize = 120;

/// A rota, já com o teto de corpo próprio. Quem monta o roteador a junta
/// **depois** do teto geral, para o de 64 KiB não valer aqui.
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/imports/b3/preview", post(preview))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

fn too_large() -> ApiError {
    ApiError {
        status: StatusCode::PAYLOAD_TOO_LARGE,
        code: "payload_too_large",
        title: "Envio grande demais",
        detail: format!(
            "Os arquivos juntos passam de {} MiB. Envie menos arquivos ou um periodo menor.",
            MAX_BODY / (1024 * 1024)
        ),
    }
}

/// O erro do multipart vira 413 (corpo acima do teto) ou 422. O texto do erro
/// original não é repassado.
fn multipart_error(error: &MultipartError) -> ApiError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        too_large()
    } else {
        ApiError::validation("O envio nao esta no formato multipart esperado.")
    }
}

/// Só o último trecho do caminho, cortado: o navegador às vezes manda o
/// caminho inteiro.
fn display_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    let name: String = last.chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        "planilha.xlsx".to_owned()
    } else {
        name
    }
}

pub async fn preview(
    _user: CurrentUser,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<Json<Preview>> {
    let mut multipart = multipart
        .map_err(|_| ApiError::validation("Envie as planilhas como multipart/form-data."))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(&error))?
    {
        // Só as partes que são arquivo; campo de texto solto é ignorado.
        let Some(name) = field.file_name().map(display_name) else {
            continue;
        };
        if files.len() >= MAX_FILES {
            return Err(ApiError::validation(format!(
                "Envie no maximo {MAX_FILES} planilhas por vez."
            )));
        }
        let bytes = field
            .bytes()
            .await
            .map_err(|error| multipart_error(&error))?;
        files.push((name, bytes.to_vec()));
    }
    if files.is_empty() {
        return Err(ApiError::validation("Envie ao menos uma planilha .xlsx."));
    }

    // Abrir xlsx é CPU: fora do executor async.
    let preview = tokio::task::spawn_blocking(move || {
        let (names, outcomes): (Vec<String>, Vec<FileOutcome>) = files
            .into_iter()
            .map(|(name, bytes)| (name, outcome_of(&bytes)))
            .unzip();
        build_preview(names, outcomes)
    })
    .await
    .map_err(|_| {
        tracing::error!("a leitura das planilhas da B3 foi interrompida");
        ApiError::internal()
    })?;
    Ok(Json(preview))
}

fn outcome_of(bytes: &[u8]) -> FileOutcome {
    match b3_xlsx::read_workbook(bytes) {
        Err(error) => FileOutcome::Unreadable(error.code(), error.message().to_owned()),
        Ok(sheets) => match read_file(&sheets) {
            Ok(reading) => FileOutcome::Read(reading),
            Err(problem) => FileOutcome::UnknownFormat(problem),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_keeps_only_the_file_name() {
        assert_eq!(
            display_name("C:\\Users\\x\\negociacao.xlsx"),
            "negociacao.xlsx"
        );
        assert_eq!(display_name("/tmp/a/b.xlsx"), "b.xlsx");
        assert_eq!(display_name("  "), "planilha.xlsx");
        assert_eq!(display_name("a/"), "planilha.xlsx");
        assert_eq!(
            display_name(&"x".repeat(300)).chars().count(),
            MAX_NAME_CHARS
        );
    }

    #[test]
    fn too_large_is_a_problem_json_413() {
        let error = too_large();
        assert_eq!(error.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error.code, "payload_too_large");
        assert!(error.detail.contains("10 MiB"));
    }
}
