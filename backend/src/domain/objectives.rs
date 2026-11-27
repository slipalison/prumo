//! Objetivos com prazo: quanto falta, a média de aporte do livro e as
//! premissas padrão da projeção. Regra pura, sem banco nem relógio: quem chama
//! passa a data de hoje.
//!
//! A conta da data estimada (mês a mês, em reais de hoje, em três cenários)
//! está em `domain::projection::goal`; o método, em `docs/projecao.md`. Aqui
//! ficam o que ela usa: o estado da data ([`Eta`]), o rótulo do mês, o
//! progresso e de onde vem cada premissa padrão.

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, Month};

use super::performance::months_before;

/// O prazo máximo que a projeção procura: 100 anos. Passou disso, a data não
/// diz nada.
pub const MAX_MONTHS: u32 = 1200;

/// Janela do aporte médio, em meses.
pub const CONTRIBUTION_WINDOW_MONTHS: u32 = 12;

/// Menor janela da série do Desempenho que serve de premissa: seis meses.
/// Com menos que isso a rentabilidade anualizada é ruído.
pub const MIN_RETURN_WINDOW_DAYS: i64 = 180;

/// Padrão sem Desempenho nem CDI: 4% ao ano **acima da inflação**. Só aparece
/// quando o banco não tem série da carteira de seis meses nem CDI; a tela o
/// rotula como "padrão do Prumo" e a pessoa pode trocar. É uma convenção, não
/// uma previsão.
pub const FALLBACK_REAL_RETURN: Decimal = Decimal::from_parts(4, 0, 0, false, 2);

/// IR sobre renda fixa de longo prazo (mais de 720 dias): a menor alíquota da
/// tabela regressiva. O padrão da premissa desconta este valor do CDI.
pub const CDI_TAX: Decimal = Decimal::from_parts(15, 0, 0, false, 2);

/// Dias úteis do ano na capitalização do CDI.
const BUSINESS_DAYS_PER_YEAR: f64 = 252.0;

/// Atingido: o valor chegou à meta.
pub fn is_reached(current: Decimal, target: Decimal) -> bool {
    target > Decimal::ZERO && current >= target
}

/// Quanto da meta já foi, de 0 a 100 (passou da meta, 100), com duas casas.
pub fn percent_done(current: Decimal, target: Decimal) -> Decimal {
    if target <= Decimal::ZERO || current <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (current / target * Decimal::ONE_HUNDRED)
        .min(Decimal::ONE_HUNDRED)
        .round_dp(2)
}

/// O que a projeção concluiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eta {
    /// O valor já está na meta.
    Reached,
    /// A meta chega daqui a tantos meses (pelo menos 1).
    In(u32),
    /// Nada faz a conta andar em direção à meta (sem aporte, sem rendimento,
    /// sem crescimento): nunca chega.
    Unreachable,
    /// Chega, mas depois de [`MAX_MONTHS`].
    Beyond,
}

fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

/// O ano e o mês `months` meses depois de `today` (mês de calendário).
pub fn month_after(today: Date, months: u32) -> (i32, Month) {
    let total = today.year() * 12 + i32::from(u8::from(today.month())) - 1
        + i32::try_from(months).unwrap_or(i32::MAX / 2);
    let month = u8::try_from(total.rem_euclid(12) + 1).unwrap_or(1);
    (
        total.div_euclid(12),
        Month::try_from(month).unwrap_or(Month::January),
    )
}

/// `2031-03`, o mês estimado de conclusão.
pub fn month_label(today: Date, months: u32) -> String {
    let (year, month) = month_after(today, months);
    format!("{year:04}-{:02}", u8::from(month))
}

/// A média mensal do que foi aportado.
///
/// `buys` são as compras em reais, datadas; `first_buy` é a data da primeira
/// compra de todas. Só entram as compras dos últimos 12 meses. A janela é a
/// menor entre 12 meses e o tempo desde a primeira compra, em meses de 30 dias
/// arredondados para cima (no mínimo um): quem começou há quatro meses é
/// medido por quatro, não por doze.
pub fn average_monthly_contribution(
    buys: &[(Date, Decimal)],
    first_buy: Option<Date>,
    today: Date,
) -> Decimal {
    let Some(first) = first_buy else {
        return Decimal::ZERO;
    };
    let window_start = months_before(today, CONTRIBUTION_WINDOW_MONTHS);
    let start = first.max(window_start);
    let days = (today - start).whole_days().max(0) + 1;
    let months = ((days + 29) / 30).clamp(1, i64::from(CONTRIBUTION_WINDOW_MONTHS));
    let total: Decimal = buys
        .iter()
        .filter(|(date, _)| *date >= window_start && *date <= today)
        .map(|(_, value)| *value)
        .sum();
    (total / Decimal::from(months)).round_dp(2)
}

/// De onde veio a rentabilidade real usada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnSource {
    /// A pessoa informou no objetivo.
    Informed,
    /// A rentabilidade da carteira na tela Desempenho, anualizada e
    /// descontado o IPCA do mesmo período.
    Performance,
    /// O CDI de hoje menos o IR de renda fixa de longo prazo, descontada a
    /// inflação esperada.
    CdiNet,
    /// O padrão fixo do Prumo ([`FALLBACK_REAL_RETURN`]).
    Fallback,
}

/// De onde veio o aporte mensal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionSource {
    /// A pessoa informou no objetivo.
    Informed,
    /// A média do livro nos últimos 12 meses.
    Ledger,
}

/// Rentabilidade anual (fração) de uma janela medida: `ret` rendeu em `days`
/// dias. `None` se a janela é curta demais ou o retorno não é um número usável.
pub fn annualize(ret: Decimal, days: i64) -> Option<Decimal> {
    if days < MIN_RETURN_WINDOW_DAYS || ret <= -Decimal::ONE {
        return None;
    }
    let annual = (1.0 + to_f64(ret)).powf(365.0 / days as f64) - 1.0;
    if !annual.is_finite() {
        return None;
    }
    Decimal::from_f64(annual).map(|d| d.round_dp(6))
}

/// O CDI de hoje ao ano, bruto: `daily_percent` é a taxa de um dia útil em %,
/// como o Banco Central publica, composta por 252 dias úteis.
pub fn cdi_annual(daily_percent: Decimal) -> Option<Decimal> {
    let daily = to_f64(daily_percent) / 100.0;
    if daily <= -1.0 {
        return None;
    }
    let gross = (1.0 + daily).powf(BUSINESS_DAYS_PER_YEAR) - 1.0;
    if !gross.is_finite() {
        return None;
    }
    Decimal::from_f64(gross).map(|g| g.round_dp(6))
}

/// O CDI de hoje, líquido do IR de renda fixa de longo prazo ([`CDI_TAX`]).
pub fn cdi_net_annual(daily_percent: Decimal) -> Option<Decimal> {
    cdi_annual(daily_percent).map(|gross| (gross * (Decimal::ONE - CDI_TAX)).round_dp(6))
}

/// Uma taxa nominal descontada a inflação (frações): `(1 + n) / (1 + i) − 1`.
/// `None` com inflação de −100% ou menos.
pub fn deflate(nominal: Decimal, inflation: Decimal) -> Option<Decimal> {
    let base = Decimal::ONE + inflation;
    if base <= Decimal::ZERO {
        return None;
    }
    Some(((Decimal::ONE + nominal) / base - Decimal::ONE).round_dp(6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use time::Duration;

    #[test]
    fn reached_needs_a_positive_target() {
        assert!(is_reached(dec!(100), dec!(100)));
        assert!(is_reached(dec!(150), dec!(100)));
        assert!(!is_reached(dec!(99.99), dec!(100)));
        assert!(!is_reached(dec!(10), dec!(0)));
    }

    #[test]
    fn labels_cross_the_year() {
        assert_eq!(month_label(date!(2026 - 11 - 25), 0), "2026-11");
        assert_eq!(month_label(date!(2026 - 11 - 25), 1), "2026-12");
        assert_eq!(month_label(date!(2026 - 11 - 25), 2), "2027-01");
        assert_eq!(month_label(date!(2026 - 11 - 25), 52), "2031-03");
        assert_eq!(month_label(date!(2026 - 01 - 31), 24), "2028-01");
    }

    #[test]
    fn percent_is_capped_and_never_negative() {
        assert_eq!(percent_done(dec!(250), dec!(1000)), dec!(25));
        assert_eq!(percent_done(dec!(1500), dec!(1000)), dec!(100));
        assert_eq!(percent_done(dec!(-5), dec!(1000)), dec!(0));
        assert_eq!(percent_done(dec!(1), dec!(3)), dec!(33.33));
        assert_eq!(percent_done(dec!(1), dec!(0)), dec!(0));
    }

    #[test]
    fn average_uses_the_months_since_the_first_buy() {
        let today = date!(2026 - 11 - 25);
        let buys = [
            (date!(2026 - 08 - 10), dec!(1000)),
            (date!(2026 - 09 - 10), dec!(1000)),
            (date!(2026 - 10 - 10), dec!(1000)),
            (date!(2026 - 11 - 10), dec!(1000)),
        ];
        // Primeira compra em 10/08: 108 dias -> 4 meses de 30 dias.
        assert_eq!(
            average_monthly_contribution(&buys, Some(date!(2026 - 08 - 10)), today),
            dec!(1000)
        );
    }

    #[test]
    fn average_over_a_full_year_divides_by_twelve() {
        let today = date!(2026 - 11 - 25);
        let buys = [
            (date!(2026 - 01 - 15), dec!(6000)),
            (date!(2026 - 10 - 15), dec!(6000)),
            // Fora da janela de 12 meses: não conta.
            (date!(2025 - 06 - 15), dec!(99999)),
        ];
        assert_eq!(
            average_monthly_contribution(&buys, Some(date!(2024 - 01 - 02)), today),
            dec!(1000)
        );
    }

    #[test]
    fn average_without_buys_is_zero() {
        let today = date!(2026 - 11 - 25);
        assert_eq!(average_monthly_contribution(&[], None, today), dec!(0));
        assert_eq!(
            average_monthly_contribution(&[], Some(date!(2026 - 01 - 01)), today),
            dec!(0)
        );
    }

    #[test]
    fn a_first_buy_today_counts_as_one_month() {
        let today = date!(2026 - 11 - 25);
        assert_eq!(
            average_monthly_contribution(&[(today, dec!(900))], Some(today), today),
            dec!(900)
        );
    }

    #[test]
    fn short_windows_are_not_a_premise() {
        assert_eq!(annualize(dec!(0.05), 90), None);
        assert_eq!(annualize(dec!(-1), 365), None);
        assert_eq!(annualize(dec!(0.1), 365), Some(dec!(0.1)));
        assert_eq!(annualize(dec!(0.21), 730), Some(dec!(0.1)));
    }

    #[test]
    fn cdi_gross_and_net_of_tax() {
        // 0,0551% ao dia útil: (1,000551)^252 - 1 ≈ 14,9%; líquido de 15% ≈ 12,7%.
        let gross = cdi_annual(dec!(0.0551)).unwrap();
        assert!(gross > dec!(0.148) && gross < dec!(0.15), "{gross}");
        let net = cdi_net_annual(dec!(0.0551)).unwrap();
        assert!(net > dec!(0.12) && net < dec!(0.13), "{net}");
        assert_eq!(net, (gross * dec!(0.85)).round_dp(6));
        assert_eq!(cdi_net_annual(dec!(-100)), None);
        assert_eq!(cdi_annual(dec!(-100)), None);
    }

    #[test]
    fn deflating_divides_the_growth_factors() {
        assert_eq!(deflate(dec!(0.1025), dec!(0.05)), Some(dec!(0.05)));
        assert_eq!(deflate(dec!(0.03), dec!(0.03)), Some(dec!(0)));
        assert_eq!(deflate(dec!(0.1), dec!(-1)), None);
        assert_eq!(
            deflate(dec!(-0.05), dec!(0.05)).map(|r| r < dec!(0)),
            Some(true)
        );
    }

    proptest! {
        #[test]
        fn a_goal_already_met_is_reached(
            target in 1i64..500_000_000,
            extra in 0i64..200_000_000,
        ) {
            let target = Decimal::new(target, 2);
            let current = target + Decimal::new(extra, 2);
            prop_assert!(is_reached(current, target));
            prop_assert_eq!(percent_done(current, target), Decimal::ONE_HUNDRED);
        }

        #[test]
        fn percent_stays_between_zero_and_a_hundred(
            current in -100_000_000i64..500_000_000,
            target in 1i64..500_000_000,
        ) {
            let p = percent_done(Decimal::new(current, 2), Decimal::new(target, 2));
            prop_assert!(p >= Decimal::ZERO && p <= Decimal::ONE_HUNDRED);
        }

        #[test]
        fn the_label_is_always_a_valid_month(months in 0u32..=MAX_MONTHS, days in 0i64..4000) {
            let today = date!(2020 - 01 - 15) + Duration::days(days);
            let label = month_label(today, months);
            let (year, month) = label.split_once('-').unwrap();
            prop_assert!(year.parse::<i32>().unwrap() >= today.year());
            let month: u8 = month.parse().unwrap();
            prop_assert!((1..=12).contains(&month));
        }

        /// Descontar e voltar a pôr a inflação devolve a taxa nominal.
        #[test]
        fn deflate_inverts_the_inflation(nominal in -500i64..=1000, inflation in -50i64..=300) {
            let n = Decimal::new(nominal, 3);
            let i = Decimal::new(inflation, 3);
            let real = deflate(n, i).unwrap();
            let back = (Decimal::ONE + real) * (Decimal::ONE + i) - Decimal::ONE;
            prop_assert!((back - n).abs() < dec!(0.00001), "{} contra {}", back, n);
        }
    }
}
