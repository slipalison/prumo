//! Yield on cost: o DTO, a montagem a partir da carteira, dos proventos e da
//! estimativa, e `GET /api/v1/income/yield-on-cost`. A conta está em
//! `domain::yoc`.

use std::collections::HashMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use super::dto::decimal_out;
use super::{AppState, CurrentUser};
use crate::domain::income::{summarize, today_in_brazil, Entry, IncomeSummary};
use crate::domain::yoc::{
    asset_yield, portfolio_yield, AssetYield, HeldAsset, PortfolioYield, YieldBasis, YieldGap,
};
use crate::error::ApiResult;
use crate::income::PortfolioIncome;
use crate::service::{self, PortfolioView};
use crate::store::IncomeEvent;

/// O yield on cost de um ativo, ou de um pagador.
#[derive(Debug, Serialize)]
pub struct YieldDto {
    /// A renda de 12 meses usada: a recebida (líquida) ou, sem recebido, a
    /// estimada (bruta); `basis` diz qual.
    #[serde(with = "decimal_out")]
    pub income_12m: Decimal,
    pub basis: YieldBasis,
    /// O custo (preço médio × quantidade) que serviu de base; `null` se desconhecido.
    #[serde(with = "decimal_out::option")]
    pub cost: Option<Decimal>,
    /// `income_12m ÷ cost × 100`; `null` com o motivo em `gap`.
    #[serde(with = "decimal_out::option")]
    pub percent: Option<Decimal>,
    pub gap: Option<YieldGap>,
    /// A frase para mostrar no lugar do percentual (`informe o preco medio`).
    pub message: Option<&'static str>,
}

impl From<&AssetYield> for YieldDto {
    fn from(y: &AssetYield) -> Self {
        Self {
            income_12m: y.income_12m,
            basis: y.basis,
            cost: y.cost,
            percent: y.percent,
            gap: y.gap,
            message: y.gap.map(YieldGap::message),
        }
    }
}

/// O yield on cost da carteira.
#[derive(Debug, Serialize)]
pub struct PortfolioYieldDto {
    #[serde(with = "decimal_out")]
    pub income_12m: Decimal,
    pub basis: YieldBasis,
    /// A soma dos custos dos ativos que entraram na conta.
    #[serde(with = "decimal_out")]
    pub cost: Decimal,
    #[serde(with = "decimal_out::option")]
    pub percent: Option<Decimal>,
    pub gap: Option<YieldGap>,
    pub message: Option<&'static str>,
    /// Algum ativo com posição ficou de fora por custo desconhecido.
    pub partial: bool,
    /// Os ativos que ficaram de fora.
    pub unknown_cost: Vec<String>,
}

impl From<&PortfolioYield> for PortfolioYieldDto {
    fn from(y: &PortfolioYield) -> Self {
        Self {
            income_12m: y.income_12m,
            basis: y.basis,
            cost: y.cost,
            percent: y.percent,
            gap: y.gap,
            message: y.gap.map(YieldGap::message),
            partial: y.partial,
            unknown_cost: y.unknown_cost.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssetYieldDto {
    pub asset_id: Uuid,
    #[serde(flatten)]
    pub yield_on_cost: YieldDto,
}

#[derive(Debug, Serialize)]
pub struct YieldReportDto {
    pub portfolio: PortfolioYieldDto,
    pub assets: Vec<AssetYieldDto>,
}

/// Os yields de cada ativo da carteira e o da carteira, da mesma carteira,
/// dos mesmos proventos e da mesma estimativa.
pub struct YieldReport {
    pub portfolio: PortfolioYield,
    pub assets: Vec<(Uuid, AssetYield)>,
}

impl YieldReport {
    pub fn build(
        view: &PortfolioView,
        summary: &IncomeSummary,
        estimate: &PortfolioIncome,
    ) -> Self {
        let received: HashMap<Uuid, Decimal> = summary
            .by_payer
            .iter()
            .filter_map(|p| Some((p.key.parse().ok()?, p.total)))
            .collect();
        let held: Vec<HeldAsset> = view
            .assets
            .iter()
            .map(|a| HeldAsset {
                asset_id: a.asset.id,
                label: a.label(),
                quantity: a.asset.quantity,
                cost: a.performance.cost_basis,
                received_12m: received.get(&a.asset.id).copied().unwrap_or(Decimal::ZERO),
                estimated_12m: estimate.asset(a.asset.id).and_then(|e| e.trailing_12m),
            })
            .collect();
        let assets = held
            .iter()
            .map(|h| {
                (
                    h.asset_id,
                    asset_yield(h.received_12m, h.estimated_12m, h.quantity, h.cost),
                )
            })
            .collect();
        Self {
            portfolio: portfolio_yield(&held),
            assets,
        }
    }

    pub fn of(&self, asset: Uuid) -> Option<&AssetYield> {
        self.assets
            .iter()
            .find(|(id, _)| *id == asset)
            .map(|(_, y)| y)
    }

    pub fn dto(&self) -> YieldReportDto {
        YieldReportDto {
            portfolio: PortfolioYieldDto::from(&self.portfolio),
            assets: self
                .assets
                .iter()
                .map(|(id, y)| AssetYieldDto {
                    asset_id: *id,
                    yield_on_cost: YieldDto::from(y),
                })
                .collect(),
        }
    }
}

/// `GET /income/yield-on-cost`: o yield de cada ativo (para a folha dele) e o
/// da carteira.
pub async fn show(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<YieldReportDto>> {
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let ledger = state.store.income_ledger(user.id).await?;
    let entries: Vec<Entry> = ledger.iter().map(IncomeEvent::entry).collect();
    let summary = summarize(&entries, today);
    let view = service::portfolio(&state.store, user.id).await?;
    let estimate = state
        .income
        .estimate_with_ledger(user.id, &view, today)
        .await;
    Ok(Json(YieldReport::build(&view, &summary, &estimate).dto()))
}
