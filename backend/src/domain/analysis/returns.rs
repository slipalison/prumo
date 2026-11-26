//! Rentabilidade para a Análise: a carteira contra CDI, Ibovespa e IPCA em 12
//! meses e em 5 anos, a fração de anos com retorno positivo e a volatilidade
//! anualizada. Tudo parte da série de cotas do Desempenho
//! ([`crate::domain::performance`]) e dos índices já guardados; aqui só se
//! decide **se há dado suficiente** e se compara.
//!
//! A regra de ouro: série curta é "sem dado", nunca falha. A comparação só
//! existe quando a série da carteira cobre a janela inteira e o índice tem dado
//! de ponta a ponta dela.

use rust_decimal::{Decimal, MathematicalOps};
use time::{Date, Duration, Month};

use crate::domain::allocation::round_cents;
use crate::domain::performance::{self, Accrued, QuotaPoint};

/// Menos anos civis fechados que isso e "% de anos positivos" não diz nada
/// (um ano só seria sempre 0% ou 100%).
pub const MIN_YEARS: u32 = 2;
/// Menos meses de retorno que isso e a volatilidade é ruído.
pub const MIN_VOLATILITY_MONTHS: usize = 12;
/// A volatilidade olha, no máximo, os últimos 36 meses fechados.
pub const VOLATILITY_MONTHS: usize = 36;
/// A série da carteira pode terminar até 7 dias antes do fim da janela
/// (fim de semana, feriado); mais que isso ela está velha.
const STALE_DAYS: i64 = 7;
/// O índice pode começar até 10 dias depois do início da janela (fim de
/// semana e feriado no começo da série) e ainda cobri-la.
const INDEX_START_SLACK_DAYS: i64 = 10;
const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

/// Com o que a carteira é comparada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Benchmark {
    Cdi,
    Ibov,
    Ipca,
}

impl Benchmark {
    pub const ALL: [Benchmark; 3] = [Benchmark::Cdi, Benchmark::Ibov, Benchmark::Ipca];

    pub fn as_str(self) -> &'static str {
        match self {
            Benchmark::Cdi => "cdi",
            Benchmark::Ibov => "ibov",
            Benchmark::Ipca => "ipca",
        }
    }
}

/// A janela da comparação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Horizon {
    OneYear,
    FiveYears,
}

impl Horizon {
    pub const ALL: [Horizon; 2] = [Horizon::OneYear, Horizon::FiveYears];

    pub fn months(self) -> u32 {
        match self {
            Horizon::OneYear => 12,
            Horizon::FiveYears => 60,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Horizon::OneYear => "12m",
            Horizon::FiveYears => "5y",
        }
    }
}

/// A carteira e o índice na mesma janela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comparison {
    /// A cota de base e a última cota da janela.
    pub from: Date,
    pub to: Date,
    /// Frações (0,05 = 5%).
    pub portfolio: Decimal,
    pub benchmark: Decimal,
}

/// Quantos anos civis fechados a carteira fechou no azul.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YearsFact {
    pub positive: u32,
    pub total: u32,
}

/// O que a dimensão Rentabilidade sabe da série. Ausente = sem dado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReturnsFacts {
    pub comparisons: Vec<(Benchmark, Horizon, Comparison)>,
    pub positive_years: Option<YearsFact>,
    /// Desvio-padrão dos retornos mensais, anualizado, em percentual.
    pub volatility_percent: Option<Decimal>,
}

impl ReturnsFacts {
    pub fn comparison(&self, benchmark: Benchmark, horizon: Horizon) -> Option<&Comparison> {
        self.comparisons
            .iter()
            .find(|(b, h, _)| *b == benchmark && *h == horizon)
            .map(|(_, _, c)| c)
    }

    /// Monta os fatos da série; as comparações vêm prontas (cada índice tem a
    /// sua forma de acumular, que mora no serviço).
    pub fn from_series(
        points: &[QuotaPoint],
        today: Date,
        comparisons: Vec<(Benchmark, Horizon, Comparison)>,
    ) -> Self {
        Self {
            comparisons,
            positive_years: positive_years(points, today),
            volatility_percent: annualized_volatility(&monthly_returns(points, today)),
        }
    }
}

/// A carteira contra um índice em `horizon` meses até `today`.
///
/// `accrue(from, to)` acumula o índice na janela e diz até onde havia dado
/// publicado (`as_of`). O fim da janela é o menor entre hoje e o último dado do
/// índice: o IPCA sai com atraso, e comparar a carteira até hoje com um IPCA
/// que acabou há dois meses favoreceria a carteira. `index_start` é a primeira
/// data do índice guardado.
///
/// `None` (sem dado) quando o índice não cobre a janela, ou a série da carteira
/// começa depois do início dela, ou já está velha.
pub fn compare(
    points: &[QuotaPoint],
    today: Date,
    horizon: Horizon,
    index_start: Option<Date>,
    accrue: impl Fn(Date, Date) -> Accrued,
) -> Option<Comparison> {
    let months = horizon.months();
    let probe = accrue(performance::months_before(today, months), today);
    let end = probe.as_of?.min(today);
    let from = performance::months_before(end, months);
    if index_start? > from + Duration::days(INDEX_START_SLACK_DAYS) {
        return None;
    }
    let benchmark = accrue(from, end);
    benchmark.as_of?;
    let window = performance::period_return(points, from, end)?;
    // A série só cobre a janela se a cota de base é de antes (ou do dia) do
    // início; senão `period_return` caiu na primeira cota e a janela é menor.
    if window.from > from || end - window.to > Duration::days(STALE_DAYS) {
        return None;
    }
    Some(Comparison {
        from: window.from,
        to: window.to,
        portfolio: window.ret,
        benchmark: benchmark.ret,
    })
}

/// Anos civis **fechados** (anteriores ao de hoje) que a série cobre de janeiro
/// a dezembro, e quantos deles renderam mais que zero. `None` com menos de
/// [`MIN_YEARS`] anos assim.
pub fn positive_years(points: &[QuotaPoint], today: Date) -> Option<YearsFact> {
    let rows = performance::monthly_table(points);
    let closed: Vec<&performance::YearRow> = rows
        .iter()
        .filter(|row| {
            // Ano inteiro: a base é de antes do ano e a série vai até dezembro.
            row.year < today.year()
                && row.from.year() < row.year
                && row.to.month() == Month::December
        })
        .collect();
    let total = u32::try_from(closed.len()).ok()?;
    if total < MIN_YEARS {
        return None;
    }
    let positive = u32::try_from(
        closed
            .iter()
            .filter(|row| row.total > Decimal::ZERO)
            .count(),
    )
    .ok()?;
    Some(YearsFact { positive, total })
}

/// Os retornos mensais (frações), do mais velho ao mais novo, sem o mês em que
/// a série começa nem o de hoje: os dois são parciais.
pub fn monthly_returns(points: &[QuotaPoint], today: Date) -> Vec<Decimal> {
    let Some(first) = points.first().map(|p| p.date) else {
        return Vec::new();
    };
    let partial = [
        (first.year(), u8::from(first.month())),
        (today.year(), u8::from(today.month())),
    ];
    let mut out = Vec::new();
    for row in performance::monthly_table(points) {
        for (index, ret) in row.months.iter().enumerate() {
            let month = u8::try_from(index + 1).unwrap_or(1);
            if let Some(ret) = ret.filter(|_| !partial.contains(&(row.year, month))) {
                out.push(ret);
            }
        }
    }
    out
}

/// Desvio-padrão amostral dos últimos [`VOLATILITY_MONTHS`] retornos mensais,
/// anualizado por `√12`, em percentual com duas casas. `None` com menos de
/// [`MIN_VOLATILITY_MONTHS`] meses.
pub fn annualized_volatility(returns: &[Decimal]) -> Option<Decimal> {
    let recent = &returns[returns.len().saturating_sub(VOLATILITY_MONTHS)..];
    if recent.len() < MIN_VOLATILITY_MONTHS {
        return None;
    }
    let count = Decimal::from(recent.len());
    let mean = recent.iter().sum::<Decimal>() / count;
    let variance = recent
        .iter()
        .map(|r| (*r - mean) * (*r - mean))
        .sum::<Decimal>()
        / (count - Decimal::ONE);
    let monthly = variance.sqrt()?;
    let annual = monthly * Decimal::from(12).sqrt()?;
    Some(round_cents(annual * HUNDRED))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const TODAY: Date = date!(2026 - 11 - 25);

    /// Um ponto por dia de `from` a `to`, valendo `value(dia)`.
    fn daily(from: Date, to: Date, value: impl Fn(i64) -> Decimal) -> Vec<QuotaPoint> {
        let mut out = Vec::new();
        let mut day = from;
        let mut n = 0;
        while day <= to {
            out.push(QuotaPoint {
                date: day,
                value: value(n),
            });
            day += Duration::days(1);
            n += 1;
        }
        out
    }

    fn flat(from: Date) -> Vec<QuotaPoint> {
        daily(from, TODAY, |_| Decimal::ONE)
    }

    fn accrued(ret: Decimal) -> impl Fn(Date, Date) -> Accrued {
        move |_, to| Accrued {
            ret,
            as_of: Some(to),
        }
    }

    #[test]
    fn a_full_window_compares_portfolio_and_index() {
        let points = daily(date!(2020 - 01 - 01), TODAY, |n| {
            Decimal::ONE + Decimal::new(n, 4)
        });
        let c = compare(
            &points,
            TODAY,
            Horizon::OneYear,
            Some(date!(2020 - 01 - 01)),
            accrued(dec!(0.1)),
        )
        .unwrap();
        assert_eq!(c.benchmark, dec!(0.1));
        assert!(c.portfolio > Decimal::ZERO);
        assert_eq!(c.to, TODAY);
        assert!(c.from <= date!(2025 - 11 - 25));
    }

    #[test]
    fn a_short_series_has_no_comparison_instead_of_a_bad_one() {
        // Seis meses de série: nem 12 meses nem 5 anos.
        let points = flat(date!(2026 - 05 - 25));
        for horizon in Horizon::ALL {
            assert_eq!(
                compare(
                    &points,
                    TODAY,
                    horizon,
                    Some(date!(2020 - 01 - 01)),
                    accrued(dec!(0.1))
                ),
                None
            );
        }
    }

    #[test]
    fn five_years_need_five_years_of_series() {
        let points = flat(date!(2023 - 01 - 01));
        let index = Some(date!(2020 - 01 - 01));
        assert!(compare(&points, TODAY, Horizon::OneYear, index, accrued(dec!(0.1))).is_some());
        assert_eq!(
            compare(
                &points,
                TODAY,
                Horizon::FiveYears,
                index,
                accrued(dec!(0.1))
            ),
            None
        );
    }

    #[test]
    fn an_index_that_stops_early_moves_the_end_of_the_window() {
        // O índice acaba em 25/09: a carteira é medida até lá, não até hoje.
        let points = flat(date!(2020 - 01 - 01));
        let stops = date!(2026 - 09 - 25);
        let c = compare(
            &points,
            TODAY,
            Horizon::OneYear,
            Some(date!(2020 - 01 - 01)),
            |_, to| Accrued {
                ret: dec!(0.05),
                as_of: Some(to.min(stops)),
            },
        )
        .unwrap();
        assert_eq!(c.to, stops);
        assert_eq!(c.from, date!(2025 - 09 - 25));
    }

    #[test]
    fn no_published_index_or_a_late_index_start_means_no_data() {
        let points = flat(date!(2020 - 01 - 01));
        let none = |_: Date, _: Date| Accrued {
            ret: Decimal::ZERO,
            as_of: None,
        };
        assert_eq!(
            compare(
                &points,
                TODAY,
                Horizon::OneYear,
                Some(date!(2020 - 01 - 01)),
                none
            ),
            None
        );
        // O índice guardado só começa depois do início da janela.
        assert_eq!(
            compare(
                &points,
                TODAY,
                Horizon::OneYear,
                Some(date!(2026 - 01 - 01)),
                accrued(dec!(0.1))
            ),
            None
        );
        assert_eq!(
            compare(&points, TODAY, Horizon::OneYear, None, accrued(dec!(0.1))),
            None
        );
    }

    #[test]
    fn a_stale_series_has_no_comparison() {
        // A última cota é de um mês atrás.
        let points = daily(date!(2020 - 01 - 01), date!(2026 - 10 - 20), |_| {
            Decimal::ONE
        });
        assert_eq!(
            compare(
                &points,
                TODAY,
                Horizon::OneYear,
                Some(date!(2020 - 01 - 01)),
                accrued(dec!(0.1))
            ),
            None
        );
    }

    /// A cota no fim de cada mês, de `start_year`/janeiro em diante.
    fn month_ends(start_year: i32, factors: &[Decimal]) -> Vec<QuotaPoint> {
        let mut value = Decimal::ONE;
        let mut out = vec![QuotaPoint {
            date: Date::from_calendar_date(start_year - 1, Month::December, 31).unwrap(),
            value,
        }];
        for (i, factor) in factors.iter().enumerate() {
            let month = Month::try_from(u8::try_from(i % 12 + 1).unwrap()).unwrap();
            let year = start_year + i32::try_from(i / 12).unwrap();
            value *= *factor;
            out.push(QuotaPoint {
                date: Date::from_calendar_date(year, month, time::util::days_in_month(month, year))
                    .unwrap(),
                value,
            });
        }
        out
    }

    #[test]
    fn positive_years_count_only_closed_full_years() {
        // 2023 sobe, 2024 cai, 2025 sobe, 2026 (em curso) cai.
        let mut factors = Vec::new();
        factors.extend([dec!(1.01); 12]);
        factors.extend([dec!(0.99); 12]);
        factors.extend([dec!(1.02); 12]);
        factors.extend([dec!(0.98); 10]);
        let points = month_ends(2023, &factors);
        assert_eq!(
            positive_years(&points, TODAY),
            Some(YearsFact {
                positive: 2,
                total: 3
            })
        );
    }

    #[test]
    fn one_closed_year_is_not_enough() {
        let mut factors = Vec::new();
        factors.extend([dec!(1.01); 12]);
        factors.extend([dec!(1.01); 10]);
        let points = month_ends(2025, &factors);
        assert_eq!(positive_years(&points, TODAY), None);
        assert_eq!(positive_years(&[], TODAY), None);
    }

    #[test]
    fn the_first_and_the_current_month_are_left_out_of_the_monthly_returns() {
        // Série que começa em 15/03/2026: março (parcial) e novembro (parcial) saem.
        let points = daily(date!(2026 - 03 - 15), TODAY, |n| {
            Decimal::ONE + Decimal::new(n, 3)
        });
        let returns = monthly_returns(&points, TODAY);
        // Abril a outubro.
        assert_eq!(returns.len(), 7);
        assert!(returns.iter().all(|r| *r > Decimal::ZERO));
    }

    #[test]
    fn volatility_needs_a_year_of_returns() {
        assert_eq!(annualized_volatility(&[dec!(0.01); 11]), None);
        assert_eq!(annualized_volatility(&[]), None);
    }

    #[test]
    fn constant_returns_have_no_volatility_and_alternating_ones_do() {
        assert_eq!(
            annualized_volatility(&[dec!(0.01); 12]),
            Some(Decimal::ZERO)
        );
        let swings: Vec<Decimal> = (0..24)
            .map(|i| if i % 2 == 0 { dec!(0.05) } else { dec!(-0.05) })
            .collect();
        // Desvio amostral de ±5% alternando ≈ 5,11% ao mês; × √12 ≈ 17,7%.
        let vol = annualized_volatility(&swings).unwrap();
        assert!(vol > dec!(17) && vol < dec!(18.5), "{vol}");
    }

    proptest! {
        /// A volatilidade nunca é negativa e só olha os últimos 36 meses.
        #[test]
        fn volatility_is_non_negative_and_uses_the_last_three_years(
            raw in proptest::collection::vec(-300i64..300, 12..80),
            old in proptest::collection::vec(-300i64..300, 0..10),
        ) {
            let recent: Vec<Decimal> = raw.iter().map(|v| Decimal::new(*v, 4)).collect();
            let vol = annualized_volatility(&recent).unwrap();
            prop_assert!(vol >= Decimal::ZERO);
            // Meses mais velhos que 36 não mudam nada.
            let mut with_old: Vec<Decimal> = old.iter().map(|v| Decimal::new(*v, 1)).collect();
            if recent.len() >= VOLATILITY_MONTHS {
                with_old.extend(recent.iter().copied());
                prop_assert_eq!(annualized_volatility(&with_old), Some(vol));
            }
        }

        /// Série que começa depois do início da janela nunca compara.
        #[test]
        fn a_series_starting_inside_the_window_never_compares(days in 1i64..360) {
            let points = flat(TODAY - Duration::days(days));
            prop_assert_eq!(
                compare(&points, TODAY, Horizon::OneYear, Some(date!(2020 - 01 - 01)), accrued(dec!(0.1))),
                None
            );
        }

        /// Com série de sobra e índice inteiro, a carteira parada rende zero.
        #[test]
        fn a_flat_series_with_a_full_index_always_compares_to_zero(extra in 0i64..2000) {
            let points = flat(TODAY - Duration::days(366 + extra));
            let c = compare(&points, TODAY, Horizon::OneYear, Some(date!(2000 - 01 - 01)), accrued(dec!(0.07))).unwrap();
            prop_assert_eq!(c.portfolio, Decimal::ZERO);
            prop_assert_eq!(c.benchmark, dec!(0.07));
        }
    }
}
