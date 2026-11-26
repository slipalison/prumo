//! `POST /api/v1/imports/b3/preview` e `POST /api/v1/imports/b3/commit`: lê as
//! planilhas da Área do Investidor da B3, confronta com a carteira e mostra o
//! plano (prévia) ou grava o plano (commit).
//!
//! **Sem estado entre as duas chamadas.** A prévia não guarda nada: nem os
//! arquivos, nem o plano. Para gravar, a tela reenvia os mesmos arquivos, as
//! decisões da pessoa e o `digest` do plano que ela conferiu; o servidor relê
//! tudo, refaz o plano dentro da transação e só grava se o `digest` for o
//! mesmo (senão, `409 preview_stale`). Assim nenhum arquivo nem plano fica em
//! disco, em banco ou em memória entre um pedido e outro.
//!
//! Os arquivos são sensíveis. Eles ficam na memória do pedido, nunca vão para
//! disco, e nada do conteúdo (nem nome de ativo, nem quantidade) vai para o log
//! nem para mensagem de erro. O log desta rota diz no máximo "falhou".

use std::collections::BTreeMap;

use axum::extract::multipart::{Multipart, MultipartError, MultipartRejection};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::dto::{decimal, optional_decimal};
use super::{AppState, CurrentUser};
use crate::b3_xlsx;
use crate::domain::asset_class::AssetClass;
use crate::domain::b3_import::{build_preview, read_file, FileOutcome, Preview};
use crate::domain::b3_plan::{
    self, AssetAction, AssetDecision, Decisions, LineState, OpeningChoice, Plan, PlanSummary,
};
use crate::error::{ApiError, ApiResult};
use crate::store::Applied;

/// Teto do corpo destas rotas (todos os arquivos juntos). O resto da API fica
/// no teto de 64 KiB de `http/mod.rs`.
pub const MAX_BODY: usize = 10 * 1024 * 1024;
/// Arquivos por envio: os três extratos e os proventos, com folga.
pub const MAX_FILES: usize = 10;
const MAX_NAME_CHARS: usize = 120;
/// Teto da parte `decisions` (JSON) do envio.
const MAX_DECISIONS_BYTES: usize = 256 * 1024;

/// Teto de quantidade e preço numa decisão (10^15), como no resto da API.
fn max_number() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

/// As rotas, já com o teto de corpo próprio. Quem monta o roteador as junta
/// **depois** do teto geral, para o de 64 KiB não valer aqui.
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/imports/b3/preview", post(preview))
        .route("/imports/b3/commit", post(commit))
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

/// 409: o plano refeito agora não é o que a pessoa conferiu.
fn preview_stale() -> ApiError {
    ApiError {
        status: StatusCode::CONFLICT,
        code: "preview_stale",
        title: "A previa mudou",
        detail: "A carteira ou as planilhas mudaram desde a previa. Confira a previa de novo antes de importar.".to_owned(),
    }
}

/// 422: ainda há conflito ou classe por confirmar. Só contagens, sem conteúdo.
fn import_blocked(summary: &PlanSummary) -> ApiError {
    ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "import_blocked",
        title: "Importacao travada",
        detail: format!(
            "Resolva antes de importar: {} ativo(s) com conflito e {} com a classe por confirmar.",
            summary.conflicts, summary.needs_confirmation
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

// ------------------------------------------------------------- decisões --

/// A parte `decisions` do envio (JSON).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionsRequest {
    /// O `plan.digest` da prévia conferida. Obrigatório no commit.
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub assets: BTreeMap<String, AssetDecisionRequest>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetDecisionRequest {
    #[serde(default)]
    pub asset_class: Option<AssetClass>,
    #[serde(default)]
    pub skip: bool,
    #[serde(default)]
    pub opening: Option<OpeningRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningRequest {
    #[serde(deserialize_with = "decimal")]
    pub quantity: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub unit_price: Option<Decimal>,
}

fn decisions_of(request: &DecisionsRequest) -> ApiResult<Decisions> {
    let mut assets = BTreeMap::new();
    for (key, decision) in &request.assets {
        let opening = match &decision.opening {
            Some(opening) => {
                let price_ok = opening
                    .unit_price
                    .is_none_or(|p| p >= Decimal::ZERO && p <= max_number());
                if opening.quantity < Decimal::ZERO || opening.quantity > max_number() || !price_ok
                {
                    return Err(ApiError::validation(
                        "O saldo inicial vai de 0 a 10^15, e o preco medio tambem.",
                    ));
                }
                Some(OpeningChoice {
                    quantity: opening.quantity,
                    unit_price: opening.unit_price,
                })
            }
            None => None,
        };
        assets.insert(
            key.clone(),
            AssetDecision {
                asset_class: decision.asset_class,
                skip: decision.skip,
                opening,
            },
        );
    }
    Ok(Decisions { assets })
}

// --------------------------------------------------------------- envio --

/// O que veio no envio: os arquivos (nome e bytes, na memória) e as decisões.
struct Upload {
    files: Vec<(String, Vec<u8>)>,
    decisions: DecisionsRequest,
}

async fn read_upload(multipart: Result<Multipart, MultipartRejection>) -> ApiResult<Upload> {
    let mut multipart = multipart
        .map_err(|_| ApiError::validation("Envie as planilhas como multipart/form-data."))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut decisions = DecisionsRequest::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(&error))?
    {
        let Some(name) = field.file_name().map(display_name) else {
            // A única parte sem arquivo que conta: as decisões da tela.
            if field.name() == Some("decisions") {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| multipart_error(&error))?;
                if bytes.len() > MAX_DECISIONS_BYTES {
                    return Err(ApiError::validation(
                        "As decisoes passam do tamanho aceito.",
                    ));
                }
                decisions = serde_json::from_slice(&bytes).map_err(|error| {
                    ApiError::validation(format!("Decisoes invalidas: {error}"))
                })?;
            }
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
    Ok(Upload { files, decisions })
}

async fn read_preview(files: Vec<(String, Vec<u8>)>) -> ApiResult<Preview> {
    // Abrir xlsx é CPU: fora do executor async.
    tokio::task::spawn_blocking(move || read_all(files, outcome_of))
        .await
        .map_err(|_| {
            tracing::error!("a leitura das planilhas da B3 foi interrompida");
            ApiError::internal()
        })?
        .map_err(|unreadable| unreadable_file(&unreadable.name))
}

// --------------------------------------------------------------- rotas --

/// A prévia com o plano: o que é novo, o que já existe e o que conflita.
#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    #[serde(flatten)]
    pub preview: Preview,
    pub plan: Plan,
}

pub async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<Json<PreviewResponse>> {
    let upload = read_upload(multipart).await?;
    let decisions = decisions_of(&upload.decisions)?;
    let preview = read_preview(upload.files).await?;
    let book = state.store.b3_book(user.id).await?;
    let plan = b3_plan::plan(&preview, &book, &decisions);
    Ok(Json(PreviewResponse { preview, plan }))
}

/// Uma linha que ficou de fora do que foi gravado.
#[derive(Debug, Serialize)]
pub struct NotImportedDto {
    pub key: String,
    pub external_ref: String,
    pub reason_code: &'static str,
}

/// O que a gravação fez.
#[derive(Debug, Serialize)]
pub struct CommitResponse {
    pub summary: PlanSummary,
    /// Linhas lidas que não entraram, com o motivo.
    pub not_imported: Vec<NotImportedDto>,
    /// Ativos que a pessoa deixou de fora.
    pub skipped_assets: Vec<String>,
    /// Linhas que a leitura não entendeu.
    pub unrecognized: usize,
    /// Linhas deixadas de fora de propósito na leitura (liquidação, repetida...).
    pub ignored: usize,
}

enum Refusal {
    Stale,
    Blocked(PlanSummary),
}

pub async fn commit(
    State(state): State<AppState>,
    user: CurrentUser,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<Json<CommitResponse>> {
    let upload = read_upload(multipart).await?;
    let decisions = decisions_of(&upload.decisions)?;
    let digest = upload
        .decisions
        .digest
        .clone()
        .filter(|d| !d.trim().is_empty())
        .ok_or_else(|| {
            ApiError::validation("Envie o resumo (digest) da previa que voce conferiu.")
        })?;
    let preview = read_preview(upload.files).await?;
    let outcome = state
        .store
        .commit_b3_import(user.id, |book| {
            let plan = b3_plan::plan(&preview, book, &decisions);
            if plan.digest != digest {
                Err(Refusal::Stale)
            } else if !plan.ready {
                Err(Refusal::Blocked(plan.summary.clone()))
            } else {
                Ok(plan)
            }
        })
        .await?;
    let applied = match outcome {
        Ok(applied) => applied,
        Err(Refusal::Stale) => return Err(preview_stale()),
        Err(Refusal::Blocked(summary)) => return Err(import_blocked(&summary)),
    };
    after_commit(&state, &applied);
    Ok(Json(response_of(&preview, &applied)))
}

/// Depois de gravar, os ativos novos buscam cotação e histórico de dividendos
/// em segundo plano. A série de desempenho não precisa de nada daqui: os
/// gatilhos da 0013 já a marcaram como suja desde o lançamento ou provento
/// mais antigo que a gravação escreveu, e a tarefa de reconstrução a refaz
/// inteira dali para a frente (uma foto de hoje refeita aqui seria
/// sobrescrita por ela, com o histórico antigo errado até lá).
fn after_commit(state: &AppState, applied: &Applied) {
    let targets: Vec<_> = applied
        .created
        .iter()
        .filter_map(|a| a.symbol.clone().map(|s| (a.market, s)))
        .collect();
    for (market, symbol) in &targets {
        state.income.prefetch(*market, Some(symbol));
    }
    if !targets.is_empty() {
        let quotes = state.quotes.clone();
        tokio::spawn(async move {
            quotes.refresh(&targets).await;
        });
    }
}

fn response_of(preview: &Preview, applied: &Applied) -> CommitResponse {
    let mut not_imported = Vec::new();
    for asset in &preview.assets {
        let refs = asset
            .trades
            .iter()
            .map(|t| &t.external_ref)
            .chain(asset.events.iter().map(|e| &e.external_ref))
            .chain(asset.income.iter().map(|i| &i.external_ref));
        for external_ref in refs {
            if let Some(line) = applied.plan.lines.get(external_ref) {
                if line.status == LineState::NotImported {
                    not_imported.push(NotImportedDto {
                        key: asset.key.clone(),
                        external_ref: external_ref.clone(),
                        reason_code: line.reason_code.unwrap_or("not_imported"),
                    });
                }
            }
        }
    }
    CommitResponse {
        summary: applied.plan.summary.clone(),
        not_imported,
        skipped_assets: applied
            .plan
            .assets
            .iter()
            .filter(|a| a.action == AssetAction::Skip)
            .map(|a| a.key.clone())
            .collect(),
        unrecognized: preview.unrecognized.len(),
        ignored: preview.ignored.iter().map(|i| i.count).sum(),
    }
}

/// Um arquivo cuja leitura entrou em pânico. Guarda só o nome que a pessoa
/// deu ao arquivo, nunca conteúdo.
#[derive(Debug, PartialEq, Eq)]
struct PanickedFile {
    name: String,
}

/// 422: o arquivo é entrada não confiável e quebrou o leitor. A mensagem não
/// cita nada de dentro dele.
fn unreadable_file(name: &str) -> ApiError {
    ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "unreadable_file",
        title: "Arquivo ilegivel",
        detail: format!("Nao consegui ler este arquivo ({name}). Baixe de novo na Area do Investidor ou tire-o do envio."),
    }
}

/// Lê todos os arquivos, cada um **isolado**: um pânico do leitor (da
/// dependência que abre o `.xlsx` ou das regras) não passa daqui. `reader` é
/// injetado para o teste poder entrar em pânico de propósito.
fn read_all<R>(files: Vec<(String, Vec<u8>)>, reader: R) -> Result<Preview, PanickedFile>
where
    R: Fn(&[u8]) -> FileOutcome,
{
    let mut names = Vec::with_capacity(files.len());
    let mut outcomes = Vec::with_capacity(files.len());
    for (name, bytes) in files {
        match b3_xlsx::isolate(|| reader(&bytes)) {
            Ok(outcome) => outcomes.push(outcome),
            Err(b3_xlsx::ReaderPanic) => return Err(PanickedFile { name }),
        }
        names.push(name);
    }
    Ok(build_preview(names, outcomes))
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

    fn file(name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
        (name.to_owned(), bytes.to_vec())
    }

    #[test]
    fn a_reader_that_panics_is_contained_and_names_only_the_file() {
        // Um leitor que panica com o "conteúdo" do arquivo na mensagem.
        let reader = |bytes: &[u8]| -> FileOutcome {
            if bytes.starts_with(b"BOOM") {
                panic!("conteudo secreto {}", String::from_utf8_lossy(bytes));
            }
            FileOutcome::Unreadable("not_xlsx", "nao e xlsx".to_owned())
        };
        let ok = read_all(vec![file("a.xlsx", b"ok")], reader).unwrap();
        assert_eq!(ok.files.len(), 1);

        // O pânico do segundo arquivo não passa; o erro cita só o nome.
        let panicked = read_all(
            vec![file("a.xlsx", b"ok"), file("b.xlsx", b"BOOM 3000 PETR4")],
            reader,
        )
        .unwrap_err();
        assert_eq!(
            panicked,
            PanickedFile {
                name: "b.xlsx".to_owned()
            }
        );

        let error = unreadable_file(&panicked.name);
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error.code, "unreadable_file");
        assert!(error.detail.contains("b.xlsx"));
        assert!(!error.detail.contains("secreto") && !error.detail.contains("PETR4"));

        // E o processo segue vivo: a próxima leitura funciona normalmente.
        assert!(read_all(vec![file("c.xlsx", b"ok")], reader).is_ok());
    }

    #[test]
    fn the_real_reader_survives_garbage_of_every_shape() {
        for bytes in [
            &b""[..],
            b"PK\x03\x04",
            b"PK\x03\x04\x14\x00\x00\x00\x08\x00garbage-garbage-garbage",
            &[0xFF; 4096],
        ] {
            let preview = read_all(vec![file("x.xlsx", bytes)], outcome_of).unwrap();
            assert_eq!(
                preview.files[0].status,
                crate::domain::b3_import::FileStatus::Unreadable
            );
        }
    }

    #[test]
    fn too_large_is_a_problem_json_413() {
        let error = too_large();
        assert_eq!(error.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error.code, "payload_too_large");
        assert!(error.detail.contains("10 MiB"));
    }
}
