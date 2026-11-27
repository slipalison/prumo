//! IR: a apuração mensal (isentômetro, base, imposto, IR retido, DARF e
//! vencimento), a declaração do ano (Bens e Direitos e rendimentos por fonte
//! pagadora, com o CNPJ da CVM), a marca de DARF pago, a multa e os juros do
//! DARF em atraso, o IR retido que a pessoa informa, a tabela de regras do ano
//! com as fontes e o saldo de prejuízo que a pessoa traz de antes do Prumo.
//! Tudo pelo dono.
//!
//! A regra mora em `domain::tax` (por ano-calendário, com a fonte de cada
//! número); a ligação com o livro, em `domain::tax_book`. Ano sem regra
//! cadastrada é erro (`tax_year_not_covered`), nunca a regra de outro ano.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::{decimal, decimal_out};
use super::{ApiJson, ApiPath, ApiQuery, AppState, CurrentUser};
use crate::domain::income::{today_in_brazil, IncomeSource};
use crate::domain::income_auto::parse_ticker;
use crate::domain::tax::irpf::{
    annual_report, AnnualReport, AssetLine, Income, IncomeLine, PayerAmount, PayerSource,
};
use crate::domain::tax::late::{late_charge, LateCharge};
use crate::domain::tax::monthly::{
    assess, Assessment, CapitalGainAssessment, Carryforward, DarfAssessment, ExemptionStatus,
    InstrumentSales, MonthReport, Notice, PoolAssessment, TaxInput, WithholdingAssessment,
};
use crate::domain::tax::rules::{
    covered_years, rules_for, ExemptionId, Pool, Regime, Sourced, YearRules,
};
use crate::domain::tax::{Instrument, TaxError, YearMonth};
use crate::domain::tax_book::{
    self, annual_doubts, month_doubts, payer_registry, Book, Doubt, PayerRegistry, Payment,
    SkipReason, Skipped,
};
use crate::error::{ApiError, ApiResult};
use crate::store::tax::{DarfPayment, StoredCarryforward};
use crate::store::Store;

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

/// O DARF e o dia do pagamento (sem `pay_on`, hoje).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LateQuery {
    pub code: String,
    #[serde(default)]
    pub pay_on: Option<String>,
}

/// O IR retido do mês, como está na nota de corretagem.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithholdingRequest {
    #[serde(deserialize_with = "decimal")]
    pub amount: Decimal,
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
    pub common_sales: Decimal,
    #[serde(with = "decimal_out")]
    pub common_rule: Decimal,
    #[serde(with = "decimal_out")]
    pub day_trade_gains: Decimal,
    #[serde(with = "decimal_out")]
    pub day_trade_rule: Decimal,
    /// O que a pessoa informou; `null` = pela regra.
    #[serde(with = "decimal_out::option")]
    pub informed: Option<Decimal>,
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
            common_sales: w.common_sales,
            common_rule: w.common_rule,
            day_trade_gains: w.day_trade_gains,
            day_trade_rule: w.day_trade_rule,
            informed: w.informed,
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
pub struct ThresholdDto {
    pub month: u8,
    #[serde(with = "decimal_out")]
    pub gross: Decimal,
    #[serde(with = "decimal_out")]
    pub withholding_by_rule: Decimal,
    #[serde(with = "decimal_out")]
    pub withheld: Decimal,
}

/// Uma fonte pagadora numa linha de rendimento.
#[derive(Debug, Serialize)]
pub struct PayerDto {
    pub symbols: Vec<String>,
    /// `null`: o Prumo não achou o CNPJ no cadastro da CVM.
    pub cnpj: Option<String>,
    pub name: Option<String>,
    pub payer_source: Option<PayerSource>,
    #[serde(with = "decimal_out")]
    pub gross: Decimal,
    #[serde(with = "decimal_out")]
    pub withheld: Decimal,
    #[serde(with = "decimal_out")]
    pub net: Decimal,
    pub above_threshold: Vec<ThresholdDto>,
}

impl From<&PayerAmount> for PayerDto {
    fn from(p: &PayerAmount) -> Self {
        Self {
            symbols: p.symbols.clone(),
            cnpj: p.payer.as_ref().map(|payer| payer.cnpj.clone()),
            name: p.payer.as_ref().map(|payer| payer.name.clone()),
            payer_source: p.payer.as_ref().map(|payer| payer.source),
            gross: p.gross,
            withheld: p.withheld,
            net: p.net,
            above_threshold: p
                .above_threshold
                .iter()
                .map(|m| ThresholdDto {
                    month: m.month,
                    gross: m.gross,
                    withholding_by_rule: m.withholding_by_rule,
                    withheld: m.withheld,
                })
                .collect(),
        }
    }
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
            by_payer: l.by_payer.iter().map(PayerDto::from).collect(),
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

/// Um número da regra e de onde ele vem.
#[derive(Debug, Serialize)]
pub struct SourcedNumber {
    #[serde(with = "decimal_out")]
    pub value: Decimal,
    pub source: &'static str,
}

impl From<&Sourced<Decimal>> for SourcedNumber {
    fn from(s: &Sourced<Decimal>) -> Self {
        Self {
            value: s.value,
            source: s.source,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SourcedText {
    pub value: &'static str,
    pub source: &'static str,
}

impl From<&Sourced<&'static str>> for SourcedText {
    fn from(s: &Sourced<&'static str>) -> Self {
        Self {
            value: s.value,
            source: s.source,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PoolRuleDto {
    pub pool: Pool,
    pub rate: SourcedNumber,
}

#[derive(Debug, Serialize)]
pub struct InstrumentRuleDto {
    pub instrument: Instrument,
    pub regime: Regime,
    pub common_pool: Option<Pool>,
    pub day_trade_pool: Option<Pool>,
    pub exemption: Option<ExemptionId>,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ExemptionRuleDto {
    pub id: ExemptionId,
    pub limit: SourcedNumber,
    pub counted: Vec<Instrument>,
    pub counts_day_trade: bool,
    pub counts_day_trade_source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BracketDto {
    #[serde(with = "decimal_out::option")]
    pub up_to: Option<Decimal>,
    #[serde(with = "decimal_out")]
    pub rate: Decimal,
}

#[derive(Debug, Serialize)]
pub struct WithholdingRuleDto {
    pub common_rate: SourcedNumber,
    pub common_dispensed_up_to: SourcedNumber,
    pub day_trade_rate: SourcedNumber,
    pub carry_within_year_source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct LatePaymentRuleDto {
    pub daily_penalty: SourcedNumber,
    pub penalty_cap: SourcedNumber,
    pub payment_month_interest: SourcedNumber,
    pub selic_series: SourcedText,
}

#[derive(Debug, Serialize)]
pub struct DividendWithholdingDto {
    #[serde(with = "decimal_out")]
    pub monthly_threshold: Decimal,
    #[serde(with = "decimal_out")]
    pub rate: Decimal,
    pub source: &'static str,
}

/// A tabela de regras de um ano, cada número com a fonte (a tela mostra o
/// que o Prumo cobre e de onde vem).
#[derive(Debug, Serialize)]
pub struct RulesDto {
    pub year: i32,
    #[serde(with = "iso_date")]
    pub reviewed_on: Date,
    pub covered_years: Vec<i32>,
    pub instruments: Vec<InstrumentRuleDto>,
    pub pools: Vec<PoolRuleDto>,
    pub exemptions: Vec<ExemptionRuleDto>,
    pub capital_gain_brackets: Vec<BracketDto>,
    pub capital_gain_brackets_source: &'static str,
    pub exchange_darf_code: SourcedText,
    pub capital_gain_darf_code: SourcedText,
    pub darf_minimum: SourcedNumber,
    pub withholding: WithholdingRuleDto,
    pub late_payment: LatePaymentRuleDto,
    pub interest_on_equity_rate: SourcedNumber,
    pub dividend_withholding: Option<DividendWithholdingDto>,
}

impl RulesDto {
    fn new(rules: &YearRules) -> Self {
        Self {
            year: rules.year,
            reviewed_on: rules.reviewed_on,
            covered_years: covered_years(),
            instruments: rules
                .instruments
                .iter()
                .map(|i| InstrumentRuleDto {
                    instrument: i.instrument,
                    regime: i.regime,
                    common_pool: i.common_pool,
                    day_trade_pool: i.day_trade_pool,
                    exemption: i.exemption,
                    source: i.sources,
                })
                .collect(),
            pools: rules
                .pools
                .iter()
                .map(|p| PoolRuleDto {
                    pool: p.pool,
                    rate: SourcedNumber::from(&p.rate),
                })
                .collect(),
            exemptions: rules
                .exemptions
                .iter()
                .map(|e| ExemptionRuleDto {
                    id: e.id,
                    limit: SourcedNumber::from(&e.limit),
                    counted: e.counted.to_vec(),
                    counts_day_trade: e.counts_day_trade.value,
                    counts_day_trade_source: e.counts_day_trade.source,
                })
                .collect(),
            capital_gain_brackets: rules
                .capital_gain_brackets
                .value
                .iter()
                .map(|b| BracketDto {
                    up_to: b.up_to,
                    rate: b.rate,
                })
                .collect(),
            capital_gain_brackets_source: rules.capital_gain_brackets.source,
            exchange_darf_code: SourcedText::from(&rules.exchange_darf_code),
            capital_gain_darf_code: SourcedText::from(&rules.capital_gain_darf_code),
            darf_minimum: SourcedNumber::from(&rules.darf_minimum),
            withholding: WithholdingRuleDto {
                common_rate: SourcedNumber::from(&rules.withholding.common_rate),
                common_dispensed_up_to: SourcedNumber::from(
                    &rules.withholding.common_dispensed_up_to,
                ),
                day_trade_rate: SourcedNumber::from(&rules.withholding.day_trade_rate),
                carry_within_year_source: rules.withholding_carry.source,
            },
            late_payment: LatePaymentRuleDto {
                daily_penalty: SourcedNumber::from(&rules.late_payment.daily_penalty),
                penalty_cap: SourcedNumber::from(&rules.late_payment.penalty_cap),
                payment_month_interest: SourcedNumber::from(
                    &rules.late_payment.payment_month_interest,
                ),
                selic_series: SourcedText::from(&rules.late_payment.selic_series),
            },
            interest_on_equity_rate: SourcedNumber::from(&rules.interest_on_equity_rate),
            dividend_withholding: rules.dividend_withholding.as_ref().map(|d| {
                DividendWithholdingDto {
                    monthly_threshold: d.monthly_threshold,
                    rate: d.rate,
                    source: d.source,
                }
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SelicMonthDto {
    pub year: i32,
    pub month: u8,
    /// % no mês; `null` = a taxa do mês ainda não está no Prumo.
    #[serde(with = "decimal_out::option")]
    pub rate: Option<Decimal>,
}

/// A multa e os juros de um DARF pago em `pay_date`.
#[derive(Debug, Serialize)]
pub struct LateDto {
    pub code: &'static str,
    pub year: i32,
    pub month: u8,
    #[serde(with = "decimal_out")]
    pub principal: Decimal,
    #[serde(with = "iso_date")]
    pub due_date: Date,
    #[serde(with = "iso_date")]
    pub pay_date: Date,
    pub days_late: i64,
    #[serde(with = "decimal_out")]
    pub penalty_rate: Decimal,
    #[serde(with = "decimal_out")]
    pub penalty: Decimal,
    pub selic_months: Vec<SelicMonthDto>,
    #[serde(with = "decimal_out")]
    pub payment_month_rate: Decimal,
    #[serde(with = "decimal_out::option")]
    pub interest_rate: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub interest: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub total: Option<Decimal>,
    pub missing_selic: Vec<SelicMonthDto>,
    pub rule: LatePaymentRuleDto,
}

impl LateDto {
    fn new(code: &'static str, period: YearMonth, charge: &LateCharge, rules: &YearRules) -> Self {
        let month_dto = |period: YearMonth, rate| SelicMonthDto {
            year: period.year,
            month: period.month,
            rate,
        };
        Self {
            code,
            year: period.year,
            month: period.month,
            principal: charge.principal,
            due_date: charge.due_date,
            pay_date: charge.pay_date,
            days_late: charge.days_late,
            penalty_rate: charge.penalty_rate,
            penalty: charge.penalty,
            selic_months: charge
                .selic_months
                .iter()
                .map(|m| month_dto(m.period, m.rate))
                .collect(),
            payment_month_rate: charge.payment_month_rate,
            interest_rate: charge.interest_rate,
            interest: charge.interest,
            total: charge.total,
            missing_selic: charge
                .missing_selic()
                .into_iter()
                .map(|period| month_dto(period, None))
                .collect(),
            rule: RulesDto::new(rules).late_payment,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InformedWithholdingDto {
    pub year: i32,
    pub month: u8,
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
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

/// O livro, os proventos recebidos, o IR retido informado e o saldo de
/// partida da pessoa, prontos para o domínio.
struct Loaded {
    book: Book,
    incomes: Vec<Income>,
    income_skipped: Vec<Skipped>,
    carryforward: Carryforward,
    informed_withholding: BTreeMap<YearMonth, Decimal>,
    since_year: i32,
}

impl Loaded {
    fn input(&self) -> TaxInput<'_> {
        TaxInput {
            openings: &self.book.openings,
            operations: &self.book.operations,
            since_year: self.since_year,
            carryforward: self.carryforward.clone(),
            informed_withholding: self.informed_withholding.clone(),
        }
    }
}

async fn load(store: &Store, user: Uuid, today: Date) -> ApiResult<Loaded> {
    let lines = store.tax_ledger(user).await?;
    let book = tax_book::book(&lines);

    let instruments: HashMap<_, _> = store
        .assets(user)
        .await?
        .into_iter()
        .map(|asset| (asset.id, asset.tax_instrument))
        .collect();
    // O que a pessoa recebeu: a estimativa não é fato, e o provisionado
    // (pagamento no futuro) ainda não aconteceu.
    let events: Vec<_> = store
        .income_ledger(user)
        .await?
        .into_iter()
        .filter(|event| event.source != IncomeSource::Estimate && event.pay_date <= today)
        .collect();
    let mut payments: Vec<(Payment, Option<String>)> = events
        .into_iter()
        .map(|event| {
            let issuer = event
                .symbol
                .as_deref()
                .and_then(parse_ticker)
                .map(|ticker| ticker.issuer);
            let payment = Payment {
                payer: event.payer_label(),
                kind: event.kind,
                instrument: event.asset_id.and_then(|id| instruments.get(&id).copied()),
                pay_date: event.pay_date,
                gross: event.gross,
                withheld: event.withholding,
                source: None,
            };
            (payment, issuer)
        })
        .collect();
    fill_payers(store, &mut payments).await?;
    let payments: Vec<Payment> = payments.into_iter().map(|(payment, _)| payment).collect();
    let (incomes, income_skipped) = tax_book::incomes(&payments);

    let stored = store.tax_carryforward(user).await?;
    let start = stored.unwrap_or_else(|| empty_carryforward(first_covered_year()));
    Ok(Loaded {
        book,
        incomes,
        income_skipped,
        carryforward: domain_carryforward(&start),
        informed_withholding: store.informed_withholding(user).await?,
        since_year: start.year,
    })
}

/// O CNPJ e a razão social de cada fonte pagadora, do cadastro da CVM que o
/// Prumo já guarda (companhia pelo código CVM do emissor; FII pelo informe).
async fn fill_payers(store: &Store, payments: &mut [(Payment, Option<String>)]) -> ApiResult<()> {
    let mut companies: HashSet<String> = HashSet::new();
    let mut funds: HashSet<String> = HashSet::new();
    for (payment, issuer) in payments.iter() {
        match (payer_registry(payment), issuer) {
            (Some(PayerRegistry::Company), Some(issuer)) => {
                companies.insert(issuer.clone());
            }
            (Some(PayerRegistry::Fund), Some(issuer)) => {
                funds.insert(issuer.clone());
            }
            _ => {}
        }
    }
    let companies: Vec<String> = companies.into_iter().collect();
    let funds: Vec<String> = funds.into_iter().collect();
    let (by_company, by_fund) = store.tax_payers(&companies, &funds).await?;
    for (payment, issuer) in payments.iter_mut() {
        let Some(issuer) = issuer else {
            continue;
        };
        payment.source = match payer_registry(payment) {
            Some(PayerRegistry::Company) => by_company.get(issuer).cloned(),
            Some(PayerRegistry::Fund) => by_fund.get(issuer).cloned(),
            None => None,
        };
    }
    Ok(())
}

async fn months_of(
    state: &AppState,
    user: &CurrentUser,
    year: i32,
) -> ApiResult<(Loaded, Assessment)> {
    let year = check_year(year)?;
    let loaded = load(&state.store, user.id, today()).await?;
    let assessment = assess(&loaded.input(), year).map_err(tax_error)?;
    Ok((loaded, assessment))
}

/// A apuração de IR de `year` para quem não é uma requisição (o resumo de
/// alertas): a mesma de `GET /tax/months`, com a mesma regra por ano. Ano sem
/// regra volta como `tax_year_not_covered`.
pub(crate) async fn assessment_for(
    store: &Store,
    user: Uuid,
    today: Date,
    year: i32,
) -> ApiResult<Assessment> {
    let year = check_year(year)?;
    let loaded = load(store, user, today).await?;
    assess(&loaded.input(), year).map_err(tax_error)
}

fn darf_code(code: &str) -> ApiResult<&'static str> {
    match code {
        "6015" => Ok("6015"),
        "4600" => Ok("4600"),
        _ => Err(ApiError::validation(
            "O codigo do DARF e 6015 (bolsa) ou 4600 (cripto).",
        )),
    }
}

fn not_due(code: &str, year: i32, month: u8) -> ApiError {
    ApiError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: "darf_not_due",
        title: "Nao ha DARF a pagar",
        detail: format!("O Prumo nao apurou DARF {code} a pagar em {month:02}/{year}."),
    }
}

fn period_of(year: i32, month: u8) -> ApiResult<YearMonth> {
    Ok(YearMonth {
        year: check_year(year)?,
        month: month_number(month)?,
    })
}

fn pay_date(text: Option<&str>, today: Date) -> ApiResult<Date> {
    let Some(text) = text else {
        return Ok(today);
    };
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let date = Date::parse(text.trim(), format)
        .map_err(|_| ApiError::validation("A data do pagamento vai como AAAA-MM-DD."))?;
    if !(2000..=2100).contains(&date.year()) {
        return Err(ApiError::validation(
            "A data do pagamento esta fora do intervalo aceito.",
        ));
    }
    Ok(date)
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
    let loaded = load(&state.store, user.id, today()).await?;
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
        .ok_or_else(|| not_due(code, year, month))?;
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

/// `GET /tax/rules/{year}`: a tabela de regras do ano, cada número com a
/// fonte.
pub async fn rules(_user: CurrentUser, ApiPath(year): ApiPath<i32>) -> ApiResult<Json<RulesDto>> {
    let rules = rules_for(check_year(year)?).map_err(tax_error)?;
    Ok(Json(RulesDto::new(&rules)))
}

/// `GET /tax/darfs/{year}/{month}/late?code=&pay_on=`: a multa e os juros do
/// DARF do mês se pago em `pay_on` (hoje, sem a data). Só um DARF que existe
/// (a pagar).
pub async fn late(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath((year, month)): ApiPath<(i32, u8)>,
    ApiQuery(query): ApiQuery<LateQuery>,
) -> ApiResult<Json<LateDto>> {
    let month = month_number(month)?;
    let code = darf_code(&query.code)?;
    let pay_on = pay_date(query.pay_on.as_deref(), today())?;
    let (_, assessment) = months_of(&state, &user, year).await?;
    let darf = assessment
        .months
        .iter()
        .find(|report| report.period == YearMonth { year, month })
        .and_then(|report| report.darfs.iter().find(|darf| darf.code == code))
        .filter(|darf| darf.payable)
        .ok_or_else(|| not_due(code, year, month))?;
    let rules = rules_for(year).map_err(tax_error)?;
    let selic = state.store.tax_selic().await?;
    let charge = late_charge(
        &rules.late_payment,
        darf.amount,
        darf.due_date,
        pay_on,
        &selic,
    );
    Ok(Json(LateDto::new(
        darf.code,
        YearMonth { year, month },
        &charge,
        &rules,
    )))
}

/// `PUT /tax/withholding/{year}/{month}`: o IR retido do mês, da nota.
pub async fn put_withholding(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath((year, month)): ApiPath<(i32, u8)>,
    ApiJson(body): ApiJson<WithholdingRequest>,
) -> ApiResult<Json<InformedWithholdingDto>> {
    let period = period_of(year, month)?;
    let value = amount("O IR retido", body.amount)?;
    state
        .store
        .set_informed_withholding(user.id, period, value)
        .await?;
    Ok(Json(InformedWithholdingDto {
        year: period.year,
        month: period.month,
        amount: value,
    }))
}

/// `DELETE /tax/withholding/{year}/{month}`: volta o mês para a regra.
pub async fn delete_withholding(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath((year, month)): ApiPath<(i32, u8)>,
) -> ApiResult<StatusCode> {
    let period = period_of(year, month)?;
    if state
        .store
        .clear_informed_withholding(user.id, period)
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
        assert!(period_of(2031, 1).is_err());
        assert!(period_of(2026, 13).is_err());
        assert_eq!(
            period_of(2026, 3).unwrap(),
            YearMonth {
                year: 2026,
                month: 3
            }
        );
    }

    #[test]
    fn pay_date_is_iso_and_defaults_to_today() {
        use time::macros::date;
        let today = date!(2026 - 11 - 27);
        assert_eq!(pay_date(None, today).unwrap(), today);
        assert_eq!(
            pay_date(Some("2026-12-15"), today).unwrap(),
            date!(2026 - 12 - 15)
        );
        assert!(pay_date(Some("15/12/2026"), today).is_err());
        assert!(pay_date(Some("1999-12-31"), today).is_err());
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
