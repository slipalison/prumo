//! O progresso da meta de renda mensal nos dois modos: a **média** por mês
//! (a regra de sempre, `domain::income::goal_progress`) e o **piso**, o menor
//! mês de 12. Só o recebido conquista a meta; sem nada recebido, a barra usa
//! a estimativa e diz que é estimativa.
//!
//! No piso, o recebido conta pelos 12 meses **fechados** antes do mês de hoje
//! (o mês corrente ainda tem pagamento por vir e seria sempre o pior). A
//! estimativa do piso é a estimativa da Renda × o peso do pior mês no ano da
//! carteira ([`Shape::min`]).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use time::Date;

use super::goal::IncomeMode;
use super::income::Shape;
use super::{to_decimal, to_f64};
use crate::domain::allocation::percent;
use crate::domain::income::{goal_progress, Entry, GoalProgress, ProgressBasis, YearMonth};

/// Quantos meses fechados o piso olha.
pub const FLOOR_MONTHS: i32 = 12;

/// O que foi recebido, como a meta lê.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Received {
    /// A média mensal dos 12 meses que terminam no mês de hoje (a da Renda).
    pub average: Decimal,
    /// O menor mês dos 12 fechados; `None` quando nada foi recebido neles.
    pub floor: Option<Decimal>,
}

/// A estimativa, como a meta lê.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estimated {
    pub average: Decimal,
    pub floor: Decimal,
}

impl Estimated {
    /// A estimativa mensal da Renda e o piso pelo calendário da carteira.
    pub fn of(monthly: Decimal, shape: &Shape) -> Self {
        Self {
            average: monthly,
            floor: to_decimal(to_f64(monthly) * shape.min(), 2),
        }
    }
}

/// O menor mês recebido nos 12 fechados antes do mês de `today`, pela data de
/// pagamento. `None` sem nenhum recebido nesses meses.
pub fn received_floor(entries: &[Entry], today: Date) -> Option<Decimal> {
    let current = YearMonth::of(today);
    let first = current.shift(-FLOOR_MONTHS);
    let mut months: BTreeMap<YearMonth, Decimal> = (0..FLOOR_MONTHS)
        .map(|i| (first.shift(i), Decimal::ZERO))
        .collect();
    let mut any = false;
    for entry in entries.iter().filter(|e| e.pay_date <= today) {
        if let Some(slot) = months.get_mut(&YearMonth::of(entry.pay_date)) {
            *slot += entry.net;
            any = true;
        }
    }
    any.then(|| months.values().copied().min().unwrap_or(Decimal::ZERO))
}

/// O progresso da meta no modo dela.
pub fn progress(
    mode: IncomeMode,
    target: Decimal,
    received: Received,
    estimated: Estimated,
) -> GoalProgress {
    match mode {
        IncomeMode::Average => goal_progress(target, received.average, estimated.average),
        IncomeMode::Floor => {
            let (current, basis) = match received.floor {
                Some(floor) => (floor, ProgressBasis::Received),
                None => (estimated.floor, ProgressBasis::Estimate),
            };
            GoalProgress {
                current_monthly: current,
                basis,
                percent: percent(current, target),
                reached: basis == ProgressBasis::Received
                    && target > Decimal::ZERO
                    && current >= target,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn paid(pay_date: Date, net: Decimal) -> Entry {
        Entry {
            pay_date,
            net,
            payer_key: "A".into(),
            payer_label: "A".into(),
        }
    }

    const TODAY: Date = date!(2026 - 11 - 27);

    #[test]
    fn the_floor_is_the_weakest_closed_month() {
        // Um pagamento de 100 em cada mês de nov/2025 a out/2026, e 40 em março.
        let mut entries: Vec<Entry> = (0..12)
            .map(|i| {
                let month = YearMonth {
                    year: 2025,
                    month: 11,
                }
                .shift(i);
                let day = Date::from_calendar_date(
                    month.year,
                    time::Month::try_from(month.month).unwrap(),
                    15,
                )
                .unwrap();
                paid(
                    day,
                    if month.month == 3 {
                        dec!(40)
                    } else {
                        dec!(100)
                    },
                )
            })
            .collect();
        assert_eq!(received_floor(&entries, TODAY), Some(dec!(40)));
        // O mês de hoje (novembro) não conta, nem o que ainda vai cair.
        entries.push(paid(date!(2026 - 11 - 02), dec!(1)));
        entries.push(paid(date!(2026 - 12 - 10), dec!(1)));
        assert_eq!(received_floor(&entries, TODAY), Some(dec!(40)));
        // Um mês sem nada derruba o piso a zero.
        entries.retain(|e| e.pay_date.month() != time::Month::June);
        assert_eq!(received_floor(&entries, TODAY), Some(dec!(0)));
        // Sem nada recebido nos 12 meses fechados: sem piso.
        assert_eq!(
            received_floor(&[paid(date!(2024 - 01 - 10), dec!(5))], TODAY),
            None
        );
        assert_eq!(received_floor(&[], TODAY), None);
    }

    #[test]
    fn average_mode_is_the_rule_of_always() {
        let p = progress(
            IncomeMode::Average,
            dec!(1000),
            Received {
                average: dec!(250),
                floor: Some(dec!(10)),
            },
            Estimated::default(),
        );
        assert_eq!(p, goal_progress(dec!(1000), dec!(250), dec!(0)));
    }

    #[test]
    fn floor_mode_prefers_the_received_floor() {
        let received = Received {
            average: dec!(900),
            floor: Some(dec!(600)),
        };
        let estimated = Estimated {
            average: dec!(2000),
            floor: dec!(1500),
        };
        let p = progress(IncomeMode::Floor, dec!(500), received, estimated);
        assert_eq!(p.basis, ProgressBasis::Received);
        assert_eq!((p.current_monthly, p.reached), (dec!(600), true));
        assert_eq!(p.percent, dec!(120.00));

        let none = progress(
            IncomeMode::Floor,
            dec!(500),
            Received {
                average: dec!(0),
                floor: None,
            },
            estimated,
        );
        assert_eq!(none.basis, ProgressBasis::Estimate);
        assert_eq!(none.current_monthly, dec!(1500));
        assert!(!none.reached, "estimativa não conquista meta");
    }

    #[test]
    fn the_estimated_floor_uses_the_weakest_month_of_the_shape() {
        let shape =
            Shape::from_payments([(date!(2026 - 05 - 10), 1.0), (date!(2026 - 06 - 10), 1.0)]);
        let e = Estimated::of(dec!(100), &shape);
        assert_eq!(e.average, dec!(100));
        assert_eq!(e.floor, dec!(0));
        assert_eq!(Estimated::of(dec!(100), &Shape::uniform()).floor, dec!(100));
    }
}
