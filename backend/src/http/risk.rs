//! Risco da carteira: volatilidade, queda máxima, correlação e risco × retorno
//! (handler e DTOs). Método e limites em `docs/risco.md`.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use super::{ApiQuery, AppState, CurrentUser};
use crate::domain::risk::{
    Drawdown, Metrics, RiskWindow, MIN_COMMON_RETURNS, MIN_RETURNS, TRADING_DAYS_PER_YEAR,
};
use crate::error::{ApiError, ApiResult};
use crate::history::IndexCode;
use crate::risk::{self as service, AssetRisk, ModelReport, PairReport};

#[derive(Debug, Deserialize)]
pub struct RiskQuery {
    window: Option<String>,
}

fn window(query: &RiskQuery) -> ApiResult<RiskWindow> {
    match query.window.as_deref().map(str::trim) {
        None | Some("") => Ok(RiskWindow::TwelveMonths),
        Some(value) => RiskWindow::parse(value)
            .ok_or_else(|| ApiError::validation("A janela precisa ser 6m, 12m, 24m, 36m ou 60m.")),
    }
}

fn day(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

/// Fração → percentual com 2 casas (0,1234 → 12,34).
fn percent(fraction: f64) -> f64 {
    (fraction * 10_000.0).round() / 100.0
}

/// Correlação com 3 casas.
fn coefficient(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[derive(Debug, Serialize)]
pub struct DrawdownDto {
    /// Negativo: −23,4 = o nível chegou a 23,4% abaixo do pico.
    depth_percent: f64,
    peak: String,
    trough: String,
    /// `null`: ainda não voltou ao pico.
    recovered: Option<String>,
}

impl From<Drawdown> for DrawdownDto {
    fn from(d: Drawdown) -> Self {
        Self {
            depth_percent: percent(d.depth),
            peak: day(d.peak),
            trough: day(d.trough),
            recovered: d.recovered.map(day),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MetricsDto {
    from: String,
    to: String,
    full_window: bool,
    returns: usize,
    volatility_percent: Option<f64>,
    return_percent: Option<f64>,
    /// `null` com histórico curto ou sem queda nenhuma na janela.
    drawdown: Option<DrawdownDto>,
}

impl From<Metrics> for MetricsDto {
    fn from(m: Metrics) -> Self {
        Self {
            from: day(m.from),
            to: day(m.to),
            full_window: m.full,
            returns: m.returns,
            volatility_percent: m.volatility.map(percent),
            return_percent: m.ret.map(percent),
            drawdown: m.drawdown.map(DrawdownDto::from),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BenchmarkDto {
    code: &'static str,
    label: &'static str,
    #[serde(flatten)]
    metrics: MetricsDto,
}

#[derive(Debug, Serialize)]
pub struct ModelDto {
    volatility_percent: f64,
    days: usize,
    covered_weight_percent: f64,
}

impl From<ModelReport> for ModelDto {
    fn from(m: ModelReport) -> Self {
        Self {
            volatility_percent: percent(m.volatility),
            days: m.days,
            covered_weight_percent: percent(m.covered_weight),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssetDto {
    asset_id: String,
    label: String,
    asset_class: &'static str,
    weight_percent: f64,
    status: &'static str,
    metrics: Option<MetricsDto>,
    risk_share_percent: Option<f64>,
    in_correlation: bool,
}

impl From<&AssetRisk> for AssetDto {
    fn from(a: &AssetRisk) -> Self {
        Self {
            asset_id: a.asset_id.to_string(),
            label: a.label.clone(),
            asset_class: a.asset_class.as_str(),
            weight_percent: percent(a.weight),
            status: a.status.as_str(),
            metrics: a.metrics.map(MetricsDto::from),
            risk_share_percent: a.risk_share.map(percent),
            in_correlation: a.correlated,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PairDto {
    a: String,
    b: String,
    value: f64,
    days: usize,
}

impl From<&PairReport> for PairDto {
    fn from(p: &PairReport) -> Self {
        Self {
            a: p.a.to_string(),
            b: p.b.to_string(),
            value: coefficient(p.value),
            days: p.days,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CorrelationDto {
    asset_ids: Vec<String>,
    values: Vec<Vec<Option<f64>>>,
    top_pairs: Vec<PairDto>,
}

#[derive(Debug, Serialize)]
pub struct LimitsDto {
    min_returns: usize,
    min_common: usize,
    trading_days_per_year: f64,
}

#[derive(Debug, Serialize)]
pub struct RiskResponse {
    window: &'static str,
    from: Option<String>,
    to: Option<String>,
    trading_days: usize,
    limits: LimitsDto,
    /// O livro mudou no passado e a série está sendo refeita desde esta data.
    rebuilding_since: Option<String>,
    portfolio: Option<MetricsDto>,
    benchmark: Option<BenchmarkDto>,
    model: Option<ModelDto>,
    assets: Vec<AssetDto>,
    correlation: CorrelationDto,
}

pub async fn risk(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<RiskQuery>,
) -> ApiResult<Json<RiskResponse>> {
    let window = window(&query)?;
    let report = service::report(&state.store, user.id, window, OffsetDateTime::now_utc()).await?;
    let rebuilding_since = state
        .store
        .dirty_since(user.id)
        .await?
        .map(|dirty| day(dirty.since));
    Ok(Json(RiskResponse {
        window: window.as_str(),
        from: report.from.map(day),
        to: report.to.map(day),
        trading_days: report.trading_days,
        limits: LimitsDto {
            min_returns: MIN_RETURNS,
            min_common: MIN_COMMON_RETURNS,
            trading_days_per_year: TRADING_DAYS_PER_YEAR,
        },
        rebuilding_since,
        portfolio: report.portfolio.map(MetricsDto::from),
        benchmark: report.benchmark.map(|m| BenchmarkDto {
            code: IndexCode::Ibov.as_str(),
            label: IndexCode::Ibov.label(),
            metrics: MetricsDto::from(m),
        }),
        model: report.model.map(ModelDto::from),
        assets: report.assets.iter().map(AssetDto::from).collect(),
        correlation: CorrelationDto {
            asset_ids: report
                .correlation
                .assets
                .iter()
                .map(ToString::to_string)
                .collect(),
            values: report
                .correlation
                .values
                .iter()
                .map(|row| row.iter().map(|v| v.map(coefficient)).collect())
                .collect(),
            top_pairs: report.correlation.pairs.iter().map(PairDto::from).collect(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_window_and_rounds() {
        let q = |w: Option<&str>| RiskQuery {
            window: w.map(str::to_owned),
        };
        assert_eq!(window(&q(None)).unwrap(), RiskWindow::TwelveMonths);
        assert_eq!(window(&q(Some(" "))).unwrap(), RiskWindow::TwelveMonths);
        assert_eq!(window(&q(Some("60m"))).unwrap(), RiskWindow::FiveYears);
        assert_eq!(
            window(&q(Some("5y"))).unwrap_err().code,
            "validation_failed"
        );
        assert_eq!(percent(0.123_456), 12.35);
        assert_eq!(percent(-0.2), -20.0);
        assert_eq!(coefficient(0.876_54), 0.877);
    }
}
