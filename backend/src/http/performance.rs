//! Patrimônio no tempo e rentabilidade contra os índices (handlers e DTOs).

use axum::extract::State;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use super::{ApiQuery, AppState, CurrentUser};
use crate::domain::monthly_stats::MonthMark;
use crate::domain::net_worth::Bridge;
use crate::domain::performance::{month_index, to_percent, Accrued, Period, Window};
use crate::error::{ApiError, ApiResult};
use crate::history::IndexCode;
use crate::performance::classes::{self as class_service, ClassRow};
use crate::performance::{self as service, Card, MonthIndex, NetWorthPoint, YearReport};

/// Os índices quando a tela não diz quais.
const DEFAULT_BENCHMARKS: [IndexCode; 4] = [
    IndexCode::Cdi,
    IndexCode::Ipca,
    IndexCode::Ibov,
    IndexCode::Ifix,
];
/// Casas dos percentuais (0,0123%).
const PERCENT_DP: u32 = 4;

#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    period: Option<String>,
    benchmarks: Option<String>,
}

fn period(query: &PeriodQuery) -> ApiResult<Period> {
    match query.period.as_deref().map(str::trim) {
        None | Some("") => Ok(Period::TwelveMonths),
        Some(value) => Period::parse(value).ok_or_else(|| {
            ApiError::validation("O periodo precisa ser month, ytd, 12m, 2y, 5y ou all.")
        }),
    }
}

fn benchmarks(query: &PeriodQuery) -> ApiResult<Vec<IndexCode>> {
    let Some(list) = query.benchmarks.as_deref() else {
        return Ok(DEFAULT_BENCHMARKS.to_vec());
    };
    let mut codes: Vec<IndexCode> = Vec::new();
    for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let code = IndexCode::parse(item).ok_or_else(|| {
            ApiError::validation(format!(
                "Indice desconhecido: {item}. Use cdi, selic, ipca, ibov ou ifix."
            ))
        })?;
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    Ok(codes)
}

fn day(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

fn money(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

fn percent(fraction: Decimal) -> f64 {
    money(to_percent(fraction, PERCENT_DP))
}

/// A data da marca de "sujo", se a série da pessoa espera ser refeita.
async fn rebuilding_since(state: &AppState, user: uuid::Uuid) -> ApiResult<Option<String>> {
    Ok(state
        .store
        .dirty_since(user)
        .await?
        .map(|dirty| day(dirty.since)))
}

#[derive(Debug, Serialize)]
pub struct NetWorthPointDto {
    date: String,
    market_value: f64,
    invested: f64,
    gain: f64,
    /// Proventos recebidos desde o começo da série.
    income: f64,
    partial: bool,
}

impl From<&NetWorthPoint> for NetWorthPointDto {
    fn from(p: &NetWorthPoint) -> Self {
        Self {
            date: day(p.date),
            market_value: money(p.market_value),
            invested: money(p.invested),
            gain: money(p.gain),
            income: money(p.income),
            partial: p.partial,
        }
    }
}

/// De onde veio a variação do patrimônio no período.
#[derive(Debug, Serialize)]
pub struct BridgeDto {
    /// O fechamento de base; `null`: a janela começa do zero (o período
    /// "Tudo", ou uma série mais nova que o período).
    from: Option<String>,
    to: String,
    start_value: f64,
    /// Aportes menos retiradas no período.
    contributions: f64,
    /// O que o preço fez no período: `end_value - start_value - contributions`.
    market: f64,
    /// Proventos recebidos no período (fora do patrimônio).
    income: f64,
    end_value: f64,
    /// Desde o começo da série: o aplicado, o ganho de preço e os proventos.
    invested: f64,
    gain: f64,
    income_total: f64,
}

impl From<Bridge> for BridgeDto {
    fn from(b: Bridge) -> Self {
        Self {
            from: b.from.map(day),
            to: day(b.to),
            start_value: money(b.start_value),
            contributions: money(b.contributions),
            market: money(b.market),
            income: money(b.income),
            end_value: money(b.end_value),
            invested: money(b.invested),
            gain: money(b.gain),
            income_total: money(b.income_total),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NetWorthResponse {
    period: &'static str,
    starts_on: Option<String>,
    /// O livro mudou no passado e a série está sendo refeita desde esta data:
    /// os números ainda são os de antes da mudança.
    rebuilding_since: Option<String>,
    points: Vec<NetWorthPointDto>,
    /// `null` sem série.
    summary: Option<BridgeDto>,
}

pub async fn net_worth(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<PeriodQuery>,
) -> ApiResult<Json<NetWorthResponse>> {
    let period = period(&query)?;
    let now = OffsetDateTime::now_utc();
    let points = service::series(&state.store, user.id, now).await?;
    let today = service::local_date(now);
    let worth = service::net_worth(&points, period, today);
    Ok(Json(NetWorthResponse {
        period: period.as_str(),
        starts_on: points.first().map(|p| day(p.date)),
        rebuilding_since: rebuilding_since(&state, user.id).await?,
        points: worth.iter().map(NetWorthPointDto::from).collect(),
        summary: service::net_worth_bridge(&points, period, today).map(BridgeDto::from),
    }))
}

#[derive(Debug, Serialize)]
pub struct WindowDto {
    from: String,
    to: String,
}

impl From<Window> for WindowDto {
    fn from(w: Window) -> Self {
        Self {
            from: day(w.from),
            to: day(w.to),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BenchmarkDto {
    code: &'static str,
    label: &'static str,
    /// `null` quando a janela ainda não tem dado do índice.
    return_percent: Option<f64>,
    as_of: Option<String>,
}

fn benchmark(code: IndexCode, accrued: Accrued) -> BenchmarkDto {
    BenchmarkDto {
        code: code.as_str(),
        label: code.label(),
        return_percent: accrued.as_of.map(|_| percent(accrued.ret)),
        as_of: accrued.as_of.map(day),
    }
}

#[derive(Debug, Serialize)]
pub struct CurvePointDto {
    date: String,
    portfolio: f64,
    benchmarks: std::collections::BTreeMap<&'static str, Option<f64>>,
}

#[derive(Debug, Serialize)]
pub struct CardDto {
    from: String,
    to: String,
    return_percent: f64,
    cdi_percent: Option<f64>,
    cdi_as_of: Option<String>,
    /// Quanto rendeu acima (+) ou abaixo (−) do CDI, de forma composta.
    excess_over_cdi: Option<f64>,
    /// A rentabilidade em % do CDI (110 = 110% do CDI).
    percent_of_cdi: Option<f64>,
    full_window: bool,
}

impl From<Card> for CardDto {
    fn from(c: Card) -> Self {
        let known = c.cdi.as_of.is_some();
        Self {
            from: day(c.window.from),
            to: day(c.window.to),
            return_percent: percent(c.window.ret),
            cdi_percent: known.then(|| percent(c.cdi.ret)),
            cdi_as_of: c.cdi.as_of.map(day),
            excess_over_cdi: known.then(|| percent(c.versus.excess)),
            percent_of_cdi: c
                .versus
                .relative
                .filter(|_| known)
                .map(|r| money((r * Decimal::ONE_HUNDRED).round_dp(1))),
            full_window: c.full,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SummaryDto {
    twelve_months: Option<CardDto>,
    since_start: Option<CardDto>,
}

/// Um índice numa linha da tabela mês × ano.
#[derive(Debug, Serialize)]
pub struct YearIndexDto {
    /// Janeiro a dezembro, em %, na janela de cada mês da carteira; `null` onde
    /// a carteira não tem o mês ou o índice não tem dado.
    months: Vec<Option<f64>>,
    /// Acumulado desde o começo da série até o fim de cada mês.
    accumulated: Vec<Option<f64>>,
    /// O ano, na janela do ano da carteira.
    return_percent: Option<f64>,
    /// Desde o começo da série até o fim do ano.
    accumulated_percent: Option<f64>,
    /// O último dia coberto por dado publicado no ano.
    as_of: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct YearDto {
    year: i32,
    /// Janeiro a dezembro, em %; `null` onde não há série.
    months: Vec<Option<f64>>,
    return_percent: f64,
    cdi_percent: Option<f64>,
    /// Desde o começo da série até o fim de cada mês, em %.
    accumulated: Vec<Option<f64>>,
    /// Desde o começo da série até o fim do ano.
    accumulated_percent: f64,
    /// Cada índice pedido, nas mesmas janelas.
    benchmarks: std::collections::BTreeMap<&'static str, YearIndexDto>,
}

fn known(accrued: Accrued) -> Option<f64> {
    accrued.as_of.map(|_| percent(accrued.ret))
}

impl From<&YearReport> for YearDto {
    fn from(y: &YearReport) -> Self {
        let mut accumulated = vec![None; 12];
        for m in &y.months {
            accumulated[month_index(m.cell.month)] = Some(percent(m.cell.accumulated));
        }
        let benchmarks = y
            .benchmarks
            .iter()
            .enumerate()
            .map(|(i, year): (usize, &MonthIndex)| {
                let mut months = vec![None; 12];
                let mut accumulated = vec![None; 12];
                for m in &y.months {
                    let at = month_index(m.cell.month);
                    months[at] = known(m.benchmarks[i].month);
                    accumulated[at] = known(m.benchmarks[i].accumulated);
                }
                (
                    year.code.as_str(),
                    YearIndexDto {
                        months,
                        accumulated,
                        return_percent: known(year.month),
                        accumulated_percent: known(year.accumulated),
                        as_of: year.month.as_of.map(day),
                    },
                )
            })
            .collect();
        Self {
            year: y.row.year,
            months: y.row.months.iter().map(|m| m.map(percent)).collect(),
            return_percent: percent(y.row.total),
            cdi_percent: known(y.cdi),
            accumulated,
            accumulated_percent: percent(y.accumulated),
            benchmarks,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MonthMarkDto {
    year: i32,
    /// 1 a 12.
    month: u8,
    return_percent: f64,
}

impl From<MonthMark> for MonthMarkDto {
    fn from(m: MonthMark) -> Self {
        Self {
            year: m.year,
            month: u8::from(m.month),
            return_percent: percent(m.ret),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VersusMonthsDto {
    code: &'static str,
    label: &'static str,
    /// Meses em que a carteira rendeu mais que o índice.
    above: usize,
    /// Meses em que o índice tinha dado para comparar.
    compared: usize,
}

#[derive(Debug, Serialize)]
pub struct MonthsSummaryDto {
    best: Option<MonthMarkDto>,
    worst: Option<MonthMarkDto>,
    positive: usize,
    negative: usize,
    flat: usize,
    versus: Vec<VersusMonthsDto>,
}

#[derive(Debug, Serialize)]
pub struct PerformanceResponse {
    period: &'static str,
    starts_on: Option<String>,
    /// Ver [`NetWorthResponse::rebuilding_since`].
    rebuilding_since: Option<String>,
    window: Option<WindowDto>,
    return_percent: Option<f64>,
    benchmarks: Vec<BenchmarkDto>,
    points: Vec<CurvePointDto>,
    summary: SummaryDto,
    years: Vec<YearDto>,
    /// Melhor e pior mês, contagens e meses acima de cada índice.
    months_summary: MonthsSummaryDto,
}

pub async fn performance(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<PeriodQuery>,
) -> ApiResult<Json<PerformanceResponse>> {
    let period = period(&query)?;
    let codes = benchmarks(&query)?;
    let now = OffsetDateTime::now_utc();
    let today = service::local_date(now);
    let points = service::series(&state.store, user.id, now).await?;
    let indexes = service::load_indexes(&state.store, &codes, &points, today).await?;
    let report = service::performance(&points, period, &codes, &indexes, today);
    Ok(Json(PerformanceResponse {
        period: period.as_str(),
        starts_on: report.starts_on.map(day),
        rebuilding_since: rebuilding_since(&state, user.id).await?,
        window: report.window.map(WindowDto::from),
        return_percent: report.window.map(|w| percent(w.ret)),
        benchmarks: report
            .benchmarks
            .iter()
            .map(|b| benchmark(b.code, b.accrued))
            .collect(),
        points: report
            .curve
            .iter()
            .map(|p| CurvePointDto {
                date: day(p.date),
                portfolio: percent(p.portfolio),
                benchmarks: p
                    .benchmarks
                    .iter()
                    .map(|(code, value)| (code.as_str(), value.map(percent)))
                    .collect(),
            })
            .collect(),
        summary: SummaryDto {
            twelve_months: report.twelve_months.map(CardDto::from),
            since_start: report.since_start.map(CardDto::from),
        },
        years: report.years.iter().map(YearDto::from).collect(),
        months_summary: MonthsSummaryDto {
            best: report.months.stats.best.map(MonthMarkDto::from),
            worst: report.months.stats.worst.map(MonthMarkDto::from),
            positive: report.months.stats.positive,
            negative: report.months.stats.negative,
            flat: report.months.stats.flat,
            versus: report
                .months
                .versus
                .iter()
                .map(|(code, beat)| VersusMonthsDto {
                    code: code.as_str(),
                    label: code.label(),
                    above: beat.above,
                    compared: beat.compared,
                })
                .collect(),
        },
    }))
}

#[derive(Debug, Serialize)]
pub struct ClassDto {
    asset_class: &'static str,
    /// Patrimônio da classe no fim do período (0 se a classe zerou nele).
    market_value: f64,
    /// Fração do patrimônio no fim do período, em %.
    weight_percent: f64,
    /// Pelo método de cotas, na janela da classe.
    return_percent: f64,
    /// A janela da classe: a base (o começo do período, ou o dia em que a
    /// classe recebeu o primeiro dinheiro) e o último dia medido.
    from: String,
    to: String,
    /// A classe mede o período inteiro.
    full_window: bool,
    /// O índice de referência da classe na mesma janela.
    benchmark: Option<BenchmarkDto>,
}

impl ClassDto {
    fn new(row: &ClassRow, window: Window) -> Self {
        Self {
            asset_class: row.ret.class.as_str(),
            market_value: money(row.ret.market_value),
            weight_percent: percent(row.weight),
            return_percent: percent(row.ret.ret),
            from: day(row.ret.from),
            to: day(row.ret.to),
            full_window: row.ret.from <= window.from && row.ret.to >= window.to,
            benchmark: row
                .benchmark
                .map(|(code, accrued)| benchmark(code, accrued)),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ClassesResponse {
    period: &'static str,
    /// Ver [`NetWorthResponse::rebuilding_since`].
    rebuilding_since: Option<String>,
    window: Option<WindowDto>,
    classes: Vec<ClassDto>,
}

pub async fn classes(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<PeriodQuery>,
) -> ApiResult<Json<ClassesResponse>> {
    let period = period(&query)?;
    let report =
        class_service::class_report(&state.store, user.id, period, OffsetDateTime::now_utc())
            .await?;
    Ok(Json(ClassesResponse {
        period: period.as_str(),
        rebuilding_since: rebuilding_since(&state, user.id).await?,
        window: report.window.map(WindowDto::from),
        classes: report
            .window
            .map(|w| report.classes.iter().map(|c| ClassDto::new(c, w)).collect())
            .unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(period: Option<&str>, benchmarks: Option<&str>) -> PeriodQuery {
        PeriodQuery {
            period: period.map(str::to_owned),
            benchmarks: benchmarks.map(str::to_owned),
        }
    }

    #[test]
    fn reads_period_and_benchmarks() {
        assert_eq!(period(&query(None, None)).unwrap(), Period::TwelveMonths);
        assert_eq!(period(&query(Some("all"), None)).unwrap(), Period::All);
        assert_eq!(
            period(&query(Some("10y"), None)).unwrap_err().code,
            "validation_failed"
        );
        assert_eq!(benchmarks(&query(None, None)).unwrap(), DEFAULT_BENCHMARKS);
        assert_eq!(
            benchmarks(&query(None, Some("ibov, cdi,cdi,"))).unwrap(),
            vec![IndexCode::Ibov, IndexCode::Cdi]
        );
        assert!(benchmarks(&query(None, Some(""))).unwrap().is_empty());
        assert_eq!(
            benchmarks(&query(None, Some("ifix"))).unwrap(),
            vec![IndexCode::Ifix]
        );
        assert!(benchmarks(&query(None, Some("ivvb11"))).is_err());
        assert_eq!(period(&query(Some("2y"), None)).unwrap(), Period::TwoYears);
        assert_eq!(period(&query(Some("5y"), None)).unwrap(), Period::FiveYears);
    }
}
