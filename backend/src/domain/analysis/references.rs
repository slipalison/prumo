//! Referências de preço públicas: **Bazin** e **Graham**. São fórmulas
//! conhecidas e abertas, mostradas como referência — não são preço justo, nem
//! alvo, nem recomendação de compra. O Prumo não diz "oportunidade".
//!
//! - **Bazin**: a média anual dos dividendos por cota (até 5 anos de histórico)
//!   dividida pelo rendimento-alvo da pessoa (padrão 6%). É o preço em que a
//!   cota pagaria o rendimento-alvo se repetisse a média.
//! - **Graham**: `√(22,5 × LPA × VPA)`. Precisa do lucro (LPA) e do patrimônio
//!   (VPA) por ação, que vêm dos fundamentos da CVM; sem eles, fica de fora.
//! - **Pelas duas** ([`combined_ceiling`]): o menor dos dois, o valor que a tela
//!   oferece como teto.

use rust_decimal::{Decimal, MathematicalOps};
use time::Date;

use crate::domain::allocation::round_cents;
use crate::domain::fundamentals::{AssetFundamentals, CompanyMetrics};
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

/// Lucro e patrimônio por ação (reais) de uma companhia, quando a B3 informou
/// as ações e os dois números existem. É o que o Graham pede.
pub fn company_per_share(metrics: &CompanyMetrics) -> Option<(Decimal, Decimal)> {
    metrics
        .earnings_per_share
        .value()
        .zip(metrics.book_value_per_share.value())
}

/// As duas referências de preço de um ativo, as que existem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PriceReferences {
    pub bazin: Option<Bazin>,
    pub graham: Option<Decimal>,
}

impl PriceReferences {
    /// Se o ativo tem ao menos uma das duas.
    pub fn any(&self) -> bool {
        self.bazin.is_some() || self.graham.is_some()
    }
}

/// As referências de um ativo a partir do que já está guardado: o histórico de
/// dividendos por cota (em reais; `None` se nunca foi buscado) e os
/// fundamentos. A Análise e o aviso do aporte chamam esta mesma conta.
pub fn price_references(
    dividends: Option<&[DividendPoint]>,
    fundamentals: Option<&AssetFundamentals>,
    today: Date,
    target_yield: Decimal,
) -> PriceReferences {
    let bazin = dividends.and_then(|points| bazin(points, today, target_yield));
    // O Graham pede lucro e patrimônio por ação, os dois positivos.
    let graham = match fundamentals {
        Some(AssetFundamentals::Company(metrics)) => company_per_share(metrics),
        _ => None,
    }
    .and_then(|(earnings, book)| graham(earnings, book));
    PriceReferences { bazin, graham }
}

/// O teto pelas duas referências: o **menor** dos valores que existem. Abaixo
/// dele, o preço passa nos dois critérios (dividendos e lucro com patrimônio);
/// a média deixaria passar um preço acima de um deles. Com uma só, é ela.
pub fn combined_ceiling(bazin: Option<Decimal>, graham: Option<Decimal>) -> Option<Decimal> {
    match (bazin, graham) {
        (Some(bazin), Some(graham)) => Some(bazin.min(graham)),
        (one, other) => one.or(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::fundamentals::{Figure, Gap, Template, Unsupported, YearCount};
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
    fn the_combined_ceiling_is_the_lower_of_the_two() {
        assert_eq!(
            combined_ceiling(Some(dec!(18.69)), Some(dec!(77.77))),
            Some(dec!(18.69))
        );
        assert_eq!(
            combined_ceiling(Some(dec!(17.36)), Some(dec!(16.89))),
            Some(dec!(16.89))
        );
        // Com uma só referência, o teto é ela.
        assert_eq!(combined_ceiling(Some(dec!(40)), None), Some(dec!(40)));
        assert_eq!(combined_ceiling(None, Some(dec!(20.12))), Some(dec!(20.12)));
        assert_eq!(combined_ceiling(None, None), None);
    }

    #[test]
    fn graham_is_the_root_of_22_5_eps_bvps() {
        // 22,5 × 4 × 10 = 900 => 30.
        assert_eq!(graham(dec!(4), dec!(10)), Some(dec!(30)));
        assert_eq!(graham(dec!(0), dec!(10)), None);
        assert_eq!(graham(dec!(-1), dec!(10)), None);
        assert_eq!(graham(dec!(4), dec!(-10)), None);
    }

    fn company(
        earnings_per_share: Option<Decimal>,
        book_value_per_share: Option<Decimal>,
    ) -> AssetFundamentals {
        let figure = |value: Option<Decimal>| {
            value.map_or_else(|| Figure::missing(Gap::NoShares), Figure::known)
        };
        AssetFundamentals::Company(Box::new(CompanyMetrics {
            cvm_code: 1,
            name: "EMPRESA FICTICIA".to_owned(),
            template: Template::Commercial,
            judicial_recovery: None,
            issuer_status: None,
            registration_status: None,
            years: Vec::new(),
            trailing: None,
            profitable_years: YearCount {
                positive: 0,
                known: 0,
                window: 0,
                gap: None,
            },
            positive_operating_cash_years: YearCount {
                positive: 0,
                known: 0,
                window: 0,
                gap: None,
            },
            revenue_cagr: Figure::missing(Gap::NoFiling),
            earnings_cagr: Figure::missing(Gap::NoFiling),
            latest_filing: None,
            shares: None,
            earnings_per_share: figure(earnings_per_share),
            book_value_per_share: figure(book_value_per_share),
            synced_at: None,
        }))
    }

    #[test]
    fn price_references_join_the_dividends_and_the_fundamentals() {
        let points = [
            point(date!(2026 - 08 - 10), dec!(0.30)),
            point(date!(2026 - 02 - 10), dec!(0.30)),
        ];
        // Bazin 0,60 ÷ 6% = 10; Graham √(22,5 × 4 × 10) = 30.
        let both = price_references(
            Some(&points),
            Some(&company(Some(dec!(4)), Some(dec!(10)))),
            TODAY,
            dec!(6),
        );
        assert_eq!(both.bazin.map(|b| b.price), Some(dec!(10)));
        assert_eq!(both.graham, Some(dec!(30)));
        assert!(both.any());
        // A mesma conta que as funções soltas: nada de segunda fórmula.
        assert_eq!(both.bazin, bazin(&points, TODAY, dec!(6)));
        assert_eq!(both.graham, graham(dec!(4), dec!(10)));
        // Só o Bazin: sem fundamentos, ou fundamentos sem lucro e patrimônio.
        for fundamentals in [None, Some(company(None, Some(dec!(10))))] {
            let only = price_references(Some(&points), fundamentals.as_ref(), TODAY, dec!(6));
            assert_eq!(only.graham, None);
            assert_eq!(only.bazin.map(|b| b.price), Some(dec!(10)));
        }
        // Só o Graham: dividendo nunca buscado ou sem pagamento.
        for dividends in [None, Some(&[][..])] {
            let only = price_references(
                dividends,
                Some(&company(Some(dec!(4)), Some(dec!(10)))),
                TODAY,
                dec!(6),
            );
            assert_eq!(only.bazin, None);
            assert_eq!(only.graham, Some(dec!(30)));
        }
    }

    #[test]
    fn no_reference_means_none_and_loss_makes_no_graham() {
        assert_eq!(
            price_references(None, None, TODAY, dec!(6)),
            PriceReferences::default()
        );
        assert!(!PriceReferences::default().any());
        // Prejuízo (LPA negativo) e fundo: a raiz não existe.
        let loss = company(Some(dec!(-1)), Some(dec!(10)));
        assert_eq!(
            price_references(None, Some(&loss), TODAY, dec!(6)).graham,
            None
        );
        let unsupported = AssetFundamentals::Unsupported(Unsupported::NotListedOnB3);
        assert!(!price_references(None, Some(&unsupported), TODAY, dec!(6)).any());
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
        fn the_combined_ceiling_never_passes_either_reference(
            bazin in proptest::option::of(1i64..1_000_000),
            graham in proptest::option::of(1i64..1_000_000),
        ) {
            let bazin = bazin.map(|c| Decimal::new(c, 2));
            let graham = graham.map(|c| Decimal::new(c, 2));
            let ceiling = combined_ceiling(bazin, graham);
            prop_assert_eq!(ceiling.is_some(), bazin.is_some() || graham.is_some());
            if let Some(ceiling) = ceiling {
                prop_assert!(bazin.is_none_or(|b| ceiling <= b));
                prop_assert!(graham.is_none_or(|g| ceiling <= g));
                prop_assert!(Some(ceiling) == bazin || Some(ceiling) == graham);
            }
        }

        #[test]
        fn graham_grows_with_earnings(eps in 1i64..10_000, bvps in 1i64..10_000, more in 1i64..1000) {
            let base = graham(Decimal::new(eps, 2), Decimal::new(bvps, 2)).unwrap();
            let bigger = graham(Decimal::new(eps + more, 2), Decimal::new(bvps, 2)).unwrap();
            prop_assert!(bigger >= base);
        }
    }
}
