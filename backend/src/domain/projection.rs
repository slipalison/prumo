//! Projeção por cenários: a renda dos próximos 12 a 24 meses e a data dos
//! objetivos. Regra pura, sem banco, sem rede e sem relógio: quem chama passa a
//! data de hoje, as expectativas do Focus que estão no banco e as premissas.
//! O método inteiro, com as fórmulas e os limites, está em `docs/projecao.md`.
//!
//! - [`rates`]: o caminho mês a mês do CDI e do IPCA esperados, montado das
//!   medianas do Focus (Banco Central).
//! - [`income`]: a renda projetada por ativo e por tipo (FII de papel com o
//!   CDI, FII de tijolo com o IPCA, o resto pela média do que pagou).
//! - [`goal`]: a simulação mês a mês dos objetivos (patrimônio, classe e renda
//!   mensal), em reais de hoje, com a data em cada cenário.
//! - [`progress`]: o progresso da meta de renda na média e no piso (o menor
//!   mês de 12).
//!
//! **Cenário é conta, não promessa.** Os três cenários usam as mesmas
//! premissas; só a [`Scenario::shift`] muda: o pessimista tira `amplitude`
//! pontos por ano do que faz a projeção crescer, o otimista soma. A amplitude
//! é uma premissa como as outras — à vista e editável.

pub mod goal;
pub mod income;
pub mod progress;
pub mod rates;

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::Serialize;

/// Amplitude padrão dos cenários: 2 pontos percentuais ao ano para cada lado.
pub const DEFAULT_SPREAD_PERCENT: Decimal = Decimal::from_parts(2, 0, 0, false, 0);

/// Os três cenários, do mais baixo ao mais alto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Pessimistic,
    Base,
    Optimistic,
}

impl Scenario {
    pub const ALL: [Scenario; 3] = [Scenario::Pessimistic, Scenario::Base, Scenario::Optimistic];

    /// Quanto o cenário soma às taxas anuais do base, em fração: `−amplitude`,
    /// zero ou `+amplitude`.
    pub fn shift(self, spread: f64) -> f64 {
        match self {
            Scenario::Pessimistic => -spread,
            Scenario::Base => 0.0,
            Scenario::Optimistic => spread,
        }
    }
}

/// A taxa mensal equivalente a uma anual (frações). Um ano que perde mais de
/// 100% não existe: no limite, a taxa mensal é −100%.
pub fn monthly_rate(annual: f64) -> f64 {
    (1.0 + annual).max(0.0).powf(1.0 / 12.0) - 1.0
}

/// `(1 + anual)^(meses/12)`: o fator de `months` meses a uma taxa anual.
pub fn growth(annual: f64, months: f64) -> f64 {
    (1.0 + annual).max(0.0).powf(months / 12.0)
}

pub(crate) fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

/// De volta a `Decimal`, com `places` casas. Número que não é número vira zero.
pub(crate) fn to_decimal(value: f64, places: u32) -> Decimal {
    if !value.is_finite() {
        return Decimal::ZERO;
    }
    Decimal::from_f64(value).map_or(Decimal::ZERO, |d| d.round_dp(places))
}

/// Percentual → fração (`4.5` → `0.045`).
pub(crate) fn fraction(percent: Decimal) -> f64 {
    to_f64(percent) / 100.0
}

/// Fração → percentual com duas casas.
pub(crate) fn percent(fraction: f64) -> Decimal {
    to_decimal(fraction * 100.0, 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn scenarios_shift_both_ways() {
        assert_eq!(Scenario::Pessimistic.shift(0.02), -0.02);
        assert_eq!(Scenario::Base.shift(0.02), 0.0);
        assert_eq!(Scenario::Optimistic.shift(0.02), 0.02);
        assert!(Scenario::Pessimistic < Scenario::Optimistic);
    }

    #[test]
    fn monthly_rate_compounds_back_to_the_year() {
        let m = monthly_rate(0.12);
        assert!(((1.0 + m).powi(12) - 1.12).abs() < 1e-12);
        assert_eq!(monthly_rate(-1.5), -1.0);
        assert!((growth(0.1, 24.0) - 1.21).abs() < 1e-12);
        assert_eq!(growth(-2.0, 12.0), 0.0);
    }

    #[test]
    fn conversions_round_and_never_panic() {
        assert_eq!(to_decimal(1.005_001, 2), dec!(1.01));
        assert_eq!(to_decimal(f64::NAN, 2), dec!(0));
        assert_eq!(to_decimal(f64::INFINITY, 2), dec!(0));
        assert_eq!(fraction(dec!(4.5)), 0.045);
        assert_eq!(percent(0.04567), dec!(4.57));
    }
}
