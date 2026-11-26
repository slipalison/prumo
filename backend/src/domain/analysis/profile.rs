//! Perfil de risco: onde a carteira (e a meta da pessoa) está entre renda fixa e
//! renda variável, e o rótulo que isso merece.
//!
//! Os limiares são **do Prumo**, simples e escritos aqui (e em
//! `docs/analise.md`): quanto mais da carteira está em renda variável, mais
//! arrojado o perfil.
//!
//! - **Conservador**: renda variável até 30%.
//! - **Moderado**: de 30% (exclusive) a 70%.
//! - **Arrojado**: acima de 70%.
//!
//! "Renda fixa" são as classes Renda Fixa e Renda Fixa Internacional; todo o
//! resto (ações, FIIs, REITs, exterior, cripto) é renda variável.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::domain::allocation::{percent, Targets};
use crate::domain::asset_class::AssetClass;

/// Até aqui (inclusive), conservador.
pub const CONSERVATIVE_MAX: Decimal = Decimal::from_parts(30, 0, 0, false, 0);
/// Até aqui (inclusive), moderado; acima, arrojado.
pub const MODERATE_MAX: Decimal = Decimal::from_parts(70, 0, 0, false, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Conservative,
    Moderate,
    Aggressive,
}

/// A mistura entre renda fixa e renda variável, em percentual; soma 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mix {
    pub fixed_income: Decimal,
    pub variable: Decimal,
    pub profile: Profile,
}

/// Renda fixa é a classe de baixo risco do Prumo.
pub fn is_fixed_income(class: AssetClass) -> bool {
    matches!(class, AssetClass::FixedIncome | AssetClass::IntlFixedIncome)
}

/// O rótulo de um percentual de renda variável.
pub fn classify(variable_percent: Decimal) -> Profile {
    if variable_percent <= CONSERVATIVE_MAX {
        Profile::Conservative
    } else if variable_percent <= MODERATE_MAX {
        Profile::Moderate
    } else {
        Profile::Aggressive
    }
}

fn mix(fixed: Decimal, total: Decimal) -> Option<Mix> {
    if total <= Decimal::ZERO {
        return None;
    }
    let fixed_income = percent(fixed, total);
    let variable = Decimal::ONE_HUNDRED - fixed_income;
    Some(Mix {
        fixed_income,
        variable,
        profile: classify(variable),
    })
}

/// A carteira de hoje: `(classe, valor em reais)` de cada ativo. `None` sem
/// valor nenhum.
pub fn current(holdings: impl IntoIterator<Item = (AssetClass, Decimal)>) -> Option<Mix> {
    let (mut fixed, mut total) = (Decimal::ZERO, Decimal::ZERO);
    for (class, value) in holdings.into_iter().filter(|(_, v)| *v > Decimal::ZERO) {
        total += value;
        if is_fixed_income(class) {
            fixed += value;
        }
    }
    mix(fixed, total)
}

/// O que a pessoa quer: as metas por classe (pontos-base). `None` sem meta.
pub fn target(targets: Targets) -> Option<Mix> {
    let fixed: u32 = AssetClass::ALL
        .into_iter()
        .filter(|c| is_fixed_income(*c))
        .map(|c| targets.get(c))
        .sum();
    mix(Decimal::from(fixed), Decimal::from(targets.sum()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;

    #[test]
    fn the_labels_follow_the_documented_thresholds() {
        assert_eq!(classify(dec!(0)), Profile::Conservative);
        assert_eq!(classify(dec!(30)), Profile::Conservative);
        assert_eq!(classify(dec!(30.01)), Profile::Moderate);
        assert_eq!(classify(dec!(70)), Profile::Moderate);
        assert_eq!(classify(dec!(70.01)), Profile::Aggressive);
        assert_eq!(classify(dec!(100)), Profile::Aggressive);
    }

    #[test]
    fn current_splits_fixed_income_from_the_rest() {
        let mix = current([
            (AssetClass::BrStocks, dec!(300)),
            (AssetClass::FixedIncome, dec!(500)),
            (AssetClass::IntlFixedIncome, dec!(100)),
            (AssetClass::Crypto, dec!(100)),
            // Sem valor, não entra.
            (AssetClass::BrReits, dec!(0)),
        ])
        .unwrap();
        assert_eq!(mix.fixed_income, dec!(60));
        assert_eq!(mix.variable, dec!(40));
        assert_eq!(mix.profile, Profile::Moderate);
    }

    #[test]
    fn an_empty_portfolio_has_no_profile() {
        assert_eq!(current([]), None);
        assert_eq!(current([(AssetClass::BrStocks, dec!(0))]), None);
        assert_eq!(target(Targets::default()), None);
    }

    #[test]
    fn the_target_is_read_from_the_class_goals() {
        let mut goals = Targets::default();
        goals.set(AssetClass::FixedIncome, 2000);
        goals.set(AssetClass::BrStocks, 5000);
        goals.set(AssetClass::BrReits, 3000);
        let mix = target(goals).unwrap();
        assert_eq!(mix.fixed_income, dec!(20));
        assert_eq!(mix.variable, dec!(80));
        assert_eq!(mix.profile, Profile::Aggressive);
        // Só renda fixa: conservador.
        let mut only = Targets::default();
        only.set(AssetClass::IntlFixedIncome, 10_000);
        assert_eq!(target(only).unwrap().profile, Profile::Conservative);
    }

    proptest! {
        /// A mistura sempre soma 100 e o rótulo acompanha a renda variável.
        #[test]
        fn the_mix_always_sums_to_one_hundred(
            values in proptest::collection::vec((0usize..7, 1i64..1_000_000), 1..12)
        ) {
            let holdings = values.iter().map(|(c, v)| (AssetClass::ALL[*c], Decimal::new(*v, 2)));
            let mix = current(holdings).unwrap();
            prop_assert_eq!(mix.fixed_income + mix.variable, Decimal::ONE_HUNDRED);
            prop_assert_eq!(mix.profile, classify(mix.variable));
            prop_assert!(mix.fixed_income >= Decimal::ZERO && mix.variable >= Decimal::ZERO);
        }
    }
}
