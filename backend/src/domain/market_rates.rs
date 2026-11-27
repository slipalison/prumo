//! As taxas de mercado que viram alvo de rendimento dos FIIs: o **juro real**
//! de ~10 anos (a taxa de compra de um título IPCA+ do Tesouro Direto) e o
//! **CDI acumulado em 12 meses**. Regra pura; quem lê o arquivo do Tesouro é
//! `crate::treasury` e quem lê o CDI guardado é `crate::ceiling`. O porquê está
//! em `docs/analise.md` e `docs/tesouro.md`.

use std::cmp::Reverse;

use rust_decimal::{Decimal, RoundingStrategy};
use time::{Date, Duration, Month};

use super::performance::{daily_rate_accrued, months_before};
use super::treasury::{Bond, BondKind};

/// O prazo de referência do juro real: o título IPCA+ que vence mais perto de
/// 10 anos depois da data-base.
pub const REAL_RATE_HORIZON_YEARS: i32 = 10;

/// O CDI guardado pode começar até tantos dias depois do começo da janela de
/// 12 meses (o primeiro dia útil, um feriado): mais que isso, a janela não
/// está inteira e o acumulado seria falsamente baixo.
pub const CDI_START_SLACK_DAYS: i64 = 7;

/// Quantos dias de CDI ler para ter 12 meses inteiros com folga.
pub const CDI_LOOKBACK_DAYS: i64 = 400;

/// A taxa de um título IPCA+ do Tesouro Direto num dia: `Taxa Compra Manha`,
/// em % ao ano acima do IPCA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RealRate {
    pub bond: Bond,
    pub percent: Decimal,
    pub base_date: Date,
}

/// Se o título paga IPCA mais um juro real: o Tesouro IPCA+ (NTN-B
/// Principal) e o Tesouro IPCA+ com Juros Semestrais (NTN-B).
pub fn is_real_rate_bond(kind: BondKind) -> bool {
    matches!(kind, BondKind::IpcaPlus | BondKind::IpcaPlusWithCoupons)
}

/// `date` mais `years` anos; 29/02 vira 28/02 quando o ano de chegada não é
/// bissexto.
fn plus_years(date: Date, years: i32) -> Date {
    let year = date.year() + years;
    let last = time::util::days_in_month(date.month(), year);
    Date::from_calendar_date(year, date.month(), date.day().min(last))
        .unwrap_or_else(|_| Date::from_calendar_date(year, Month::January, 1).unwrap_or(date))
}

/// O juro real de referência entre as taxas do dia mais novo da tabela:
///
/// 1. só títulos IPCA+ ([`is_real_rate_bond`]) da data-base mais recente (um
///    título que saiu de oferta fica com a linha velha e não entra);
/// 2. a NTN-B (com juros semestrais) vence a NTN-B Principal: é o título que
///    o mercado usa como "juro real de 10 anos";
/// 3. dentro do tipo, o vencimento mais perto de 10 anos depois da data-base;
///    no empate, o mais longo.
///
/// `None` sem nenhum título IPCA+.
pub fn pick_real_rate(rates: &[RealRate]) -> Option<RealRate> {
    let latest = rates
        .iter()
        .filter(|r| is_real_rate_bond(r.bond.kind))
        .map(|r| r.base_date)
        .max()?;
    let target = plus_years(latest, REAL_RATE_HORIZON_YEARS);
    rates
        .iter()
        .filter(|r| {
            is_real_rate_bond(r.bond.kind) && r.base_date == latest && r.bond.maturity > latest
        })
        .min_by_key(|r| {
            (
                r.bond.kind != BondKind::IpcaPlusWithCoupons,
                (r.bond.maturity - target).abs(),
                Reverse(r.bond.maturity),
            )
        })
        .copied()
}

/// O CDI acumulado nos últimos 12 meses, em %.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cdi12m {
    /// Com duas casas.
    pub percent: Decimal,
    /// O primeiro dia da janela.
    pub from: Date,
    /// A data da última taxa publicada que entrou.
    pub to: Date,
}

/// O CDI de 12 meses a partir da série diária (% ao dia, em ordem crescente):
/// a janela termina na última taxa publicada e começa 12 meses antes. `None`
/// quando a série não cobre a janela inteira (com [`CDI_START_SLACK_DAYS`] de
/// folga no começo) ou está vazia.
pub fn cdi_12m(series: &[(Date, Decimal)]) -> Option<Cdi12m> {
    let (first, _) = *series.first()?;
    let (last, _) = *series.last()?;
    // A taxa do dia rende até o dia útil seguinte: a janela vai até o dia
    // seguinte à última taxa, exclusive.
    let end = last.next_day()?;
    let start = months_before(end, 12);
    if first > start + Duration::days(CDI_START_SLACK_DAYS) {
        return None;
    }
    let accrued = daily_rate_accrued(series, start, end);
    Some(Cdi12m {
        percent: (accrued.ret * Decimal::ONE_HUNDRED)
            .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
        from: start,
        to: last,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn rate(kind: BondKind, maturity: Date, percent: Decimal, base_date: Date) -> RealRate {
        RealRate {
            bond: Bond::new(kind, maturity),
            percent,
            base_date,
        }
    }

    /// As linhas do Tesouro Transparente de 26/11/2026 (taxas de compra).
    fn table() -> Vec<RealRate> {
        let day = date!(2026 - 11 - 26);
        let ntnb = BondKind::IpcaPlusWithCoupons;
        let principal = BondKind::IpcaPlus;
        vec![
            rate(ntnb, date!(2035 - 05 - 15), dec!(7.69), day),
            rate(ntnb, date!(2037 - 05 - 15), dec!(7.60), day),
            rate(ntnb, date!(2040 - 08 - 15), dec!(7.46), day),
            rate(ntnb, date!(2032 - 08 - 15), dec!(7.67), day),
            rate(principal, date!(2035 - 05 - 15), dec!(7.73), day),
            rate(principal, date!(2040 - 08 - 15), dec!(7.41), day),
            // Linha do dia anterior: não entra.
            rate(
                ntnb,
                date!(2036 - 11 - 26),
                dec!(9.99),
                date!(2026 - 11 - 25),
            ),
            rate(BondKind::Prefixed, date!(2036 - 11 - 26), dec!(13.5), day),
        ]
    }

    #[test]
    fn the_ntnb_closest_to_ten_years_wins() {
        let picked = pick_real_rate(&table()).unwrap();
        // Data-base 26/11/2026: 10 anos dão 26/11/2036; a NTN-B 2037 está a
        // menos de 6 meses, a 2035 a um ano e meio.
        assert_eq!(picked.bond.kind, BondKind::IpcaPlusWithCoupons);
        assert_eq!(picked.bond.maturity, date!(2037 - 05 - 15));
        assert_eq!(picked.percent, dec!(7.60));
        assert_eq!(picked.base_date, date!(2026 - 11 - 26));
    }

    #[test]
    fn without_ntnb_the_principal_is_the_reference() {
        let only_principal: Vec<RealRate> = table()
            .into_iter()
            .filter(|r| r.bond.kind != BondKind::IpcaPlusWithCoupons)
            .collect();
        let picked = pick_real_rate(&only_principal).unwrap();
        assert_eq!(
            picked.bond,
            Bond::new(BondKind::IpcaPlus, date!(2035 - 05 - 15))
        );
        assert_eq!(picked.percent, dec!(7.73));
        assert_eq!(pick_real_rate(&[]), None);
        assert_eq!(
            pick_real_rate(&[rate(
                BondKind::Selic,
                date!(2031 - 03 - 01),
                dec!(0.1),
                date!(2026 - 11 - 26)
            )]),
            None
        );
    }

    #[test]
    fn on_a_tie_the_longer_bond_wins() {
        let day = date!(2026 - 01 - 15);
        let ntnb = BondKind::IpcaPlusWithCoupons;
        // 10 anos dão 15/01/2036: os dois vencem a 10 dias dele.
        let picked = pick_real_rate(&[
            rate(ntnb, date!(2036 - 01 - 05), dec!(7), day),
            rate(ntnb, date!(2036 - 01 - 25), dec!(6), day),
        ])
        .unwrap();
        assert_eq!(picked.bond.maturity, date!(2036 - 01 - 25));
    }

    #[test]
    fn ten_years_after_a_leap_day_is_the_28th() {
        assert_eq!(plus_years(date!(2028 - 02 - 29), 10), date!(2038 - 02 - 28));
        assert_eq!(plus_years(date!(2026 - 11 - 26), 10), date!(2036 - 11 - 26));
    }

    /// Uma taxa diária constante em todos os dias da janela.
    fn flat(from: Date, to: Date, daily: Decimal) -> Vec<(Date, Decimal)> {
        let mut out = Vec::new();
        let mut day = from;
        while day <= to {
            out.push((day, daily));
            day = day.next_day().unwrap();
        }
        out
    }

    #[test]
    fn the_cdi_of_twelve_months_compounds_the_daily_rate() {
        let series = flat(date!(2025 - 11 - 20), date!(2026 - 11 - 25), dec!(0.05));
        let cdi = cdi_12m(&series).unwrap();
        assert_eq!(cdi.to, date!(2026 - 11 - 25));
        assert_eq!(cdi.from, date!(2025 - 11 - 26));
        // 365 dias a 0,05% ao dia: 1,0005^365 − 1 = 20,016…%.
        assert_eq!(cdi.percent, dec!(20.02));
    }

    #[test]
    fn a_series_that_does_not_cover_the_year_has_no_cdi() {
        let short = flat(date!(2026 - 06 - 01), date!(2026 - 11 - 25), dec!(0.05));
        assert_eq!(cdi_12m(&short), None);
        assert_eq!(cdi_12m(&[]), None);
        // Começar uns dias depois do início da janela (feriado) ainda vale.
        let late = flat(date!(2025 - 12 - 01), date!(2026 - 11 - 25), dec!(0.05));
        assert!(cdi_12m(&late).is_some());
    }

    proptest! {
        /// Uma taxa diária maior nunca dá um CDI de 12 meses menor.
        #[test]
        fn a_higher_daily_rate_never_lowers_the_twelve_months(low in 1i64..100, extra in 0i64..100) {
            let from = date!(2025 - 11 - 01);
            let to = date!(2026 - 11 - 25);
            let base = cdi_12m(&flat(from, to, Decimal::new(low, 3))).unwrap();
            let higher = cdi_12m(&flat(from, to, Decimal::new(low + extra, 3))).unwrap();
            prop_assert!(higher.percent >= base.percent);
            prop_assert!(base.percent > Decimal::ZERO);
        }

        /// O escolhido é sempre um título IPCA+ do dia mais novo.
        #[test]
        fn the_pick_is_an_ipca_bond_of_the_latest_day(
            rows in proptest::collection::vec((0usize..3, 2027i32..2065, 0i64..5, 100i64..1500), 1..20),
        ) {
            let kinds = [BondKind::IpcaPlus, BondKind::IpcaPlusWithCoupons, BondKind::Prefixed];
            let rates: Vec<RealRate> = rows
                .iter()
                .map(|(kind, year, back, cents)| rate(
                    kinds[*kind],
                    Date::from_calendar_date(*year, Month::May, 15).unwrap(),
                    Decimal::new(*cents, 2),
                    date!(2026 - 11 - 26) - Duration::days(*back),
                ))
                .collect();
            match pick_real_rate(&rates) {
                Some(picked) => {
                    prop_assert!(is_real_rate_bond(picked.bond.kind));
                    let latest = rates.iter().filter(|r| is_real_rate_bond(r.bond.kind)).map(|r| r.base_date).max().unwrap();
                    prop_assert_eq!(picked.base_date, latest);
                    let ntnb_today = rates.iter().any(|r| r.bond.kind == BondKind::IpcaPlusWithCoupons && r.base_date == latest);
                    prop_assert_eq!(picked.bond.kind == BondKind::IpcaPlusWithCoupons, ntnb_today);
                }
                None => prop_assert!(rates.iter().all(|r| !is_real_rate_bond(r.bond.kind))),
            }
        }
    }
}
