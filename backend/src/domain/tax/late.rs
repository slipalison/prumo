//! Multa e juros do DARF pago depois do vencimento (Lei 9.430/1996, art. 61),
//! a conta que a Receita faz no Sicalc:
//!
//! - **multa de mora**: 0,33% por dia de atraso, contado do dia seguinte ao
//!   vencimento até o dia do pagamento (dias corridos), com teto de 20% (art.
//!   61, caput e par. 1 e 2);
//! - **juros de mora**: a soma da Selic de cada mês (a "acumulada
//!   mensalmente", taxa do mês somada, não composta), do mês seguinte ao do
//!   vencimento até o mês anterior ao do pagamento, mais 1% no mês do
//!   pagamento, sobre o imposto **sem** a multa (art. 61, par. 3, e art. 5,
//!   par. 3; Perguntas e Respostas PJ, cap. XVIII, 001). Os juros só começam
//!   no primeiro dia do mês seguinte ao vencimento: pago ainda no mês do
//!   vencimento, há multa e não há juros.
//!
//! A taxa de cada mês vem de fora (a série 4390 do SGS, a mesma da tabela de
//! acréscimos legais da Receita). Mês sem taxa conhecida não vira estimativa:
//! os juros ficam sem valor e o mês que falta é dito.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;
use time::Date;

use super::rules::LatePaymentRule;
use super::{cents, YearMonth};

/// A Selic de um mês somada nos juros, em % no mês (`None` = ainda sem taxa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SelicMonth {
    pub period: YearMonth,
    pub rate: Option<Decimal>,
}

/// O que se paga a mais por um DARF pago em `pay_date`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LateCharge {
    /// O imposto do DARF (sem acréscimo).
    pub principal: Decimal,
    pub due_date: Date,
    pub pay_date: Date,
    /// Dias corridos depois do vencimento; zero = em dia.
    pub days_late: i64,
    /// Fração (0,0330 = 3,30%).
    pub penalty_rate: Decimal,
    pub penalty: Decimal,
    /// Os meses de Selic somados, na ordem.
    pub selic_months: Vec<SelicMonth>,
    /// O 1% do mês do pagamento (zero em dia ou pago no mês do vencimento).
    pub payment_month_rate: Decimal,
    /// Fração dos juros (Selic somada + 1%); `None` quando falta a Selic de
    /// algum mês.
    pub interest_rate: Option<Decimal>,
    pub interest: Option<Decimal>,
    /// Imposto + multa + juros; `None` quando os juros ficam sem valor.
    pub total: Option<Decimal>,
}

impl LateCharge {
    /// Os meses sem Selic conhecida.
    pub fn missing_selic(&self) -> Vec<YearMonth> {
        self.selic_months
            .iter()
            .filter(|month| month.rate.is_none())
            .map(|month| month.period)
            .collect()
    }
}

/// A multa e os juros do imposto `principal`, vencido em `due_date` e pago em
/// `pay_date`. `selic` é a taxa de cada mês em % (1,12 = 1,12% no mês).
pub fn late_charge(
    rule: &LatePaymentRule,
    principal: Decimal,
    due_date: Date,
    pay_date: Date,
    selic: &BTreeMap<YearMonth, Decimal>,
) -> LateCharge {
    let principal = principal.max(Decimal::ZERO);
    let days_late = (pay_date - due_date).whole_days().max(0);
    let penalty_rate =
        (Decimal::from(days_late) * rule.daily_penalty.value).min(rule.penalty_cap.value);
    let due_month = YearMonth::of(due_date);
    let pay_month = YearMonth::of(pay_date);

    // Juros: só a partir do mês seguinte ao do vencimento.
    let (selic_months, payment_month_rate) = if days_late > 0 && pay_month > due_month {
        let mut months = Vec::new();
        let mut period = due_month.next();
        while period < pay_month {
            months.push(SelicMonth {
                period,
                rate: selic.get(&period).copied(),
            });
            period = period.next();
        }
        (months, rule.payment_month_interest.value)
    } else {
        (Vec::new(), Decimal::ZERO)
    };
    let interest_rate = selic_months
        .iter()
        .map(|month| month.rate.map(|rate| rate / Decimal::ONE_HUNDRED))
        .sum::<Option<Decimal>>()
        .map(|sum| sum + payment_month_rate);

    let penalty = cents(principal * penalty_rate);
    let interest = interest_rate.map(|rate| cents(principal * rate));
    LateCharge {
        principal,
        due_date,
        pay_date,
        days_late,
        penalty_rate,
        penalty,
        selic_months,
        payment_month_rate,
        interest_rate,
        interest,
        total: interest.map(|interest| principal + penalty + interest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tax::rules::rules_for;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    /// A Selic acumulada no mês, % a.m. (BCB, SGS 4390), de jan/2025 a
    /// out/2026, como a série publica (consulta em 27/11/2026).
    const SELIC: [(i32, u8, Decimal); 22] = [
        (2025, 1, dec!(1.01)),
        (2025, 2, dec!(0.99)),
        (2025, 3, dec!(0.96)),
        (2025, 4, dec!(1.06)),
        (2025, 5, dec!(1.14)),
        (2025, 6, dec!(1.10)),
        (2025, 7, dec!(1.28)),
        (2025, 8, dec!(1.16)),
        (2025, 9, dec!(1.22)),
        (2025, 10, dec!(1.28)),
        (2025, 11, dec!(1.05)),
        (2025, 12, dec!(1.22)),
        (2026, 1, dec!(1.16)),
        (2026, 2, dec!(1.00)),
        (2026, 3, dec!(1.21)),
        (2026, 4, dec!(1.09)),
        (2026, 5, dec!(1.07)),
        (2026, 6, dec!(1.12)),
        (2026, 7, dec!(1.22)),
        (2026, 8, dec!(1.09)),
        (2026, 9, dec!(1.08)),
        (2026, 10, dec!(1.07)),
    ];

    fn selic() -> BTreeMap<YearMonth, Decimal> {
        SELIC
            .iter()
            .map(|&(year, month, rate)| (YearMonth { year, month }, rate))
            .collect()
    }

    fn rule() -> LatePaymentRule {
        rules_for(2026).unwrap().late_payment
    }

    fn last_day(year: i32, month: u8) -> Date {
        let next = YearMonth { year, month }.next();
        Date::from_calendar_date(next.year, next.time_month(), 1).unwrap() - time::Duration::days(1)
    }

    #[test]
    fn interest_matches_the_receita_table_for_july_2026() {
        // Tabela de juros de mora da Receita para pagamento em julho de 2026
        // (republicada pelo CNB/SP): percentual por mês de vencimento. O
        // Prumo soma a Selic de cada mês da SGS 4390 e chega ao mesmo número.
        let published: [(i32, u8, Decimal); 18] = [
            (2025, 1, dec!(20.11)),
            (2025, 2, dec!(19.12)),
            (2025, 3, dec!(18.16)),
            (2025, 4, dec!(17.10)),
            (2025, 5, dec!(15.96)),
            (2025, 6, dec!(14.86)),
            (2025, 7, dec!(13.58)),
            (2025, 8, dec!(12.42)),
            (2025, 9, dec!(11.20)),
            (2025, 10, dec!(9.92)),
            (2025, 11, dec!(8.87)),
            (2025, 12, dec!(7.65)),
            (2026, 1, dec!(6.49)),
            (2026, 2, dec!(5.49)),
            (2026, 3, dec!(4.28)),
            (2026, 4, dec!(3.19)),
            (2026, 5, dec!(2.12)),
            (2026, 6, dec!(1.00)),
        ];
        let selic = selic();
        for (year, month, percent) in published {
            let due = last_day(year, month);
            let charge = late_charge(&rule(), dec!(1000), due, date!(2026 - 07 - 15), &selic);
            assert_eq!(
                charge.interest_rate.map(|rate| rate * dec!(100)),
                Some(percent),
                "{month}/{year}"
            );
        }
    }

    #[test]
    fn quota_interest_follows_the_receita_example() {
        // P&R 064: a 3ª quota (31/07/2026) leva a Selic de junho mais 1%; a
        // 7ª (30/11/2026), a de junho a outubro mais 1%. É a mesma conta do
        // art. 61, par. 3, contada do vencimento da 1ª quota (29/05/2026).
        let selic = selic();
        let third = late_charge(
            &rule(),
            dec!(1000),
            date!(2026 - 05 - 29),
            date!(2026 - 07 - 31),
            &selic,
        );
        assert_eq!(third.interest_rate, Some(dec!(0.0212)));
        assert_eq!(third.interest, Some(dec!(21.20)));
        let seventh = late_charge(
            &rule(),
            dec!(1000),
            date!(2026 - 05 - 29),
            date!(2026 - 11 - 30),
            &selic,
        );
        let months: Vec<u8> = seventh
            .selic_months
            .iter()
            .map(|m| m.period.month)
            .collect();
        assert_eq!(months, vec![6, 7, 8, 9, 10]);
        assert_eq!(seventh.interest_rate, Some(dec!(0.0658)));
    }

    #[test]
    fn penalty_is_a_third_of_a_percent_a_day_up_to_20() {
        // Lei 9.430/1996, art. 61, caput e par. 1 e 2: do dia seguinte ao
        // vencimento até o dia do pagamento, em dias corridos.
        let selic = selic();
        // DARF de maio de 2025, vencido em 30/06/2025, pago em 10/07/2025.
        let ten = late_charge(
            &rule(),
            dec!(450),
            date!(2025 - 06 - 30),
            date!(2025 - 07 - 10),
            &selic,
        );
        assert_eq!(ten.days_late, 10);
        assert_eq!(ten.penalty_rate, dec!(0.0330));
        assert_eq!(ten.penalty, dec!(14.85));
        // Julho: só o 1% do mês do pagamento.
        assert_eq!(ten.selic_months, vec![]);
        assert_eq!(ten.interest_rate, Some(dec!(0.01)));
        assert_eq!(ten.interest, Some(dec!(4.50)));
        assert_eq!(ten.total, Some(dec!(469.35)));
        // 60 dias dão 19,8%; 61, o teto de 20%.
        let sixty = late_charge(
            &rule(),
            dec!(450),
            date!(2025 - 06 - 30),
            date!(2025 - 08 - 29),
            &selic,
        );
        assert_eq!(sixty.days_late, 60);
        assert_eq!(sixty.penalty_rate, dec!(0.1980));
        let capped = late_charge(
            &rule(),
            dec!(450),
            date!(2025 - 06 - 30),
            date!(2026 - 02 - 10),
            &selic,
        );
        assert_eq!(capped.penalty_rate, dec!(0.20));
        assert_eq!(capped.penalty, dec!(90));
        // Juros sobre o imposto sem a multa: Selic de jul/2025 a jan/2026 + 1%.
        assert_eq!(capped.interest_rate, Some(dec!(0.0837) + dec!(0.01)));
        assert_eq!(capped.interest, Some(dec!(42.17)));
    }

    #[test]
    fn on_time_and_same_month_payments() {
        let selic = selic();
        let on_time = late_charge(
            &rule(),
            dec!(100),
            date!(2026 - 06 - 30),
            date!(2026 - 06 - 30),
            &selic,
        );
        assert_eq!(on_time.days_late, 0);
        assert_eq!(on_time.penalty, dec!(0));
        assert_eq!(on_time.interest, Some(dec!(0)));
        assert_eq!(on_time.total, Some(dec!(100)));
        let early = late_charge(
            &rule(),
            dec!(100),
            date!(2026 - 06 - 30),
            date!(2026 - 06 - 01),
            &selic,
        );
        assert_eq!(early.days_late, 0);
        assert_eq!(early.total, Some(dec!(100)));
        // Vencido em 29/05 (sexta) e pago em 31/05: multa de 2 dias, e os
        // juros ainda não começaram (só no 1º dia do mês seguinte).
        let same = late_charge(
            &rule(),
            dec!(100),
            date!(2026 - 05 - 29),
            date!(2026 - 05 - 31),
            &selic,
        );
        assert_eq!(same.days_late, 2);
        assert_eq!(same.penalty, dec!(0.66));
        assert_eq!(same.payment_month_rate, dec!(0));
        assert_eq!(same.interest, Some(dec!(0)));
        assert_eq!(same.total, Some(dec!(100.66)));
    }

    #[test]
    fn a_month_without_selic_leaves_the_interest_unknown() {
        // Pago em janeiro de 2027: falta a Selic de novembro e de dezembro de
        // 2026 (ainda não fechadas na tabela). Os juros não são estimados.
        let charge = late_charge(
            &rule(),
            dec!(100),
            date!(2026 - 09 - 30),
            date!(2027 - 01 - 05),
            &selic(),
        );
        assert_eq!(charge.interest, None);
        assert_eq!(charge.total, None);
        assert_eq!(
            charge.missing_selic(),
            vec![
                YearMonth {
                    year: 2026,
                    month: 11
                },
                YearMonth {
                    year: 2026,
                    month: 12
                }
            ]
        );
        // A multa não depende da Selic.
        assert_eq!(charge.penalty_rate, dec!(0.20));
        assert_eq!(charge.penalty, dec!(20));
    }

    proptest! {
        #[test]
        fn penalty_never_passes_20_percent_and_total_adds_up(
            principal in 0i64..10_000_000,
            due_offset in 0i64..600,
            days in 0i64..900,
        ) {
            let principal = Decimal::new(principal, 2);
            let due = date!(2025 - 01 - 31) + time::Duration::days(due_offset);
            let pay = due + time::Duration::days(days);
            let charge = late_charge(&rule(), principal, due, pay, &selic());
            prop_assert!(charge.penalty_rate <= dec!(0.20));
            prop_assert!(charge.penalty <= cents(principal * dec!(0.20)));
            prop_assert!(charge.penalty >= Decimal::ZERO);
            if let (Some(interest), Some(total)) = (charge.interest, charge.total) {
                prop_assert!(interest >= Decimal::ZERO);
                prop_assert_eq!(total, principal + charge.penalty + interest);
            } else {
                prop_assert!(!charge.missing_selic().is_empty());
            }
            if days == 0 {
                prop_assert_eq!(charge.total, Some(principal));
            }
        }
    }
}
