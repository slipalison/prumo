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
use super::references::{CdiDto, RealRateDto};
use super::{AppState, CurrentUser};
use crate::domain::fund_market::FundMarket;
use crate::domain::fundamentals::{
    AssetFundamentals, Column, CompanyMetrics, Figure, FilingKind, FilingRef, FiscalYear,
    FundMetrics, Gap, PeriodFigures, Scope, Template, Unsupported, YearCount,
};
use crate::domain::income::today_in_brazil;
use crate::error::ApiResult;
use crate::fund_indicators::{self, FundIndicators};
use crate::service;

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

#[derive(Debug, Serialize, PartialEq)]
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
    /// O ativo total e o caixa sobre ele (fração), do informe mensal.
    pub total_assets: FigureDto,
    pub cash_share: FigureDto,
    /// O histórico de mercado (liquidez, CAGR, rendimento de 12 meses).
    pub market: Option<FundMarketDto>,
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
            total_assets: m.total_assets.into(),
            cash_share: m.cash_share.into(),
            market: None,
            synced_at: m.synced_at,
        }
    }
}

/// O que o histórico de mercado de um FII dá (`domain::fund_market`): as
/// razões em fração (0,1 = 10%), a liquidez em reais por dia.
#[derive(Debug, Serialize, PartialEq)]
pub struct FundMarketDto {
    /// Volume financeiro médio dos últimos 30 e 90 pregões.
    pub liquidity_30d: FigureDto,
    pub liquidity_90d: FigureDto,
    /// CAGR dos proventos por cota e da cota, em fração ao ano.
    pub dividend_cagr_3y: FigureDto,
    pub dividend_cagr_5y: FigureDto,
    pub price_cagr_3y: FigureDto,
    pub price_cagr_5y: FigureDto,
    /// Proventos por cota dos últimos 365 dias sobre a cotação.
    pub dividend_yield_12m: FigureDto,
    #[serde(with = "decimal_out::option")]
    pub dividends_12m: Option<Decimal>,
}

impl From<&FundMarket> for FundMarketDto {
    fn from(m: &FundMarket) -> Self {
        Self {
            liquidity_30d: m.liquidity_short.into(),
            liquidity_90d: m.liquidity_long.into(),
            dividend_cagr_3y: m.dividend_cagr_short.into(),
            dividend_cagr_5y: m.dividend_cagr_long.into(),
            price_cagr_3y: m.price_cagr_short.into(),
            price_cagr_5y: m.price_cagr_long.into(),
            dividend_yield_12m: m.dividend_yield_12m.into(),
            dividends_12m: m.dividends_12m,
        }
    }
}

/// O caixa do informe da CVM e o histórico de mercado, juntos (a tabela de
/// FIIs da Análise).
#[derive(Debug, Serialize, PartialEq)]
pub struct FundIndicatorsDto {
    pub cash_share: FigureDto,
    #[serde(flatten)]
    pub market: FundMarketDto,
}

impl From<&FundIndicators> for FundIndicatorsDto {
    fn from(i: &FundIndicators) -> Self {
        Self {
            cash_share: i.cash_share.into(),
            market: FundMarketDto::from(&i.market),
        }
    }
}

/// As taxas de mercado ao lado do rendimento de 12 meses do FII (só
/// contexto): o CDI de 12 meses (bruto e depois do IR de 15%) e o juro real da
/// NTN-B de referência.
#[derive(Debug, Serialize)]
pub struct RatesDto {
    pub cdi_12m: Option<CdiDto>,
    pub real_rate: Option<RealRateDto>,
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
    /// As taxas de mercado ao lado do rendimento dos FIIs.
    pub rates: RatesDto,
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<FundamentalsDto>> {
    let by_symbol = state.store.fundamentals_for_user(user.id).await?;
    let view = service::portfolio(&state.store, user.id).await?;
    let today = today_in_brazil(OffsetDateTime::now_utc());
    // O histórico de mercado só dos FIIs com informe (os outros não mostram).
    let funds: Vec<(String, Option<Decimal>)> = view
        .assets
        .iter()
        .filter_map(|a| {
            let symbol = a.asset.symbol.clone()?;
            matches!(by_symbol.get(&symbol), Some(AssetFundamentals::Fund(_)))
                .then_some((symbol, a.unit_price))
        })
        .collect();
    let market = fund_indicators::market_of(&state.store, &funds, today).await?;
    let rates = RatesDto {
        cdi_12m: state.store.cdi_12m(today).await?.map(CdiDto::from),
        real_rate: state.store.real_rate().await?.map(RealRateDto::from),
    };
    Ok(Json(FundamentalsDto {
        assets: view
            .assets
            .iter()
            .filter_map(|a| {
                let symbol = a.asset.symbol.as_deref()?;
                let fundamentals = by_symbol.get(symbol)?;
                let mut dto = AssetFundamentalsDto::new(a.asset.id, symbol, fundamentals);
                if let Some(fund) = dto.fund.as_mut() {
                    fund.market = market.get(symbol).map(FundMarketDto::from);
                }
                Some(dto)
            })
            .collect(),
        rates,
    }))
}
