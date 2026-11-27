//! O histórico de proventos flexível e o mapa de calor da tela Renda:
//! `GET /api/v1/income/history` e `GET /api/v1/income/heatmap`. A regra está em
//! `domain::income_history`; o método, em `docs/renda.md`.

use std::collections::HashMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::{ApiQuery, AppState, CurrentUser};
use crate::domain::asset_class::AssetClass;
use crate::domain::income::today_in_brazil;
use crate::domain::income_history::heatmap::{heatmap, monthly_costs, range, HeatCell, HeatYear};
use crate::domain::income_history::{
    history, segment_of, AmountKind, Bucket, DateBasis, Group, GroupKey, Grouping, History,
    HistoryEvent, HistoryOptions, Interval, Split,
};
use crate::error::ApiResult;
use crate::service;
use crate::store::IncomeEvent;

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

// ---------------------------------------------------------------- entrada --

/// `GET /income/history`: tudo opcional; o padrão é mês, data de pagamento,
/// total e líquido.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    #[serde(default)]
    pub interval: Interval,
    #[serde(default)]
    pub basis: DateBasis,
    #[serde(default)]
    pub group: Grouping,
    #[serde(default)]
    pub value: AmountKind,
}

/// `GET /income/heatmap`: data de pagamento e líquido por padrão.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeatmapQuery {
    #[serde(default)]
    pub basis: DateBasis,
    #[serde(default)]
    pub value: AmountKind,
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize)]
pub struct GroupDto {
    /// `total`, `class:br_reits`, `segment:br_stocks:Bancos`, `asset:<pagador>`
    /// ou `removed`.
    pub key: String,
    /// `total`, `class`, `segment`, `asset` ou `removed` (ativo apagado da
    /// carteira, no agrupamento por classe ou segmento).
    pub kind: &'static str,
    pub asset_class: Option<AssetClass>,
    /// O segmento; `null` na classe sem segmento.
    pub segment: Option<String>,
    /// Só no agrupamento por ativo.
    pub asset_id: Option<Uuid>,
    pub label: Option<String>,
    /// Recebido e a receber no histórico inteiro: a ordem da legenda.
    #[serde(with = "decimal_out")]
    pub total: Decimal,
}

impl From<&Group> for GroupDto {
    fn from(group: &Group) -> Self {
        let (kind, asset_class, segment) = match &group.key {
            GroupKey::Total => ("total", None, None),
            GroupKey::Class(class) => ("class", Some(*class), None),
            GroupKey::Segment(class, segment) => ("segment", Some(*class), segment.clone()),
            GroupKey::Asset(_) => ("asset", None, None),
            GroupKey::Removed => ("removed", None, None),
        };
        Self {
            key: group.key.wire(),
            kind,
            asset_class,
            segment,
            asset_id: group.asset_id,
            label: group.label.clone(),
            total: group.total,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PartDto {
    pub key: String,
    #[serde(with = "decimal_out")]
    pub received: Decimal,
    #[serde(with = "decimal_out")]
    pub upcoming: Decimal,
}

impl PartDto {
    fn new(key: String, split: Split) -> Self {
        Self {
            key,
            received: split.received,
            upcoming: split.upcoming,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BucketDto {
    /// O primeiro mês do intervalo, `2026-10` (o 4º trimestre de 2026).
    pub start: String,
    #[serde(with = "decimal_out")]
    pub received: Decimal,
    #[serde(with = "decimal_out")]
    pub upcoming: Decimal,
    /// Os grupos com valor, na ordem de `groups`.
    pub groups: Vec<PartDto>,
}

impl From<&Bucket> for BucketDto {
    fn from(bucket: &Bucket) -> Self {
        Self {
            start: bucket.start.label(),
            received: bucket.total.received,
            upcoming: bucket.total.upcoming,
            groups: bucket
                .groups
                .iter()
                .map(|(key, split)| PartDto::new(key.wire(), *split))
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryDto {
    #[serde(with = "iso_date")]
    pub today: Date,
    pub interval: Interval,
    pub basis: DateBasis,
    pub group: Grouping,
    pub value: AmountKind,
    pub groups: Vec<GroupDto>,
    pub buckets: Vec<BucketDto>,
}

impl HistoryDto {
    fn new(today: Date, options: HistoryOptions, history: &History) -> Self {
        Self {
            today,
            interval: options.interval,
            basis: options.basis,
            group: options.grouping,
            value: options.amount,
            groups: history.groups.iter().map(GroupDto::from).collect(),
            buckets: history.buckets.iter().map(BucketDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HeatCellDto {
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    #[serde(with = "decimal_out")]
    pub upcoming: Decimal,
    /// O patrimônio médio do mês (no ano, a média dos meses com foto).
    #[serde(with = "decimal_out::option")]
    pub market_value: Option<Decimal>,
    /// Em %, de 0 a 100.
    #[serde(with = "decimal_out::option")]
    pub dy: Option<Decimal>,
    pub dy_partial: bool,
    /// O custo médio do mês (no ano, a média dos meses com custo).
    #[serde(with = "decimal_out::option")]
    pub cost: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub yield_on_cost: Option<Decimal>,
    pub yield_on_cost_partial: bool,
}

impl From<&HeatCell> for HeatCellDto {
    fn from(cell: &HeatCell) -> Self {
        Self {
            amount: cell.amount,
            upcoming: cell.upcoming,
            market_value: cell.market_value,
            dy: cell.dy,
            dy_partial: cell.dy_partial,
            cost: cell.cost,
            yield_on_cost: cell.yield_on_cost,
            yield_on_cost_partial: cell.yield_on_cost_partial,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HeatYearDto {
    pub year: i32,
    /// Janeiro a dezembro.
    pub months: Vec<HeatCellDto>,
    pub total: HeatCellDto,
}

impl From<&HeatYear> for HeatYearDto {
    fn from(year: &HeatYear) -> Self {
        Self {
            year: year.year,
            months: year.months.iter().map(HeatCellDto::from).collect(),
            total: HeatCellDto::from(&year.total),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HeatmapDto {
    #[serde(with = "iso_date")]
    pub today: Date,
    pub basis: DateBasis,
    pub value: AmountKind,
    /// Do ano mais novo ao mais velho.
    pub years: Vec<HeatYearDto>,
}

// ---------------------------------------------------------------- helpers --

/// A classe e o segmento de cada ativo da carteira de hoje.
type Classification = HashMap<Uuid, (AssetClass, Option<String>)>;

/// Os proventos do livro como o histórico os lê. O ativo apagado (ou fora de
/// `classes`) fica sem classe.
fn history_events(ledger: &[IncomeEvent], classes: &Classification) -> Vec<HistoryEvent> {
    ledger
        .iter()
        .map(|event| {
            let found = event.asset_id.and_then(|id| classes.get(&id));
            HistoryEvent {
                pay_date: event.pay_date,
                ex_date: event.ex_date,
                gross: event.gross,
                net: event.net,
                asset_id: event.asset_id,
                payer_key: event.payer_key(),
                payer_label: event.payer_label(),
                class: found.map(|(class, _)| *class),
                segment: found.and_then(|(_, segment)| segment.clone()),
            }
        })
        .collect()
}

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

// --------------------------------------------------------------- handlers --

pub async fn show_history(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<HistoryQuery>,
) -> ApiResult<Json<HistoryDto>> {
    let today = today();
    let ledger = state.store.income_ledger(user.id).await?;
    let view = service::portfolio(&state.store, user.id).await?;
    let classes: Classification = view
        .assets
        .iter()
        .map(|a| {
            let class = a.asset.asset_class;
            (a.asset.id, (class, segment_of(class, a.sector.as_ref())))
        })
        .collect();
    let options = HistoryOptions {
        interval: query.interval,
        basis: query.basis,
        grouping: query.group,
        amount: query.value,
    };
    let result = history(&history_events(&ledger, &classes), options, today);
    Ok(Json(HistoryDto::new(today, options, &result)))
}

pub async fn show_heatmap(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<HeatmapQuery>,
) -> ApiResult<Json<HeatmapDto>> {
    let today = today();
    let ledger = state.store.income_ledger(user.id).await?;
    let events = history_events(&ledger, &Classification::new());
    let years = match range(&events, query.basis, today) {
        None => Vec::new(),
        Some((from, to)) => {
            let market = state.store.monthly_market_values(user.id).await?;
            let books = state.store.entries_by_asset(user.id).await?;
            let costs = monthly_costs(&books, from, to, today);
            for asset in &costs.broken {
                tracing::error!(asset = %asset, "livro nao fecha no mapa de calor da renda");
            }
            heatmap(&events, query.basis, query.value, today, &market, &costs)
        }
    };
    Ok(Json(HeatmapDto {
        today,
        basis: query.basis,
        value: query.value,
        years: years.iter().map(HeatYearDto::from).collect(),
    }))
}
