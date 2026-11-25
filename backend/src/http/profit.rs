//! O lucro total no patrimônio da carteira: ganho de capital (realizado e não
//! realizado) mais os proventos recebidos. A conta está em `domain::profit`; o
//! bloco vai no `GET /portfolio` e no evento `prices` do stream, porque o ganho
//! não realizado muda com o preço.

use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use super::dto::decimal_out;
use crate::domain::profit::{PartialReason, TotalProfit};

#[derive(Debug, Serialize, PartialEq)]
pub struct PartialAssetDto {
    pub asset_id: Uuid,
    pub label: String,
    pub reason: PartialReason,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProfitDto {
    /// Ganho não realizado dos ativos de custo e preço conhecidos.
    #[serde(with = "decimal_out")]
    pub unrealized: Decimal,
    /// Lucro realizado das vendas de custo conhecido.
    #[serde(with = "decimal_out")]
    pub realized: Decimal,
    /// Proventos líquidos já recebidos (pagamento até hoje).
    #[serde(with = "decimal_out")]
    pub income_received: Decimal,
    /// `realized + unrealized + income_received`, só do que é conhecido.
    #[serde(with = "decimal_out")]
    pub total: Decimal,
    /// Algum ativo ficou fora, no todo ou em parte (custo desconhecido ou sem cotação).
    pub partial: bool,
    pub partial_assets: Vec<PartialAssetDto>,
    /// Há venda de ativo já removido da carteira, que o total não enxerga.
    pub removed_assets: bool,
}

impl From<&TotalProfit> for ProfitDto {
    fn from(p: &TotalProfit) -> Self {
        Self {
            unrealized: p.unrealized,
            realized: p.realized,
            income_received: p.income_received,
            total: p.total,
            partial: p.partial,
            partial_assets: p
                .partial_assets
                .iter()
                .map(|a| PartialAssetDto {
                    asset_id: a.asset_id,
                    label: a.label.clone(),
                    reason: a.reason,
                })
                .collect(),
            removed_assets: p.removed_assets,
        }
    }
}
