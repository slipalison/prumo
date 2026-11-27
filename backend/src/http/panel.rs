//! Painel da Carteira: `GET /api/v1/portfolio/month` (a faixa de indicadores
//! do mês) e `GET /api/v1/portfolio/day-reference` (o fechamento anterior de
//! cada ativo e o que os negócios mudaram depois dele, para a tela medir o dia
//! contra a cotação ao vivo). A conta está em `domain::panel`; o método, em
//! `docs/painel.md`.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::Serialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::{AppState, CurrentUser};
use crate::domain::panel::{self, DayPoint, MonthEnd, MonthReturn, MonthSummary, MonthTotals};
use crate::domain::performance::to_percent;
use crate::error::ApiResult;
use crate::performance::{local_date, price_symbol, series};
use crate::service::{self, PriceSource};
use crate::store::{Transaction, TransactionFilter};

/// Casas do percentual (0,0123%).
const PERCENT_DP: u32 = 4;

fn day(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MonthEndDto {
    date: String,
    #[serde(with = "decimal_out")]
    value: Decimal,
}

impl From<MonthEnd> for MonthEndDto {
    fn from(end: MonthEnd) -> Self {
        Self {
            date: day(end.date),
            value: end.value,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MonthReturnDto {
    from: String,
    to: String,
    /// Rentabilidade pela cota (TWR), em %.
    #[serde(with = "decimal_out")]
    return_percent: Decimal,
    /// `false`: a série começou no meio do mês, e a janela é mais curta.
    full_window: bool,
}

impl From<MonthReturn> for MonthReturnDto {
    fn from(r: MonthReturn) -> Self {
        Self {
            from: day(r.from),
            to: day(r.to),
            return_percent: to_percent(r.ret, PERCENT_DP),
            full_window: r.full,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MonthTotalsDto {
    /// Proventos líquidos recebidos no mês.
    #[serde(with = "decimal_out")]
    income: Decimal,
    /// Aportes menos retiradas do mês.
    #[serde(with = "decimal_out")]
    contribution: Decimal,
}

impl From<MonthTotals> for MonthTotalsDto {
    fn from(t: MonthTotals) -> Self {
        Self {
            income: t.income,
            contribution: t.contribution,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MonthDto {
    /// O último dia da série; `null` quando a pessoa ainda não tem série.
    as_of: Option<String>,
    /// O primeiro dia do mês que os números "do mês" cobrem.
    month_starts_on: String,
    /// Patrimônio no fim do mês anterior; `null` se a série começou depois.
    previous_month_end: Option<MonthEndDto>,
    current_return: Option<MonthReturnDto>,
    previous_return: Option<MonthReturnDto>,
    current: MonthTotalsDto,
    previous: MonthTotalsDto,
}

impl MonthDto {
    fn build(summary: Option<MonthSummary>, today: Date) -> Self {
        let month_starts_on = day(today.replace_day(1).unwrap_or(today));
        match summary {
            None => Self {
                as_of: None,
                month_starts_on,
                previous_month_end: None,
                current_return: None,
                previous_return: None,
                current: MonthTotals::default().into(),
                previous: MonthTotals::default().into(),
            },
            Some(s) => Self {
                as_of: Some(day(s.as_of)),
                month_starts_on,
                previous_month_end: s.previous_month_end.map(Into::into),
                current_return: s.current_return.map(Into::into),
                previous_return: s.previous_return.map(Into::into),
                current: s.current.into(),
                previous: s.previous.into(),
            },
        }
    }
}

pub async fn month(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<MonthDto>> {
    let now = OffsetDateTime::now_utc();
    let today = local_date(now);
    let points: Vec<DayPoint> = series(&state.store, user.id, now)
        .await?
        .into_iter()
        .map(|p| DayPoint {
            date: p.date,
            market_value: p.market_value,
            net_flow: p.net_flow,
            income: p.income,
            quota_value: p.quota_value,
        })
        .collect();
    Ok(Json(MonthDto::build(
        panel::month_summary(&points, today),
        today,
    )))
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DayReferenceDto {
    asset_id: Uuid,
    /// O preço do ativo no fechamento anterior, em reais por unidade. `null`
    /// para o ativo que não estava na foto e teve compra ou venda depois dela.
    #[serde(with = "decimal_out::option")]
    previous_close: Option<Decimal>,
    /// A data desse fechamento.
    date: String,
    /// As cotas que havia nesse fechamento.
    #[serde(with = "decimal_out")]
    previous_quantity: Decimal,
    /// O que as compras depois do fechamento custaram, com taxas, em reais.
    #[serde(with = "decimal_out")]
    bought: Decimal,
    /// O líquido das vendas e resgates depois do fechamento, em reais.
    #[serde(with = "decimal_out")]
    sold: Decimal,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DayReferencesDto {
    assets: Vec<DayReferenceDto>,
}

pub async fn day_reference(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<DayReferencesDto>> {
    let today = local_date(OffsetDateTime::now_utc());
    let view = service::portfolio(&state.store, user.id).await?;
    // Cada ativo mede contra a foto anterior ao dia da cotação dele: no fim de
    // semana a cotação de sexta mede contra quinta, não contra ela mesma.
    let mut by_price_date: BTreeMap<Date, Vec<&service::AssetView>> = BTreeMap::new();
    for asset in &view.assets {
        let (Some(_), Some(at), PriceSource::Quote) = (
            price_symbol(asset),
            asset.quote_updated_at,
            asset.price_source,
        ) else {
            continue;
        };
        by_price_date.entry(local_date(at)).or_default().push(asset);
    }
    let mut snapshots = Vec::new();
    for (price_date, group) in by_price_date {
        if let Some(snapshot) = state.store.snapshot_before(user.id, price_date).await? {
            snapshots.push((price_date, snapshot, group));
        }
    }
    // Os lançamentos depois da foto mais antiga, de uma vez: a quantidade de
    // agora já conta todos eles, então nenhum fica de fora (nem o de data
    // futura).
    let entries = match snapshots.iter().map(|(_, s, _)| s.date).min() {
        Some(oldest) => {
            let filter = TransactionFilter {
                asset_id: None,
                from: Some(oldest + Duration::days(1)),
                to: None,
            };
            state.store.transactions(user.id, filter).await?
        }
        None => Vec::new(),
    };
    let mut assets = Vec::new();
    for (price_date, snapshot, group) in snapshots {
        for asset in group {
            let id = asset.asset.id;
            let position = snapshot.positions.iter().find(|p| p.asset_id == id);
            let own: Vec<_> = entries
                .iter()
                .filter(|t| t.asset_id == Some(id))
                .map(Transaction::entry)
                .collect();
            let trades = panel::day_trades(&own, snapshot.date);
            if let Some(reference) = panel::day_reference(
                position,
                snapshot.date,
                price_date,
                today,
                asset.asset.quantity,
                trades,
            ) {
                assets.push(DayReferenceDto {
                    asset_id: id,
                    previous_close: reference.previous_close,
                    date: day(reference.date),
                    previous_quantity: reference.previous_quantity,
                    bought: reference.bought,
                    sold: reference.sold,
                });
            }
        }
    }
    Ok(Json(DayReferencesDto { assets }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    #[test]
    fn an_empty_series_serializes_with_nulls_and_zeros() {
        let dto = MonthDto::build(None, date!(2026 - 11 - 25));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["as_of"], serde_json::Value::Null);
        assert_eq!(json["month_starts_on"], "2026-11-01");
        assert_eq!(json["current"]["income"], 0.0);
        assert!(json["previous_month_end"].is_null());
    }

    #[test]
    fn the_return_goes_out_in_percent() {
        let dto = MonthReturnDto::from(MonthReturn {
            from: date!(2026 - 10 - 31),
            to: date!(2026 - 11 - 25),
            ret: dec!(0.0123456),
            full: true,
        });
        assert_eq!(dto.return_percent, dec!(1.2346));
        assert_eq!(dto.from, "2026-10-31");
    }
}
