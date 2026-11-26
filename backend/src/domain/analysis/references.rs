//! Referências de preço públicas: **Bazin** e **Graham**. São fórmulas
//! conhecidas e abertas, mostradas como referência — não são preço justo, nem
//! alvo, nem recomendação de compra. O Prumo não diz "oportunidade".
//!
//! - **Bazin**: a média anual dos dividendos por cota (até 5 anos de histórico)
//!   dividida pelo rendimento-alvo da pessoa (padrão 6%). É o preço em que a
//!   cota pagaria o rendimento-alvo se repetisse a média.
//! - **Graham**: `√(22,5 × LPA × VPA)`. Precisa do lucro (LPA) e do patrimônio
//!   (VPA) por ação, que vêm dos fundamentos da CVM; sem eles, fica de fora.

use rust_decimal::{Decimal, MathematicalOps};
use time::Date;

use crate::domain::allocation::round_cents;
use crate::domain::income::DividendPoint;
use crate::domain::performance::months_before;

/// Rendimento-alvo de fábrica do Bazin, em percentual.
pub const DEFAULT_BAZIN_YIELD: Decimal = Decimal::from_parts(6, 0, 0, false, 0);
/// Quantos anos de dividendo entram, no máximo.
pub const MAX_YEARS: u32 = 5;
const HUNDRED: Decimal = Decimal::ONE_HUNDRED;
/// A constante de Graham: 15 (P/L máximo) × 1,5 (P/VP máximo).
const GRAHAM_FACTOR: Decimal = Decimal::from_parts(225, 0, 0, false, 1);

/// Confere o rendimento-alvo que a pessoa mandou (percentual de 0 a 100,
/// exclusive o zero, até duas casas). A mensagem vai sem acento.
pub fn validate_bazin_yield(value: Decimal) -> Result<Decimal, String> {
    if value > Decimal::ZERO && value <= HUNDRED && value.normalize().scale() <= 2 {
        Ok(value.normalize())
    } else {
        Err("O rendimento-alvo precisa ser um percentual maior que 0 e ate 100, com ate duas casas."
            .to_owned())
    }
}

/// O teto de Bazin de um papel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bazin {
    /// Dividendo anual médio por cota, em reais.
    pub annual_average: Decimal,
    /// Quantos anos de 12 meses entraram na média (1 a 5).
    pub years: u32,
    /// `média ÷ rendimento-alvo`, em reais por cota.
    pub price: Decimal,
}

/// O Bazin de um papel pelo histórico de dividendos por cota (já em reais).
///
/// A média é sobre janelas de 12 meses que terminam hoje, hoje − 12 meses, e
/// assim por diante, até [`MAX_YEARS`]. A janela de agora entra sempre; as
/// mais velhas só entram **inteiras**, isto é, se o primeiro dividendo
/// conhecido é de antes do início delas — uma janela em que o papel ainda nem
/// existia (ou o histórico não chega) não vira "zero dividendo". `None` sem
/// nenhum dividendo, ou com média zero.
pub fn bazin(points: &[DividendPoint], today: Date, target_yield: Decimal) -> Option<Bazin> {
    if target_yield <= Decimal::ZERO {
        return None;
    }
    let first = points.iter().map(|p| p.ex_date).min()?;
    let mut total = Decimal::ZERO;
    let mut years = 0u32;
    for back in 0..MAX_YEARS {
        let end = months_before(today, 12 * back);
        let start = months_before(today, 12 * (back + 1));
        if back > 0 && first > start {
            break;
        }
        total += points
            .iter()
            .filter(|p| p.ex_date > start && p.ex_date <= end)
            .map(|p| p.amount)
            .sum::<Decimal>();
        years += 1;
    }
    let annual_average = total / Decimal::from(years);
    (annual_average > Decimal::ZERO).then(|| Bazin {
        annual_average: annual_average.round_dp(8),
        years,
        price: round_cents(annual_average * HUNDRED / target_yield),
    })
}

/// O número de Graham: `√(22,5 × LPA × VPA)`, em reais por ação. `None` com
/// lucro ou patrimônio por ação zero ou negativo (a raiz não existe).
pub fn graham(earnings_per_share: Decimal, book_value_per_share: Decimal) -> Option<Decimal> {
    if earnings_per_share <= Decimal::ZERO || book_value_per_share <= Decimal::ZERO {
        return None;
    }
    let product = GRAHAM_FACTOR
        .checked_mul(earnings_per_share)?
        .checked_mul(book_value_per_share)?;
    product.sqrt().map(round_cents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const TODAY: Date = date!(2026 - 11 - 25);

    fn january_15(year: i32) -> Date {
        Date::from_calendar_date(year, time::Month::January, 15).unwrap()
    }

    fn point(day: Date, amount: Decimal) -> DividendPoint {
        DividendPoint {
            ex_date: day,
            amount,
        }
    }

    #[test]
    fn bazin_is_the_average_dividend_over_the_target_yield() {
        // Três anos completos: 1,20; 0,90 e 0,60 por cota => média 0,90.
        let points = [
            point(date!(2026 - 03 - 10), dec!(1.20)),
            point(date!(2025 - 03 - 10), dec!(0.90)),
            point(date!(2024 - 03 - 10), dec!(0.60)),
            // O primeiro dividendo conhecido é de antes do início da 3ª janela.
            point(date!(2023 - 01 - 10), dec!(0.10)),
        ];
        let bazin = bazin(&points, TODAY, dec!(6)).unwrap();
        // A janela de 2022 a 2023 não entra: o primeiro dividendo conhecido
        // (10/01/2023) é depois do início dela.
        assert_eq!(bazin.years, 3);
        assert_eq!(bazin.annual_average, dec!(0.9));
        assert_eq!(bazin.price, dec!(15));
    }

    #[test]
    fn a_new_payer_uses_only_the_trailing_year() {
        let points = [
            point(date!(2026 - 08 - 10), dec!(0.30)),
            point(date!(2026 - 02 - 10), dec!(0.30)),
        ];
        let bazin = bazin(&points, TODAY, dec!(6)).unwrap();
        assert_eq!(bazin.years, 1);
        assert_eq!(bazin.price, dec!(10));
    }

    #[test]
    fn no_dividends_means_no_reference() {
        assert_eq!(bazin(&[], TODAY, dec!(6)), None);
        // Dividendo só do futuro (ex depois de hoje): média zero.
        assert_eq!(
            bazin(&[point(date!(2026 - 12 - 01), dec!(1))], TODAY, dec!(6)),
            None
        );
        assert_eq!(bazin(&[point(TODAY, dec!(1))], TODAY, Decimal::ZERO), None);
    }

    #[test]
    fn the_history_is_capped_at_five_years() {
        // Um dividendo de 1 por ano, de 2018 a 2026: só 5 janelas entram.
        let points: Vec<DividendPoint> = (2018..=2026)
            .map(|y| point(january_15(y), dec!(1)))
            .collect();
        let bazin = bazin(&points, TODAY, dec!(10)).unwrap();
        assert_eq!(bazin.years, 5);
        assert_eq!(bazin.annual_average, dec!(1));
        assert_eq!(bazin.price, dec!(10));
    }

    #[test]
    fn graham_is_the_root_of_22_5_eps_bvps() {
        // 22,5 × 4 × 10 = 900 => 30.
        assert_eq!(graham(dec!(4), dec!(10)), Some(dec!(30)));
        assert_eq!(graham(dec!(0), dec!(10)), None);
        assert_eq!(graham(dec!(-1), dec!(10)), None);
        assert_eq!(graham(dec!(4), dec!(-10)), None);
    }

    #[test]
    fn the_target_yield_is_validated() {
        assert_eq!(validate_bazin_yield(dec!(6.00)), Ok(dec!(6)));
        assert!(validate_bazin_yield(dec!(0)).is_err());
        assert!(validate_bazin_yield(dec!(100.01)).is_err());
        assert!(validate_bazin_yield(dec!(5.555)).is_err());
        assert_eq!(validate_bazin_yield(DEFAULT_BAZIN_YIELD), Ok(dec!(6)));
    }

    proptest! {
        /// Um rendimento-alvo maior dá um teto menor, e o teto nunca é negativo.
        #[test]
        fn a_higher_target_yield_lowers_the_price(
            cents in proptest::collection::vec(1i64..500, 1..12),
            low in 1u32..50,
            extra in 1u32..50,
        ) {
            let points: Vec<DividendPoint> = cents
                .iter()
                .enumerate()
                .map(|(i, c)| point(TODAY - time::Duration::days(30 * i64::try_from(i).unwrap() + 1), Decimal::new(*c, 2)))
                .collect();
            let cheap = bazin(&points, TODAY, Decimal::from(low)).unwrap();
            let dear = bazin(&points, TODAY, Decimal::from(low + extra)).unwrap();
            prop_assert!(cheap.price >= dear.price);
            prop_assert!(dear.price >= Decimal::ZERO);
            prop_assert!(cheap.years >= 1 && cheap.years <= MAX_YEARS);
        }

        /// Mais lucro por ação nunca baixa o número de Graham.
        #[test]
        fn graham_grows_with_earnings(eps in 1i64..10_000, bvps in 1i64..10_000, more in 1i64..1000) {
            let base = graham(Decimal::new(eps, 2), Decimal::new(bvps, 2)).unwrap();
            let bigger = graham(Decimal::new(eps + more, 2), Decimal::new(bvps, 2)).unwrap();
            prop_assert!(bigger >= base);
        }
    }
}
