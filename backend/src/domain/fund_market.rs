//! Os indicadores de um FII que saem do histórico de mercado, e não do informe
//! da CVM: a liquidez média diária da cota, o crescimento anual composto (CAGR)
//! dos proventos por cota e da cota em 3 e 5 anos, e o rendimento de 12 meses.
//! Regra pura: sem banco, sem rede. Método e limites em `docs/fundamentos.md`
//! ("FII completo").
//!
//! - **Liquidez média diária**: a média do volume financeiro (cotas negociadas
//!   × fechamento, em reais) dos últimos [`SHORT_SESSIONS`] e
//!   [`LONG_SESSIONS`] pregões guardados. Pregão sem negócio não tem fechamento
//!   nem volume e não entra na conta.
//! - **CAGR da cota**: o último fechamento sobre o de `n` anos antes, os dois no
//!   pé de hoje (o desdobramento desfeito, `compare::adjust_for_splits`),
//!   elevado a `1/n`, menos 1.
//! - **CAGR dos proventos**: a soma dos proventos por cota com data ex nos 365
//!   dias até hoje sobre a mesma soma nos 365 dias até `n` anos antes. Os
//!   proventos do Yahoo já vêm no pé do preço ajustado (`docs/risco.md`). Só
//!   conta ano **inteiro**: precisa de provento conhecido de antes do começo do
//!   ano da base (a mesma regra da média do Bazin), senão a base sairia menor e
//!   o crescimento, inflado.
//!
//! Nenhum deles é veredito: a tela mostra o número e a conta.

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use time::{Date, Duration};

use super::compare::{adjust_for_splits, dividend_yield, value_at, BASE_SLACK_DAYS};
use super::fundamentals::{Figure, Gap, RATIO_DP};
use super::performance::months_before;

/// Pregões da liquidez curta e da longa.
pub const SHORT_SESSIONS: usize = 30;
pub const LONG_SESSIONS: usize = 90;
/// Os horizontes do CAGR, em anos.
pub const SHORT_YEARS: u32 = 3;
pub const LONG_YEARS: u32 = 5;
/// O último pregão com volume tem de estar a até 10 dias de hoje: média de
/// pregões de meses atrás não é a liquidez de agora.
pub const STALE_DAYS: i64 = 10;
/// A janela de dias, para trás, que o histórico de volume precisa cobrir para
/// a liquidez longa: 90 pregões cabem em ~130 dias corridos; 150 dão folga
/// para feriados.
pub const TRADED_LOOKBACK_DAYS: i64 = 150;
/// Dias de proventos de cada ponta do CAGR (o mesmo do rendimento de 12 meses).
const DIVIDEND_WINDOW_DAYS: i64 = 365;

/// O que o histórico de um FII dá.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundMarket {
    /// Volume financeiro médio dos últimos 30 pregões, em reais por dia.
    pub liquidity_short: Figure,
    /// O mesmo nos últimos 90 pregões.
    pub liquidity_long: Figure,
    /// CAGR dos proventos por cota, em fração ao ano.
    pub dividend_cagr_short: Figure,
    pub dividend_cagr_long: Figure,
    /// CAGR da cota (preço, sem proventos), em fração ao ano.
    pub price_cagr_short: Figure,
    pub price_cagr_long: Figure,
    /// Proventos por cota dos últimos 365 dias sobre o preço, em fração.
    pub dividend_yield_12m: Figure,
    /// Proventos por cota dos últimos 365 dias, em reais.
    pub dividends_12m: Option<Decimal>,
}

/// O histórico de um símbolo, do mais velho para o mais novo.
#[derive(Debug, Clone, Copy)]
pub struct MarketHistory<'a> {
    /// O fechamento real de cada pregão (`price_history`).
    pub closes: &'a [(Date, Decimal)],
    /// O volume financeiro de cada pregão que a fonte informou.
    pub traded: &'a [(Date, Decimal)],
    /// Desdobramentos e bonificações (data e fator).
    pub splits: &'a [(Date, Decimal)],
    /// Proventos por cota na data ex; `None` = o símbolo ainda não foi buscado.
    pub dividends: Option<&'a [(Date, Decimal)]>,
    /// A cotação de agora (para o rendimento de 12 meses).
    pub price: Option<Decimal>,
}

pub fn fund_market(history: &MarketHistory<'_>, today: Date) -> FundMarket {
    let adjusted = adjust_for_splits(history.closes, history.splits);
    let dividends = history.dividends;
    let dividend_cagr = |years| {
        dividends.map_or(Figure::missing(Gap::InsufficientHistory), |d| {
            dividend_cagr(d, today, years)
        })
    };
    let dividends_12m = dividends.map(|d| trailing_sum(d, today));
    let dividend_yield_12m = match (dividends, history.price) {
        (None, _) => Figure::missing(Gap::InsufficientHistory),
        (_, None) => Figure::missing(Gap::NoQuote),
        (Some(d), Some(price)) => dividend_yield(d, price, today)
            .map_or(Figure::missing(Gap::NonPositiveBase), |y| {
                Figure::known(y.round_dp(RATIO_DP).normalize())
            }),
    };
    FundMarket {
        liquidity_short: average_traded(history.traded, today, SHORT_SESSIONS),
        liquidity_long: average_traded(history.traded, today, LONG_SESSIONS),
        dividend_cagr_short: dividend_cagr(SHORT_YEARS),
        dividend_cagr_long: dividend_cagr(LONG_YEARS),
        price_cagr_short: price_cagr(&adjusted, today, SHORT_YEARS),
        price_cagr_long: price_cagr(&adjusted, today, LONG_YEARS),
        dividend_yield_12m,
        dividends_12m,
    }
}

/// A média do volume financeiro dos últimos `sessions` pregões até `today`.
/// Menos pregões que isso com volume, ou o último há mais de [`STALE_DAYS`]:
/// [`Gap::InsufficientHistory`].
pub fn average_traded(traded: &[(Date, Decimal)], today: Date, sessions: usize) -> Figure {
    let upto = traded.partition_point(|(d, _)| *d <= today);
    let known = &traded[..upto];
    let Some((last, _)) = known.last() else {
        return Figure::missing(Gap::InsufficientHistory);
    };
    if known.len() < sessions || *last < today - Duration::days(STALE_DAYS) || sessions == 0 {
        return Figure::missing(Gap::InsufficientHistory);
    }
    let window = &known[known.len() - sessions..];
    let total: Decimal = window.iter().map(|(_, v)| *v).sum();
    Figure::known((total / Decimal::from(sessions)).round_dp(2).normalize())
}

/// O crescimento anual composto de `start` a `end` em `years` anos.
fn compound(start: Decimal, end: Decimal, years: u32) -> Figure {
    if start <= Decimal::ZERO || end <= Decimal::ZERO || years == 0 {
        return Figure::missing(Gap::NonPositiveEndpoint);
    }
    (end / start)
        .to_f64()
        .map(|r| r.powf(1.0 / f64::from(years)) - 1.0)
        .filter(|r| r.is_finite())
        .and_then(Decimal::from_f64)
        .map_or(Figure::missing(Gap::NonPositiveEndpoint), |r| {
            Figure::known(r.round_dp(RATIO_DP).normalize())
        })
}

/// CAGR da cota em `years` anos, sobre o fechamento já no pé de hoje. A série
/// precisa começar até [`BASE_SLACK_DAYS`] depois da data da base, e o último
/// fechamento não pode ter mais de [`STALE_DAYS`].
pub fn price_cagr(adjusted: &[(Date, Decimal)], today: Date, years: u32) -> Figure {
    let upto = adjusted.partition_point(|(d, _)| *d <= today);
    let known = &adjusted[..upto];
    let (Some((first, first_close)), Some((last, end))) = (known.first(), known.last()) else {
        return Figure::missing(Gap::InsufficientHistory);
    };
    let base_date = months_before(today, 12 * years);
    if *first > base_date + Duration::days(BASE_SLACK_DAYS)
        || *last < today - Duration::days(STALE_DAYS)
    {
        return Figure::missing(Gap::InsufficientHistory);
    }
    let start = value_at(known, base_date).unwrap_or(*first_close);
    compound(start, *end, years)
}

/// A soma dos proventos com data ex nos 365 dias até `day`.
fn trailing_sum(dividends: &[(Date, Decimal)], day: Date) -> Decimal {
    let since = day - Duration::days(DIVIDEND_WINDOW_DAYS);
    dividends
        .iter()
        .filter(|(ex, _)| *ex > since && *ex <= day)
        .map(|(_, amount)| *amount)
        .sum()
}

/// CAGR dos proventos por cota em `years` anos: os 365 dias até hoje sobre os
/// 365 dias até `years` anos antes. Sem provento conhecido de antes do começo
/// da janela da base: [`Gap::InsufficientHistory`]; ponta zerada (não pagou):
/// [`Gap::NonPositiveEndpoint`].
pub fn dividend_cagr(dividends: &[(Date, Decimal)], today: Date, years: u32) -> Figure {
    let base_end = months_before(today, 12 * years);
    let base_start = base_end - Duration::days(DIVIDEND_WINDOW_DAYS);
    if !dividends.iter().any(|(ex, _)| *ex <= base_start) {
        return Figure::missing(Gap::InsufficientHistory);
    }
    compound(
        trailing_sum(dividends, base_end),
        trailing_sum(dividends, today),
        years,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal::MathematicalOps;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const TODAY: Date = date!(2026 - 11 - 27);

    /// Um pregão por dia útil, de `from` a `to`, com o valor que `f` der.
    fn sessions(from: Date, to: Date, f: impl Fn(Date) -> Decimal) -> Vec<(Date, Decimal)> {
        let mut out = Vec::new();
        let mut day = from;
        while day <= to {
            if !matches!(
                day.weekday(),
                time::Weekday::Saturday | time::Weekday::Sunday
            ) {
                out.push((day, f(day)));
            }
            day = day.next_day().unwrap();
        }
        out
    }

    /// Um provento por mês, no dia 10, de `from` (ano, mês) até hoje.
    fn monthly(from: Date, amount: impl Fn(Date) -> Decimal) -> Vec<(Date, Decimal)> {
        let mut out = Vec::new();
        let mut day = from;
        while day <= TODAY {
            out.push((day, amount(day)));
            day = (day + Duration::days(31)).replace_day(10).unwrap();
        }
        out
    }

    #[test]
    fn liquidity_is_the_mean_of_the_last_sessions() {
        // 100 pregões: os 90 mais velhos a R$ 1 mil, os 10 mais novos a R$ 4 mil.
        let traded = sessions(date!(2026 - 06 - 01), TODAY, |_| dec!(1000));
        let n = traded.len();
        assert!(n >= 100, "{n}");
        let mut traded = traded[n - 100..].to_vec();
        for row in traded.iter_mut().skip(90) {
            row.1 = dec!(4000);
        }
        // 30 pregões: 20 × 1.000 + 10 × 4.000 = 60.000 / 30 = 2.000.
        assert_eq!(average_traded(&traded, TODAY, 30).value(), Some(dec!(2000)));
        // 90 pregões: 80 × 1.000 + 10 × 4.000 = 120.000 / 90.
        assert_eq!(
            average_traded(&traded, TODAY, 90).value(),
            Some((dec!(120000) / dec!(90)).round_dp(2).normalize())
        );
        // Menos pregões que a janela: falta histórico.
        assert_eq!(
            average_traded(&traded[..29], TODAY, 30).gap(),
            Some(Gap::InsufficientHistory)
        );
        // O mais novo de meses atrás: a média não é a de agora.
        let old: Vec<_> = traded
            .iter()
            .map(|(d, v)| (*d - Duration::days(60), *v))
            .collect();
        assert_eq!(
            average_traded(&old, TODAY, 30).gap(),
            Some(Gap::InsufficientHistory)
        );
        assert_eq!(
            average_traded(&[], TODAY, 30).gap(),
            Some(Gap::InsufficientHistory)
        );
        // Pregão depois de hoje não entra.
        let mut ahead = traded.clone();
        ahead.push((TODAY + Duration::days(1), dec!(1000000)));
        assert_eq!(average_traded(&ahead, TODAY, 30).value(), Some(dec!(2000)));
    }

    #[test]
    fn price_growth_compounds_over_the_years_on_todays_basis() {
        // A cota foi de 100 a 133,1 em três anos: 10% ao ano.
        let closes = vec![
            (date!(2023 - 11 - 24), dec!(100)),
            (date!(2025 - 06 - 02), dec!(120)),
            (date!(2026 - 11 - 26), dec!(133.1)),
        ];
        assert_eq!(price_cagr(&closes, TODAY, 3).value(), Some(dec!(0.1)));
        // Cinco anos atrás a série não chega: falta histórico.
        assert_eq!(
            price_cagr(&closes, TODAY, 5).gap(),
            Some(Gap::InsufficientHistory)
        );
        // Um desdobramento de 1 para 10 no caminho: o fechamento real caiu a
        // um décimo, a cota no pé de hoje não.
        let real = vec![
            (date!(2023 - 11 - 24), dec!(1000)),
            (date!(2025 - 06 - 02), dec!(120)),
            (date!(2026 - 11 - 26), dec!(133.1)),
        ];
        let history = MarketHistory {
            closes: &real,
            traded: &[],
            splits: &[(date!(2024 - 03 - 01), dec!(10))],
            dividends: None,
            price: None,
        };
        let m = fund_market(&history, TODAY);
        assert_eq!(m.price_cagr_short.value(), Some(dec!(0.1)));
        // Último fechamento velho demais: falta histórico.
        let stale = vec![
            (date!(2023 - 11 - 24), dec!(100)),
            (date!(2026 - 10 - 01), dec!(120)),
        ];
        assert_eq!(
            price_cagr(&stale, TODAY, 3).gap(),
            Some(Gap::InsufficientHistory)
        );
        assert_eq!(
            price_cagr(&[], TODAY, 3).gap(),
            Some(Gap::InsufficientHistory)
        );
    }

    #[test]
    fn dividend_growth_compares_whole_years() {
        // R$ 0,80 por mês até 3 anos atrás; R$ 1,00 depois. Os 12 meses de hoje
        // somam 12,00; os de 3 anos atrás, 9,60 (sem pagamento novo ainda).
        let three_years_ago = months_before(TODAY, 36);
        let paid = monthly(date!(2022 - 01 - 10), |d| {
            if d <= three_years_ago {
                dec!(0.8)
            } else {
                dec!(1)
            }
        });
        let expected = (12.0_f64 / 9.6).powf(1.0 / 3.0) - 1.0;
        let got = dividend_cagr(&paid, TODAY, 3)
            .value()
            .unwrap()
            .to_f64()
            .unwrap();
        assert!((got - expected).abs() < 1e-6, "{got} vs {expected}");
        // A série começa em 2022: cinco anos atrás o ano da base não está
        // inteiro (o primeiro pagamento é depois do começo dele).
        assert_eq!(
            dividend_cagr(&paid, TODAY, 5).gap(),
            Some(Gap::InsufficientHistory)
        );
        // Parou de pagar: a ponta de hoje é zero.
        let stopped: Vec<_> = paid
            .iter()
            .copied()
            .filter(|(d, _)| *d < date!(2025 - 06 - 01))
            .collect();
        assert_eq!(
            dividend_cagr(&stopped, TODAY, 3).gap(),
            Some(Gap::NonPositiveEndpoint)
        );
        assert_eq!(
            dividend_cagr(&[], TODAY, 3).gap(),
            Some(Gap::InsufficientHistory)
        );
    }

    #[test]
    fn the_twelve_month_yield_and_its_reasons() {
        let paid = vec![
            (date!(2026 - 01 - 10), dec!(0.5)),
            (date!(2026 - 07 - 10), dec!(0.5)),
        ];
        let history = |dividends, price| MarketHistory {
            closes: &[],
            traded: &[],
            splits: &[],
            dividends,
            price,
        };
        let m = fund_market(&history(Some(&paid), Some(dec!(10))), TODAY);
        assert_eq!(m.dividend_yield_12m.value(), Some(dec!(0.1)));
        assert_eq!(m.dividends_12m, Some(dec!(1)));
        let no_price = fund_market(&history(Some(&paid), None), TODAY);
        assert_eq!(no_price.dividend_yield_12m.gap(), Some(Gap::NoQuote));
        let unknown = fund_market(&history(None, Some(dec!(10))), TODAY);
        assert_eq!(
            unknown.dividend_yield_12m.gap(),
            Some(Gap::InsufficientHistory)
        );
        assert_eq!(unknown.dividends_12m, None);
        assert_eq!(
            unknown.dividend_cagr_short.gap(),
            Some(Gap::InsufficientHistory)
        );
        assert_eq!(
            unknown.liquidity_short.gap(),
            Some(Gap::InsufficientHistory)
        );
        let zero_price = fund_market(&history(Some(&paid), Some(dec!(0))), TODAY);
        assert_eq!(
            zero_price.dividend_yield_12m.gap(),
            Some(Gap::NonPositiveBase)
        );
    }

    proptest! {
        #[test]
        fn price_growth_recovers_the_rate(rate in -40i64..60, start in 1i64..10_000) {
            let r = Decimal::new(rate, 2);
            let start = Decimal::from(start);
            let base = months_before(TODAY, 36);
            let end = start * (Decimal::ONE + r).powu(3);
            prop_assume!(end > Decimal::ZERO);
            let closes = vec![(base, start), (TODAY, end)];
            let got = price_cagr(&closes, TODAY, 3).value().unwrap();
            prop_assert!((got - r).abs() < dec!(0.000002), "{got} vs {r}");
        }

        #[test]
        fn liquidity_stays_between_the_extremes(values in proptest::collection::vec(0i64..10_000_000, 30..120)) {
            let traded: Vec<(Date, Decimal)> = values
                .iter()
                .enumerate()
                .map(|(i, v)| (TODAY - Duration::days((values.len() - 1 - i) as i64), Decimal::from(*v)))
                .collect();
            let got = average_traded(&traded, TODAY, 30).value().unwrap();
            let last = &traded[traded.len() - 30..];
            let lo = last.iter().map(|(_, v)| *v).min().unwrap();
            let hi = last.iter().map(|(_, v)| *v).max().unwrap();
            prop_assert!(got >= lo - dec!(0.01) && got <= hi + dec!(0.01));
        }
    }
}
