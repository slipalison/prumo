//! `GET /api/v1/fundamentals`: os indicadores fundamentalistas dos ativos da
//! carteira (companhias pela DFP/ITR da CVM, FIIs pelos informes da CVM), para
//! a folha do ativo, a tela de Análise e a conferência. A regra está em
//! `domain::fundamentals`; a busca, em `crate::fundamentals`; o contrato, em
//! `docs/api.md`.
//!
//! Toda medida é `{"value": número | null, "gap": motivo | null}`: nunca um
//! zero no lugar de um número que falta.

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::decimal_out;
use super::{AppState, CurrentUser};
use crate::domain::fundamentals::{
    AssetFundamentals, Column, CompanyMetrics, Figure, FilingKind, FilingRef, FiscalYear,
    FundMetrics, Gap, PeriodFigures, Scope, Template, Unsupported, YearCount,
};
use crate::error::ApiResult;

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

#[derive(Debug, Serialize)]
pub struct FigureDto {
    #[serde(with = "decimal_out::option")]
    pub value: Option<Decimal>,
    pub gap: Option<Gap>,
}

impl From<Figure> for FigureDto {
    fn from(figure: Figure) -> Self {
        Self {
            value: figure.value(),
            gap: figure.gap(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FilingRefDto {
    pub kind: FilingKind,
    #[serde(with = "iso_date")]
    pub reference_date: Date,
    pub version: i32,
    pub scope: Scope,
    pub column: Column,
}

impl From<&FilingRef> for FilingRefDto {
    fn from(r: &FilingRef) -> Self {
        Self {
            kind: r.kind,
            reference_date: r.reference_date,
            version: r.version,
            scope: r.scope,
            column: r.column,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PeriodDto {
    #[serde(with = "iso_date")]
    pub period_start: Date,
    #[serde(with = "iso_date")]
    pub period_end: Date,
    pub template: Template,
    pub sources: Vec<FilingRefDto>,
    pub revenue: FigureDto,
    pub net_income: FigureDto,
    pub equity: FigureDto,
    pub equity_controlling: FigureDto,
    pub gross_debt: FigureDto,
    pub cash: FigureDto,
    pub net_debt: FigureDto,
    pub ebit: FigureDto,
    pub depreciation_amortization: FigureDto,
    pub ebitda: FigureDto,
    pub operating_cash_flow: FigureDto,
    pub roe: FigureDto,
    pub net_margin: FigureDto,
    pub ebitda_margin: FigureDto,
    pub net_debt_to_ebitda: FigureDto,
    pub net_debt_to_equity: FigureDto,
}

impl From<&PeriodFigures> for PeriodDto {
    fn from(p: &PeriodFigures) -> Self {
        Self {
            period_start: p.period_start,
            period_end: p.period_end,
            template: p.template,
            sources: p.sources.iter().map(FilingRefDto::from).collect(),
            revenue: p.revenue.into(),
            net_income: p.net_income.into(),
            equity: p.equity.into(),
            equity_controlling: p.equity_controlling.into(),
            gross_debt: p.gross_debt.into(),
            cash: p.cash.into(),
            net_debt: p.net_debt.into(),
            ebit: p.ebit.into(),
            depreciation_amortization: p.depreciation_amortization.into(),
            ebitda: p.ebitda.into(),
            operating_cash_flow: p.operating_cash_flow.into(),
            roe: p.roe.into(),
            net_margin: p.net_margin.into(),
            ebitda_margin: p.ebitda_margin.into(),
            net_debt_to_ebitda: p.net_debt_to_ebitda.into(),
            net_debt_to_equity: p.net_debt_to_equity.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FiscalYearDto {
    pub year: i32,
    /// `null`: sem balanço do exercício nos arquivos da CVM.
    pub figures: Option<PeriodDto>,
}

impl From<&FiscalYear> for FiscalYearDto {
    fn from(y: &FiscalYear) -> Self {
        Self {
            year: y.year,
            figures: y.figures.as_ref().map(PeriodDto::from),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct YearCountDto {
    pub positive: u8,
    pub known: u8,
    pub window: u8,
    pub gap: Option<Gap>,
}

impl From<YearCount> for YearCountDto {
    fn from(c: YearCount) -> Self {
        Self {
            positive: c.positive,
            known: c.known,
            window: c.window,
            gap: c.gap,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CompanyDto {
    pub cvm_code: i32,
    pub name: String,
    pub template: Template,
    /// Holding de participações: margens, dívida sobre EBITDA e CAGR da receita
    /// vêm com `gap: "holding"`.
    pub holding: bool,
    pub judicial_recovery: Option<bool>,
    pub issuer_status: Option<String>,
    pub registration_status: Option<String>,
    pub latest_filing: Option<FilingRefDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub synced_at: Option<OffsetDateTime>,
    pub trailing: Option<PeriodDto>,
    pub years: Vec<FiscalYearDto>,
    pub profitable_years: YearCountDto,
    pub positive_operating_cash_years: YearCountDto,
    pub revenue_cagr: FigureDto,
    pub earnings_cagr: FigureDto,
    /// Total de ações (ON + PN) da B3; `null` se a B3 não informou.
    #[serde(with = "decimal_out::option")]
    pub shares: Option<Decimal>,
    /// LPA e VPA dos últimos 12 meses (`trailing`), em reais por ação.
    pub earnings_per_share: FigureDto,
    pub book_value_per_share: FigureDto,
}

impl From<&CompanyMetrics> for CompanyDto {
    fn from(m: &CompanyMetrics) -> Self {
        Self {
            cvm_code: m.cvm_code,
            name: m.name.clone(),
            template: m.template,
            holding: m.holding,
            judicial_recovery: m.judicial_recovery,
            issuer_status: m.issuer_status.clone(),
            registration_status: m.registration_status.clone(),
            latest_filing: m.latest_filing.as_ref().map(FilingRefDto::from),
            synced_at: m.synced_at,
            trailing: m.trailing.as_ref().map(PeriodDto::from),
            years: m.years.iter().map(FiscalYearDto::from).collect(),
            profitable_years: m.profitable_years.into(),
            positive_operating_cash_years: m.positive_operating_cash_years.into(),
            revenue_cagr: m.revenue_cagr.into(),
            earnings_cagr: m.earnings_cagr.into(),
            shares: m.shares,
            earnings_per_share: m.earnings_per_share.into(),
            book_value_per_share: m.book_value_per_share.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FundDto {
    pub cnpj: String,
    pub name: String,
    #[serde(with = "iso_date")]
    pub reference_date: Date,
    pub net_assets: FigureDto,
    pub shares: FigureDto,
    pub nav_per_share: FigureDto,
    /// A cotação usada no P/VP (a tela recalcula com o preço ao vivo).
    #[serde(with = "decimal_out::option")]
    pub price: Option<Decimal>,
    pub price_to_book: FigureDto,
    pub vacancy: FigureDto,
    #[serde(with = "iso_date_option")]
    pub vacancy_reference_date: Option<Date>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub synced_at: Option<OffsetDateTime>,
}

impl From<&FundMetrics> for FundDto {
    fn from(m: &FundMetrics) -> Self {
        Self {
            cnpj: m.cnpj.clone(),
            name: m.name.clone(),
            reference_date: m.reference_date,
            net_assets: m.net_assets.into(),
            shares: m.shares.into(),
            nav_per_share: m.nav_per_share.into(),
            price: m.price,
            price_to_book: m.price_to_book.into(),
            vacancy: m.vacancy.into(),
            vacancy_reference_date: m.vacancy_reference_date,
            synced_at: m.synced_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FundamentalsKind {
    Company,
    Fund,
    Unsupported,
}

#[derive(Debug, Serialize)]
pub struct AssetFundamentalsDto {
    pub asset_id: Uuid,
    pub symbol: String,
    pub kind: FundamentalsKind,
    pub company: Option<CompanyDto>,
    pub fund: Option<FundDto>,
    /// Com `kind = "unsupported"`: por quê.
    pub reason: Option<Unsupported>,
}

impl AssetFundamentalsDto {
    fn new(asset_id: Uuid, symbol: &str, fundamentals: &AssetFundamentals) -> Self {
        let (kind, company, fund, reason) = match fundamentals {
            AssetFundamentals::Company(m) => (
                FundamentalsKind::Company,
                Some(CompanyDto::from(m.as_ref())),
                None,
                None,
            ),
            AssetFundamentals::Fund(m) => (
                FundamentalsKind::Fund,
                None,
                Some(FundDto::from(m.as_ref())),
                None,
            ),
            AssetFundamentals::Unsupported(reason) => {
                (FundamentalsKind::Unsupported, None, None, Some(*reason))
            }
        };
        Self {
            asset_id,
            symbol: symbol.to_owned(),
            kind,
            company,
            fund,
            reason,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FundamentalsDto {
    /// Na ordem da carteira; ativo sem símbolo (renda fixa por valor) fica fora.
    pub assets: Vec<AssetFundamentalsDto>,
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<FundamentalsDto>> {
    let by_symbol = state.store.fundamentals_for_user(user.id).await?;
    let assets = state.store.assets(user.id).await?;
    Ok(Json(FundamentalsDto {
        assets: assets
            .iter()
            .filter_map(|asset| {
                let symbol = asset.symbol.as_deref()?;
                let fundamentals = by_symbol.get(symbol)?;
                Some(AssetFundamentalsDto::new(asset.id, symbol, fundamentals))
            })
            .collect(),
    }))
}
