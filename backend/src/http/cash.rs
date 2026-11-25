//! `GET /api/v1/cash`: o caixa da pessoa — o que os proventos e as vendas
//! deixaram e ainda não foi aportado. A regra está em `domain::cash`; o campo
//! do aporte oferece o saldo ("+ R$ X em caixa — incluir?") e o motor não muda.

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, OffsetDateTime};

use super::dto::decimal_out;
use super::{AppState, CurrentUser};
use crate::domain::cash::MovementKind;
use crate::domain::income::today_in_brazil;
use crate::error::ApiResult;
use crate::store::CashReport;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// Quanto entrou de uma fonte e em quantos pagamentos.
#[derive(Debug, Serialize)]
pub struct CashSourceDto {
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct CashMovementDto {
    #[serde(with = "iso_date")]
    pub date: Date,
    pub kind: MovementKind,
    /// O ativo (símbolo ou nome) ou "Aporte".
    pub label: String,
    /// Sempre positivo; o `kind` diz se entrou ou saiu.
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct CashDto {
    /// O que há para aportar. Nunca negativo.
    #[serde(with = "decimal_out")]
    pub balance: Decimal,
    /// O dia (Brasília) em que o caixa começa a contar.
    #[serde(with = "iso_date")]
    pub since: Date,
    /// Proventos líquidos recebidos desde `since`.
    pub income: CashSourceDto,
    /// Líquido das vendas e resgates do livro desde `since`.
    pub sales: CashSourceDto,
    /// O que os aportes já tiraram do caixa.
    #[serde(with = "decimal_out")]
    pub used: Decimal,
    /// Saída que passou do saldo e foi cortada em zero (normalmente `0`):
    /// `balance = income + sales − used + adjustment`.
    #[serde(with = "decimal_out")]
    pub adjustment: Decimal,
    /// Os movimentos mais novos, do mais novo para o mais velho.
    pub recent: Vec<CashMovementDto>,
}

impl From<&CashReport> for CashDto {
    fn from(report: &CashReport) -> Self {
        let state = &report.state;
        Self {
            balance: state.balance,
            since: report.since,
            income: CashSourceDto {
                amount: state.income_in,
                count: state.income_count,
            },
            sales: CashSourceDto {
                amount: state.sales_in,
                count: state.sales_count,
            },
            used: state.contributions_out,
            adjustment: state.adjustment,
            recent: report
                .recent
                .iter()
                .map(|m| CashMovementDto {
                    date: m.date,
                    kind: m.kind,
                    label: m.label.clone(),
                    amount: m.amount,
                })
                .collect(),
        }
    }
}

pub async fn show(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<CashDto>> {
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let report = state.store.cash(user.id, today).await?;
    Ok(Json(CashDto::from(&report)))
}
