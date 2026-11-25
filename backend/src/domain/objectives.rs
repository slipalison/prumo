//! Objetivos com prazo: quanto falta, a conta do aporte médio e a data
//! estimada de conclusão. Regra pura, sem banco nem relógio: quem chama passa
//! a data de hoje.
//!
//! A projeção é uma simulação mês a mês. Cada mês o saldo rende à taxa mensal
//! equivalente à rentabilidade anual da premissa e recebe o aporte mensal:
//!
//! ```text
//! saldo[m] = saldo[m - 1] × (1 + anual)^(1/12) + aporte
//! ```
//!
//! O primeiro mês `m` em que `saldo >= meta` é a conclusão; a data é o mês de
//! hoje mais `m` meses. Simulação no lugar da fórmula fechada porque o caso
//! sem aporte, sem rendimento ou com rendimento negativo cai fora da fórmula
//! (logaritmo de número não positivo) e porque assim o teto de prazo é uma
//! condição simples do laço. A conta é em ponto flutuante: a data é em meses
//! inteiros e a premissa é uma estimativa, não um saldo contábil.

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, Month};

use super::asset_class::AssetClass;
use super::income::ObjectiveKind;
use super::performance::months_before;

/// O prazo máximo que a projeção procura: 100 anos. Passou disso, a data não
/// diz nada.
pub const MAX_MONTHS: u32 = 1200;

/// Janela do aporte médio, em meses.
pub const CONTRIBUTION_WINDOW_MONTHS: u32 = 12;

/// Menor janela da série do Desempenho que serve de premissa: seis meses.
/// Com menos que isso a rentabilidade anualizada é ruído.
pub const MIN_RETURN_WINDOW_DAYS: i64 = 180;

/// Padrão sem Desempenho nem CDI: 8% ao ano, bruto. Só aparece quando o banco
/// não tem série da carteira de seis meses nem CDI; a tela o rotula como
/// "padrão do Prumo" e a pessoa pode trocar.
pub const FALLBACK_ANNUAL_RETURN: Decimal = Decimal::from_parts(8, 0, 0, false, 2);

/// IR sobre renda fixa de longo prazo (mais de 720 dias): a menor alíquota da
/// tabela regressiva. O padrão da premissa desconta este valor do CDI.
pub const CDI_TAX: Decimal = Decimal::from_parts(15, 0, 0, false, 2);

/// Dias úteis do ano na capitalização do CDI.
const BUSINESS_DAYS_PER_YEAR: f64 = 252.0;

/// O tipo do objetivo que esta área calcula. A meta de renda mensal tem as
/// regras próprias de `domain::income`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// Patrimônio total.
    NetWorth,
    /// Valor numa classe.
    ClassValue,
}

impl Target {
    /// O que o objetivo mede, dado o patrimônio e o valor por classe.
    pub fn current(
        self,
        asset_class: Option<AssetClass>,
        net_worth: Decimal,
        by_class: impl Fn(AssetClass) -> Decimal,
    ) -> Decimal {
        match (self, asset_class) {
            (Target::ClassValue, Some(class)) => by_class(class),
            _ => net_worth,
        }
    }
}

impl From<Target> for ObjectiveKind {
    fn from(target: Target) -> Self {
        match target {
            Target::NetWorth => ObjectiveKind::NetWorth,
            Target::ClassValue => ObjectiveKind::ClassValue,
        }
    }
}

impl TryFrom<ObjectiveKind> for Target {
    type Error = ();

    /// A meta de renda mensal não é deste módulo.
    fn try_from(kind: ObjectiveKind) -> Result<Self, ()> {
        match kind {
            ObjectiveKind::NetWorth => Ok(Target::NetWorth),
            ObjectiveKind::ClassValue => Ok(Target::ClassValue),
            ObjectiveKind::MonthlyIncome => Err(()),
        }
    }
}

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
    /// Sem aporte e sem rendimento positivo, o valor nunca chega.
    Unreachable,
    /// Chega, mas depois de [`MAX_MONTHS`].
    Beyond,
}

fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

/// Quantos meses faltam para `current` chegar a `target`, aportando
/// `monthly_contribution` por mês e rendendo `annual_return` ao ano (fração:
/// 0,10 = 10%). Aporte negativo vale zero.
pub fn months_to_reach(
    current: Decimal,
    target: Decimal,
    monthly_contribution: Decimal,
    annual_return: Decimal,
) -> Eta {
    if is_reached(current, target) {
        return Eta::Reached;
    }
    let contribution = to_f64(monthly_contribution).max(0.0);
    let annual = to_f64(annual_return);
    if contribution <= 0.0 && annual <= 0.0 {
        return Eta::Unreachable;
    }
    // Um ano que perde mais de 100% não existe: no limite, o saldo zera.
    let factor = (1.0 + annual).max(0.0).powf(1.0 / 12.0);
    let goal = to_f64(target);
    let mut balance = to_f64(current).max(0.0);
    for month in 1..=MAX_MONTHS {
        balance = balance * factor + contribution;
        if balance >= goal {
            return Eta::In(month);
        }
    }
    Eta::Beyond
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

/// De onde veio a rentabilidade usada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnSource {
    /// A pessoa informou no objetivo.
    Informed,
    /// A rentabilidade da carteira na tela Desempenho, anualizada.
    Performance,
    /// O CDI de hoje menos o IR de renda fixa de longo prazo.
    CdiNet,
    /// O padrão fixo do Prumo ([`FALLBACK_ANNUAL_RETURN`]).
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

/// O CDI de hoje, líquido do IR: `daily_percent` é a taxa de um dia útil em
/// %, como o Banco Central publica.
pub fn cdi_net_annual(daily_percent: Decimal) -> Option<Decimal> {
    let daily = to_f64(daily_percent) / 100.0;
    if daily <= -1.0 {
        return None;
    }
    let gross = (1.0 + daily).powf(BUSINESS_DAYS_PER_YEAR) - 1.0;
    if !gross.is_finite() {
        return None;
    }
    Decimal::from_f64(gross).map(|g| (g * (Decimal::ONE - CDI_TAX)).round_dp(6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use time::Duration;

    #[test]
    fn already_there_is_reached() {
        assert_eq!(
            months_to_reach(dec!(100), dec!(100), dec!(0), dec!(0)),
            Eta::Reached
        );
        assert_eq!(
            months_to_reach(dec!(150), dec!(100), dec!(0), dec!(-0.5)),
            Eta::Reached
        );
    }

    #[test]
    fn contribution_without_return_is_a_division() {
        // 1.000 faltando 4.000 a 500 por mês: 8 meses.
        assert_eq!(
            months_to_reach(dec!(1000), dec!(5000), dec!(500), dec!(0)),
            Eta::In(8)
        );
        // 1.000 + 7 x 500 = 4.500 < 4.600: precisa do oitavo.
        assert_eq!(
            months_to_reach(dec!(1000), dec!(4600), dec!(500), dec!(0)),
            Eta::In(8)
        );
    }

    #[test]
    fn return_alone_compounds() {
        // 12% ao ano, sem aporte: dobra em ~6,1 anos (ln 2 / ln 1,12).
        assert_eq!(
            months_to_reach(dec!(1000), dec!(2000), dec!(0), dec!(0.12)),
            Eta::In(74)
        );
    }

    #[test]
    fn nothing_coming_in_never_gets_there() {
        assert_eq!(
            months_to_reach(dec!(1000), dec!(2000), dec!(0), dec!(0)),
            Eta::Unreachable
        );
        assert_eq!(
            months_to_reach(dec!(1000), dec!(2000), dec!(0), dec!(-0.05)),
            Eta::Unreachable
        );
        assert_eq!(
            months_to_reach(dec!(1000), dec!(2000), dec!(-10), dec!(0)),
            Eta::Unreachable
        );
    }

    #[test]
    fn a_loss_with_contributions_still_gets_there() {
        match months_to_reach(dec!(0), dec!(10000), dec!(1000), dec!(-0.2)) {
            Eta::In(months) => assert!(months > 10),
            other => panic!("esperava uma data, veio {other:?}"),
        }
    }

    #[test]
    fn too_far_is_beyond() {
        assert_eq!(
            months_to_reach(dec!(0), dec!(1000000000), dec!(1), dec!(0)),
            Eta::Beyond
        );
    }

    #[test]
    fn a_year_that_loses_everything_keeps_only_the_last_contribution() {
        // Rende -100%: o saldo de cada mês é só o aporte que acabou de entrar.
        assert_eq!(
            months_to_reach(dec!(100), dec!(400), dec!(500), dec!(-1)),
            Eta::In(1)
        );
        assert_eq!(
            months_to_reach(dec!(100), dec!(600), dec!(500), dec!(-1)),
            Eta::Beyond
        );
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
    fn cdi_net_of_tax() {
        // 0,0551% ao dia útil: (1,000551)^252 - 1 ≈ 14,9%; líquido de 15% ≈ 12,7%.
        let net = cdi_net_annual(dec!(0.0551)).unwrap();
        assert!(net > dec!(0.12) && net < dec!(0.13), "{net}");
        assert_eq!(cdi_net_annual(dec!(-100)), None);
    }

    #[test]
    fn the_current_value_depends_on_the_kind() {
        let by_class = |class: AssetClass| {
            if class == AssetClass::BrReits {
                dec!(300)
            } else {
                dec!(0)
            }
        };
        assert_eq!(
            Target::NetWorth.current(None, dec!(1000), by_class),
            dec!(1000)
        );
        assert_eq!(
            Target::ClassValue.current(Some(AssetClass::BrReits), dec!(1000), by_class),
            dec!(300)
        );
    }

    #[test]
    fn monthly_income_is_not_a_target() {
        assert_eq!(Target::try_from(ObjectiveKind::MonthlyIncome), Err(()));
        assert_eq!(
            Target::try_from(ObjectiveKind::NetWorth),
            Ok(Target::NetWorth)
        );
        assert_eq!(
            ObjectiveKind::from(Target::ClassValue),
            ObjectiveKind::ClassValue
        );
    }

    fn money() -> impl Strategy<Value = Decimal> {
        (0i64..200_000_000).prop_map(|cents| Decimal::new(cents, 2))
    }

    fn goal() -> impl Strategy<Value = Decimal> {
        (1i64..500_000_000).prop_map(|cents| Decimal::new(cents, 2))
    }

    /// De -30% a +60% ao ano, em passos de 0,1 ponto.
    fn rate() -> impl Strategy<Value = Decimal> {
        (-300i64..=600).prop_map(|steps| Decimal::new(steps, 3))
    }

    proptest! {
        #[test]
        fn a_bigger_contribution_never_delays(
            current in money(),
            target in goal(),
            low in money(),
            extra in money(),
            annual in rate(),
        ) {
            let slow = months_to_reach(current, target, low, annual);
            let fast = months_to_reach(current, target, low + extra, annual);
            prop_assert!(rank(fast) <= rank(slow), "{:?} depois de {:?}", fast, slow);
        }

        #[test]
        fn a_bigger_return_never_delays(
            current in money(),
            target in goal(),
            contribution in money(),
            low in rate(),
            extra in 0i64..=300,
        ) {
            // Passos de 0,1 ponto no mínimo: a diferença de uma ulp na
            // potência não vira um mês.
            let high = low + Decimal::new(extra, 3);
            let slow = months_to_reach(current, target, contribution, low);
            let fast = months_to_reach(current, target, contribution, high);
            prop_assert!(rank(fast) <= rank(slow), "{:?} depois de {:?}", fast, slow);
        }

        #[test]
        fn a_goal_already_met_is_reached(
            target in goal(),
            extra in money(),
            contribution in money(),
            annual in rate(),
        ) {
            let current = target + extra;
            prop_assert!(is_reached(current, target));
            prop_assert_eq!(months_to_reach(current, target, contribution, annual), Eta::Reached);
            prop_assert_eq!(percent_done(current, target), Decimal::ONE_HUNDRED);
        }

        #[test]
        fn a_bigger_balance_never_delays(
            low in money(),
            extra in money(),
            target in goal(),
            contribution in money(),
            annual in rate(),
        ) {
            let slow = months_to_reach(low, target, contribution, annual);
            let fast = months_to_reach(low + extra, target, contribution, annual);
            prop_assert!(rank(fast) <= rank(slow), "{:?} depois de {:?}", fast, slow);
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
    }

    /// Da mais cedo para a mais tarde: chegou, daqui a N meses, depois do
    /// teto, nunca.
    fn rank(eta: Eta) -> (u8, u32) {
        match eta {
            Eta::Reached => (0, 0),
            Eta::In(months) => (1, months),
            Eta::Beyond => (2, 0),
            Eta::Unreachable => (3, 0),
        }
    }
}
