//! Lucro total da carteira: ganho de capital (realizado + não realizado) mais
//! os proventos recebidos.
//!
//! `lucro total = realizado + não realizado + proventos recebidos`
//!
//! O que o Prumo não sabe **fica de fora e é dito**: ativo com custo
//! desconhecido (saldo inicial sem preço médio), venda feita sobre custo
//! desconhecido e ativo sem cotação não entram na soma, e o resultado vem
//! marcado como parcial com a lista de quem faltou. Nunca vira zero, nem
//! estimativa.
//!
//! Limite conhecido: o ganho realizado de um ativo **removido** da carteira se
//! perde junto com ele (o lançamento fica no livro, sem o ativo para refazer a
//! posição); `removed_assets` avisa quando há venda assim. Os proventos
//! recebidos de ativo removido continuam no total: o evento guarda o que
//! foi pago.

use rust_decimal::Decimal;
use uuid::Uuid;

use super::ledger::Performance;

/// Por que um ativo não entrou inteiro no lucro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartialReason {
    /// A posição tem custo desconhecido: falta o preço médio.
    UnknownCost,
    /// Uma venda saiu de custo desconhecido: o lucro realizado é desconhecido.
    UnknownRealized,
    /// Sem cotação: o valor de hoje, e o ganho não realizado, faltam.
    MissingPrice,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartialAsset {
    pub asset_id: Uuid,
    pub label: String,
    pub reason: PartialReason,
}

/// Um ativo, para a soma do lucro.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetProfit {
    pub asset_id: Uuid,
    pub label: String,
    pub quantity: Decimal,
    pub performance: Performance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TotalProfit {
    /// Ganho não realizado dos ativos de custo e preço conhecidos.
    pub unrealized: Decimal,
    /// Lucro realizado das vendas de custo conhecido.
    pub realized: Decimal,
    pub income_received: Decimal,
    /// `unrealized + realized + income_received`, só do que é conhecido.
    pub total: Decimal,
    /// Algum ativo ficou fora, no todo ou em parte.
    pub partial: bool,
    pub partial_assets: Vec<PartialAsset>,
    /// Há venda de ativo já removido da carteira, que a soma não enxerga.
    pub removed_assets: bool,
}

/// Soma o lucro. Cada ativo entra com o que é conhecido; o que não é vai para
/// `partial_assets`, uma vez por ativo (o primeiro motivo).
pub fn total_profit(
    assets: &[AssetProfit],
    income_received: Decimal,
    removed_assets: bool,
) -> TotalProfit {
    let mut unrealized = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut partial_assets = Vec::new();
    for asset in assets {
        let p = &asset.performance;
        let held = asset.quantity > Decimal::ZERO;
        let mut reason = None;
        if let Some(gain) = p.realized_gain {
            realized += gain;
        } else {
            reason = Some(PartialReason::UnknownRealized);
        }
        if held {
            match (p.cost_basis, p.unrealized_gain) {
                (Some(_), Some(gain)) => unrealized += gain,
                (None, _) => reason = Some(PartialReason::UnknownCost),
                (Some(_), None) => {
                    reason.get_or_insert(PartialReason::MissingPrice);
                }
            }
        }
        if let Some(reason) = reason {
            partial_assets.push(PartialAsset {
                asset_id: asset.asset_id,
                label: asset.label.clone(),
                reason,
            });
        }
    }
    TotalProfit {
        unrealized,
        realized,
        income_received,
        total: unrealized + realized + income_received,
        partial: !partial_assets.is_empty(),
        partial_assets,
        removed_assets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    fn asset(
        label: &str,
        quantity: Decimal,
        cost: Option<Decimal>,
        unrealized: Option<Decimal>,
        realized: Option<Decimal>,
    ) -> AssetProfit {
        AssetProfit {
            asset_id: Uuid::new_v4(),
            label: label.to_owned(),
            quantity,
            performance: Performance {
                cost_basis: cost,
                unrealized_gain: unrealized,
                realized_gain: realized,
                ..Performance::default()
            },
        }
    }

    #[test]
    fn total_is_realized_plus_unrealized_plus_income() {
        let profit = total_profit(
            &[
                asset(
                    "AAA",
                    dec!(10),
                    Some(dec!(1000)),
                    Some(dec!(150)),
                    Some(dec!(40)),
                ),
                asset(
                    "BBB",
                    dec!(5),
                    Some(dec!(500)),
                    Some(dec!(-30)),
                    Some(dec!(0)),
                ),
                // Vendido por inteiro: só o realizado.
                asset("OLD", dec!(0), Some(dec!(0)), Some(dec!(0)), Some(dec!(25))),
            ],
            dec!(80),
            false,
        );
        assert_eq!(profit.unrealized, dec!(120));
        assert_eq!(profit.realized, dec!(65));
        assert_eq!(profit.income_received, dec!(80));
        assert_eq!(profit.total, dec!(265));
        assert!(!profit.partial);
        assert!(profit.partial_assets.is_empty());
    }

    #[test]
    fn unknown_cost_is_left_out_and_flagged_not_invented() {
        let profit = total_profit(
            &[
                asset(
                    "AAA",
                    dec!(10),
                    Some(dec!(1000)),
                    Some(dec!(100)),
                    Some(dec!(0)),
                ),
                asset("BBB", dec!(5), None, None, Some(dec!(0))),
            ],
            dec!(10),
            false,
        );
        assert_eq!(profit.total, dec!(110));
        assert!(profit.partial);
        assert_eq!(profit.partial_assets.len(), 1);
        assert_eq!(profit.partial_assets[0].label, "BBB");
        assert_eq!(profit.partial_assets[0].reason, PartialReason::UnknownCost);
    }

    #[test]
    fn sale_from_unknown_cost_and_missing_price_are_flagged() {
        let profit = total_profit(
            &[
                // Zerou depois de vender de custo desconhecido.
                asset("SOLD", dec!(0), Some(dec!(0)), Some(dec!(0)), None),
                asset("NOQUOTE", dec!(3), Some(dec!(300)), None, Some(dec!(0))),
            ],
            dec!(0),
            false,
        );
        assert_eq!(profit.total, dec!(0));
        let reasons: Vec<_> = profit.partial_assets.iter().map(|a| a.reason).collect();
        assert_eq!(
            reasons,
            vec![PartialReason::UnknownRealized, PartialReason::MissingPrice]
        );
    }

    #[test]
    fn a_loss_makes_the_total_negative() {
        let profit = total_profit(
            &[asset(
                "AAA",
                dec!(10),
                Some(dec!(1000)),
                Some(dec!(-400)),
                Some(dec!(0)),
            )],
            dec!(100),
            true,
        );
        assert_eq!(profit.total, dec!(-300));
        assert!(profit.removed_assets);
    }

    #[test]
    fn income_alone_is_a_profit_even_with_no_assets() {
        let profit = total_profit(&[], dec!(42), false);
        assert_eq!(profit.total, dec!(42));
        assert!(!profit.partial);
    }

    proptest! {
        #[test]
        fn total_is_always_the_sum_of_its_parts(
            gains in prop::collection::vec((-100_000i64..100_000, -100_000i64..100_000), 0..12),
            income in 0i64..1_000_000,
        ) {
            let assets: Vec<AssetProfit> = gains
                .iter()
                .map(|(u, r)| asset("X", Decimal::ONE, Some(Decimal::ONE), Some(Decimal::new(*u, 2)), Some(Decimal::new(*r, 2))))
                .collect();
            let profit = total_profit(&assets, Decimal::new(income, 2), false);
            prop_assert_eq!(profit.total, profit.unrealized + profit.realized + profit.income_received);
            prop_assert!(!profit.partial);
        }
    }
}
