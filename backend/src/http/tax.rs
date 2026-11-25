//! IR: a apuração mensal (isentômetro, base, imposto, DARF e vencimento), a
//! declaração do ano (Bens e Direitos e rendimentos), a marca de DARF pago e o
//! saldo de prejuízo que a pessoa traz de antes do Prumo. Tudo pelo dono.
//!
//! A regra mora em `domain::tax` (por ano-calendário, com a fonte de cada
//! número); a ligação com o livro, em `domain::tax_book`. Ano sem regra
//! cadastrada é erro (`tax_year_not_covered`), nunca a regra de outro ano.

use std::collections::{BTreeMap, HashMap};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use super::dto::{decimal, decimal_out};
use super::{ApiJson, ApiPath, ApiQuery, AppState, CurrentUser};
use crate::domain::income::{today_in_brazil, IncomeSource};
use crate::domain::tax::irpf::{annual_report, AnnualReport, AssetLine, Income, IncomeLine};
use crate::domain::tax::monthly::{
    assess, Assessment, CapitalGainAssessment, Carryforward, DarfAssessment, ExemptionStatus,
    InstrumentSales, MonthReport, Notice, PoolAssessment, TaxInput, WithholdingAssessment,
};
use crate::domain::tax::rules::{covered_years, ExemptionId, Pool, Regime};
use crate::domain::tax::{Instrument, TaxError, YearMonth};
use crate::domain::tax_book::{
    self, annual_doubts, month_doubts, Book, Doubt, Payment, SkipReason, Skipped,
};
use crate::error::{ApiError, ApiResult};
use crate::store::tax::{DarfPayment, StoredCarryforward};

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// Teto de cada valor informado (um erro de digitação não vira trilhão).
const MAX_AMOUNT: i64 = 1_000_000_000_000;

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YearQuery {
    #[serde(default)]
    pub year: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeQuery {
    pub code: String,
}

/// O saldo em 1º de janeiro de `year`. Valor ausente é zero.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryforwardRequest {
    pub year: i32,
    #[serde(default = "zero", deserialize_with = "decimal")]
    pub common_loss: Decimal,
    #[serde(default = "zero", deserialize_with = "decimal")]
    pub day_trade_loss: Decimal,
    #[serde(default = "zero", deserialize_with = "decimal")]
    pub real_estate_fund_loss: Decimal,
    #[serde(default = "zero", deserialize_with = "decimal")]
    pub agribusiness_fund_loss: Decimal,
    #[serde(default = "zero", deserialize_with = "decimal")]
    pub pending_exchange_darf: Decimal,
    #[serde(default = "zero", deserialize_with = "decimal")]
    pub pending_capital_gain_darf: Decimal,
}

fn zero() -> Decimal {
    Decimal::ZERO
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize)]
pub struct SkippedDto {
    pub subject: String,
    #[serde(with = "iso_date::option")]
    pub date: Option<Date>,
    pub reason: SkipReason,
    #[serde(with = "decimal_out::option")]
    pub amount: Option<Decimal>,
}

impl From<&Skipped> for SkippedDto {
    fn from(s: &Skipped) -> Self {
        Self {
            subject: s.subject.clone(),
            date: s.date,
            reason: s.reason,
            amount: s.amount,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SalesDto {
    pub instrument: Instrument,
    #[serde(with = "decimal_out")]
    pub common_sales: Decimal,
    #[serde(with = "decimal_out")]
    pub day_trade_sales: Decimal,
    #[serde(with = "decimal_out")]
    pub common_result: Decimal,
    #[serde(with = "decimal_out")]
    pub day_trade_result: Decimal,
}

impl From<&InstrumentSales> for SalesDto {
    fn from(s: &InstrumentSales) -> Self {
        Self {
            instrument: s.instrument,
            common_sales: s.common_sales,
            day_trade_sales: s.day_trade_sales,
            common_result: s.common_result,
            day_trade_result: s.day_trade_result,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExemptionDto {
    pub id: ExemptionId,
    #[serde(with = "decimal_out")]
    pub sales: Decimal,
    #[serde(with = "decimal_out")]
    pub limit: Decimal,
    #[serde(with = "decimal_out")]
    pub remaining: Decimal,
    pub exempt: bool,
    #[serde(with = "decimal_out")]
    pub exempt_gain: Decimal,
}

impl From<&ExemptionStatus> for ExemptionDto {
    fn from(e: &ExemptionStatus) -> Self {
        Self {
            id: e.id,
            sales: e.sales,
            limit: e.limit,
            remaining: e.remaining,
            exempt: e.exempt,
            exempt_gain: e.exempt_gain,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PoolDto {
    pub pool: Pool,
    #[serde(with = "decimal_out")]
    pub result: Decimal,
    #[serde(with = "decimal_out")]
    pub loss_before: Decimal,
    #[serde(with = "decimal_out")]
    pub loss_used: Decimal,
    #[serde(with = "decimal_out")]
    pub base: Decimal,
    #[serde(with = "decimal_out")]
    pub rate: Decimal,
    #[serde(with = "decimal_out")]
    pub tax: Decimal,
    #[serde(with = "decimal_out")]
    pub loss_after: Decimal,
}

impl From<&PoolAssessment> for PoolDto {
    fn from(p: &PoolAssessment) -> Self {
        Self {
            pool: p.pool,
            result: p.result,
            loss_before: p.loss_before,
            loss_used: p.loss_used,
            base: p.base,
            rate: p.rate,
            tax: p.tax,
            loss_after: p.loss_after,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CapitalGainDto {
    #[serde(with = "decimal_out")]
    pub gains: Decimal,
    #[serde(with = "decimal_out")]
    pub losses: Decimal,
    pub exempt: bool,
    #[serde(with = "decimal_out")]
    pub base: Decimal,
    #[serde(with = "decimal_out")]
    pub tax: Decimal,
}

impl From<&CapitalGainAssessment> for CapitalGainDto {
    fn from(c: &CapitalGainAssessment) -> Self {
        Self {
            gains: c.gains,
            losses: c.losses,
            exempt: c.exempt,
            base: c.base,
            tax: c.tax,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WithholdingDto {
    #[serde(with = "decimal_out")]
    pub withheld: Decimal,
    #[serde(with = "decimal_out")]
    pub carried_in: Decimal,
    #[serde(with = "decimal_out")]
    pub used: Decimal,
    #[serde(with = "decimal_out")]
    pub carried_out: Decimal,
}

impl From<&WithholdingAssessment> for WithholdingDto {
    fn from(w: &WithholdingAssessment) -> Self {
        Self {
            withheld: w.withheld,
            carried_in: w.carried_in,
            used: w.used,
            carried_out: w.carried_out,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PaidDto {
    /// O valor que a tela mostrava quando a pessoa marcou.
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    #[serde(with = "time::serde::rfc3339")]
    pub paid_at: OffsetDateTime,
    /// `false`: o livro mudou depois e o DARF de agora é outro valor.
    pub same_amount: bool,
}

#[derive(Debug, Serialize)]
pub struct DarfDto {
    pub code: &'static str,
    pub regime: Regime,
    #[serde(with = "decimal_out")]
    pub tax: Decimal,
    #[serde(with = "decimal_out")]
    pub pending_before: Decimal,
    #[serde(with = "decimal_out")]
    pub total: Decimal,
    pub payable: bool,
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    #[serde(with = "iso_date")]
    pub due_date: Date,
    #[serde(with = "decimal_out")]
    pub pending_after: Decimal,
    pub paid: Option<PaidDto>,
}

impl DarfDto {
    fn new(darf: &DarfAssessment, paid: Option<&DarfPayment>) -> Self {
        Self {
            code: darf.code,
            regime: darf.regime,
            tax: darf.tax,
            pending_before: darf.pending_before,
            total: darf.total,
            payable: darf.payable,
            amount: darf.amount,
            due_date: darf.due_date,
            pending_after: darf.pending_after,
            paid: paid.map(|p| PaidDto {
                amount: p.amount,
                paid_at: p.paid_at,
                same_amount: p.amount == darf.amount,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MonthDto {
    pub year: i32,
    pub month: u8,
    pub sales: Vec<SalesDto>,
    pub exemptions: Vec<ExemptionDto>,
    pub pools: Vec<PoolDto>,
    pub capital_gain: CapitalGainDto,
    pub withholding: WithholdingDto,
    pub darfs: Vec<DarfDto>,
    pub notices: Vec<Notice>,
    /// Pontos de `docs/ir.md` § 5 que mexem num número deste mês.
    pub doubts: Vec<Doubt>,
}

impl MonthDto {
    fn new(report: &MonthReport, payments: &[DarfPayment]) -> Self {
        let YearMonth { year, month } = report.period;
        Self {
            year,
            month,
            sales: report.sales.iter().map(SalesDto::from).collect(),
            exemptions: report.exemptions.iter().map(ExemptionDto::from).collect(),
            pools: report.pools.iter().map(PoolDto::from).collect(),
            capital_gain: CapitalGainDto::from(&report.capital_gain),
            withholding: WithholdingDto::from(&report.withholding),
            darfs: report
                .darfs
                .iter()
                .map(|darf| {
                    let paid = payments
                        .iter()
                        .find(|p| p.year == year && p.month == month && p.code == darf.code);
                    DarfDto::new(darf, paid)
                })
                .collect(),
            notices: report.notices.clone(),
            doubts: month_doubts(report),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BalanceDto {
    pub losses: BTreeMap<Pool, f64>,
    pub pending_darf: BTreeMap<Regime, f64>,
}

impl BalanceDto {
    fn new(losses: &BTreeMap<Pool, Decimal>, pending: &BTreeMap<Regime, Decimal>) -> Self {
        let number = |value: &Decimal| value.to_f64().unwrap_or(0.0);
        Self {
            losses: losses.iter().map(|(k, v)| (*k, number(v))).collect(),
            pending_darf: pending.iter().map(|(k, v)| (*k, number(v))).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MonthsDto {
    pub year: i32,
    /// O primeiro ano apurado (o saldo informado vale em 1º de janeiro dele).
    pub since_year: i32,
    pub covered_years: Vec<i32>,
    #[serde(with = "iso_date")]
    pub today: Date,
    pub months: Vec<MonthDto>,
    /// Prejuízo e DARF abaixo do mínimo em 31/12 do ano.
    pub closing: BalanceDto,
    /// O que ficou fora da conta, e por quê.
    pub skipped: Vec<SkippedDto>,
}

#[derive(Debug, Serialize)]
pub struct AssetLineDto {
    pub symbol: String,
    pub instrument: Instrument,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out")]
    pub total_cost: Decimal,
    #[serde(with = "decimal_out")]
    pub average_price: Decimal,
    pub group: &'static str,
    pub code: &'static str,
    pub label: &'static str,
    pub code_source: &'static str,
}

impl From<&AssetLine> for AssetLineDto {
    fn from(a: &AssetLine) -> Self {
        Self {
            symbol: a.symbol.clone(),
            instrument: a.instrument,
            quantity: a.quantity,
            total_cost: a.total_cost,
            average_price: a.average_price,
            group: a.code.group,
            code: a.code.code,
            label: a.code.label,
            code_source: a.code_source,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PayerDto {
    pub symbol: String,
    #[serde(with = "decimal_out")]
    pub gross: Decimal,
    #[serde(with = "decimal_out")]
    pub withheld: Decimal,
}

#[derive(Debug, Serialize)]
pub struct IncomeLineDto {
    pub group: &'static str,
    pub code: &'static str,
    pub label: &'static str,
    pub code_source: &'static str,
    #[serde(with = "decimal_out")]
    pub gross: Decimal,
    #[serde(with = "decimal_out")]
    pub withheld: Decimal,
    #[serde(with = "decimal_out")]
    pub net: Decimal,
    pub by_payer: Vec<PayerDto>,
}

impl From<&IncomeLine> for IncomeLineDto {
    fn from(l: &IncomeLine) -> Self {
        Self {
            group: l.code.group,
            code: l.code.code,
            label: l.code.label,
            code_source: l.code_source,
            gross: l.gross,
            withheld: l.withheld,
            net: l.net,
            by_payer: l
                .by_payer
                .iter()
                .map(|p| PayerDto {
                    symbol: p.symbol.clone(),
                    gross: p.gross,
                    withheld: p.withheld,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IrpfDto {
    pub year: i32,
    pub since_year: i32,
    pub covered_years: Vec<i32>,
    pub assets: Vec<AssetLineDto>,
    pub exempt_income: Vec<IncomeLineDto>,
    pub exclusive_income: Vec<IncomeLineDto>,
    /// Prejuízo e DARF abaixo do mínimo que passam para o ano seguinte.
    pub year_end: BalanceDto,
    #[serde(with = "decimal_out")]
    pub withholding_left: Decimal,
    #[serde(with = "decimal_out")]
    pub darf_total: Decimal,
    pub doubts: Vec<Doubt>,
    pub skipped: Vec<SkippedDto>,
}

impl IrpfDto {
    fn new(report: &AnnualReport, since_year: i32, skipped: Vec<SkippedDto>) -> Self {
        Self {
            year: report.year,
            since_year,
            covered_years: covered_years(),
            assets: report.assets.iter().map(AssetLineDto::from).collect(),
            exempt_income: report
                .exempt_income
                .iter()
                .map(IncomeLineDto::from)
                .collect(),
            exclusive_income: report
                .exclusive_income
                .iter()
                .map(IncomeLineDto::from)
                .collect(),
            year_end: BalanceDto::new(&report.losses_at_year_end, &report.pending_darf_at_year_end),
            withholding_left: report.withholding_left,
            darf_total: report.darf_total,
            doubts: annual_doubts(report),
            skipped,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CarryforwardDto {
    /// O saldo vale em 1º de janeiro deste ano (o primeiro apurado).
    pub year: i32,
    /// A pessoa já informou (sem isso, tudo zero no primeiro ano com regra).
    pub informed: bool,
    pub covered_years: Vec<i32>,
    #[serde(with = "decimal_out")]
    pub common_loss: Decimal,
    #[serde(with = "decimal_out")]
    pub day_trade_loss: Decimal,
    #[serde(with = "decimal_out")]
    pub real_estate_fund_loss: Decimal,
    #[serde(with = "decimal_out")]
    pub agribusiness_fund_loss: Decimal,
    #[serde(with = "decimal_out")]
    pub pending_exchange_darf: Decimal,
    #[serde(with = "decimal_out")]
    pub pending_capital_gain_darf: Decimal,
}

impl CarryforwardDto {
    fn new(stored: Option<StoredCarryforward>) -> Self {
        let value = stored.unwrap_or_else(|| empty_carryforward(first_covered_year()));
        Self {
            year: value.year,
            informed: stored.is_some(),
            covered_years: covered_years(),
            common_loss: value.common_loss,
            day_trade_loss: value.day_trade_loss,
            real_estate_fund_loss: value.real_estate_fund_loss,
            agribusiness_fund_loss: value.agribusiness_fund_loss,
            pending_exchange_darf: value.pending_exchange_darf,
            pending_capital_gain_darf: value.pending_capital_gain_darf,
        }
    }
}

// ------------------------------------------------------------------ apoio --

fn first_covered_year() -> i32 {
    covered_years().into_iter().min().unwrap_or(2025)
}

fn empty_carryforward(year: i32) -> StoredCarryforward {
    StoredCarryforward {
        year,
        common_loss: Decimal::ZERO,
        day_trade_loss: Decimal::ZERO,
        real_estate_fund_loss: Decimal::ZERO,
        agribusiness_fund_loss: Decimal::ZERO,
        pending_exchange_darf: Decimal::ZERO,
        pending_capital_gain_darf: Decimal::ZERO,
    }
}

fn domain_carryforward(stored: &StoredCarryforward) -> Carryforward {
    let mut out = Carryforward::default();
    for (pool, value) in [
        (Pool::Common, stored.common_loss),
        (Pool::DayTrade, stored.day_trade_loss),
        (Pool::RealEstateFund, stored.real_estate_fund_loss),
        (Pool::AgribusinessFund, stored.agribusiness_fund_loss),
    ] {
        if !value.is_zero() {
            out.losses.insert(pool, value);
        }
    }
    for (regime, value) in [
        (Regime::ExchangeNetGain, stored.pending_exchange_darf),
        (Regime::CapitalGain, stored.pending_capital_gain_darf),
    ] {
        if !value.is_zero() {
            out.pending_darf.insert(regime, value);
        }
    }
    out
}

fn years_text() -> String {
    let years: Vec<String> = covered_years().iter().map(i32::to_string).collect();
    years.join(" e ")
}

/// O erro do domínio como a pessoa lê.
fn tax_error(error: TaxError) -> ApiError {
    match error {
        TaxError::NoRulesForYear(year) | TaxError::NoCalendarForYear(year) => ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "tax_year_not_covered",
            title: "Ano sem regra de IR",
            detail: format!(
                "O Prumo tem a regra de IR de {} e ainda nao a de {year}. Ele nao usa a regra de outro ano.",
                years_text()
            ),
        },
        TaxError::InvalidYearRange { first, last } => ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "tax_year_before_balance",
            title: "Ano antes do saldo informado",
            detail: format!(
                "O saldo de prejuizo informado vale em 1/1/{first}; {last} vem antes. Informe o saldo de 1/1/{last} para apurar esse ano."
            ),
        },
        TaxError::IncompleteRules { .. } => {
            tracing::error!(error = %error, "tabela de regras de IR incompleta");
            ApiError::internal()
        }
        other => ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "tax_book_invalid",
            title: "O livro nao fecha para o IR",
            detail: other.to_string(),
        },
    }
}

fn check_year(year: i32) -> ApiResult<i32> {
    if covered_years().contains(&year) {
        Ok(year)
    } else {
        Err(tax_error(TaxError::NoRulesForYear(year)))
    }
}

fn today() -> Date {
    today_in_brazil(OffsetDateTime::now_utc())
}

/// O livro, os proventos recebidos e o saldo de partida da pessoa, prontos
/// para o domínio.
struct Loaded {
    book: Book,
    incomes: Vec<Income>,
    income_skipped: Vec<Skipped>,
    carryforward: Carryforward,
    since_year: i32,
}

impl Loaded {
    fn input(&self) -> TaxInput<'_> {
        TaxInput {
            openings: &self.book.openings,
            operations: &self.book.operations,
            since_year: self.since_year,
            carryforward: self.carryforward.clone(),
        }
    }
}

async fn load(state: &AppState, user: &CurrentUser, today: Date) -> ApiResult<Loaded> {
    let lines = state.store.tax_ledger(user.id).await?;
    let book = tax_book::book(&lines);

    let instruments: HashMap<_, _> = state
        .store
        .assets(user.id)
        .await?
        .into_iter()
        .map(|asset| (asset.id, asset.tax_instrument))
        .collect();
    // O que a pessoa recebeu: a estimativa não é fato, e o provisionado
    // (pagamento no futuro) ainda não aconteceu.
    let payments: Vec<Payment> = state
        .store
        .income_ledger(user.id)
        .await?
        .into_iter()
        .filter(|event| event.source != IncomeSource::Estimate && event.pay_date <= today)
        .map(|event| Payment {
            payer: event.payer_label(),
            kind: event.kind,
            instrument: event.asset_id.and_then(|id| instruments.get(&id).copied()),
            pay_date: event.pay_date,
            gross: event.gross,
            withheld: event.withholding,
        })
        .collect();
    let (incomes, income_skipped) = tax_book::incomes(&payments);

    let stored = state.store.tax_carryforward(user.id).await?;
    let start = stored.unwrap_or_else(|| empty_carryforward(first_covered_year()));
    Ok(Loaded {
        book,
        incomes,
        income_skipped,
        carryforward: domain_carryforward(&start),
        since_year: start.year,
    })
}

async fn months_of(
    state: &AppState,
    user: &CurrentUser,
    year: i32,
) -> ApiResult<(Loaded, Assessment)> {
    let year = check_year(year)?;
    let loaded = load(state, user, today()).await?;
    let assessment = assess(&loaded.input(), year).map_err(tax_error)?;
    Ok((loaded, assessment))
}

fn darf_code(code: &str) -> ApiResult<&str> {
    match code {
        "6015" | "4600" => Ok(code),
        _ => Err(ApiError::validation(
            "O codigo do DARF e 6015 (bolsa) ou 4600 (cripto).",
        )),
    }
}

fn month_number(month: u8) -> ApiResult<u8> {
    if (1..=12).contains(&month) {
        Ok(month)
    } else {
        Err(ApiError::not_found())
    }
}

fn amount(name: &str, value: Decimal) -> ApiResult<Decimal> {
    if value < Decimal::ZERO {
        return Err(ApiError::validation(format!(
            "{name} nao pode ser negativo."
        )));
    }
    if value.normalize().scale() > 2 {
        return Err(ApiError::validation(format!("{name} vai em centavos.")));
    }
    if value > Decimal::new(MAX_AMOUNT, 0) {
        return Err(ApiError::validation(format!("{name} passa do limite.")));
    }
    Ok(value)
}

// ----------------------------------------------------------------- rotas --

/// `GET /tax/months?year=`: os 12 meses do ano (o ano de hoje, sem `year`).
pub async fn months(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<YearQuery>,
) -> ApiResult<Json<MonthsDto>> {
    let today = today();
    let year = query.year.unwrap_or(today.year());
    let (loaded, assessment) = months_of(&state, &user, year).await?;
    let payments = state.store.darf_payments(user.id, year).await?;
    Ok(Json(MonthsDto {
        year,
        since_year: loaded.since_year,
        covered_years: covered_years(),
        today,
        months: assessment
            .months
            .iter()
            .filter(|report| report.period.year == year)
            .map(|report| MonthDto::new(report, &payments))
            .collect(),
        closing: BalanceDto::new(&assessment.closing.losses, &assessment.closing.pending_darf),
        skipped: loaded.book.skipped.iter().map(SkippedDto::from).collect(),
    }))
}

/// `GET /tax/irpf/{year}`: Bens e Direitos, rendimentos e o saldo do ano.
pub async fn irpf(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(year): ApiPath<i32>,
) -> ApiResult<Json<IrpfDto>> {
    let year = check_year(year)?;
    let loaded = load(&state, &user, today()).await?;
    let report = annual_report(&loaded.input(), &loaded.incomes, year).map_err(tax_error)?;
    let skipped = loaded
        .book
        .skipped
        .iter()
        .chain(
            loaded
                .income_skipped
                .iter()
                .filter(|s| s.date.is_some_and(|d| d.year() == year)),
        )
        .map(SkippedDto::from)
        .collect();
    Ok(Json(IrpfDto::new(&report, loaded.since_year, skipped)))
}

/// `POST /tax/darfs/{year}/{month}/paid`: marca o DARF do mês como pago, com o
/// valor de agora. Só um DARF que existe (a pagar) pode ser marcado.
pub async fn mark_paid(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath((year, month)): ApiPath<(i32, u8)>,
    ApiJson(body): ApiJson<PaidRequest>,
) -> ApiResult<Json<DarfDto>> {
    let month = month_number(month)?;
    let code = darf_code(&body.code)?;
    let (_, assessment) = months_of(&state, &user, year).await?;
    let darf = assessment
        .months
        .iter()
        .find(|report| report.period == YearMonth { year, month })
        .and_then(|report| report.darfs.iter().find(|darf| darf.code == code))
        .filter(|darf| darf.payable)
        .ok_or_else(|| ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "darf_not_due",
            title: "Nao ha DARF a pagar",
            detail: format!("O Prumo nao apurou DARF {code} a pagar em {month:02}/{year}."),
        })?;
    let paid = state
        .store
        .mark_darf_paid(user.id, year, month, code, darf.amount)
        .await?;
    Ok(Json(DarfDto::new(darf, Some(&paid))))
}

/// `DELETE /tax/darfs/{year}/{month}/paid?code=`: tira a marca.
pub async fn unmark_paid(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath((year, month)): ApiPath<(i32, u8)>,
    ApiQuery(query): ApiQuery<CodeQuery>,
) -> ApiResult<StatusCode> {
    let month = month_number(month)?;
    let code = darf_code(&query.code)?;
    if state
        .store
        .unmark_darf_paid(user.id, year, month, code)
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

/// `GET /tax/carryforward`: o saldo de antes do Prumo (zero se não informado).
pub async fn get_carryforward(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<CarryforwardDto>> {
    let stored = state.store.tax_carryforward(user.id).await?;
    Ok(Json(CarryforwardDto::new(stored)))
}

/// `PUT /tax/carryforward`: troca o saldo inteiro.
pub async fn put_carryforward(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<CarryforwardRequest>,
) -> ApiResult<Json<CarryforwardDto>> {
    let value = StoredCarryforward {
        year: check_year(body.year)?,
        common_loss: amount("O prejuizo em operacoes comuns", body.common_loss)?,
        day_trade_loss: amount("O prejuizo em day trade", body.day_trade_loss)?,
        real_estate_fund_loss: amount("O prejuizo em FII", body.real_estate_fund_loss)?,
        agribusiness_fund_loss: amount("O prejuizo em Fiagro", body.agribusiness_fund_loss)?,
        pending_exchange_darf: amount("O DARF 6015 acumulado", body.pending_exchange_darf)?,
        pending_capital_gain_darf: amount("O DARF 4600 acumulado", body.pending_capital_gain_darf)?,
    };
    state.store.set_tax_carryforward(user.id, &value).await?;
    Ok(Json(CarryforwardDto::new(Some(value))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn domain_errors_read_clearly() {
        let error = tax_error(TaxError::NoRulesForYear(2031));
        assert_eq!(error.code, "tax_year_not_covered");
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(error.detail.contains("2025 e 2026"), "{}", error.detail);
        assert!(error.detail.contains("2031"));
        let error = tax_error(TaxError::InvalidYearRange {
            first: 2026,
            last: 2025,
        });
        assert_eq!(error.code, "tax_year_before_balance");
        let error = tax_error(TaxError::InstrumentMismatch {
            symbol: "XPTO3".into(),
        });
        assert_eq!(error.code, "tax_book_invalid");
        assert!(error.detail.contains("XPTO3"));
        let error = tax_error(TaxError::IncompleteRules {
            year: 2025,
            missing: "isencao",
        });
        assert_eq!(error.code, "internal");
    }

    #[test]
    fn amounts_are_cents_and_not_negative() {
        assert_eq!(amount("X", dec!(10.5)).unwrap(), dec!(10.5));
        assert_eq!(amount("X", dec!(10.500)).unwrap(), dec!(10.500));
        assert!(amount("X", dec!(-1)).is_err());
        assert!(amount("X", dec!(0.001)).is_err());
        assert!(amount("X", dec!(1000000000001)).is_err());
        assert!(darf_code("6015").is_ok());
        assert!(darf_code("0190").is_err());
        assert!(month_number(13).is_err());
        assert!(month_number(0).is_err());
    }

    #[test]
    fn stored_balance_becomes_the_domain_carryforward() {
        let stored = StoredCarryforward {
            common_loss: dec!(100),
            pending_capital_gain_darf: dec!(4),
            ..empty_carryforward(2025)
        };
        let carry = domain_carryforward(&stored);
        assert_eq!(carry.losses.get(&Pool::Common), Some(&dec!(100)));
        assert_eq!(carry.losses.len(), 1);
        assert_eq!(carry.pending_darf.get(&Regime::CapitalGain), Some(&dec!(4)));
        assert_eq!(carry.pending_darf.len(), 1);
    }
}
