//! `GET /api/v1/compare` e `GET /api/v1/compare/returns`: o comparador de
//! ativos (tabela lado a lado e curvas de rentabilidade). A regra está em
//! `domain::compare`; a busca, em `crate::compare`; o contrato, em
//! `docs/api.md`. Texto de tela (rótulos, dicas, motivos) é da tela: aqui só
//! ids e códigos estáveis.

use axum::extract::State;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use super::fundamentals::RatesDto;
use super::references::{CdiDto, RealRateDto};
use super::{ApiQuery, AppState, CurrentUser};
use crate::compare::{self, Column, CompareError, Curve, CurveKind, CurveRequest, Sources};
use crate::domain::asset_class::AssetClass;
use crate::domain::compare::{self as rules, Direction, Family, Group, Mark, Period, Row, Unit};
use crate::error::{ApiError, ApiResult};
use crate::history::IndexCode;

/// Os índices quando a tela não diz quais.
const DEFAULT_BENCHMARKS: [IndexCode; 2] = [IndexCode::Cdi, IndexCode::Ibov];

impl From<CompareError> for ApiError {
    fn from(error: CompareError) -> Self {
        let CompareError::Invalid(message) = error;
        ApiError::validation(message)
    }
}

fn day(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

fn float(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

#[derive(Debug, Deserialize)]
pub struct TableQuery {
    assets: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReturnsQuery {
    assets: Option<String>,
    period: Option<String>,
    total_return: Option<bool>,
    align_start: Option<bool>,
    portfolio: Option<bool>,
    benchmarks: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssetDto {
    /// A escolha como a tela mandou (`PETR4`, `KNRI11:br_reits`).
    key: String,
    symbol: String,
    name: String,
    asset_class: AssetClass,
    in_portfolio: bool,
    family: Family,
    currency: String,
    price: Option<f64>,
}

impl From<&Column> for AssetDto {
    fn from(column: &Column) -> Self {
        let subject = &column.subject;
        Self {
            key: subject.key.clone(),
            symbol: subject.symbol.clone(),
            name: subject.name.clone(),
            asset_class: subject.class,
            in_portfolio: subject.in_portfolio(),
            family: subject.family(),
            currency: column.currency.clone(),
            price: column.price.map(float),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CellDto {
    value: Option<f64>,
    text: Option<String>,
    /// Por que não há valor: um código que a tela traduz (os motivos dos
    /// fundamentos mais `not_applicable`, `no_price`, `no_price_history`,
    /// `short_history`, `no_dividend_history`, `no_sector`,
    /// `no_sector_source`, `not_tracked`).
    reason: Option<&'static str>,
    /// O extremo da linha, entre ativos do mesmo tipo.
    mark: Option<Mark>,
}

#[derive(Debug, Serialize)]
pub struct RowDto {
    id: Row,
    group: Group,
    unit: Unit,
    direction: Direction,
    /// Uma célula por ativo, na ordem de `assets`.
    cells: Vec<CellDto>,
}

#[derive(Debug, Serialize)]
pub struct CompareResponse {
    as_of: String,
    assets: Vec<AssetDto>,
    rows: Vec<RowDto>,
    /// O CDI de 12 meses e o juro real, ao lado do rendimento de 12 meses (só
    /// contexto, sem destaque).
    rates: RatesDto,
}

fn sources(state: &AppState) -> Sources<'_> {
    Sources {
        store: &state.store,
        history: state.history.as_ref(),
        income: state.income.as_ref(),
        quotes: state.quotes.as_ref(),
    }
}

async fn subjects(
    state: &AppState,
    user: &CurrentUser,
    list: Option<&str>,
) -> ApiResult<Vec<compare::Subject>> {
    let portfolio = state.store.assets(user.id).await?;
    Ok(compare::resolve(&portfolio, list.unwrap_or_default())?)
}

pub async fn indicators(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<TableQuery>,
) -> ApiResult<Json<CompareResponse>> {
    let subjects = subjects(&state, &user, query.assets.as_deref()).await?;
    let table = compare::table(&sources(&state), &subjects, OffsetDateTime::now_utc()).await?;
    let families: Vec<Family> = table.columns.iter().map(|c| c.subject.family()).collect();
    let rows = Row::ALL
        .iter()
        .filter(|row| {
            table
                .columns
                .iter()
                .any(|c| c.cells.get(row).is_some_and(compare::Cell::applies))
        })
        .map(|row| {
            let values: Vec<(Family, Option<Decimal>)> = table
                .columns
                .iter()
                .zip(&families)
                .map(|(column, family)| (*family, column.cells.get(row).and_then(|c| c.value)))
                .collect();
            let marks = rules::marks(&values, row.direction());
            let cells = table
                .columns
                .iter()
                .zip(marks)
                .map(|(column, mark)| {
                    let cell = column.cells.get(row);
                    CellDto {
                        value: cell.and_then(|c| c.value).map(float),
                        text: cell.and_then(|c| c.text.clone()),
                        reason: cell.and_then(|c| c.reason),
                        mark,
                    }
                })
                .collect();
            RowDto {
                id: *row,
                group: row.group(),
                unit: row.unit(),
                direction: row.direction(),
                cells,
            }
        })
        .collect();
    let rates = RatesDto {
        cdi_12m: state.store.cdi_12m(table.as_of).await?.map(CdiDto::from),
        real_rate: state.store.real_rate().await?.map(RealRateDto::from),
    };
    Ok(Json(CompareResponse {
        as_of: day(table.as_of),
        assets: table.columns.iter().map(AssetDto::from).collect(),
        rows,
        rates,
    }))
}

#[derive(Debug, Serialize)]
pub struct CurveDto {
    key: String,
    kind: &'static str,
    name: String,
    /// Em %, uma por data de `dates`; `null` antes de a curva existir.
    values: Vec<Option<f64>>,
    return_percent: Option<f64>,
    /// Onde a curva abre em 0%.
    starts_on: Option<String>,
    /// Por que não há curva: `no_price_history`, `no_series`, `no_index_data`.
    reason: Option<&'static str>,
    /// Os proventos entram na conta desta curva.
    includes_income: bool,
    /// O preço foi levado a reais pelo câmbio de cada dia.
    converted_to_brl: bool,
}

impl From<&Curve> for CurveDto {
    fn from(curve: &Curve) -> Self {
        Self {
            key: curve.key.clone(),
            kind: match curve.kind {
                CurveKind::Asset => "asset",
                CurveKind::Portfolio => "portfolio",
                CurveKind::Index => "index",
            },
            name: curve.name.clone(),
            values: curve.values.clone(),
            return_percent: curve.return_percent,
            starts_on: curve.starts_on.map(day),
            reason: curve.reason,
            includes_income: curve.includes_income,
            converted_to_brl: curve.converted,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReturnsResponse {
    period: &'static str,
    total_return: bool,
    align_start: bool,
    from: String,
    to: String,
    dates: Vec<String>,
    series: Vec<CurveDto>,
}

fn benchmarks(list: Option<&str>) -> ApiResult<Vec<IndexCode>> {
    let Some(list) = list else {
        return Ok(DEFAULT_BENCHMARKS.to_vec());
    };
    let mut codes: Vec<IndexCode> = Vec::new();
    for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let code = match IndexCode::parse(item) {
            Some(code @ (IndexCode::Cdi | IndexCode::Ibov)) => code,
            _ => {
                return Err(ApiError::validation(format!(
                    "Indice desconhecido: {item}. Use cdi ou ibov."
                )))
            }
        };
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    Ok(codes)
}

pub async fn returns(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<ReturnsQuery>,
) -> ApiResult<Json<ReturnsResponse>> {
    let period = match query.period.as_deref().map(str::trim) {
        None | Some("") => Period::TwelveMonths,
        Some(value) => Period::parse(value).ok_or_else(|| {
            ApiError::validation("O periodo precisa ser 1m, 3m, 6m, ytd, 12m, 3y, 5y ou max.")
        })?,
    };
    let request = CurveRequest {
        period,
        total_return: query.total_return.unwrap_or(false),
        align_start: query.align_start.unwrap_or(false),
        portfolio: query.portfolio.unwrap_or(true),
        benchmarks: benchmarks(query.benchmarks.as_deref())?,
    };
    let subjects = subjects(&state, &user, query.assets.as_deref()).await?;
    let now = OffsetDateTime::now_utc();
    let curves = compare::curves(&sources(&state), user.id, &subjects, &request, now).await?;
    Ok(Json(ReturnsResponse {
        period: curves.period.as_str(),
        total_return: request.total_return,
        align_start: request.align_start,
        from: day(curves.from),
        to: day(curves.to),
        dates: curves.dates.iter().map(|d| day(*d)).collect(),
        series: curves.curves.iter().map(CurveDto::from).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmarks_default_to_cdi_and_ibov() {
        assert_eq!(benchmarks(None).unwrap(), DEFAULT_BENCHMARKS);
        assert_eq!(
            benchmarks(Some("ibov, cdi,cdi,")).unwrap(),
            vec![IndexCode::Ibov, IndexCode::Cdi]
        );
        assert!(benchmarks(Some("")).unwrap().is_empty());
        assert!(benchmarks(Some("ipca")).is_err());
        assert!(benchmarks(Some("ifix")).is_err());
    }
}
