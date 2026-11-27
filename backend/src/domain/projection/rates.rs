//! O caminho mês a mês do CDI e do IPCA esperados, a partir das medianas do
//! Focus (pesquisa do Banco Central com o mercado) que estão no banco.
//!
//! - **IPCA.** Mês a mês pela mediana mensal do Focus (ela cobre uns 24
//!   meses à frente); mês sem mediana mensal usa a anual daquele ano, em taxa
//!   mensal equivalente; ano sem nenhuma, a última anual publicada. Sem Focus
//!   nenhum, a inflação de reserva que quem chama passa.
//! - **CDI.** O Focus publica a meta Selic esperada para o **fim** de cada ano.
//!   O CDI esperado em dezembro do ano `A` é essa meta menos
//!   [`CDI_BELOW_SELIC`]; entre um dezembro e outro, o caminho é uma reta, mês
//!   a mês, que parte do CDI de hoje (a taxa diária mais recente do banco).
//!   Depois do último ano publicado, fica no último valor.
//!
//! Tudo em frações: `0.045` é 4,5%.

use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, Month};

use super::{monthly_rate, to_f64};
use crate::domain::income::YearMonth;

/// O CDI anda 0,10 ponto abaixo da meta Selic: o DI de um dia fica colado na
/// Selic efetiva, que o Banco Central mantém 0,10 abaixo da meta desde 2019.
pub const CDI_BELOW_SELIC: f64 = 0.0010;

/// A meta de inflação do Conselho Monetário Nacional (3% ao ano, contínua desde
/// 2025): a reserva quando não há Focus nem IPCA medido no banco.
pub const INFLATION_TARGET: Decimal = Decimal::from_parts(3, 0, 0, false, 0);

/// O indicador do Focus. O nome vai para o banco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Indicator {
    Ipca,
    Selic,
}

impl Indicator {
    pub const ALL: [Indicator; 2] = [Indicator::Ipca, Indicator::Selic];

    pub fn as_str(self) -> &'static str {
        match self {
            Indicator::Ipca => "ipca",
            Indicator::Selic => "selic",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Indicator::ALL.into_iter().find(|i| i.as_str() == value)
    }
}

/// A que período a mediana se refere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    /// O ano: IPCA no ano; Selic no fim do ano.
    Annual,
    /// O mês: IPCA no mês.
    Monthly,
}

impl Period {
    pub const ALL: [Period; 2] = [Period::Annual, Period::Monthly];

    pub fn as_str(self) -> &'static str {
        match self {
            Period::Annual => "annual",
            Period::Monthly => "monthly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Period::ALL.into_iter().find(|p| p.as_str() == value)
    }
}

/// Uma mediana do Focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expectation {
    pub indicator: Indicator,
    pub period: Period,
    /// O dia 1º do período (1º de janeiro no anual).
    pub reference: Date,
    /// Em %, como o Focus publica.
    pub median: Decimal,
    /// A data da pesquisa.
    pub survey_date: Date,
}

/// As medianas que estão no banco.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Focus {
    pub rows: Vec<Expectation>,
}

impl Focus {
    pub fn new(rows: Vec<Expectation>) -> Self {
        Self { rows }
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// A pesquisa mais recente entre as medianas guardadas.
    pub fn survey_date(&self) -> Option<Date> {
        self.rows.iter().map(|r| r.survey_date).max()
    }

    fn find(&self, indicator: Indicator, period: Period, reference: Date) -> Option<f64> {
        self.rows
            .iter()
            .find(|r| r.indicator == indicator && r.period == period && r.reference == reference)
            .map(|r| to_f64(r.median) / 100.0)
    }

    /// O IPCA esperado no mês (fração no mês).
    pub fn ipca_month(&self, month: YearMonth) -> Option<f64> {
        self.find(Indicator::Ipca, Period::Monthly, first_day(month)?)
    }

    /// O IPCA esperado no ano (fração no ano).
    pub fn ipca_year(&self, year: i32) -> Option<f64> {
        self.find(Indicator::Ipca, Period::Annual, first_day(jan(year))?)
    }

    /// A meta Selic esperada no fim do ano (fração ao ano).
    pub fn selic_year_end(&self, year: i32) -> Option<f64> {
        self.find(Indicator::Selic, Period::Annual, first_day(jan(year))?)
    }

    fn annual_years(&self, indicator: Indicator) -> Vec<i32> {
        let mut years: Vec<i32> = self
            .rows
            .iter()
            .filter(|r| r.indicator == indicator && r.period == Period::Annual)
            .map(|r| r.reference.year())
            .collect();
        years.sort_unstable();
        years.dedup();
        years
    }

    /// A inflação de longo prazo do Focus: a média geométrica das medianas
    /// anuais do IPCA dos anos **depois** do de hoje (o ano corrente já tem
    /// meses publicados e puxaria a média para o passado). Nenhum ano à
    /// frente: o último publicado. Nada: `None`. Devolve a taxa e os anos
    /// usados.
    pub fn long_run_ipca(&self, today: Date) -> Option<(f64, i32, i32)> {
        let years = self.annual_years(Indicator::Ipca);
        let ahead: Vec<i32> = years
            .iter()
            .copied()
            .filter(|y| *y > today.year())
            .collect();
        let used = if ahead.is_empty() {
            vec![*years.last()?]
        } else {
            ahead
        };
        let (first, last) = (*used.first()?, *used.last()?);
        let product: f64 = used
            .iter()
            .filter_map(|y| self.ipca_year(*y))
            .map(|r| 1.0 + r)
            .product();
        let rate = product.powf(1.0 / used.len() as f64) - 1.0;
        rate.is_finite().then_some((rate, first, last))
    }
}

fn jan(year: i32) -> YearMonth {
    YearMonth { year, month: 1 }
}

/// O dia 1º do mês.
pub fn first_day(month: YearMonth) -> Option<Date> {
    Date::from_calendar_date(month.year, Month::try_from(month.month).ok()?, 1).ok()
}

/// De onde veio cada lado do caminho.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSource {
    /// As medianas do Focus.
    Focus,
    /// A pessoa informou uma taxa só para o período inteiro.
    Informed,
    /// Sem Focus: o que o banco mediu (o CDI de hoje parado; o IPCA de 12 meses).
    Measured,
    /// Sem Focus nem medida: a meta de inflação ([`INFLATION_TARGET`]).
    Fallback,
    /// Sem dado nenhum: o CDI não entra na conta (a renda de papel fica parada).
    Missing,
}

/// Um mês do caminho.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathMonth {
    pub month: YearMonth,
    /// CDI esperado, ao ano. `None` quando não há CDI nenhum.
    pub cdi: Option<f64>,
    /// IPCA esperado no mês.
    pub ipca: f64,
}

/// O caminho dos próximos meses, do mês seguinte ao de hoje em diante.
#[derive(Debug, Clone, PartialEq)]
pub struct RatePath {
    pub months: Vec<PathMonth>,
    pub cdi_source: PathSource,
    pub ipca_source: PathSource,
}

impl RatePath {
    /// O mês `k` (1 = o mês que vem).
    pub fn at(&self, k: usize) -> Option<&PathMonth> {
        k.checked_sub(1).and_then(|i| self.months.get(i))
    }

    /// O IPCA acumulado do mês que vem até o mês `k`, inclusive (fator).
    pub fn ipca_factor(&self, k: usize) -> f64 {
        self.months.iter().take(k).map(|m| 1.0 + m.ipca).product()
    }

    /// O IPCA ao ano equivalente ao caminho inteiro.
    pub fn ipca_annual(&self) -> f64 {
        let n = self.months.len();
        if n == 0 {
            return 0.0;
        }
        self.ipca_factor(n).powf(12.0 / n as f64) - 1.0
    }

    /// O CDI do último mês do caminho.
    pub fn cdi_end(&self) -> Option<f64> {
        self.months.last().and_then(|m| m.cdi)
    }
}

/// O que o caminho usa além do Focus.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathInputs {
    /// O CDI de hoje, ao ano (da taxa diária mais recente do banco).
    pub cdi_now: Option<f64>,
    /// A inflação ao ano quando não há Focus (o IPCA de 12 meses medido, ou
    /// a meta de inflação), e de onde ela veio.
    pub ipca_fallback: f64,
    pub ipca_fallback_source: Option<PathSource>,
    /// A pessoa trocou o CDI do período inteiro por uma taxa só (ao ano).
    pub cdi_informed: Option<f64>,
    /// A pessoa trocou o IPCA do período inteiro por uma taxa só (ao ano).
    pub ipca_informed: Option<f64>,
}

/// O caminho de `horizon` meses, do mês seguinte ao de `today`.
pub fn rate_path(focus: &Focus, today: Date, horizon: u32, inputs: PathInputs) -> RatePath {
    let now = YearMonth::of(today);
    let (cdi_source, cdi_at) = cdi_curve(focus, now, inputs);
    let long_run = focus.long_run_ipca(today).map(|(rate, _, _)| rate);
    let ipca_source = if inputs.ipca_informed.is_some() {
        PathSource::Informed
    } else if !focus.is_empty() && (long_run.is_some() || has_monthly_ipca(focus)) {
        PathSource::Focus
    } else {
        inputs.ipca_fallback_source.unwrap_or(PathSource::Fallback)
    };
    let months = (1..=horizon)
        .map(|k| {
            let month = now.shift(i32::try_from(k).unwrap_or(0));
            let ipca = match inputs.ipca_informed {
                Some(annual) => monthly_rate(annual),
                None => ipca_of(focus, month, long_run)
                    .unwrap_or_else(|| monthly_rate(inputs.ipca_fallback)),
            };
            PathMonth {
                month,
                cdi: cdi_at(month),
                ipca,
            }
        })
        .collect();
    RatePath {
        months,
        cdi_source,
        ipca_source,
    }
}

fn has_monthly_ipca(focus: &Focus) -> bool {
    focus
        .rows
        .iter()
        .any(|r| r.indicator == Indicator::Ipca && r.period == Period::Monthly)
}

/// O IPCA do mês: a mediana mensal, senão a anual do ano, senão a de longo
/// prazo do Focus. `None` sem Focus.
fn ipca_of(focus: &Focus, month: YearMonth, long_run: Option<f64>) -> Option<f64> {
    focus
        .ipca_month(month)
        .or_else(|| focus.ipca_year(month.year).map(monthly_rate))
        .or_else(|| long_run.map(monthly_rate))
}

/// Índice contínuo de meses, para a reta entre dois pontos.
fn index(month: YearMonth) -> i32 {
    month.year * 12 + i32::from(month.month) - 1
}

type CdiAt = Box<dyn Fn(YearMonth) -> Option<f64>>;

/// A fonte do CDI e a função que dá o CDI de cada mês.
fn cdi_curve(focus: &Focus, now: YearMonth, inputs: PathInputs) -> (PathSource, CdiAt) {
    if let Some(informed) = inputs.cdi_informed {
        return (PathSource::Informed, Box::new(move |_| Some(informed)));
    }
    // Os pontos da reta: hoje (se o banco tem o CDI) e cada dezembro que o
    // Focus cobre, do de hoje em diante.
    let mut anchors: Vec<(i32, f64)> = Vec::new();
    if let Some(cdi) = inputs.cdi_now {
        anchors.push((index(now), cdi));
    }
    let mut years: Vec<i32> = focus
        .rows
        .iter()
        .filter(|r| r.indicator == Indicator::Selic && r.period == Period::Annual)
        .map(|r| r.reference.year())
        .collect();
    years.sort_unstable();
    years.dedup();
    let mut from_focus = false;
    for year in years {
        let december = index(YearMonth { year, month: 12 });
        if december <= index(now) && inputs.cdi_now.is_some() {
            continue;
        }
        if let Some(selic) = focus.selic_year_end(year) {
            anchors.push((december, (selic - CDI_BELOW_SELIC).max(0.0)));
            from_focus = true;
        }
    }
    let source = match (from_focus, inputs.cdi_now) {
        (true, _) => PathSource::Focus,
        (false, Some(_)) => PathSource::Measured,
        (false, None) => PathSource::Missing,
    };
    if anchors.is_empty() {
        return (source, Box::new(|_| None));
    }
    (
        source,
        Box::new(move |month| Some(interpolate(&anchors, index(month)))),
    )
}

/// A reta entre os dois pontos em volta de `at`; antes do primeiro e depois
/// do último, o valor do ponto da ponta.
fn interpolate(anchors: &[(i32, f64)], at: i32) -> f64 {
    let mut previous: Option<(i32, f64)> = None;
    for &(month, value) in anchors {
        if at <= month {
            return match previous {
                Some((p_month, p_value)) if month > p_month => {
                    let t = f64::from(at - p_month) / f64::from(month - p_month);
                    p_value + (value - p_value) * t
                }
                _ => value,
            };
        }
        previous = Some((month, value));
    }
    previous.map_or(0.0, |(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const SURVEY: Date = date!(2026 - 11 - 19);

    fn row(indicator: Indicator, period: Period, reference: Date, median: Decimal) -> Expectation {
        Expectation {
            indicator,
            period,
            reference,
            median,
            survey_date: SURVEY,
        }
    }

    /// Um Focus fictício: IPCA anual de 2026 a 2028, dois meses de IPCA
    /// mensal e a Selic de fim de ano de 2026 a 2028.
    fn focus() -> Focus {
        Focus::new(vec![
            row(
                Indicator::Ipca,
                Period::Annual,
                date!(2026 - 01 - 01),
                dec!(5.0),
            ),
            row(
                Indicator::Ipca,
                Period::Annual,
                date!(2027 - 01 - 01),
                dec!(4.5),
            ),
            row(
                Indicator::Ipca,
                Period::Annual,
                date!(2028 - 01 - 01),
                dec!(4.0),
            ),
            row(
                Indicator::Ipca,
                Period::Monthly,
                date!(2026 - 12 - 01),
                dec!(0.40),
            ),
            row(
                Indicator::Ipca,
                Period::Monthly,
                date!(2027 - 01 - 01),
                dec!(0.30),
            ),
            row(
                Indicator::Selic,
                Period::Annual,
                date!(2026 - 01 - 01),
                dec!(14.0),
            ),
            row(
                Indicator::Selic,
                Period::Annual,
                date!(2027 - 01 - 01),
                dec!(12.0),
            ),
            row(
                Indicator::Selic,
                Period::Annual,
                date!(2028 - 01 - 01),
                dec!(11.0),
            ),
        ])
    }

    #[test]
    fn names_round_trip() {
        for i in Indicator::ALL {
            assert_eq!(Indicator::parse(i.as_str()), Some(i));
        }
        for p in Period::ALL {
            assert_eq!(Period::parse(p.as_str()), Some(p));
        }
        assert_eq!(Indicator::parse("igpm"), None);
        assert_eq!(Period::parse("weekly"), None);
    }

    #[test]
    fn lookups_read_the_medians_as_fractions() {
        let f = focus();
        assert_eq!(f.survey_date(), Some(SURVEY));
        assert_eq!(f.ipca_year(2027), Some(0.045));
        assert_eq!(f.selic_year_end(2026), Some(0.14));
        assert_eq!(
            f.ipca_month(YearMonth {
                year: 2026,
                month: 12
            }),
            Some(0.004)
        );
        assert_eq!(f.ipca_year(2031), None);
        assert_eq!(Focus::default().survey_date(), None);
    }

    #[test]
    fn long_run_inflation_skips_the_current_year() {
        let (rate, first, last) = focus().long_run_ipca(date!(2026 - 11 - 27)).unwrap();
        assert_eq!((first, last), (2027, 2028));
        let expected = (1.045f64 * 1.04).sqrt() - 1.0;
        assert!((rate - expected).abs() < 1e-12);
        // Só o ano de hoje publicado: ele.
        let only = Focus::new(vec![row(
            Indicator::Ipca,
            Period::Annual,
            date!(2026 - 01 - 01),
            dec!(5.0),
        )]);
        let (rate, first, _) = only.long_run_ipca(date!(2026 - 11 - 27)).unwrap();
        assert_eq!(first, 2026);
        assert!((rate - 0.05).abs() < 1e-12);
        assert_eq!(Focus::default().long_run_ipca(date!(2026 - 11 - 27)), None);
    }

    #[test]
    fn ipca_uses_the_monthly_median_then_the_annual_one() {
        let path = rate_path(&focus(), date!(2026 - 11 - 27), 3, PathInputs::default());
        assert_eq!(path.ipca_source, PathSource::Focus);
        assert_eq!(path.months[0].month.label(), "2026-12");
        assert!((path.months[0].ipca - 0.004).abs() < 1e-12);
        assert!((path.months[1].ipca - 0.003).abs() < 1e-12);
        // Fevereiro de 2027 não tem mensal: a anual de 2027 em taxa mensal.
        assert!((path.months[2].ipca - monthly_rate(0.045)).abs() < 1e-12);
        assert!((path.ipca_factor(2) - 1.004 * 1.003).abs() < 1e-12);
    }

    #[test]
    fn ipca_past_the_focus_uses_the_long_run_and_without_focus_the_fallback() {
        // 2030 não tem anual: a de longo prazo.
        let path = rate_path(&focus(), date!(2029 - 11 - 27), 2, PathInputs::default());
        let (long, _, _) = focus().long_run_ipca(date!(2029 - 11 - 27)).unwrap();
        assert!((path.months[1].ipca - monthly_rate(long)).abs() < 1e-12);

        let inputs = PathInputs {
            ipca_fallback: 0.03,
            ipca_fallback_source: Some(PathSource::Measured),
            ..PathInputs::default()
        };
        let none = rate_path(&Focus::default(), date!(2026 - 11 - 27), 12, inputs);
        assert_eq!(none.ipca_source, PathSource::Measured);
        assert!((none.ipca_annual() - 0.03).abs() < 1e-9);
    }

    #[test]
    fn cdi_is_a_straight_line_from_today_to_each_december() {
        let inputs = PathInputs {
            cdi_now: Some(0.149),
            ..PathInputs::default()
        };
        // Hoje é novembro de 2026; dezembro de 2026 = 14,0 − 0,1 = 13,9%.
        let path = rate_path(&focus(), date!(2026 - 11 - 27), 26, inputs);
        assert_eq!(path.cdi_source, PathSource::Focus);
        let cdi = |k: usize| path.at(k).unwrap().cdi.unwrap();
        assert!((cdi(1) - 0.139).abs() < 1e-12);
        // Junho de 2027: metade do caminho de 13,9% a 11,9%.
        assert!((cdi(7) - (0.139 + (0.119 - 0.139) * 6.0 / 12.0)).abs() < 1e-12);
        assert!((cdi(13) - 0.119).abs() < 1e-12);
        // Depois de dezembro de 2028, fica no último.
        assert!((cdi(26) - 0.109).abs() < 1e-12);
        assert_eq!(path.cdi_end(), Some(cdi(26)));
        assert!(path.at(0).is_none());
    }

    #[test]
    fn cdi_starts_from_today_when_december_is_far() {
        let inputs = PathInputs {
            cdi_now: Some(0.15),
            ..PathInputs::default()
        };
        // Em março de 2026: de 15% hoje até 13,9% em dezembro (9 meses).
        let path = rate_path(&focus(), date!(2026 - 03 - 10), 9, inputs);
        let first = path.at(1).unwrap().cdi.unwrap();
        assert!((first - (0.15 + (0.139 - 0.15) / 9.0)).abs() < 1e-12);
        assert!((path.at(9).unwrap().cdi.unwrap() - 0.139).abs() < 1e-12);
    }

    #[test]
    fn cdi_without_focus_stays_where_it_is_and_without_anything_is_missing() {
        let measured = rate_path(
            &Focus::default(),
            date!(2026 - 11 - 27),
            3,
            PathInputs {
                cdi_now: Some(0.14),
                ..PathInputs::default()
            },
        );
        assert_eq!(measured.cdi_source, PathSource::Measured);
        assert!(measured.months.iter().all(|m| m.cdi == Some(0.14)));

        let missing = rate_path(
            &Focus::default(),
            date!(2026 - 11 - 27),
            3,
            PathInputs::default(),
        );
        assert_eq!(missing.cdi_source, PathSource::Missing);
        assert!(missing.months.iter().all(|m| m.cdi.is_none()));

        // Só o Focus, sem o CDI de hoje: antes do primeiro dezembro, ele.
        let focus_only = rate_path(&focus(), date!(2026 - 11 - 27), 2, PathInputs::default());
        assert!((focus_only.at(1).unwrap().cdi.unwrap() - 0.139).abs() < 1e-12);
    }

    #[test]
    fn informed_rates_replace_the_whole_path() {
        let path = rate_path(
            &focus(),
            date!(2026 - 11 - 27),
            24,
            PathInputs {
                cdi_now: Some(0.15),
                cdi_informed: Some(0.10),
                ipca_informed: Some(0.06),
                ..PathInputs::default()
            },
        );
        assert_eq!(path.cdi_source, PathSource::Informed);
        assert_eq!(path.ipca_source, PathSource::Informed);
        assert!(path.months.iter().all(|m| m.cdi == Some(0.10)));
        assert!((path.ipca_annual() - 0.06).abs() < 1e-9);
    }

    #[test]
    fn an_empty_path_has_no_inflation() {
        let path = rate_path(&focus(), date!(2026 - 11 - 27), 0, PathInputs::default());
        assert_eq!(path.ipca_annual(), 0.0);
        assert_eq!(path.cdi_end(), None);
    }
}
