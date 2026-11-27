//! A simulação dos objetivos, mês a mês, em **reais de hoje** (poder de
//! compra de hoje). Vale para os três tipos: patrimônio, valor numa classe e
//! renda mensal.
//!
//! Por mês, com as taxas anuais convertidas em mensais equivalentes:
//!
//! ```text
//! valor[m] = valor[m-1] × (1 + taxa)^(1/12) + aporte
//!   taxa   = real                              (reinvestindo os proventos)
//!          = (1 + real) / (1 + rendimento) − 1 (sem reinvestir: só o preço)
//! renda[m] = renda[m-1] × (1 + crescimento)^(1/12)
//!          + (aporte + (reinveste ? renda[m-1] : 0)) × rendimento / 12
//! ```
//!
//! - `real` é a rentabilidade real **total** (com os proventos dentro, como a
//!   cota do Desempenho). Quem não reinveste tira os proventos todo mês, e o
//!   valor anda só pelo preço: a rentabilidade total descontado o rendimento.
//! - O aporte é constante em reais de hoje (sobe com a inflação em reais
//!   correntes).
//! - A renda de partida é a estimativa da tela Renda; ela cresce pelo
//!   crescimento real dos proventos e pelo que o dinheiro novo compra de renda
//!   ao rendimento líquido da carteira de hoje (o aporte e, reinvestindo, o
//!   próprio provento).
//!
//! A meta pode ser lida de dois jeitos, e a resposta traz os dois:
//! [`TargetBasis::Nominal`] (o número digitado, em reais da época: em reais de
//! hoje ele encolhe com a inflação) e [`TargetBasis::Today`] (o número com o
//! poder de compra de hoje). Os cenários somam [`Scenario::shift`] à
//! rentabilidade real e ao crescimento dos proventos: o valor só depende da
//! primeira, a renda só do segundo, e por isso o otimista nunca fica atrás do
//! base.

use serde::Serialize;

use super::income::Shape;
use super::{growth, monthly_rate, Scenario};
use crate::domain::income::YearMonth;
use crate::domain::objectives::{Eta, MAX_MONTHS};

/// O que a meta de renda mede.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeMode {
    /// A renda média por mês.
    #[default]
    Average,
    /// O menor mês de 12 seguidos: a renda cobre a meta todo mês.
    Floor,
}

impl IncomeMode {
    pub const ALL: [IncomeMode; 2] = [IncomeMode::Average, IncomeMode::Floor];

    pub fn as_str(self) -> &'static str {
        match self {
            IncomeMode::Average => "average",
            IncomeMode::Floor => "floor",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        IncomeMode::ALL.into_iter().find(|m| m.as_str() == value)
    }
}

/// O que o objetivo mede.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measure {
    /// O valor (patrimônio ou classe).
    Value,
    /// A renda média por mês.
    IncomeAverage,
    /// O menor mês dos 12 seguintes: a renda do mês × o peso do mês no ano da
    /// carteira ([`Shape`]); `start` é o mês de hoje.
    IncomeFloor { shape: Shape, start: YearMonth },
}

/// As premissas da conta, em frações ao ano (o aporte em reais de hoje).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Premises {
    pub monthly_contribution: f64,
    pub real_return: f64,
    pub inflation: f64,
    pub reinvest: bool,
    pub income_growth: f64,
    /// O rendimento líquido do dinheiro novo, ao ano.
    pub income_yield: f64,
    pub spread: f64,
}

/// O ponto de partida: o valor de hoje e a renda mensal de hoje.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Start {
    pub value: f64,
    pub monthly_income: f64,
}

/// O estado no fim de um mês, em reais de hoje.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    pub value: f64,
    pub monthly_income: f64,
}

/// Como ler a meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetBasis {
    /// O número digitado, em reais da época.
    Nominal,
    /// O número com o poder de compra de hoje.
    Today,
}

/// `months + 1` estados: o de hoje e o fim de cada mês.
pub fn simulate(start: Start, premises: &Premises, scenario: Scenario, months: u32) -> Vec<State> {
    let shift = scenario.shift(premises.spread);
    let income_yield = premises.income_yield.max(0.0);
    let total = premises.real_return + shift;
    let value_rate = monthly_rate(if premises.reinvest {
        total
    } else {
        (1.0 + total) / (1.0 + income_yield) - 1.0
    });
    let income_rate = monthly_rate(premises.income_growth + shift);
    let contribution = premises.monthly_contribution.max(0.0);
    let yield_month = income_yield / 12.0;
    let mut state = State {
        value: start.value.max(0.0),
        monthly_income: start.monthly_income.max(0.0),
    };
    let mut out = Vec::with_capacity(months as usize + 1);
    out.push(state);
    for _ in 0..months {
        let reinvested = if premises.reinvest {
            state.monthly_income
        } else {
            0.0
        };
        state = State {
            value: (state.value * (1.0 + value_rate) + contribution).max(0.0),
            monthly_income: (state.monthly_income * (1.0 + income_rate)
                + (contribution + reinvested) * yield_month)
                .max(0.0),
        };
        out.push(state);
    }
    out
}

/// O que o objetivo mede em cada mês de `states`. No piso, cada mês olha os
/// 12 seguintes (ele inclusive), então a série sai 11 meses mais curta.
pub fn measure_series(states: &[State], measure: Measure) -> Vec<f64> {
    match measure {
        Measure::Value => states.iter().map(|s| s.value).collect(),
        Measure::IncomeAverage => states.iter().map(|s| s.monthly_income).collect(),
        Measure::IncomeFloor { shape, start } => {
            let paid: Vec<f64> = states
                .iter()
                .enumerate()
                .map(|(m, s)| {
                    let month = start.shift(i32::try_from(m).unwrap_or(0));
                    s.monthly_income * shape.weight(month.month)
                })
                .collect();
            paid.windows(12)
                .map(|w| w.iter().copied().fold(f64::INFINITY, f64::min))
                .collect()
        }
    }
}

/// A meta do mês `m` em reais de hoje.
pub fn target_today(target: f64, basis: TargetBasis, inflation: f64, m: u32) -> f64 {
    match basis {
        TargetBasis::Today => target,
        TargetBasis::Nominal => target / growth(inflation, f64::from(m)).max(f64::MIN_POSITIVE),
    }
}

/// Os meses até a meta num cenário. `Reached` quando o mês de hoje já chega;
/// `Unreachable` quando a distância até a meta não diminui em 100 anos (nada
/// faz a conta crescer); `Beyond` quando diminui mas não chega.
pub fn months_to(
    start: Start,
    target: f64,
    measure: Measure,
    premises: &Premises,
    scenario: Scenario,
    basis: TargetBasis,
) -> Eta {
    if target <= 0.0 {
        return Eta::Reached;
    }
    let extra = match measure {
        Measure::IncomeFloor { .. } => 11,
        _ => 0,
    };
    let states = simulate(start, premises, scenario, MAX_MONTHS + extra);
    let series = measure_series(&states, measure);
    let ratio = |m: u32, value: f64| value / target_today(target, basis, premises.inflation, m);
    for (m, value) in series.iter().enumerate() {
        let m = u32::try_from(m).unwrap_or(u32::MAX);
        if m > MAX_MONTHS {
            break;
        }
        if ratio(m, *value) >= 1.0 - 1e-12 {
            return if m == 0 { Eta::Reached } else { Eta::In(m) };
        }
    }
    let first = series.first().map_or(0.0, |v| ratio(0, *v));
    let last = series
        .get(MAX_MONTHS as usize)
        .map_or(first, |v| ratio(MAX_MONTHS, *v));
    if last <= first + 1e-9 {
        Eta::Unreachable
    } else {
        Eta::Beyond
    }
}

/// Um ponto da curva: o que o objetivo mede, nos três cenários, em reais de
/// hoje e em reais da época.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurvePoint {
    pub month: u32,
    pub today: [f64; 3],
    pub nominal: [f64; 3],
}

/// A curva até `horizon` meses, de `step` em `step` (o último ponto é sempre
/// o `horizon`). Os cenários na ordem de [`Scenario::ALL`].
pub fn curve(
    start: Start,
    measure: Measure,
    premises: &Premises,
    horizon: u32,
    step: u32,
) -> Vec<CurvePoint> {
    let step = step.max(1);
    let extra = match measure {
        Measure::IncomeFloor { .. } => 11,
        _ => 0,
    };
    let series: Vec<Vec<f64>> = Scenario::ALL
        .iter()
        .map(|s| measure_series(&simulate(start, premises, *s, horizon + extra), measure))
        .collect();
    let mut months: Vec<u32> = (0..=horizon).step_by(step as usize).collect();
    if months.last() != Some(&horizon) {
        months.push(horizon);
    }
    months
        .into_iter()
        .map(|m| {
            let at = |i: usize| series[i].get(m as usize).copied().unwrap_or(0.0);
            let today = [at(0), at(1), at(2)];
            let inflate = growth(premises.inflation, f64::from(m));
            CurvePoint {
                month: m,
                today,
                nominal: today.map(|v| v * inflate),
            }
        })
        .collect()
}

/// Quantos meses a curva mostra: até a data mais distante entre as que
/// chegam (quem chama passa as que a tela compara), mais um ano, de 2 a 50
/// anos. Nenhuma chega: 20 anos.
pub fn chart_horizon(etas: &[Eta]) -> u32 {
    const MIN: u32 = 24;
    const MAX: u32 = 600;
    const NONE: u32 = 240;
    etas.iter()
        .filter_map(|e| match e {
            Eta::In(m) => Some(*m),
            _ => None,
        })
        .max()
        .map_or(NONE, |m| (m.saturating_add(12)).clamp(MIN, MAX))
}

/// O passo da curva para ela ter no máximo uns 120 pontos.
pub fn chart_step(horizon: u32) -> u32 {
    horizon.div_ceil(120).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn premises() -> Premises {
        Premises {
            monthly_contribution: 1000.0,
            real_return: 0.0,
            inflation: 0.0,
            reinvest: true,
            income_growth: 0.0,
            income_yield: 0.0,
            spread: 0.0,
        }
    }

    fn start(value: f64, income: f64) -> Start {
        Start {
            value,
            monthly_income: income,
        }
    }

    const NOV: YearMonth = YearMonth {
        year: 2026,
        month: 11,
    };

    #[test]
    fn modes_round_trip() {
        for m in IncomeMode::ALL {
            assert_eq!(IncomeMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(IncomeMode::parse("max"), None);
        assert_eq!(IncomeMode::default(), IncomeMode::Average);
    }

    #[test]
    fn contribution_without_return_is_a_division() {
        let p = premises();
        let eta = months_to(
            start(1000.0, 0.0),
            5000.0,
            Measure::Value,
            &p,
            Scenario::Base,
            TargetBasis::Today,
        );
        assert_eq!(eta, Eta::In(4));
        // Sem inflação as duas leituras da meta são a mesma.
        let nominal = months_to(
            start(1000.0, 0.0),
            5000.0,
            Measure::Value,
            &p,
            Scenario::Base,
            TargetBasis::Nominal,
        );
        assert_eq!(nominal, Eta::In(4));
        assert_eq!(
            months_to(
                start(6000.0, 0.0),
                5000.0,
                Measure::Value,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Reached
        );
    }

    #[test]
    fn the_real_return_compounds() {
        let p = Premises {
            monthly_contribution: 0.0,
            real_return: 0.12,
            ..premises()
        };
        // Dobra em ~6,1 anos a 12% ao ano.
        assert_eq!(
            months_to(
                start(1000.0, 0.0),
                2000.0,
                Measure::Value,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::In(74)
        );
    }

    #[test]
    fn inflation_brings_a_nominal_target_closer_but_not_a_real_one() {
        let p = Premises {
            monthly_contribution: 0.0,
            inflation: 0.05,
            ..premises()
        };
        // Em reais de hoje nada cresce: nunca chega.
        assert_eq!(
            months_to(
                start(1000.0, 0.0),
                2000.0,
                Measure::Value,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Unreachable
        );
        // O número digitado chega só pela inflação: 1,05^(m/12) ≥ 2 → m = 171.
        assert_eq!(
            months_to(
                start(1000.0, 0.0),
                2000.0,
                Measure::Value,
                &p,
                Scenario::Base,
                TargetBasis::Nominal
            ),
            Eta::In(171)
        );
    }

    #[test]
    fn not_reinvesting_takes_the_income_out_of_the_value() {
        let keep = Premises {
            monthly_contribution: 0.0,
            reinvest: false,
            real_return: 0.12,
            income_yield: 0.12,
            ..premises()
        };
        // Rende 12% ao ano, tudo em provento: sem reinvestir, o valor fica parado.
        let states = simulate(start(1200.0, 12.0), &keep, Scenario::Base, 12);
        assert!((states[12].value - 1200.0).abs() < 1e-9, "{:?}", states[12]);
        // E a renda não cresce: o provento não compra mais cotas.
        assert!((states[12].monthly_income - 12.0).abs() < 1e-9);
        let reinvest = Premises {
            reinvest: true,
            ..keep
        };
        let grown = simulate(start(1200.0, 12.0), &reinvest, Scenario::Base, 12);
        assert!((grown[12].value - 1344.0).abs() < 1e-6);
        assert!(grown[12].monthly_income > 13.0);
    }

    #[test]
    fn income_grows_with_new_money_and_with_dividend_growth() {
        let p = Premises {
            monthly_contribution: 1200.0,
            income_yield: 0.10,
            ..premises()
        };
        // Cada mês o aporte compra 1.200 × 10% / 12 = 10 de renda (e a renda
        // reinvestida compra mais um pouco).
        let states = simulate(start(0.0, 0.0), &p, Scenario::Base, 2);
        assert!((states[1].monthly_income - 10.0).abs() < 1e-9);
        assert!((states[2].monthly_income - (10.0 + 10.0 + 10.0 * 0.1 / 12.0)).abs() < 1e-9);
        let growing = Premises {
            monthly_contribution: 0.0,
            income_growth: 0.12,
            ..premises()
        };
        let year = simulate(start(0.0, 100.0), &growing, Scenario::Base, 12);
        assert!((year[12].monthly_income - 112.0).abs() < 1e-9);
    }

    #[test]
    fn income_targets_use_the_monthly_income() {
        let p = Premises {
            monthly_contribution: 1200.0,
            income_yield: 0.12,
            reinvest: false,
            ..premises()
        };
        // 12 por mês a cada mês: de 100 a 160 em 5 meses.
        assert_eq!(
            months_to(
                start(0.0, 100.0),
                160.0,
                Measure::IncomeAverage,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::In(5)
        );
    }

    #[test]
    fn the_floor_waits_for_the_weakest_month() {
        // Paga em dobro em novembro e dezembro, metade no resto... média 1.
        let mut weights = [0.8f64; 12];
        weights[10] = 2.0;
        weights[11] = 2.0;
        let payments = weights.iter().enumerate().map(|(i, w)| {
            (
                time::Date::from_calendar_date(
                    2026,
                    time::Month::try_from(u8::try_from(i + 1).unwrap()).unwrap(),
                    1,
                )
                .unwrap(),
                *w,
            )
        });
        let shape = Shape::from_payments(payments);
        let p = Premises {
            monthly_contribution: 0.0,
            ..premises()
        };
        let floor = Measure::IncomeFloor { shape, start: NOV };
        // Renda parada de 100: o pior mês paga 80. Meta de 80 já chegou; 81, nunca.
        assert_eq!(
            months_to(
                start(0.0, 100.0),
                80.0,
                floor,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Reached
        );
        assert_eq!(
            months_to(
                start(0.0, 100.0),
                81.0,
                floor,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Unreachable
        );
        // A média já está em 100.
        assert_eq!(
            months_to(
                start(0.0, 100.0),
                100.0,
                Measure::IncomeAverage,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Reached
        );
        // Um mês sem provento nenhum: o piso não sai do zero.
        let may_only = Shape::from_payments([(time::macros::date!(2026 - 05 - 10), 1.0)]);
        let growing = Premises {
            income_growth: 0.2,
            ..p
        };
        assert_eq!(
            months_to(
                start(0.0, 100.0),
                10.0,
                Measure::IncomeFloor {
                    shape: may_only,
                    start: NOV
                },
                &growing,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Unreachable
        );
    }

    #[test]
    fn too_far_is_beyond() {
        let p = Premises {
            monthly_contribution: 1.0,
            ..premises()
        };
        assert_eq!(
            months_to(
                start(0.0, 0.0),
                1e9,
                Measure::Value,
                &p,
                Scenario::Base,
                TargetBasis::Today
            ),
            Eta::Beyond
        );
    }

    #[test]
    fn the_curve_starts_today_and_ends_on_the_horizon() {
        let p = Premises {
            monthly_contribution: 100.0,
            real_return: 0.05,
            inflation: 0.04,
            spread: 0.02,
            ..premises()
        };
        let points = curve(start(1000.0, 0.0), Measure::Value, &p, 25, 10);
        let months: Vec<u32> = points.iter().map(|c| c.month).collect();
        assert_eq!(months, [0, 10, 20, 25]);
        assert_eq!(points[0].today, [1000.0; 3]);
        assert_eq!(points[0].nominal, [1000.0; 3]);
        let last = points[3];
        assert!(last.today[0] < last.today[1] && last.today[1] < last.today[2]);
        assert!((last.nominal[1] - last.today[1] * 1.04f64.powf(25.0 / 12.0)).abs() < 1e-6);
    }

    #[test]
    fn the_chart_horizon_covers_the_furthest_date() {
        assert_eq!(chart_horizon(&[Eta::In(5)]), 24);
        assert_eq!(chart_horizon(&[Eta::In(100), Eta::In(150)]), 162);
        assert_eq!(chart_horizon(&[Eta::In(100), Eta::Beyond]), 112);
        assert_eq!(chart_horizon(&[Eta::In(900)]), 600);
        assert_eq!(chart_horizon(&[Eta::Unreachable]), 240);
        assert_eq!(chart_horizon(&[Eta::Reached]), 240);
        assert_eq!(chart_step(120), 1);
        assert_eq!(chart_step(121), 2);
        assert_eq!(chart_step(600), 5);
        assert_eq!(chart_step(0), 1);
    }

    fn rank(eta: Eta) -> (u8, u32) {
        match eta {
            Eta::Reached => (0, 0),
            Eta::In(m) => (1, m),
            Eta::Beyond => (2, 0),
            Eta::Unreachable => (3, 0),
        }
    }

    fn any_premises() -> impl Strategy<Value = Premises> {
        (
            0u32..5_000,
            -300i32..=300,
            0u32..=150,
            any::<bool>(),
            -200i32..=200,
            0u32..=200,
            0u32..=500,
        )
            .prop_map(|(c, r, i, reinvest, g, y, s)| Premises {
                monthly_contribution: f64::from(c),
                real_return: f64::from(r) / 1000.0,
                inflation: f64::from(i) / 1000.0,
                reinvest,
                income_growth: f64::from(g) / 1000.0,
                income_yield: f64::from(y) / 1000.0,
                spread: f64::from(s) / 10_000.0,
            })
    }

    proptest! {
        /// O otimista nunca chega depois do base, nem o base depois do
        /// pessimista, em nenhuma medida nem leitura da meta.
        #[test]
        fn scenarios_are_ordered(
            p in any_premises(),
            value in 0u32..1_000_000,
            income in 0u32..10_000,
            target in 1u32..5_000_000,
            kind in 0usize..3,
            today in any::<bool>(),
        ) {
            let measure = match kind {
                0 => Measure::Value,
                1 => Measure::IncomeAverage,
                _ => Measure::IncomeFloor { shape: Shape::uniform(), start: NOV },
            };
            let basis = if today { TargetBasis::Today } else { TargetBasis::Nominal };
            let s = start(f64::from(value), f64::from(income));
            let target = f64::from(target) / if kind == 0 { 1.0 } else { 100.0 };
            let eta = |scenario| rank(months_to(s, target, measure, &p, scenario, basis));
            prop_assert!(eta(Scenario::Optimistic) <= eta(Scenario::Base));
            prop_assert!(eta(Scenario::Base) <= eta(Scenario::Pessimistic));
        }

        /// Aporte maior nunca atrasa a meta.
        #[test]
        fn a_bigger_contribution_never_delays(
            p in any_premises(),
            extra in 0u32..5_000,
            value in 0u32..1_000_000,
            income in 0u32..10_000,
            target in 1u32..5_000_000,
            kind in 0usize..2,
        ) {
            let measure = if kind == 0 { Measure::Value } else { Measure::IncomeAverage };
            let s = start(f64::from(value), f64::from(income));
            let target = f64::from(target) / if kind == 0 { 1.0 } else { 100.0 };
            let more = Premises { monthly_contribution: p.monthly_contribution + f64::from(extra), ..p };
            let slow = rank(months_to(s, target, measure, &p, Scenario::Base, TargetBasis::Today));
            let fast = rank(months_to(s, target, measure, &more, Scenario::Base, TargetBasis::Today));
            prop_assert!(fast <= slow, "{:?} depois de {:?}", fast, slow);
        }

        /// Com inflação positiva, a meta em reais de hoje nunca chega antes da
        /// meta em reais da época; e a curva tem os cenários em ordem.
        #[test]
        fn todays_money_is_never_earlier(
            p in any_premises(),
            value in 0u32..1_000_000,
            target in 1u32..5_000_000,
        ) {
            let s = start(f64::from(value), 0.0);
            let target = f64::from(target);
            let nominal = rank(months_to(s, target, Measure::Value, &p, Scenario::Base, TargetBasis::Nominal));
            let today = rank(months_to(s, target, Measure::Value, &p, Scenario::Base, TargetBasis::Today));
            prop_assert!(nominal <= today || (nominal.0 >= 2 && today.0 >= 2));
            for point in curve(s, Measure::Value, &p, 120, 7) {
                prop_assert!(point.today[0] <= point.today[1] + 1e-6);
                prop_assert!(point.today[1] <= point.today[2] + 1e-6);
            }
        }
    }
}
