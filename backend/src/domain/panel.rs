//! Painel da Carteira: a faixa de indicadores do mês e a referência do dia.
//! Regra pura, sem banco nem rede; método e limites em `docs/painel.md`.
//!
//! - **Mês**: tudo sai da série diária da carteira (`portfolio_snapshots`).
//!   A rentabilidade é a variação da cota (TWR, `domain::performance`), então
//!   aporte não vira rendimento. Proventos e aportes são a soma dos dias do mês.
//! - **Dia**: o fechamento anterior de um ativo é o preço guardado na última
//!   foto diária anterior à data da cotação dele. O painel compara a cotação ao
//!   vivo com esse número.

use rust_decimal::Decimal;
use time::{Date, Duration};

use super::performance::{period_return, Position, QuotaPoint};

/// Quantos dias a foto de referência pode ficar atrás da data da cotação: além
/// disso a "variação do dia" mediria outra coisa (série com buraco).
pub const MAX_REFERENCE_GAP_DAYS: i64 = 7;
/// Cotação mais velha que isto não é "de hoje": o ativo fica fora do painel do dia.
pub const MAX_STALE_PRICE_DAYS: i64 = 5;

/// Um dia da série da carteira, no que o painel usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayPoint {
    pub date: Date,
    pub market_value: Decimal,
    /// Aporte (positivo) ou retirada (negativo) do dia.
    pub net_flow: Decimal,
    /// Proventos líquidos recebidos no dia.
    pub income: Decimal,
    pub quota_value: Decimal,
}

/// O patrimônio no último dia de um mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthEnd {
    pub date: Date,
    pub value: Decimal,
}

/// A rentabilidade (TWR) de um mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthReturn {
    /// Fechamento de onde se mediu.
    pub from: Date,
    pub to: Date,
    /// Fração (0,02 = 2%).
    pub ret: Decimal,
    /// `false`: a série começou no meio do mês e a janela ficou mais curta.
    pub full: bool,
}

/// O que entrou num mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonthTotals {
    /// Proventos líquidos recebidos.
    pub income: Decimal,
    /// Aportes menos retiradas.
    pub contribution: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthSummary {
    /// O último dia da série (hoje, com a foto parcial).
    pub as_of: Date,
    pub patrimony: Decimal,
    /// O patrimônio no fim do mês anterior; `None` se a série começou depois.
    pub previous_month_end: Option<MonthEnd>,
    pub current_return: Option<MonthReturn>,
    pub previous_return: Option<MonthReturn>,
    pub current: MonthTotals,
    pub previous: MonthTotals,
}

fn first_of_month(date: Date) -> Date {
    date.replace_day(1).unwrap_or(date)
}

fn totals(points: &[DayPoint], from: Date, to: Date) -> MonthTotals {
    points
        .iter()
        .filter(|p| p.date >= from && p.date <= to)
        .fold(MonthTotals::default(), |acc, p| MonthTotals {
            income: acc.income + p.income,
            contribution: acc.contribution + p.net_flow,
        })
}

fn month_return(quotas: &[QuotaPoint], from: Date, to: Date) -> Option<MonthReturn> {
    period_return(quotas, from, to)
        .filter(|w| w.to > w.from)
        .map(|w| MonthReturn {
            from: w.from,
            to: w.to,
            ret: w.ret,
            full: w.from <= from,
        })
}

/// O resumo do mês de `today` contra o mês anterior. `None` sem série.
/// `points` vem em ordem de data.
pub fn month_summary(points: &[DayPoint], today: Date) -> Option<MonthSummary> {
    let last = points.iter().rev().find(|p| p.date <= today)?;
    let month_start = first_of_month(today);
    let previous_end = month_start - Duration::days(1);
    let previous_start = first_of_month(previous_end);
    let before_previous_end = previous_start - Duration::days(1);
    let quotas: Vec<QuotaPoint> = points
        .iter()
        .map(|p| QuotaPoint {
            date: p.date,
            value: p.quota_value,
        })
        .collect();
    Some(MonthSummary {
        as_of: last.date,
        patrimony: last.market_value,
        previous_month_end: points
            .iter()
            .rev()
            .find(|p| p.date <= previous_end)
            .map(|p| MonthEnd {
                date: p.date,
                value: p.market_value,
            }),
        current_return: month_return(&quotas, previous_end, today),
        previous_return: month_return(&quotas, before_previous_end, previous_end),
        current: totals(points, month_start, today),
        previous: totals(points, previous_start, previous_end),
    })
}

/// O fechamento anterior de um ativo, e de que dia ele é.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayReference {
    pub previous_close: Decimal,
    pub date: Date,
}

/// A referência do dia de um ativo: o preço dele na foto de `snapshot_date`
/// (a última anterior à data da cotação, `price_date`). `None` quando o ativo
/// não tem preço de mercado na foto (por valor, sem cotação), quando a foto não
/// é anterior à cotação, quando está longe demais dela ou quando a própria
/// cotação é velha (`today` − `price_date`).
pub fn day_reference(
    position: Option<&Position>,
    snapshot_date: Date,
    price_date: Date,
    today: Date,
) -> Option<DayReference> {
    let position = position.filter(|p| !p.by_value)?;
    let close = position.unit_price.filter(|p| *p > Decimal::ZERO)?;
    let gap = (price_date - snapshot_date).whole_days();
    let stale = (today - price_date).whole_days();
    if !(1..=MAX_REFERENCE_GAP_DAYS).contains(&gap) || stale > MAX_STALE_PRICE_DAYS {
        return None;
    }
    Some(DayReference {
        previous_close: close,
        date: snapshot_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use uuid::Uuid;

    fn point(
        date: Date,
        value: Decimal,
        flow: Decimal,
        income: Decimal,
        quota: Decimal,
    ) -> DayPoint {
        DayPoint {
            date,
            market_value: value,
            net_flow: flow,
            income,
            quota_value: quota,
        }
    }

    /// Outubro e novembro de uma carteira fictícia.
    fn series() -> Vec<DayPoint> {
        vec![
            point(
                date!(2026 - 09 - 30),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 10 - 15),
                dec!(1250),
                dec!(100),
                dec!(5),
                dec!(1.10),
            ),
            point(
                date!(2026 - 10 - 31),
                dec!(1300),
                dec!(0),
                dec!(7),
                dec!(1.15),
            ),
            point(
                date!(2026 - 11 - 10),
                dec!(1500),
                dec!(200),
                dec!(4),
                dec!(1.20),
            ),
            point(
                date!(2026 - 11 - 25),
                dec!(1450),
                dec!(0),
                dec!(0),
                dec!(1.265),
            ),
        ]
    }

    #[test]
    fn summarizes_the_month_against_the_previous() {
        let s = month_summary(&series(), date!(2026 - 11 - 25)).unwrap();
        assert_eq!(s.as_of, date!(2026 - 11 - 25));
        assert_eq!(s.patrimony, dec!(1450));
        assert_eq!(
            s.previous_month_end,
            Some(MonthEnd {
                date: date!(2026 - 10 - 31),
                value: dec!(1300)
            })
        );
        // Novembro: cota de 1,15 (31/10) a 1,265.
        let current = s.current_return.unwrap();
        assert_eq!(current.ret, dec!(0.10));
        assert!(current.full);
        // Outubro: cota de 1,00 (30/09) a 1,15.
        let previous = s.previous_return.unwrap();
        assert_eq!(previous.ret, dec!(0.15));
        assert!(previous.full);
        assert_eq!(s.current.income, dec!(4));
        assert_eq!(s.current.contribution, dec!(200));
        assert_eq!(s.previous.income, dec!(12));
        assert_eq!(s.previous.contribution, dec!(100));
    }

    #[test]
    fn a_series_that_starts_mid_month_has_a_shorter_window() {
        let points = vec![
            point(
                date!(2026 - 10 - 20),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 10 - 31),
                dec!(1100),
                dec!(0),
                dec!(0),
                dec!(1.1),
            ),
            point(
                date!(2026 - 11 - 02),
                dec!(1100),
                dec!(0),
                dec!(0),
                dec!(1.1),
            ),
        ];
        let s = month_summary(&points, date!(2026 - 11 - 02)).unwrap();
        let previous = s.previous_return.unwrap();
        assert!(!previous.full);
        assert_eq!(previous.from, date!(2026 - 10 - 20));
        assert_eq!(previous.ret, dec!(0.1));
        assert_eq!(s.current_return.unwrap().ret, dec!(0));
    }

    #[test]
    fn a_series_born_this_month_has_no_previous_month() {
        let points = vec![
            point(
                date!(2026 - 11 - 03),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 11 - 25),
                dec!(1050),
                dec!(0),
                dec!(2),
                dec!(1.05),
            ),
        ];
        let s = month_summary(&points, date!(2026 - 11 - 25)).unwrap();
        assert_eq!(s.previous_month_end, None);
        assert_eq!(s.previous_return, None);
        assert_eq!(s.previous, MonthTotals::default());
        let current = s.current_return.unwrap();
        assert!(!current.full);
        assert_eq!(current.ret, dec!(0.05));
    }

    #[test]
    fn a_single_day_has_no_return_to_measure() {
        let points = vec![point(
            date!(2026 - 11 - 25),
            dec!(1000),
            dec!(1000),
            dec!(0),
            dec!(1),
        )];
        let s = month_summary(&points, date!(2026 - 11 - 25)).unwrap();
        assert_eq!(s.current_return, None);
        assert_eq!(s.current.contribution, dec!(1000));
        assert_eq!(month_summary(&[], date!(2026 - 11 - 25)), None);
    }

    #[test]
    fn january_looks_back_to_december_and_november() {
        let points = vec![
            point(
                date!(2026 - 11 - 28),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 12 - 30),
                dec!(1020),
                dec!(0),
                dec!(3),
                dec!(1.02),
            ),
            point(
                date!(2027 - 01 - 15),
                dec!(1071),
                dec!(0),
                dec!(1),
                dec!(1.071),
            ),
        ];
        let s = month_summary(&points, date!(2027 - 01 - 15)).unwrap();
        assert_eq!(s.previous_month_end.unwrap().date, date!(2026 - 12 - 30));
        assert_eq!(s.current_return.unwrap().ret, dec!(0.05));
        assert_eq!(s.previous_return.unwrap().ret, dec!(0.02));
        assert_eq!(s.previous.income, dec!(3));
    }

    fn position(by_value: bool, price: Option<Decimal>) -> Position {
        Position {
            asset_id: Uuid::nil(),
            by_value,
            quantity: dec!(10),
            unit_price: price,
        }
    }

    #[test]
    fn day_reference_is_the_price_of_the_previous_snapshot() {
        let p = position(false, Some(dec!(10.5)));
        let r = day_reference(
            Some(&p),
            date!(2026 - 11 - 24),
            date!(2026 - 11 - 25),
            date!(2026 - 11 - 25),
        )
        .unwrap();
        assert_eq!(r.previous_close, dec!(10.5));
        assert_eq!(r.date, date!(2026 - 11 - 24));
        // Fim de semana: a cotação é de sexta, a referência é a quinta.
        assert!(day_reference(
            Some(&p),
            date!(2026 - 11 - 19),
            date!(2026 - 11 - 20),
            date!(2026 - 11 - 22)
        )
        .is_some());
    }

    #[test]
    fn day_reference_refuses_what_would_measure_something_else() {
        let today = date!(2026 - 11 - 25);
        let ok = position(false, Some(dec!(10)));
        // Sem posição na foto, por valor, sem preço, preço zero.
        assert!(day_reference(None, date!(2026 - 11 - 24), today, today).is_none());
        assert!(day_reference(
            Some(&position(true, Some(dec!(10)))),
            date!(2026 - 11 - 24),
            today,
            today
        )
        .is_none());
        assert!(day_reference(
            Some(&position(false, None)),
            date!(2026 - 11 - 24),
            today,
            today
        )
        .is_none());
        assert!(day_reference(
            Some(&position(false, Some(dec!(0)))),
            date!(2026 - 11 - 24),
            today,
            today
        )
        .is_none());
        // A foto não é anterior à cotação.
        assert!(day_reference(Some(&ok), today, today, today).is_none());
        // Foto longe demais; cotação velha.
        assert!(day_reference(Some(&ok), date!(2026 - 11 - 10), today, today).is_none());
        assert!(day_reference(
            Some(&ok),
            date!(2026 - 11 - 14),
            date!(2026 - 11 - 15),
            today
        )
        .is_none());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// O total de cada mês é a soma dos dias daquele mês (nada some nem conta
        /// em dobro), venha a data de onde vier.
        #[test]
        fn month_totals_partition_the_series(
            flows in proptest::collection::vec((-500i64..2000, 0i64..100), 1..60),
        ) {
            let start = date!(2026 - 09 - 20);
            let mut quota = dec!(1);
            let points: Vec<DayPoint> = flows
                .iter()
                .enumerate()
                .map(|(i, (flow, income))| {
                    quota += dec!(0.001);
                    point(start + Duration::days(i as i64), dec!(1000), Decimal::from(*flow), Decimal::from(*income), quota)
                })
                .collect();
            let today = points.last().unwrap().date;
            let s = month_summary(&points, today).unwrap();
            let (year, month) = (today.year(), u8::from(today.month()));
            let key = |p: &DayPoint| (p.date.year(), u8::from(p.date.month()));
            let previous_key = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
            let sum = |wanted: (i32, u8), pick: fn(&DayPoint) -> Decimal| -> Decimal {
                points.iter().filter(|p| key(p) == wanted).map(pick).sum()
            };
            prop_assert_eq!(s.current.income, sum((year, month), |p| p.income));
            prop_assert_eq!(s.current.contribution, sum((year, month), |p| p.net_flow));
            prop_assert_eq!(s.previous.income, sum(previous_key, |p| p.income));
            prop_assert_eq!(s.previous.contribution, sum(previous_key, |p| p.net_flow));
            if let Some(r) = s.current_return {
                prop_assert!(r.from < r.to);
            }
            if let Some(r) = s.previous_return {
                prop_assert!(r.from < r.to);
            }
        }

        /// Sem foto anterior à cotação, nunca há referência.
        #[test]
        fn no_reference_unless_the_snapshot_precedes_the_price(gap in -10i64..15, stale in 0i64..10) {
            let price_date = date!(2026 - 11 - 20);
            let snapshot_date = price_date - Duration::days(gap);
            let today = price_date + Duration::days(stale);
            let p = position(false, Some(dec!(10)));
            let r = day_reference(Some(&p), snapshot_date, price_date, today);
            if !(1..=MAX_REFERENCE_GAP_DAYS).contains(&gap) || stale > MAX_STALE_PRICE_DAYS {
                prop_assert!(r.is_none());
            } else {
                prop_assert!(r.is_some());
            }
        }
    }
}
