//! Yield on cost: quanto a renda dos últimos 12 meses rende **sobre o que a
//! pessoa pagou**, e não sobre o preço de hoje.
//!
//! `yield on cost = renda de 12 meses ÷ custo (cost_basis do livro) × 100`.
//!
//! A renda é a **recebida** (líquida, do livro de proventos). Quando nada foi
//! recebido nos 12 meses, vale a estimativa do histórico por cota, rotulada
//! como tal: o dado do livro sempre vence a estimativa, a mesma regra da meta
//! de renda.
//!
//! Custo desconhecido (saldo inicial sem preço) **não vira zero nem chute**: o
//! yield é nulo, com o motivo, e a tela pede o preço médio. Na carteira, o
//! ativo de custo desconhecido sai da conta inteira (do custo e da renda) e a
//! carteira é marcada como parcial.
//!
//! Limite conhecido: a renda dos 12 meses é dividida pelo custo **de hoje**.
//! Quem comprou mais cotas há poucos meses tem custo cheio e renda de uma
//! posição menor, então o número sai baixo até completar um ano.

use rust_decimal::Decimal;
use uuid::Uuid;

use super::allocation::percent;

/// De onde vem a renda do yield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldBasis {
    /// Proventos recebidos e lançados (ou importados).
    Received,
    /// Estimativa pelo histórico por cota; só quando não há recebido.
    Estimate,
}

/// Por que não há percentual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldGap {
    /// O custo é desconhecido: a pessoa precisa informar o preço médio.
    UnknownCost,
    /// Há posição, mas o custo é zero (só bonificação): não há base.
    ZeroCost,
    /// Sem posição: ativo zerado ou removido.
    NoPosition,
}

impl YieldGap {
    /// A frase que acompanha o motivo na API (sem acento: atravessa log).
    pub fn message(self) -> &'static str {
        match self {
            YieldGap::UnknownCost => "informe o preco medio",
            YieldGap::ZeroCost => "custo zero: sem base para o yield",
            YieldGap::NoPosition => "sem posicao",
        }
    }
}

/// O yield on cost de um ativo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetYield {
    pub income_12m: Decimal,
    pub basis: YieldBasis,
    /// O custo usado, em reais. `None`: desconhecido.
    pub cost: Option<Decimal>,
    pub percent: Option<Decimal>,
    pub gap: Option<YieldGap>,
}

/// O yield on cost de uma posição. `estimated_12m` é a renda estimada em 12
/// meses (bruta), usada só se `received_12m` for zero.
pub fn asset_yield(
    received_12m: Decimal,
    estimated_12m: Option<Decimal>,
    quantity: Decimal,
    cost: Option<Decimal>,
) -> AssetYield {
    let (income_12m, basis) = if received_12m > Decimal::ZERO {
        (received_12m, YieldBasis::Received)
    } else if let Some(estimated) = estimated_12m.filter(|e| *e > Decimal::ZERO) {
        (estimated, YieldBasis::Estimate)
    } else {
        (Decimal::ZERO, YieldBasis::Received)
    };
    let gap = if quantity <= Decimal::ZERO {
        Some(YieldGap::NoPosition)
    } else {
        match cost {
            None => Some(YieldGap::UnknownCost),
            Some(c) if c <= Decimal::ZERO => Some(YieldGap::ZeroCost),
            Some(_) => None,
        }
    };
    let percent = match (gap, cost) {
        (None, Some(cost)) => Some(percent(income_12m, cost)),
        _ => None,
    };
    AssetYield {
        income_12m,
        basis,
        cost,
        percent,
        gap,
    }
}

/// Um ativo em carteira, para o yield da carteira.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldAsset {
    pub asset_id: Uuid,
    pub label: String,
    pub quantity: Decimal,
    pub cost: Option<Decimal>,
    pub received_12m: Decimal,
    pub estimated_12m: Option<Decimal>,
}

/// O yield on cost da carteira.
#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioYield {
    pub income_12m: Decimal,
    pub basis: YieldBasis,
    /// A soma dos custos dos ativos que entraram na conta.
    pub cost: Decimal,
    pub percent: Option<Decimal>,
    pub gap: Option<YieldGap>,
    /// Há ativo com posição e custo desconhecido, fora da conta.
    pub partial: bool,
    /// Quem ficou de fora por custo desconhecido.
    pub unknown_cost: Vec<String>,
}

/// O yield on cost da carteira: a renda dos ativos de custo conhecido sobre o
/// custo deles. Ativo sem posição não entra; ativo de custo desconhecido sai
/// da renda e do custo e marca o resultado como parcial.
pub fn portfolio_yield(assets: &[HeldAsset]) -> PortfolioYield {
    let held: Vec<&HeldAsset> = assets
        .iter()
        .filter(|a| a.quantity > Decimal::ZERO)
        .collect();
    let unknown_cost: Vec<String> = held
        .iter()
        .filter(|a| a.cost.is_none())
        .map(|a| a.label.clone())
        .collect();
    let known: Vec<&&HeldAsset> = held.iter().filter(|a| a.cost.is_some()).collect();
    let cost: Decimal = known.iter().filter_map(|a| a.cost).sum();
    let received: Decimal = known.iter().map(|a| a.received_12m).sum();
    let estimated: Decimal = known.iter().filter_map(|a| a.estimated_12m).sum();
    let (income_12m, basis) = if received > Decimal::ZERO {
        (received, YieldBasis::Received)
    } else if estimated > Decimal::ZERO {
        (estimated, YieldBasis::Estimate)
    } else {
        (Decimal::ZERO, YieldBasis::Received)
    };
    let gap = if cost > Decimal::ZERO {
        None
    } else if !unknown_cost.is_empty() {
        Some(YieldGap::UnknownCost)
    } else if held.is_empty() {
        Some(YieldGap::NoPosition)
    } else {
        Some(YieldGap::ZeroCost)
    };
    PortfolioYield {
        income_12m,
        basis,
        cost,
        percent: gap.is_none().then(|| percent(income_12m, cost)),
        gap,
        partial: !unknown_cost.is_empty(),
        unknown_cost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    #[test]
    fn yield_is_income_over_cost() {
        let y = asset_yield(dec!(120), None, dec!(10), Some(dec!(1000)));
        assert_eq!(y.percent, Some(dec!(12)));
        assert_eq!(y.basis, YieldBasis::Received);
        assert_eq!(y.gap, None);
        assert_eq!(y.cost, Some(dec!(1000)));
    }

    #[test]
    fn unknown_cost_gives_no_percentage_and_says_why() {
        let y = asset_yield(dec!(120), None, dec!(10), None);
        assert_eq!(y.percent, None);
        assert_eq!(y.gap, Some(YieldGap::UnknownCost));
        assert_eq!(YieldGap::UnknownCost.message(), "informe o preco medio");
        // A renda continua à vista: só o percentual falta.
        assert_eq!(y.income_12m, dec!(120));
    }

    #[test]
    fn zero_cost_and_no_position_have_no_base() {
        let zero = asset_yield(dec!(5), None, dec!(3), Some(dec!(0)));
        assert_eq!((zero.percent, zero.gap), (None, Some(YieldGap::ZeroCost)));
        let gone = asset_yield(dec!(5), None, dec!(0), Some(dec!(0)));
        assert_eq!((gone.percent, gone.gap), (None, Some(YieldGap::NoPosition)));
    }

    #[test]
    fn received_beats_the_estimate_and_the_estimate_fills_in_when_nothing_was_received() {
        let received = asset_yield(dec!(100), Some(dec!(900)), dec!(1), Some(dec!(1000)));
        assert_eq!(
            (received.income_12m, received.basis),
            (dec!(100), YieldBasis::Received)
        );
        let estimate = asset_yield(dec!(0), Some(dec!(90)), dec!(1), Some(dec!(1000)));
        assert_eq!(
            (estimate.income_12m, estimate.basis, estimate.percent),
            (dec!(90), YieldBasis::Estimate, Some(dec!(9)))
        );
        let nothing = asset_yield(dec!(0), None, dec!(1), Some(dec!(1000)));
        assert_eq!(
            (nothing.income_12m, nothing.percent),
            (dec!(0), Some(dec!(0)))
        );
    }

    fn held(label: &str, quantity: Decimal, cost: Option<Decimal>, received: Decimal) -> HeldAsset {
        HeldAsset {
            asset_id: Uuid::new_v4(),
            label: label.to_owned(),
            quantity,
            cost,
            received_12m: received,
            estimated_12m: None,
        }
    }

    #[test]
    fn portfolio_yield_sums_income_and_cost_of_the_known_assets() {
        let y = portfolio_yield(&[
            held("AAA", dec!(10), Some(dec!(1000)), dec!(80)),
            held("BBB", dec!(5), Some(dec!(500)), dec!(20)),
            // Sem renda, mas com custo: puxa o yield para baixo, como deve.
            held("CCC", dec!(1), Some(dec!(500)), dec!(0)),
        ]);
        assert_eq!(y.income_12m, dec!(100));
        assert_eq!(y.cost, dec!(2000));
        assert_eq!(y.percent, Some(dec!(5)));
        assert!(!y.partial);
    }

    #[test]
    fn unknown_cost_asset_leaves_the_portfolio_yield_partial() {
        let y = portfolio_yield(&[
            held("AAA", dec!(10), Some(dec!(1000)), dec!(100)),
            held("BBB", dec!(5), None, dec!(300)),
            held("OLD", dec!(0), Some(dec!(0)), dec!(50)),
        ]);
        // BBB sai da conta (renda e custo); OLD não tem posição.
        assert_eq!(y.income_12m, dec!(100));
        assert_eq!(y.percent, Some(dec!(10)));
        assert!(y.partial);
        assert_eq!(y.unknown_cost, vec!["BBB".to_owned()]);
    }

    #[test]
    fn portfolio_with_only_unknown_costs_has_no_percentage() {
        let y = portfolio_yield(&[held("BBB", dec!(5), None, dec!(300))]);
        assert_eq!(y.percent, None);
        assert_eq!(y.gap, Some(YieldGap::UnknownCost));
        assert!(y.partial);
        let empty = portfolio_yield(&[]);
        assert_eq!(empty.gap, Some(YieldGap::NoPosition));
        assert!(!empty.partial);
    }

    proptest! {
        #[test]
        fn doubling_the_cost_halves_the_yield(
            income in 1u32..1_000_000,
            cost in 1u32..1_000_000,
        ) {
            let income = Decimal::from(income);
            let cost = Decimal::from(cost);
            let one = asset_yield(income, None, Decimal::ONE, Some(cost));
            let two = asset_yield(income, None, Decimal::ONE, Some(cost * Decimal::TWO));
            let (Some(one), Some(two)) = (one.percent, two.percent) else {
                return Err(TestCaseError::fail("yield esperado"));
            };
            // Arredondamento a centésimo: a metade vale até 1 centésimo de diferença.
            prop_assert!((one / Decimal::TWO - two).abs() <= dec!(0.01));
        }

        #[test]
        fn yield_is_never_negative(income in 0u32..1_000_000, cost in 1u32..1_000_000) {
            let y = asset_yield(Decimal::from(income), None, Decimal::ONE, Some(Decimal::from(cost)));
            prop_assert!(y.percent.is_some_and(|p| p >= Decimal::ZERO));
        }
    }
}
