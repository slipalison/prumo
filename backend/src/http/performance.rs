//! Patrimônio no tempo e rentabilidade contra os índices (handlers e DTOs).

use axum::extract::State;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use super::{ApiQuery, AppState, CurrentUser};
use crate::domain::performance::{to_percent, Accrued, Period, Window};
use crate::error::{ApiError, ApiResult};
use crate::history::IndexCode;
use crate::performance::{self as service, Card, NetWorthPoint};

/// Os índices quando a tela não diz quais.
const DEFAULT_BENCHMARKS: [IndexCode; 3] = [IndexCode::Cdi, IndexCode::Ipca, IndexCode::Ibov];
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
        Some(value) => Period::parse(value)
            .ok_or_else(|| ApiError::validation("O periodo precisa ser month, ytd, 12m ou all.")),
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
                "Indice desconhecido: {item}. Use cdi, selic, ipca ou ibov."
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

#[derive(Debug, Serialize)]
pub struct NetWorthResponse {
    period: &'static str,
    starts_on: Option<String>,
    /// O livro mudou no passado e a série está sendo refeita desde esta data:
    /// os números ainda são os de antes da mudança.
    rebuilding_since: Option<String>,
    points: Vec<NetWorthPointDto>,
}

pub async fn net_worth(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<PeriodQuery>,
) -> ApiResult<Json<NetWorthResponse>> {
    let period = period(&query)?;
    let now = OffsetDateTime::now_utc();
    let points = service::series(&state.store, user.id, now).await?;
    let worth = service::net_worth(&points, period, service::local_date(now));
    Ok(Json(NetWorthResponse {
        period: period.as_str(),
        starts_on: points.first().map(|p| day(p.date)),
        rebuilding_since: rebuilding_since(&state, user.id).await?,
        points: worth.iter().map(NetWorthPointDto::from).collect(),
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

#[derive(Debug, Serialize)]
pub struct YearDto {
    year: i32,
    /// Janeiro a dezembro, em %; `null` onde não há série.
    months: Vec<Option<f64>>,
    return_percent: f64,
    cdi_percent: Option<f64>,
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
        years: report
            .years
            .iter()
            .map(|(row, cdi)| YearDto {
                year: row.year,
                months: row.months.iter().map(|m| m.map(percent)).collect(),
                return_percent: percent(row.total),
                cdi_percent: cdi.as_of.map(|_| percent(cdi.ret)),
            })
            .collect(),
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
            period(&query(Some("5y"), None)).unwrap_err().code,
            "validation_failed"
        );
        assert_eq!(benchmarks(&query(None, None)).unwrap(), DEFAULT_BENCHMARKS);
        assert_eq!(
            benchmarks(&query(None, Some("ibov, cdi,cdi,"))).unwrap(),
            vec![IndexCode::Ibov, IndexCode::Cdi]
        );
        assert!(benchmarks(&query(None, Some(""))).unwrap().is_empty());
        assert!(benchmarks(&query(None, Some("ifix"))).is_err());
    }
}
