//! `GET /api/v1/results`: os resultados trimestrais das empresas da carteira
//! (receita, lucro e margem do trimestre mais novo, contra o mesmo trimestre do
//! ano anterior e contra o anterior, com a série de oito trimestres) e a série
//! mensal dos FIIs. A regra está em `domain::fundamentals::quarters`; a leitura,
//! em `store::results`; o contrato, em `docs/api.md` e `docs/resultados.md`.
//!
//! Toda medida é `{"value": número | null, "gap": motivo | null}`, como nos
//! fundamentos; a variação é `{"kind", "value", "gap"}` e só tem percentual
//! com a base positiva.

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::fundamentals::{FigureDto, FilingRefDto};
use super::{AppState, CurrentUser};
use crate::domain::fundamentals::{
    Change, Comparison, FundMonthFigures, Gap, Quarter, QuarterMethod, Template, TopLine,
    Unsupported, RATIO_DP,
};
use crate::error::ApiResult;
use crate::service;
use crate::store::results::{AssetResults, CompanyResults, FundResults};

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

mod iso_date_option {
    use serde::Serializer;
    use time::Date;

    pub fn serialize<S: Serializer>(
        value: &Option<Date>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(date) => super::iso_date::serialize(date, serializer),
            None => serializer.serialize_none(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Percent,
    LeftLoss,
    EnteredLoss,
    SmallerLoss,
    LargerLoss,
    Unchanged,
    NoBase,
    Missing,
}

/// Uma variação: `kind` sempre; `value` (fração) só em `percent`; `gap` só em
/// `missing`.
#[derive(Debug, Serialize)]
pub struct ChangeDto {
    pub kind: ChangeKind,
    #[serde(with = "decimal_out::option")]
    pub value: Option<Decimal>,
    pub gap: Option<Gap>,
}

impl From<Change> for ChangeDto {
    fn from(change: Change) -> Self {
        let (kind, value, gap) = match change {
            Change::Percent { value } => (ChangeKind::Percent, Some(value), None),
            Change::LeftLoss => (ChangeKind::LeftLoss, None, None),
            Change::EnteredLoss => (ChangeKind::EnteredLoss, None, None),
            Change::SmallerLoss => (ChangeKind::SmallerLoss, None, None),
            Change::LargerLoss => (ChangeKind::LargerLoss, None, None),
            Change::Unchanged => (ChangeKind::Unchanged, None, None),
            Change::NoBase => (ChangeKind::NoBase, None, None),
            Change::Missing { gap } => (ChangeKind::Missing, None, Some(gap)),
        };
        Self { kind, value, gap }
    }
}

#[derive(Debug, Serialize)]
pub struct QuarterDto {
    pub fiscal_quarter: u8,
    #[serde(with = "iso_date")]
    pub period_start: Date,
    #[serde(with = "iso_date")]
    pub period_end: Date,
    pub method: QuarterMethod,
    pub top_line: FigureDto,
    pub net_income: FigureDto,
    pub net_margin: FigureDto,
    pub source: FilingRefDto,
    /// Quando a CVM recebeu o documento que fecha o trimestre.
    #[serde(with = "iso_date_option")]
    pub received_on: Option<Date>,
}

impl From<&Quarter> for QuarterDto {
    fn from(q: &Quarter) -> Self {
        Self {
            fiscal_quarter: q.fiscal_quarter,
            period_start: q.period_start,
            period_end: q.period_end,
            method: q.method,
            top_line: q.top_line.into(),
            net_income: q.net_income.into(),
            net_margin: q.net_margin.into(),
            source: FilingRefDto::from(&q.source),
            received_on: q.received_on,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ComparisonDto {
    /// O fim do trimestre comparado.
    #[serde(with = "iso_date")]
    pub period_end: Date,
    pub top_line: ChangeDto,
    pub net_income: ChangeDto,
    /// Margem de agora − a de antes, em fração (0,012 = 1,2 ponto).
    pub net_margin_points: FigureDto,
}

impl From<&Comparison> for ComparisonDto {
    fn from(c: &Comparison) -> Self {
        Self {
            period_end: c.period_end,
            top_line: c.top_line.into(),
            net_income: c.net_income.into(),
            net_margin_points: c.net_margin_points.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CompanyResultsDto {
    pub name: String,
    pub template: Template,
    pub holding: bool,
    pub top_line: TopLine,
    #[serde(with = "time::serde::rfc3339::option")]
    pub synced_at: Option<OffsetDateTime>,
    /// Até oito trimestres, do mais velho ao mais novo.
    pub quarters: Vec<QuarterDto>,
    pub year_ago: Option<ComparisonDto>,
    pub previous_quarter: Option<ComparisonDto>,
}

impl From<&CompanyResults> for CompanyResultsDto {
    fn from(c: &CompanyResults) -> Self {
        Self {
            name: c.name.clone(),
            template: c.template,
            holding: c.holding,
            top_line: c.results.top_line,
            synced_at: c.synced_at,
            quarters: c.results.quarters.iter().map(QuarterDto::from).collect(),
            year_ago: c.results.year_ago.as_ref().map(ComparisonDto::from),
            previous_quarter: c.results.previous_quarter.as_ref().map(ComparisonDto::from),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FundMonthDto {
    #[serde(with = "iso_date")]
    pub reference_date: Date,
    pub nav_per_share: FigureDto,
    pub dividend_yield: FigureDto,
}

impl From<&FundMonthFigures> for FundMonthDto {
    fn from(m: &FundMonthFigures) -> Self {
        Self {
            reference_date: m.reference_date,
            nav_per_share: m.nav_per_share.into(),
            dividend_yield: m.dividend_yield.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FundResultsDto {
    pub name: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub synced_at: Option<OffsetDateTime>,
    /// Até doze meses, do mais velho ao mais novo.
    pub months: Vec<FundMonthDto>,
}

impl From<&FundResults> for FundResultsDto {
    fn from(f: &FundResults) -> Self {
        Self {
            name: f.name.clone(),
            synced_at: f.synced_at,
            months: f.months.iter().map(FundMonthDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultsKind {
    Company,
    Fund,
    Unsupported,
}

#[derive(Debug, Serialize)]
pub struct AssetResultsDto {
    pub asset_id: Uuid,
    pub symbol: String,
    /// O peso do ativo na carteira (fração do valor total); `null` com a
    /// carteira zerada.
    #[serde(with = "decimal_out::option")]
    pub weight: Option<Decimal>,
    pub kind: ResultsKind,
    pub company: Option<CompanyResultsDto>,
    pub fund: Option<FundResultsDto>,
    pub reason: Option<Unsupported>,
}

impl AssetResultsDto {
    fn new(asset_id: Uuid, symbol: &str, weight: Option<Decimal>, results: &AssetResults) -> Self {
        let (kind, company, fund, reason) = match results {
            AssetResults::Company(c) => (
                ResultsKind::Company,
                Some(CompanyResultsDto::from(c.as_ref())),
                None,
                None,
            ),
            AssetResults::Fund(f) => (
                ResultsKind::Fund,
                None,
                Some(FundResultsDto::from(f.as_ref())),
                None,
            ),
            AssetResults::Unsupported(reason) => {
                (ResultsKind::Unsupported, None, None, Some(*reason))
            }
        };
        Self {
            asset_id,
            symbol: symbol.to_owned(),
            weight,
            kind,
            company,
            fund,
            reason,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ResultsDto {
    /// Ações Nacionais e FIIs da carteira, na ordem dela.
    pub assets: Vec<AssetResultsDto>,
}

/// O peso de `value` em `total`, em fração com seis casas; `None` sem total.
fn weight(value: Decimal, total: Decimal) -> Option<Decimal> {
    (total > Decimal::ZERO)
        .then(|| value.checked_div(total))
        .flatten()
        .map(|w| w.round_dp(RATIO_DP).normalize())
}

pub async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<ResultsDto>> {
    let by_symbol = state.store.results_for_user(user.id).await?;
    let portfolio = service::portfolio(&state.store, user.id).await?;
    let total = portfolio.total_value;
    Ok(Json(ResultsDto {
        assets: portfolio
            .assets
            .iter()
            .filter_map(|view| {
                let symbol = view.asset.symbol.as_deref()?;
                let results = by_symbol.get(symbol)?;
                Some(AssetResultsDto::new(
                    view.asset.id,
                    symbol,
                    weight(view.current_value, total),
                    results,
                ))
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn a_change_says_its_kind_and_only_a_percent_has_a_value() {
        let json = |c: Change| serde_json::to_value(ChangeDto::from(c)).unwrap();
        assert_eq!(
            json(Change::Percent { value: dec!(0.25) }),
            serde_json::json!({ "kind": "percent", "value": 0.25, "gap": null })
        );
        assert_eq!(
            json(Change::LeftLoss),
            serde_json::json!({ "kind": "left_loss", "value": null, "gap": null })
        );
        assert_eq!(
            json(Change::Missing {
                gap: Gap::FinancialTemplate
            }),
            serde_json::json!({ "kind": "missing", "value": null, "gap": "financial_template" })
        );
        for (change, kind) in [
            (Change::EnteredLoss, "entered_loss"),
            (Change::SmallerLoss, "smaller_loss"),
            (Change::LargerLoss, "larger_loss"),
            (Change::Unchanged, "unchanged"),
            (Change::NoBase, "no_base"),
        ] {
            assert_eq!(json(change)["kind"], kind);
        }
    }

    #[test]
    fn the_weight_is_a_fraction_of_the_whole_portfolio() {
        assert_eq!(weight(dec!(25), dec!(200)), Some(dec!(0.125)));
        assert_eq!(weight(dec!(1), dec!(3)), Some(dec!(0.333333)));
        assert_eq!(weight(dec!(1), dec!(0)), None);
    }
}
