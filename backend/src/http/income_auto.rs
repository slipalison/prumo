//! `GET/PUT /api/v1/income/auto`: a chave dos proventos automáticos da pessoa
//! e as pendências de eventos em ações (bonificação, desdobramento,
//! grupamento) que a B3 anunciou e o livro ainda não tem. A regra está em
//! `domain::income_auto`; a busca, em `crate::income_auto`.

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::{ApiJson, AppState, CurrentUser};
use crate::domain::income::today_in_brazil;
use crate::domain::income_auto::{PendingStockEvent, StockEventKind};
use crate::error::ApiResult;
use crate::store::IncomeAutoState;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeAutoRequest {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct PendingEventDto {
    pub asset_id: Uuid,
    pub symbol: String,
    pub kind: StockEventKind,
    /// A data com do evento.
    #[serde(with = "iso_date")]
    pub record_date: Date,
    /// Como a B3 publica: percentual na bonificação e no desdobramento (`2` é
    /// 2%, `100` é cada ação virar duas); multiplicador no grupamento (`0.1`
    /// é dez virarem uma).
    #[serde(with = "decimal_out")]
    pub factor: Decimal,
    /// As cotas do livro no fim da data com.
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
}

impl From<&PendingStockEvent> for PendingEventDto {
    fn from(event: &PendingStockEvent) -> Self {
        Self {
            asset_id: event.asset_id,
            symbol: event.symbol.clone(),
            kind: event.kind,
            record_date: event.record_date,
            factor: event.factor,
            quantity: event.quantity,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IncomeAutoDto {
    pub enabled: bool,
    /// A busca mais velha entre os emissores da carteira; `null` antes da primeira.
    #[serde(with = "time::serde::rfc3339::option")]
    pub fetched_at: Option<OffsetDateTime>,
    /// Ativos da B3 que a fonte cobre.
    pub covered: Vec<String>,
    /// Ativos da B3 sem ISIN conhecido (sem anúncio, ou código que a regra não
    /// resolve): ficam sem automático.
    pub unmatched: Vec<String>,
    pub pending_events: Vec<PendingEventDto>,
}

impl From<IncomeAutoState> for IncomeAutoDto {
    fn from(state: IncomeAutoState) -> Self {
        Self {
            enabled: state.enabled,
            fetched_at: state.fetched_at,
            covered: state.covered_symbols,
            unmatched: state.unmatched_symbols,
            pending_events: state.pending.iter().map(PendingEventDto::from).collect(),
        }
    }
}

async fn state_of(state: &AppState, user: Uuid) -> ApiResult<IncomeAutoDto> {
    let today = today_in_brazil(OffsetDateTime::now_utc());
    Ok(state.store.income_auto_state(user, today).await?.into())
}

pub async fn get(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<IncomeAutoDto>> {
    Ok(Json(state_of(&state, user.id).await?))
}

/// Liga ou desliga. Desligar apaga os automáticos; ligar os refaz na hora com
/// o que já está no banco (sem esperar a próxima rodada, e sem ir à B3).
pub async fn save(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<IncomeAutoRequest>,
) -> ApiResult<Json<IncomeAutoDto>> {
    state.store.set_income_auto(user.id, body.enabled).await?;
    if body.enabled {
        state.store.regenerate_income_auto(user.id).await?;
    }
    Ok(Json(state_of(&state, user.id).await?))
}
