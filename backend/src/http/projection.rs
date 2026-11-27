//! `GET /income/projection`: a renda dos próximos 12 ou 24 meses em três
//! cenários, por mês, por tipo e por ativo. A regra está em
//! `domain::projection::income`; o método, em `docs/projecao.md`. Lê só o
//! banco (a estimativa da Renda, o Focus da busca diária, o CDI e o IPCA do
//! Desempenho): nenhuma requisição espera a rede.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::{ApiQuery, AppState, CurrentUser};
use crate::domain::income::today_in_brazil;
use crate::domain::projection::income::{project, Driver, Group, IncomePremises};
use crate::domain::projection::rates::PathSource;
use crate::domain::projection::{fraction, percent, to_decimal, DEFAULT_SPREAD_PERCENT};
use crate::error::{ApiError, ApiResult};
use crate::projection::{projected_assets, reconcile, LevelBasis, Outlook};
use crate::service;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// Os horizontes que a tela oferece.
const HORIZONS: [u32; 2] = [12, 24];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionQuery {
    /// 12 ou 24.
    #[serde(default)]
    pub months: Option<u32>,
    /// Amplitude dos cenários, em pontos percentuais ao ano (0 a 20).
    #[serde(default)]
    pub spread: Option<Decimal>,
    /// O CDI do período inteiro, em % ao ano, no lugar do Focus (0 a 50).
    #[serde(default)]
    pub cdi: Option<Decimal>,
    /// O IPCA do período inteiro, em % ao ano, no lugar do Focus (−10 a 50).
    #[serde(default)]
    pub ipca: Option<Decimal>,
}

fn in_range(name: &str, value: Decimal, min: i64, max: i64) -> ApiResult<Decimal> {
    if value < Decimal::from(min) || value > Decimal::from(max) {
        return Err(ApiError::validation(format!(
            "{name} precisa estar entre {min} e {max}."
        )));
    }
    Ok(value.round_dp(4))
}

#[derive(Debug, Serialize)]
pub struct MonthDto {
    /// `AAAA-MM`.
    month: String,
    #[serde(with = "decimal_out")]
    low: Decimal,
    #[serde(with = "decimal_out")]
    base: Decimal,
    #[serde(with = "decimal_out")]
    high: Decimal,
    /// O base de cada tipo.
    by_group: BTreeMap<Group, f64>,
}

#[derive(Debug, Serialize)]
pub struct TotalsDto {
    #[serde(with = "decimal_out")]
    low: Decimal,
    #[serde(with = "decimal_out")]
    base: Decimal,
    #[serde(with = "decimal_out")]
    high: Decimal,
}

#[derive(Debug, Serialize)]
pub struct GroupDto {
    group: Group,
    driver: Driver,
    assets: usize,
    /// A renda de partida do tipo, por mês.
    #[serde(with = "decimal_out")]
    monthly_start: Decimal,
    totals: TotalsDto,
}

#[derive(Debug, Serialize)]
pub struct AssetDto {
    asset_id: Uuid,
    symbol: Option<String>,
    display_name: String,
    group: Group,
    driver: Driver,
    /// `trailing_12m` (os 12 meses da Renda) ou `usual` (a média do habitual).
    basis: LevelBasis,
    usual_years: Option<u32>,
    /// A partida da projeção, por mês.
    #[serde(with = "decimal_out")]
    monthly_start: Decimal,
    /// Os 12 meses da estimativa da Renda, por mês (difere da partida quando
    /// ela é a média do habitual).
    #[serde(with = "decimal_out")]
    estimate_monthly: Decimal,
    totals: TotalsDto,
}

/// Por que a partida não é a estimativa da Renda:
/// `estimate_monthly = start_monthly − usual_difference + unprojected_monthly`.
#[derive(Debug, Serialize)]
pub struct ReconciliationDto {
    #[serde(with = "decimal_out")]
    start_monthly: Decimal,
    /// Média do habitual menos os 12 meses nos ativos que partem da média
    /// (negativa: os 12 meses vieram acima do habitual).
    #[serde(with = "decimal_out")]
    usual_difference: Decimal,
    /// A estimativa dos ativos sem tipo de renda, que a projeção não leva.
    #[serde(with = "decimal_out")]
    unprojected_monthly: Decimal,
    /// A maior janela da média do habitual (anos).
    usual_years: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CdiDto {
    source: PathSource,
    /// O CDI de hoje, ao ano.
    #[serde(with = "decimal_out::option")]
    now_percent: Option<Decimal>,
    /// O CDI médio dos 12 meses que passaram, a base do FII de papel.
    #[serde(with = "decimal_out::option")]
    trailing_percent: Option<Decimal>,
    /// O CDI esperado no último mês do período.
    #[serde(with = "decimal_out::option")]
    end_percent: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct IpcaDto {
    source: PathSource,
    /// O IPCA do período, ao ano equivalente.
    #[serde(with = "decimal_out")]
    annual_percent: Decimal,
}

#[derive(Debug, Serialize)]
pub struct FocusDto {
    #[serde(with = "iso_date")]
    survey_date: Date,
    #[serde(with = "time::serde::rfc3339::option")]
    fetched_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct PremisesDto {
    horizon_months: u32,
    #[serde(with = "decimal_out")]
    scenario_spread_percent: Decimal,
    /// `informed` quando a amplitude veio na consulta.
    spread_source: &'static str,
    cdi: CdiDto,
    ipca: IpcaDto,
    /// A pesquisa do Focus que entrou na conta; `null` sem Focus no banco.
    focus: Option<FocusDto>,
}

#[derive(Debug, Serialize)]
pub struct IncomeProjectionDto {
    #[serde(with = "iso_date")]
    today: Date,
    /// A estimativa mensal da Renda (líquida), de onde a projeção parte.
    #[serde(with = "decimal_out")]
    estimate_monthly: Decimal,
    /// A conta da partida contra a estimativa.
    reconciliation: ReconciliationDto,
    months: Vec<MonthDto>,
    totals: TotalsDto,
    groups: Vec<GroupDto>,
    assets: Vec<AssetDto>,
    premises: PremisesDto,
    /// Os papéis que pagam provento e ficaram sem estimativa (fora da conta).
    missing: Vec<String>,
}

fn totals(low: f64, base: f64, high: f64) -> TotalsDto {
    TotalsDto {
        low: to_decimal(low, 2),
        base: to_decimal(base, 2),
        high: to_decimal(high, 2),
    }
}

pub async fn income_projection(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<ProjectionQuery>,
) -> ApiResult<Json<IncomeProjectionDto>> {
    let horizon = query.months.unwrap_or(HORIZONS[0]);
    if !HORIZONS.contains(&horizon) {
        return Err(ApiError::validation("months precisa ser 12 ou 24."));
    }
    let spread = query
        .spread
        .map(|v| in_range("spread", v, 0, 20))
        .transpose()?;
    let cdi = query.cdi.map(|v| in_range("cdi", v, 0, 50)).transpose()?;
    let ipca = query
        .ipca
        .map(|v| in_range("ipca", v, -10, 50))
        .transpose()?;

    let now = OffsetDateTime::now_utc();
    let today = today_in_brazil(now);
    let view = service::portfolio(&state.store, user.id).await?;
    let income = state.income.estimate(&view, today).await;
    let outlook = Outlook::load(&state.store, today).await?;
    let path = outlook.path(today, horizon, cdi, ipca);
    let assets = projected_assets(&view, &income);
    let inputs: Vec<_> = assets.iter().map(|a| a.input.clone()).collect();
    let spread_percent = spread.unwrap_or(DEFAULT_SPREAD_PERCENT);
    let projection = project(
        &inputs,
        &path,
        IncomePremises {
            spread: fraction(spread_percent),
            cdi_trailing: outlook.cdi_trailing,
        },
    );

    let asset_dtos: Vec<AssetDto> = assets
        .iter()
        .zip(&projection.assets)
        .map(|(a, p)| AssetDto {
            asset_id: a.input.asset_id,
            symbol: a.symbol.clone(),
            display_name: a.display_name.clone(),
            group: a.input.group,
            driver: a.input.group.driver(),
            basis: a.basis,
            usual_years: a.usual_years,
            monthly_start: a.input.monthly,
            estimate_monthly: a.estimate_monthly,
            totals: totals(p.low, p.base, p.high),
        })
        .collect();
    let mut groups: BTreeMap<Group, (usize, Decimal, f64, f64, f64)> = BTreeMap::new();
    for (a, p) in assets.iter().zip(&projection.assets) {
        let slot = groups
            .entry(a.input.group)
            .or_insert((0, Decimal::ZERO, 0.0, 0.0, 0.0));
        slot.0 += 1;
        slot.1 += a.input.monthly;
        slot.2 += p.low;
        slot.3 += p.base;
        slot.4 += p.high;
    }
    let (low, base, high) = projection.totals();
    let reconciliation = reconcile(&assets, income.monthly_total);
    let dto = IncomeProjectionDto {
        today,
        estimate_monthly: income.monthly_total,
        reconciliation: ReconciliationDto {
            start_monthly: reconciliation.start_monthly,
            usual_difference: reconciliation.usual_difference,
            unprojected_monthly: reconciliation.unprojected_monthly,
            usual_years: reconciliation.usual_years,
        },
        months: projection
            .months
            .iter()
            .map(|m| MonthDto {
                month: m.month.label(),
                low: to_decimal(m.low, 2),
                base: to_decimal(m.base, 2),
                high: to_decimal(m.high, 2),
                by_group: m
                    .by_group
                    .iter()
                    .map(|(g, v)| (*g, (v * 100.0).round() / 100.0))
                    .collect(),
            })
            .collect(),
        totals: totals(low, base, high),
        groups: groups
            .into_iter()
            .map(|(group, (count, start, l, b, h))| GroupDto {
                group,
                driver: group.driver(),
                assets: count,
                monthly_start: start,
                totals: totals(l, b, h),
            })
            .collect(),
        assets: asset_dtos,
        premises: PremisesDto {
            horizon_months: horizon,
            scenario_spread_percent: spread_percent,
            spread_source: if spread.is_some() {
                "informed"
            } else {
                "default"
            },
            cdi: CdiDto {
                source: path.cdi_source,
                now_percent: outlook.cdi_now.map(percent),
                trailing_percent: outlook.cdi_trailing.map(percent),
                end_percent: path.cdi_end().map(percent),
            },
            ipca: IpcaDto {
                source: path.ipca_source,
                annual_percent: percent(path.ipca_annual()),
            },
            focus: outlook.focus.survey_date().map(|survey_date| FocusDto {
                survey_date,
                fetched_at: outlook.fetched_at,
            }),
        },
        missing: income.missing,
    };
    Ok(Json(dto))
}
