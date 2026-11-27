//! Apuração mensal: isentômetro, compensação de prejuízo, imposto, IR retido e
//! DARF.
//!
//! Por mês, com a regra do ano do mês ([`rules_for`]):
//!
//! 1. **Vendas por instrumento** (comum e day trade) somam no limite de
//!    isenção. Ações: vendas no mês até R$ 20 mil (inclusive) deixam o lucro
//!    das ações em operação comum isento; prejuízo do mês isento continua
//!    compensável (IN RFB 1.585/2015, art. 59, par. 1; P&R 729). Day trade
//!    nunca é isento (art. 65, par. 15).
//! 2. **Grupos de compensação** ([`Pool`]): comum (ações, ETF, BDR) a 15%, day
//!    trade a 20%, FII a 20% e Fiagro a 20%. Prejuízo de um grupo só abate
//!    lucro do mesmo grupo, no mês ou nos seguintes, sem prazo e atravessando o
//!    ano (P&R 709 a 711; IN RFB 1.585/2015, arts. 37, par. 2, e 64). Prejuízo
//!    não volta para meses anteriores (P&R 710).
//! 3. **Cripto** é ganho de capital: cada alienação com lucro soma, prejuízo
//!    não compensa (P&R 583), e o mês com alienações até R$ 35 mil é isento
//!    (P&R 653). Imposto pela tabela progressiva (P&R 574).
//! 4. **IR retido** ("dedo-duro" de 0,005% e o 1% do day trade) abate o imposto
//!    do mês em bolsa; a sobra passa para os meses seguintes só dentro do ano
//!    (Lei 11.033/2004, art. 2, par. 7; P&R 714 e 715). A sobra de dezembro vai
//!    para a declaração (ou pedido de restituição) e fica em
//!    [`Assessment::withholding_left`]. O retido do mês é o que a pessoa
//!    informou (da nota ou do informe da corretora) ou, sem isso, o que a regra
//!    manda a corretora reter ([`super::rules::WithholdingRule`]): 0,005% das
//!    vendas em operação comum, dispensado quando a soma do mês fica em R$ 1,00
//!    ou menos, e 1% do resultado positivo do day trade de cada dia, com as
//!    perdas do mesmo dia compensadas. A regra supõe uma corretora só, como o
//!    casamento do day trade.
//! 5. **DARF** por código (6015 bolsa, 4600 cripto): abaixo de R$ 10 não se
//!    paga; soma ao do mesmo código no mês seguinte e vence no prazo deste
//!    último mês (Lei 9.430/1996, art. 68). Vencimento no último dia útil do
//!    mês seguinte ([`calendar::darf_due_date`]).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, Month};

use super::calendar;
use super::positions::{self, Holding, OpeningPosition, Operation, Realized};
use super::rules::{rules_for, ExemptionId, Pool, Regime, YearRules};
use super::{cents, Instrument, TaxError, YearMonth};

/// O que entra na apuração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxInput<'a> {
    /// Posições anteriores ao primeiro lançamento, com o custo pelo PM.
    pub openings: &'a [OpeningPosition],
    /// O livro inteiro (lançamentos de antes de `since_year` só formam o PM).
    pub operations: &'a [Operation],
    /// Primeiro ano apurado. O saldo de `carryforward` vale em 1º de janeiro
    /// dele.
    pub since_year: i32,
    pub carryforward: Carryforward,
    /// O IR retido na fonte em bolsa que a pessoa informou, por mês. Mês sem
    /// valor usa a retenção pela regra.
    pub informed_withholding: BTreeMap<YearMonth, Decimal>,
}

/// O que passa de um mês para o outro (e de um ano para o outro).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Carryforward {
    /// Prejuízo a compensar, por grupo.
    pub losses: BTreeMap<Pool, Decimal>,
    /// Imposto abaixo do mínimo do DARF ainda não pago, por regime (código).
    pub pending_darf: BTreeMap<Regime, Decimal>,
}

/// Resultado da apuração de `since_year` até o ano pedido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assessment {
    pub months: Vec<MonthReport>,
    /// Saldo em 31/12 do último ano.
    pub closing: Carryforward,
    /// IR retido que sobrou em dezembro de cada ano (vai para a declaração).
    pub withholding_left: BTreeMap<i32, Decimal>,
    /// Posições em 31/12 do último ano.
    pub holdings: BTreeMap<String, Holding>,
}

/// Um mês apurado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MonthReport {
    pub period: YearMonth,
    /// Vendas e resultados por instrumento (todos os instrumentos, com zero).
    pub sales: Vec<InstrumentSales>,
    /// O isentômetro: vendas contra o limite.
    pub exemptions: Vec<ExemptionStatus>,
    pub pools: Vec<PoolAssessment>,
    pub capital_gain: CapitalGainAssessment,
    pub withholding: WithholdingAssessment,
    pub darfs: Vec<DarfAssessment>,
    pub notices: Vec<Notice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstrumentSales {
    pub instrument: Instrument,
    pub common_sales: Decimal,
    pub day_trade_sales: Decimal,
    pub common_result: Decimal,
    pub day_trade_result: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExemptionStatus {
    pub id: ExemptionId,
    pub sales: Decimal,
    pub limit: Decimal,
    /// Quanto ainda dá para vender no mês sem perder a isenção.
    pub remaining: Decimal,
    pub exempt: bool,
    /// Lucro que ficou isento (rendimento isento na declaração).
    pub exempt_gain: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoolAssessment {
    pub pool: Pool,
    /// Resultado tributável do mês (lucro positivo, prejuízo negativo).
    pub result: Decimal,
    pub loss_before: Decimal,
    pub loss_used: Decimal,
    pub base: Decimal,
    pub rate: Decimal,
    pub tax: Decimal,
    pub loss_after: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapitalGainAssessment {
    /// Soma dos lucros por alienação.
    pub gains: Decimal,
    /// Soma dos prejuízos por alienação (não compensam).
    pub losses: Decimal,
    pub exempt: bool,
    pub base: Decimal,
    pub tax: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WithholdingAssessment {
    /// Base do 0,005%: o valor das vendas em operação comum em bolsa.
    pub common_sales: Decimal,
    /// O 0,005% pela regra (zero quando a soma do mês fica em R$ 1,00 ou
    /// menos).
    pub common_rule: Decimal,
    /// Base do 1%: a soma dos dias em que o day trade deu lucro.
    pub day_trade_gains: Decimal,
    /// O 1% do day trade pela regra.
    pub day_trade_rule: Decimal,
    /// O que a pessoa informou para o mês; `None` usa a regra.
    pub informed: Option<Decimal>,
    /// IR retido do mês que entra na conta: o informado, senão a regra.
    pub withheld: Decimal,
    /// Sobra de meses anteriores do mesmo ano.
    pub carried_in: Decimal,
    /// Quanto abateu do imposto do mês.
    pub used: Decimal,
    pub carried_out: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DarfAssessment {
    pub regime: Regime,
    pub code: &'static str,
    /// Imposto do mês já sem o IR retido.
    pub tax: Decimal,
    /// Acumulado de meses abaixo do mínimo.
    pub pending_before: Decimal,
    pub total: Decimal,
    /// Se há DARF a pagar neste mês (total no mínimo ou acima).
    pub payable: bool,
    pub amount: Decimal,
    pub due_date: Date,
    pub pending_after: Decimal,
}

/// Avisos de algo que o Prumo não calcula sozinho.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Notice {
    /// Ganho de capital acima da primeira faixa: a regra de somar alienações
    /// parciais do mesmo bem até o ano seguinte (P&R 574) não é aplicada.
    CapitalGainAboveFirstBracket,
    /// O IR retido do mês saiu da regra (a pessoa não informou o da nota):
    /// confira na nota de corretagem.
    WithholdingByRule,
}

/// Apura todos os meses de `input.since_year` a `until_year`.
pub fn assess(input: &TaxInput<'_>, until_year: i32) -> Result<Assessment, TaxError> {
    if until_year < input.since_year {
        return Err(TaxError::InvalidYearRange {
            first: input.since_year,
            last: until_year,
        });
    }
    let rules: BTreeMap<i32, YearRules> = (input.since_year..=until_year)
        .map(|year| rules_for(year).map(|rules| (year, rules)))
        .collect::<Result<_, _>>()?;
    let until = Date::from_calendar_date(until_year, Month::December, 31).map_err(|_| {
        TaxError::InvalidYearRange {
            first: input.since_year,
            last: until_year,
        }
    })?;
    let replay = positions::replay(input.openings, input.operations, until)?;

    let mut realized: BTreeMap<YearMonth, Vec<&Realized>> = BTreeMap::new();
    for item in &replay.realized {
        realized
            .entry(YearMonth::of(item.date))
            .or_default()
            .push(item);
    }

    let mut state = input.carryforward.clone();
    let mut withholding_carry = Decimal::ZERO;
    let mut withholding_left = BTreeMap::new();
    let mut months = Vec::new();
    for (&year, year_rules) in &rules {
        for month in 1..=12u8 {
            let period = YearMonth { year, month };
            let report = assess_month(
                period,
                year_rules,
                realized.get(&period).map_or(&[][..], Vec::as_slice),
                input.informed_withholding.get(&period).copied(),
                &mut state,
                &mut withholding_carry,
            )?;
            months.push(report);
        }
        withholding_left.insert(year, withholding_carry);
        withholding_carry = Decimal::ZERO;
    }
    Ok(Assessment {
        months,
        closing: state,
        withholding_left,
        holdings: replay.holdings,
    })
}

fn assess_month(
    period: YearMonth,
    rules: &YearRules,
    realized: &[&Realized],
    informed: Option<Decimal>,
    state: &mut Carryforward,
    withholding_carry: &mut Decimal,
) -> Result<MonthReport, TaxError> {
    let mut sales: Vec<InstrumentSales> = Instrument::ALL
        .iter()
        .map(|&instrument| InstrumentSales {
            instrument,
            common_sales: Decimal::ZERO,
            day_trade_sales: Decimal::ZERO,
            common_result: Decimal::ZERO,
            day_trade_result: Decimal::ZERO,
        })
        .collect();
    let mut capital_gains = Decimal::ZERO;
    let mut capital_losses = Decimal::ZERO;
    // Bases da retenção pela regra: vendas comuns em bolsa no mês e o
    // resultado do day trade de cada dia.
    let mut common_exchange_sales = Decimal::ZERO;
    let mut day_trade_by_day: BTreeMap<Date, Decimal> = BTreeMap::new();
    for item in realized {
        let rule = rules.instrument(item.instrument)?;
        if rule.regime == Regime::ExchangeNetGain {
            if item.day_trade {
                *day_trade_by_day.entry(item.date).or_default() += item.result();
            } else {
                common_exchange_sales += item.proceeds;
            }
        }
        if let Some(line) = sales.iter_mut().find(|s| s.instrument == item.instrument) {
            if item.day_trade {
                line.day_trade_sales += item.proceeds;
                line.day_trade_result += item.result();
            } else {
                line.common_sales += item.proceeds;
                line.common_result += item.result();
            }
        }
        if rule.regime == Regime::CapitalGain {
            let result = item.result();
            if result > Decimal::ZERO {
                capital_gains += result;
            } else {
                capital_losses -= result;
            }
        }
    }

    // Isentômetro.
    let mut exemptions = Vec::new();
    for exemption in &rules.exemptions {
        let total: Decimal = sales
            .iter()
            .filter(|line| exemption.counted.contains(&line.instrument))
            .map(|line| {
                if exemption.counts_day_trade.value {
                    line.common_sales + line.day_trade_sales
                } else {
                    line.common_sales
                }
            })
            .sum();
        let limit = exemption.limit.value;
        exemptions.push(ExemptionStatus {
            id: exemption.id,
            sales: cents(total),
            limit,
            remaining: (limit - total).max(Decimal::ZERO),
            exempt: total <= limit,
            exempt_gain: Decimal::ZERO,
        });
    }
    let is_exempt = |id: ExemptionId| exemptions.iter().any(|e| e.id == id && e.exempt);

    // Resultado de cada grupo de compensação (ganho líquido em bolsa).
    let mut pool_results: BTreeMap<Pool, Decimal> = BTreeMap::new();
    let mut exempt_gains: BTreeMap<ExemptionId, Decimal> = BTreeMap::new();
    for line in &sales {
        let rule = rules.instrument(line.instrument)?;
        if rule.regime != Regime::ExchangeNetGain {
            continue;
        }
        let (Some(common_pool), Some(day_trade_pool)) = (rule.common_pool, rule.day_trade_pool)
        else {
            return Err(TaxError::IncompleteRules {
                year: rules.year,
                missing: "grupo de compensacao do instrumento",
            });
        };
        match rule.exemption {
            Some(id) if is_exempt(id) && line.common_result > Decimal::ZERO => {
                *exempt_gains.entry(id).or_default() += line.common_result;
            }
            _ => *pool_results.entry(common_pool).or_default() += line.common_result,
        }
        *pool_results.entry(day_trade_pool).or_default() += line.day_trade_result;
    }

    let mut pools = Vec::new();
    for pool in Pool::ALL {
        let result = cents(pool_results.get(&pool).copied().unwrap_or_default());
        let loss_before = state.losses.get(&pool).copied().unwrap_or_default();
        let (loss_used, base, loss_after) = compensate(result, loss_before);
        let rate = rules.pool(pool)?.rate.value;
        let tax = cents(base * rate);
        state.losses.insert(pool, loss_after);
        pools.push(PoolAssessment {
            pool,
            result,
            loss_before,
            loss_used,
            base,
            rate,
            tax,
            loss_after,
        });
    }

    // Ganho de capital (cripto).
    let capital_gains = cents(capital_gains);
    let crypto_exempt = is_exempt(ExemptionId::CryptoSales);
    if crypto_exempt {
        *exempt_gains.entry(ExemptionId::CryptoSales).or_default() += capital_gains;
    }
    let capital_base = if crypto_exempt {
        Decimal::ZERO
    } else {
        capital_gains
    };
    let capital_gain = CapitalGainAssessment {
        gains: capital_gains,
        losses: cents(capital_losses),
        exempt: crypto_exempt,
        base: capital_base,
        tax: cents(rules.capital_gain_tax(capital_base)),
    };
    let mut notices = Vec::new();
    let first_bracket = rules
        .capital_gain_brackets
        .value
        .first()
        .and_then(|bracket| bracket.up_to);
    if first_bracket.is_some_and(|up_to| capital_base > up_to) {
        notices.push(Notice::CapitalGainAboveFirstBracket);
    }
    let by_rule = withholding_by_rule(rules, common_exchange_sales, &day_trade_by_day);
    if informed.is_none() && by_rule.total() > Decimal::ZERO {
        notices.push(Notice::WithholdingByRule);
    }
    for status in &mut exemptions {
        status.exempt_gain = cents(exempt_gains.get(&status.id).copied().unwrap_or_default());
    }

    // IR retido.
    let exchange_tax: Decimal = pools.iter().map(|pool| pool.tax).sum();
    let carried_in = if rules.withholding_carry.value && period.month > 1 {
        *withholding_carry
    } else {
        Decimal::ZERO
    };
    let withheld = cents(informed.unwrap_or_else(|| by_rule.total()));
    let available = withheld + carried_in;
    let used = available.min(exchange_tax);
    let carried_out = available - used;
    *withholding_carry = carried_out;
    let withholding = WithholdingAssessment {
        common_sales: cents(common_exchange_sales),
        common_rule: by_rule.common,
        day_trade_gains: by_rule.day_trade_gains,
        day_trade_rule: by_rule.day_trade,
        informed,
        withheld,
        carried_in,
        used,
        carried_out,
    };

    // DARF.
    let due_date = calendar::darf_due_date(period)?;
    let minimum = rules.darf_minimum.value;
    let mut darfs = Vec::new();
    for (regime, tax) in [
        (Regime::ExchangeNetGain, exchange_tax - used),
        (Regime::CapitalGain, capital_gain.tax),
    ] {
        let pending_before = state.pending_darf.get(&regime).copied().unwrap_or_default();
        let total = pending_before + tax;
        let payable = total > Decimal::ZERO && total >= minimum;
        let pending_after = if payable { Decimal::ZERO } else { total };
        state.pending_darf.insert(regime, pending_after);
        darfs.push(DarfAssessment {
            regime,
            code: rules.darf_code(regime),
            tax,
            pending_before,
            total,
            payable,
            amount: if payable { total } else { Decimal::ZERO },
            due_date,
            pending_after,
        });
    }

    for line in &mut sales {
        line.common_sales = cents(line.common_sales);
        line.day_trade_sales = cents(line.day_trade_sales);
        line.common_result = cents(line.common_result);
        line.day_trade_result = cents(line.day_trade_result);
    }

    Ok(MonthReport {
        period,
        sales,
        exemptions,
        pools,
        capital_gain,
        withholding,
        darfs,
        notices,
    })
}

/// A retenção que a regra manda a corretora fazer no mês.
struct RuleWithholding {
    common: Decimal,
    day_trade_gains: Decimal,
    day_trade: Decimal,
}

impl RuleWithholding {
    fn total(&self) -> Decimal {
        self.common + self.day_trade
    }
}

/// 0,005% do valor das vendas comuns do mês, dispensado quando a soma fica em
/// R$ 1,00 ou menos (IN RFB 1.585/2015, art. 63, par. 4 e 5); 1% do resultado
/// positivo de cada dia de day trade, com as perdas do mesmo dia já
/// compensadas (art. 65, par. 1, II, e par. 4). Cada valor em centavos.
fn withholding_by_rule(
    rules: &YearRules,
    common_sales: Decimal,
    day_trade_by_day: &BTreeMap<Date, Decimal>,
) -> RuleWithholding {
    let rule = &rules.withholding;
    let common = cents(common_sales * rule.common_rate.value);
    let common = if common <= rule.common_dispensed_up_to.value {
        Decimal::ZERO
    } else {
        common
    };
    let positive: Vec<Decimal> = day_trade_by_day
        .values()
        .copied()
        .filter(|result| *result > Decimal::ZERO)
        .collect();
    RuleWithholding {
        common,
        day_trade_gains: cents(positive.iter().copied().sum()),
        day_trade: positive
            .iter()
            .map(|result| cents(*result * rule.day_trade_rate.value))
            .sum(),
    }
}

/// Compensa o prejuízo acumulado: devolve (prejuízo usado, base, prejuízo que
/// sobra). Prejuízo só abate lucro; o prejuízo do mês soma no acumulado.
fn compensate(result: Decimal, loss_before: Decimal) -> (Decimal, Decimal, Decimal) {
    if result > Decimal::ZERO {
        let used = loss_before.min(result);
        (used, result - used, loss_before - used)
    } else {
        (Decimal::ZERO, Decimal::ZERO, loss_before - result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tax::positions::tests::{buy, sell, with_fees};
    use crate::domain::tax::positions::Action;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn input(operations: &[Operation]) -> TaxInput<'_> {
        TaxInput {
            openings: &[],
            operations,
            since_year: 2025,
            carryforward: Carryforward::default(),
            informed_withholding: BTreeMap::new(),
        }
    }

    /// O mesmo livro, com o IR retido informado por mês.
    fn input_with<'a>(
        operations: &'a [Operation],
        informed: &[(i32, u8, Decimal)],
    ) -> TaxInput<'a> {
        TaxInput {
            informed_withholding: informed
                .iter()
                .map(|&(year, month, amount)| (YearMonth { year, month }, amount))
                .collect(),
            ..input(operations)
        }
    }

    fn month(assessment: &Assessment, year: i32, month: u8) -> &MonthReport {
        assessment
            .months
            .iter()
            .find(|m| m.period == YearMonth { year, month })
            .unwrap()
    }

    fn pool(report: &MonthReport, pool: Pool) -> &PoolAssessment {
        report.pools.iter().find(|p| p.pool == pool).unwrap()
    }

    fn darf(report: &MonthReport, regime: Regime) -> &DarfAssessment {
        report.darfs.iter().find(|d| d.regime == regime).unwrap()
    }

    fn exemption(report: &MonthReport, id: ExemptionId) -> &ExemptionStatus {
        report.exemptions.iter().find(|e| e.id == id).unwrap()
    }

    use Instrument::{AgribusinessFund, Bdr, Crypto, Etf, RealEstateFund, Stock};

    #[test]
    fn forward_buyer_example_is_exempt_under_20k() {
        // P&R 723, item 1: compra a termo de 1.000 ações K a R$ 10 e venda à
        // vista por R$ 12.000: ganho líquido de R$ 2.000. A venda à vista fica
        // abaixo de R$ 20 mil, então o ganho é isento (P&R 707).
        let operations = [
            buy(date!(2025 - 03 - 10), "KKKK3", Stock, dec!(1000), dec!(10)),
            sell(date!(2025 - 04 - 09), "KKKK3", Stock, dec!(1000), dec!(12)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let april = month(&assessment, 2025, 4);
        let stocks = exemption(april, ExemptionId::StockSales);
        assert_eq!(stocks.sales, dec!(12000));
        assert!(stocks.exempt);
        assert_eq!(stocks.remaining, dec!(8000));
        assert_eq!(stocks.exempt_gain, dec!(2000));
        assert_eq!(pool(april, Pool::Common).tax, dec!(0));
        assert!(!darf(april, Regime::ExchangeNetGain).payable);
    }

    #[test]
    fn call_exercise_example_pays_15_percent() {
        // P&R 725, item 1: custo de R$ 110.000 (exercício + prêmio) e venda à
        // vista de R$ 130.000: ganho líquido de R$ 20.000. Acima de R$ 20 mil
        // de vendas, 15% (P&R 706): R$ 3.000. A corretora retém 0,005% da
        // venda (P&R 706): R$ 6,50, que abatem o DARF 6015 (P&R 730).
        let operations = [
            buy(date!(2025 - 05 - 12), "KKKK3", Stock, dec!(10000), dec!(11)),
            sell(
                date!(2025 - 05 - 12) + time::Duration::days(1),
                "KKKK3",
                Stock,
                dec!(10000),
                dec!(13),
            ),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let may = month(&assessment, 2025, 5);
        assert!(!exemption(may, ExemptionId::StockSales).exempt);
        let common = pool(may, Pool::Common);
        assert_eq!(common.result, dec!(20000));
        assert_eq!(common.tax, dec!(3000));
        assert_eq!(may.withholding.common_sales, dec!(130000));
        assert_eq!(may.withholding.common_rule, dec!(6.50));
        assert_eq!(may.withholding.withheld, dec!(6.50));
        assert_eq!(may.notices, vec![Notice::WithholdingByRule]);
        let darf = darf(may, Regime::ExchangeNetGain);
        assert!(darf.payable);
        assert_eq!(darf.code, "6015");
        assert_eq!(darf.amount, dec!(2993.50));
        assert_eq!(darf.due_date, date!(2025 - 06 - 30));
    }

    #[test]
    fn exactly_20k_is_exempt_and_one_cent_more_is_not() {
        // "não exceder a R$ 20.000,00" (Lei 11.033/2004, art. 3, I).
        let at_limit = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(15)),
            sell(date!(2025 - 02 - 03), "AAAA3", Stock, dec!(1000), dec!(20)),
        ];
        let assessment = assess(&input(&at_limit), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        assert!(exemption(february, ExemptionId::StockSales).exempt);
        assert_eq!(pool(february, Pool::Common).tax, dec!(0));

        let over = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(15)),
            sell(
                date!(2025 - 02 - 03),
                "AAAA3",
                Stock,
                dec!(1000),
                dec!(20.00001),
            ),
        ];
        let assessment = assess(&input(&over), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        assert!(!exemption(february, ExemptionId::StockSales).exempt);
        assert_eq!(pool(february, Pool::Common).tax, dec!(750));
    }

    #[test]
    fn etf_and_bdr_have_no_exemption() {
        // P&R 707 (fundo de índice) e IN RFB 1.585/2015, art. 56, par. 1, I, a
        // (BDR): 15% mesmo com vendas pequenas.
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            buy(date!(2025 - 01 - 06), "BBBB34", Bdr, dec!(100), dec!(50)),
            sell(date!(2025 - 02 - 03), "EEEE11", Etf, dec!(50), dec!(120)),
            sell(date!(2025 - 02 - 03), "BBBB34", Bdr, dec!(50), dec!(60)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        let common = pool(february, Pool::Common);
        assert_eq!(common.result, dec!(1000) + dec!(500));
        assert_eq!(common.tax, dec!(225));
        // As vendas de ETF e BDR não somam no isentômetro de ações.
        assert_eq!(exemption(february, ExemptionId::StockSales).sales, dec!(0));
    }

    #[test]
    fn loss_in_an_exempt_month_offsets_a_later_gain() {
        // P&R 729 e IN RFB 1.585/2015, art. 59, par. 1: perda de mês isento é
        // compensável; P&R 709: compensa nos meses seguintes.
        let operations = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(20)),
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            // Fevereiro: vende R$ 15 mil de ações com prejuízo de R$ 5 mil.
            sell(date!(2025 - 02 - 03), "AAAA3", Stock, dec!(1000), dec!(15)),
            // Março: lucro de R$ 8 mil em ETF.
            sell(date!(2025 - 03 - 03), "EEEE11", Etf, dec!(100), dec!(180)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        assert!(exemption(february, ExemptionId::StockSales).exempt);
        assert_eq!(pool(february, Pool::Common).loss_after, dec!(5000));
        let march = pool(month(&assessment, 2025, 3), Pool::Common);
        assert_eq!(march.loss_before, dec!(5000));
        assert_eq!(march.loss_used, dec!(5000));
        assert_eq!(march.base, dec!(3000));
        assert_eq!(march.tax, dec!(450));
    }

    #[test]
    fn exempt_gain_does_not_consume_the_accumulated_loss() {
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(10)),
            sell(date!(2025 - 01 - 20), "EEEE11", Etf, dec!(100), dec!(90)),
            sell(date!(2025 - 02 - 03), "AAAA3", Stock, dec!(1000), dec!(12)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        assert_eq!(
            exemption(february, ExemptionId::StockSales).exempt_gain,
            dec!(2000)
        );
        let common = pool(february, Pool::Common);
        assert_eq!(common.loss_before, dec!(1000));
        assert_eq!(common.loss_used, dec!(0));
        assert_eq!(common.loss_after, dec!(1000));
    }

    #[test]
    fn day_trade_and_common_in_the_same_month_are_separate() {
        // P&R 709, atenção: perda de day trade só compensa com day trade, e
        // perda comum só com operação comum. P&R 707: day trade não é isento.
        let operations = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(10)),
            // Day trade com prejuízo de R$ 500.
            buy(date!(2025 - 03 - 10), "BBBB3", Stock, dec!(500), dec!(10)),
            sell(date!(2025 - 03 - 10), "BBBB3", Stock, dec!(500), dec!(9)),
            // Operação comum com lucro de R$ 2.000 e vendas de R$ 12 mil.
            sell(date!(2025 - 03 - 20), "AAAA3", Stock, dec!(1000), dec!(12)),
            // Abril: day trade com lucro de R$ 800.
            buy(date!(2025 - 04 - 10), "BBBB3", Stock, dec!(400), dec!(10)),
            sell(date!(2025 - 04 - 10), "BBBB3", Stock, dec!(400), dec!(12)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let march = month(&assessment, 2025, 3);
        let stocks = exemption(march, ExemptionId::StockSales);
        // Vendas: R$ 4.500 em day trade + R$ 12.000 comuns = R$ 16.500.
        assert_eq!(stocks.sales, dec!(16500));
        assert!(stocks.exempt);
        assert_eq!(stocks.exempt_gain, dec!(2000));
        assert_eq!(pool(march, Pool::DayTrade).loss_after, dec!(500));
        assert_eq!(pool(march, Pool::Common).loss_after, dec!(0));
        let april = pool(month(&assessment, 2025, 4), Pool::DayTrade);
        assert_eq!(april.result, dec!(800));
        assert_eq!(april.loss_used, dec!(500));
        assert_eq!(april.tax, dec!(60));
    }

    #[test]
    fn day_trade_sales_count_toward_the_20k_limit() {
        // Leitura conservadora do art. 59, I (ver docs/ir.md, dúvidas): R$ 15
        // mil comuns + R$ 6 mil em day trade passam do limite.
        let operations = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(10)),
            buy(date!(2025 - 03 - 10), "BBBB3", Stock, dec!(600), dec!(10)),
            sell(date!(2025 - 03 - 10), "BBBB3", Stock, dec!(600), dec!(10)),
            sell(date!(2025 - 03 - 20), "AAAA3", Stock, dec!(1000), dec!(15)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let march = month(&assessment, 2025, 3);
        assert!(!exemption(march, ExemptionId::StockSales).exempt);
        assert_eq!(pool(march, Pool::Common).tax, dec!(750));
    }

    #[test]
    fn december_loss_offsets_next_year_gain() {
        // P&R 711: prejuízo de dezembro compensa em qualquer mês seguinte.
        let operations = [
            buy(date!(2025 - 11 - 03), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(date!(2025 - 12 - 15), "EEEE11", Etf, dec!(50), dec!(80)),
            sell(date!(2026 - 02 - 10), "EEEE11", Etf, dec!(50), dec!(130)),
        ];
        let assessment = assess(&input(&operations), 2026).unwrap();
        assert_eq!(
            pool(month(&assessment, 2025, 12), Pool::Common).loss_after,
            dec!(1000)
        );
        let february = pool(month(&assessment, 2026, 2), Pool::Common);
        assert_eq!(february.loss_before, dec!(1000));
        assert_eq!(february.base, dec!(500));
        assert_eq!(february.tax, dec!(75));
        assert_eq!(assessment.closing.losses[&Pool::Common], dec!(0));
    }

    #[test]
    fn real_estate_funds_pay_20_percent_in_their_own_pool() {
        // IN RFB 1.585/2015, art. 37, caput e par. 2: 20%, e perda de FII só
        // compensa com fundo da mesma espécie. Fiagro fica em grupo próprio.
        let operations = [
            buy(
                date!(2025 - 01 - 06),
                "FFFF11",
                RealEstateFund,
                dec!(100),
                dec!(100),
            ),
            buy(
                date!(2025 - 01 - 06),
                "GGGG11",
                AgribusinessFund,
                dec!(100),
                dec!(10),
            ),
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(
                date!(2025 - 02 - 03),
                "FFFF11",
                RealEstateFund,
                dec!(10),
                dec!(110),
            ),
            sell(
                date!(2025 - 02 - 03),
                "GGGG11",
                AgribusinessFund,
                dec!(100),
                dec!(9),
            ),
            sell(date!(2025 - 02 - 03), "EEEE11", Etf, dec!(100), dec!(90)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        let fii = pool(february, Pool::RealEstateFund);
        assert_eq!(fii.result, dec!(100));
        assert_eq!(fii.tax, dec!(20));
        assert_eq!(pool(february, Pool::AgribusinessFund).loss_after, dec!(100));
        assert_eq!(pool(february, Pool::Common).loss_after, dec!(1000));
        assert_eq!(darf(february, Regime::ExchangeNetGain).amount, dec!(20));
    }

    #[test]
    fn darf_below_10_accumulates_until_it_reaches_the_minimum() {
        // Lei 9.430/1996, art. 68, par. 1: soma ao do mesmo código nos meses
        // seguintes e vence no prazo do último.
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(date!(2025 - 01 - 20), "EEEE11", Etf, dec!(10), dec!(104)), // lucro 40, IR 6
            sell(date!(2025 - 02 - 20), "EEEE11", Etf, dec!(10), dec!(104)), // lucro 40, IR 6
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let january = darf(month(&assessment, 2025, 1), Regime::ExchangeNetGain);
        assert_eq!(january.tax, dec!(6));
        assert!(!january.payable);
        assert_eq!(january.pending_after, dec!(6));
        let february = darf(month(&assessment, 2025, 2), Regime::ExchangeNetGain);
        assert_eq!(february.pending_before, dec!(6));
        assert!(february.payable);
        assert_eq!(february.amount, dec!(12));
        assert_eq!(february.due_date, date!(2025 - 03 - 31));
        assert_eq!(february.pending_after, dec!(0));
    }

    #[test]
    fn pending_darf_crosses_the_year() {
        let operations = [
            buy(date!(2025 - 11 - 03), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(date!(2025 - 12 - 10), "EEEE11", Etf, dec!(10), dec!(103)), // IR 4,50
            sell(date!(2026 - 01 - 12), "EEEE11", Etf, dec!(10), dec!(104)), // IR 6
        ];
        let assessment = assess(&input(&operations), 2026).unwrap();
        let december = darf(month(&assessment, 2025, 12), Regime::ExchangeNetGain);
        assert_eq!(december.pending_after, dec!(4.50));
        let january = darf(month(&assessment, 2026, 1), Regime::ExchangeNetGain);
        assert_eq!(january.amount, dec!(10.50));
        assert_eq!(january.due_date, date!(2026 - 02 - 27));
    }

    #[test]
    fn pending_darf_from_before_the_first_year_is_honoured() {
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(date!(2025 - 03 - 10), "EEEE11", Etf, dec!(10), dec!(104)),
        ];
        let mut tax_input = input(&operations);
        tax_input
            .carryforward
            .pending_darf
            .insert(Regime::ExchangeNetGain, dec!(5));
        tax_input
            .carryforward
            .losses
            .insert(Pool::DayTrade, dec!(300));
        let assessment = assess(&tax_input, 2025).unwrap();
        assert!(
            darf(month(&assessment, 2025, 1), Regime::ExchangeNetGain).pending_after == dec!(5)
        );
        assert_eq!(
            darf(month(&assessment, 2025, 3), Regime::ExchangeNetGain).amount,
            dec!(11)
        );
        assert_eq!(assessment.closing.losses[&Pool::DayTrade], dec!(300));
    }

    #[test]
    fn withholding_is_deducted_and_carried_only_within_the_year() {
        // Lei 11.033/2004, art. 2, par. 7; P&R 714 e 715. O retido de cada mês
        // é o que a pessoa leu na nota.
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(1000), dec!(100)),
            // Novembro: prejuízo, mas IR retido de R$ 3.
            sell(date!(2025 - 11 - 10), "EEEE11", Etf, dec!(100), dec!(90)),
            // Dezembro: lucro de R$ 2.000 menos R$ 1.000 de prejuízo = base de
            // R$ 1.000, IR de R$ 150, menos R$ 2 retidos e R$ 3 que sobraram.
            sell(date!(2025 - 12 - 10), "EEEE11", Etf, dec!(100), dec!(120)),
            // Janeiro de 2026: retido de R$ 4 sem imposto; sobra fica no ano.
            sell(date!(2026 - 01 - 12), "EEEE11", Etf, dec!(100), dec!(90)),
        ];
        let informed = [(2025, 11, dec!(3)), (2025, 12, dec!(2)), (2026, 1, dec!(4))];
        let assessment = assess(&input_with(&operations, &informed), 2026).unwrap();
        let november = &month(&assessment, 2025, 11).withholding;
        assert_eq!(november.informed, Some(dec!(3)));
        assert_eq!(november.carried_out, dec!(3));
        let december = month(&assessment, 2025, 12);
        assert_eq!(december.withholding.carried_in, dec!(3));
        assert_eq!(december.withholding.used, dec!(5));
        assert_eq!(pool(december, Pool::Common).base, dec!(1000));
        assert_eq!(darf(december, Regime::ExchangeNetGain).amount, dec!(145));
        assert!(december.notices.is_empty());
        assert_eq!(assessment.withholding_left[&2025], dec!(0));
        let january = &month(&assessment, 2026, 1).withholding;
        assert_eq!(january.carried_in, dec!(0));
        assert_eq!(assessment.withholding_left[&2026], dec!(4));
    }

    #[test]
    fn informed_withholding_replaces_the_rule() {
        // R$ 12 mil vendidos: pela regra, R$ 0,60, dispensados (até R$ 1). A
        // nota diz que a corretora reteve R$ 0,60: vale a nota.
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(1000), dec!(100)),
            sell(date!(2025 - 03 - 10), "EEEE11", Etf, dec!(100), dec!(120)),
        ];
        let by_rule = assess(&input(&operations), 2025).unwrap();
        let march = month(&by_rule, 2025, 3);
        assert_eq!(march.withholding.common_rule, dec!(0));
        assert_eq!(march.withholding.withheld, dec!(0));
        assert_eq!(darf(march, Regime::ExchangeNetGain).amount, dec!(300));

        let informed = assess(&input_with(&operations, &[(2025, 3, dec!(0.60))]), 2025).unwrap();
        let march = month(&informed, 2025, 3);
        assert_eq!(march.withholding.informed, Some(dec!(0.60)));
        assert_eq!(march.withholding.used, dec!(0.60));
        assert_eq!(darf(march, Regime::ExchangeNetGain).amount, dec!(299.40));
        assert_eq!(informed.withholding_left[&2025], dec!(0));
    }

    #[test]
    fn common_withholding_is_dispensed_up_to_one_real_in_the_month() {
        // Lei 11.033/2004, art. 2, par. 1, IV, 4 e 5; IN RFB 1.585/2015, art.
        // 63, par. 4 e 5: 0,005% do valor vendido, somadas as vendas do mês;
        // R$ 1,00 ou menos não se retém. Day trade e cripto ficam fora.
        let operations = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(10000), dec!(1)),
            buy(
                date!(2025 - 01 - 06),
                "FFFF11",
                RealEstateFund,
                dec!(1000),
                dec!(100),
            ),
            buy(date!(2025 - 01 - 06), "BTC", Crypto, dec!(1), dec!(100000)),
            // Fevereiro: R$ 20 mil vendidos dão R$ 1,00, dispensado.
            sell(date!(2025 - 02 - 03), "AAAA3", Stock, dec!(10000), dec!(2)),
            // Março: duas vendas de R$ 15 mil (ação e FII) somam R$ 1,50; o
            // day trade e a cripto não entram na base.
            sell(
                date!(2025 - 03 - 03),
                "FFFF11",
                RealEstateFund,
                dec!(150),
                dec!(100),
            ),
            buy(date!(2025 - 03 - 10), "AAAA3", Stock, dec!(15000), dec!(1)),
            sell(date!(2025 - 03 - 10), "AAAA3", Stock, dec!(15000), dec!(1)),
            buy(date!(2025 - 03 - 20), "AAAA3", Stock, dec!(15000), dec!(1)),
            sell(date!(2025 - 03 - 21), "AAAA3", Stock, dec!(15000), dec!(1)),
            sell(
                date!(2025 - 03 - 21),
                "BTC",
                Crypto,
                dec!(0.5),
                dec!(100000),
            ),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = &month(&assessment, 2025, 2).withholding;
        assert_eq!(february.common_sales, dec!(20000));
        assert_eq!(february.common_rule, dec!(0));
        assert!(month(&assessment, 2025, 2).notices.is_empty());
        let march = &month(&assessment, 2025, 3).withholding;
        assert_eq!(march.common_sales, dec!(30000));
        assert_eq!(march.common_rule, dec!(1.50));
        assert_eq!(march.day_trade_rule, dec!(0));
        assert_eq!(march.withheld, dec!(1.50));
        // Sem imposto no mês, o retido passa para abril.
        assert_eq!(march.used, dec!(0));
        assert_eq!(march.carried_out, dec!(1.50));
    }

    #[test]
    fn day_trade_withholding_is_one_percent_of_each_days_net_gain() {
        // P&R 705 e 712; IN RFB 1.585/2015, art. 65, par. 1, II, e par. 4: 1%
        // do resultado positivo do dia, com a perda de outro ativo no mesmo dia
        // compensada; dia de prejuízo não retém nem desconta de outro dia.
        let (first, second) = (date!(2025 - 06 - 02), date!(2025 - 06 - 03));
        let operations = [
            buy(first, "BBBB3", Stock, dec!(1000), dec!(10)),
            sell(first, "BBBB3", Stock, dec!(1000), dec!(10.5)),
            buy(first, "CCCC3", Stock, dec!(100), dec!(10)),
            sell(first, "CCCC3", Stock, dec!(100), dec!(8)),
            buy(second, "BBBB3", Stock, dec!(100), dec!(10)),
            sell(second, "BBBB3", Stock, dec!(100), dec!(9)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let june = month(&assessment, 2025, 6);
        // Dia 2: 500 − 200 = 300 → R$ 3,00. Dia 3: −100, nada.
        assert_eq!(june.withholding.day_trade_gains, dec!(300));
        assert_eq!(june.withholding.day_trade_rule, dec!(3));
        assert_eq!(june.withholding.common_rule, dec!(0));
        let day_trade = pool(june, Pool::DayTrade);
        assert_eq!(day_trade.result, dec!(200));
        assert_eq!(day_trade.tax, dec!(40));
        assert_eq!(darf(june, Regime::ExchangeNetGain).amount, dec!(37));
    }

    #[test]
    fn day_trade_withholding_offsets_day_trade_tax() {
        // P&R 712 e 714: 1% retido no day trade abate o imposto do mês.
        let day = date!(2025 - 06 - 02);
        let operations = [
            buy(day, "BBBB3", Stock, dec!(1000), dec!(10)),
            sell(day, "BBBB3", Stock, dec!(1000), dec!(11)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let june = month(&assessment, 2025, 6);
        assert_eq!(june.withholding.day_trade_rule, dec!(10));
        assert_eq!(pool(june, Pool::DayTrade).tax, dec!(200));
        assert_eq!(darf(june, Regime::ExchangeNetGain).amount, dec!(190));
        assert_eq!(
            darf(june, Regime::ExchangeNetGain).due_date,
            date!(2025 - 07 - 31)
        );
    }

    #[test]
    fn crypto_is_exempt_up_to_35k_for_the_whole_set() {
        // P&R 653, item 2: o limite vale para o conjunto de criptoativos.
        let operations = [
            buy(date!(2025 - 01 - 06), "BTC", Crypto, dec!(1), dec!(300000)),
            buy(date!(2025 - 01 - 06), "ETH", Crypto, dec!(10), dec!(10000)),
            sell(
                date!(2025 - 02 - 03),
                "BTC",
                Crypto,
                dec!(0.1),
                dec!(350000),
            ), // 35.000, lucro 5.000
            sell(
                date!(2025 - 03 - 03),
                "BTC",
                Crypto,
                dec!(0.1),
                dec!(350000),
            ), // 35.000
            sell(date!(2025 - 03 - 03), "ETH", Crypto, dec!(1), dec!(9000)), // 9.000, perda 1.000
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        let crypto = exemption(february, ExemptionId::CryptoSales);
        assert_eq!(crypto.sales, dec!(35000));
        assert!(crypto.exempt);
        assert_eq!(crypto.exempt_gain, dec!(5000));
        assert_eq!(february.capital_gain.tax, dec!(0));

        let march = month(&assessment, 2025, 3);
        assert!(!exemption(march, ExemptionId::CryptoSales).exempt);
        // P&R 583: a perda do ETH não abate o lucro do BTC.
        assert_eq!(march.capital_gain.gains, dec!(5000));
        assert_eq!(march.capital_gain.losses, dec!(1000));
        assert_eq!(march.capital_gain.tax, dec!(750));
        let darf = darf(march, Regime::CapitalGain);
        assert_eq!(darf.code, "4600");
        assert_eq!(darf.amount, dec!(750));
        assert_eq!(darf.due_date, date!(2025 - 04 - 30));
        assert!(march.notices.is_empty());
    }

    #[test]
    fn huge_crypto_gain_raises_a_notice() {
        let operations = [
            buy(
                date!(2025 - 01 - 06),
                "BTC",
                Crypto,
                dec!(100),
                dec!(100000),
            ),
            sell(
                date!(2025 - 02 - 03),
                "BTC",
                Crypto,
                dec!(100),
                dec!(160000),
            ),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        let february = month(&assessment, 2025, 2);
        assert_eq!(february.notices, vec![Notice::CapitalGainAboveFirstBracket]);
        assert_eq!(february.capital_gain.tax, dec!(750000) + dec!(175000));
    }

    #[test]
    fn fees_reduce_the_gain() {
        // P&R 708: despesas somam na compra e saem da venda.
        let operations = [
            with_fees(
                buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
                dec!(10),
            ),
            with_fees(
                sell(date!(2025 - 02 - 03), "EEEE11", Etf, dec!(100), dec!(110)),
                dec!(10),
            ),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        assert_eq!(
            pool(month(&assessment, 2025, 2), Pool::Common).result,
            dec!(980)
        );
    }

    #[test]
    fn year_without_rules_is_an_explicit_error() {
        let operations = [buy(date!(2024 - 01 - 06), "AAAA3", Stock, dec!(1), dec!(1))];
        let mut tax_input = input(&operations);
        tax_input.since_year = 2024;
        assert_eq!(
            assess(&tax_input, 2025),
            Err(TaxError::NoRulesForYear(2024))
        );
        assert_eq!(
            assess(&input(&operations), 2027),
            Err(TaxError::NoRulesForYear(2027))
        );
        assert_eq!(
            assess(&input(&operations), 2024),
            Err(TaxError::InvalidYearRange {
                first: 2025,
                last: 2024
            })
        );
    }

    #[test]
    fn earlier_operations_only_build_the_average_price() {
        // Venda em 2024 (sem regra) não é apurada; só forma a posição.
        let operations = [
            buy(date!(2024 - 03 - 04), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(date!(2024 - 06 - 03), "EEEE11", Etf, dec!(50), dec!(150)),
            sell(date!(2025 - 02 - 03), "EEEE11", Etf, dec!(50), dec!(120)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        assert_eq!(assessment.months.len(), 12);
        assert_eq!(
            pool(month(&assessment, 2025, 2), Pool::Common).result,
            dec!(1000)
        );
        assert!(assessment.holdings.is_empty());
    }

    #[test]
    fn every_instrument_is_listed_every_month() {
        let assessment = assess(&input(&[]), 2025).unwrap();
        for report in &assessment.months {
            assert_eq!(report.sales.len(), Instrument::ALL.len());
            assert_eq!(report.pools.len(), Pool::ALL.len());
            assert_eq!(report.darfs.len(), 2);
        }
    }

    // --- Propriedades ----------------------------------------------------

    const SYMBOLS: [(&str, Instrument); 6] = [
        ("AAAA3", Stock),
        ("EEEE11", Etf),
        ("BBBB34", Bdr),
        ("FFFF11", RealEstateFund),
        ("GGGG11", AgribusinessFund),
        ("BTC", Crypto),
    ];

    /// (dia desde 1/1/2025, ativo, compra?, quantidade, preço, taxa, retido
    /// informado no mês; zero = pela regra)
    type Step = (i64, usize, bool, u32, u32, u32, u32);

    fn steps() -> impl Strategy<Value = Vec<Step>> {
        prop::collection::vec(
            (
                0i64..730,
                0usize..6,
                any::<bool>(),
                1u32..400,
                1u32..20_000,
                0u32..500,
                0u32..300,
            ),
            0..60,
        )
    }

    /// Monta lançamentos válidos (venda nunca passa da posição) e o IR retido
    /// informado em alguns meses.
    fn build(mut steps: Vec<Step>) -> (Vec<Operation>, Vec<(i32, u8, Decimal)>) {
        steps.sort_by_key(|step| step.0);
        let mut held = [Decimal::ZERO; 6];
        let mut operations = Vec::new();
        let mut informed: BTreeMap<YearMonth, Decimal> = BTreeMap::new();
        for (day, asset, is_buy, quantity, price, fees, withheld) in steps {
            let (symbol, instrument) = SYMBOLS[asset];
            let date = date!(2025 - 01 - 01) + time::Duration::days(day);
            let price = Decimal::new(i64::from(price), 2);
            let fees = Decimal::new(i64::from(fees), 2);
            let quantity = Decimal::from(quantity);
            if is_buy {
                held[asset] += quantity;
                operations.push(with_fees(
                    buy(date, symbol, instrument, quantity, price),
                    fees,
                ));
            } else {
                let quantity = quantity.min(held[asset]);
                if quantity.is_zero() {
                    continue;
                }
                held[asset] -= quantity;
                if instrument != Crypto && withheld > 0 {
                    *informed.entry(YearMonth::of(date)).or_default() +=
                        Decimal::new(i64::from(withheld), 2);
                }
                operations.push(with_fees(
                    sell(date, symbol, instrument, quantity, price),
                    fees,
                ));
            }
        }
        let informed = informed
            .into_iter()
            .map(|(period, amount)| (period.year, period.month, amount))
            .collect();
        (operations, informed)
    }

    proptest! {
        #[test]
        fn tax_is_never_negative_and_loss_only_offsets_gain(steps in steps()) {
            let (operations, informed) = build(steps);
            let assessment = assess(&input_with(&operations, &informed), 2026).unwrap();
            for report in &assessment.months {
                for pool in &report.pools {
                    prop_assert!(pool.tax >= Decimal::ZERO);
                    prop_assert!(pool.base >= Decimal::ZERO);
                    prop_assert!(pool.loss_after >= Decimal::ZERO);
                    prop_assert!(pool.loss_used <= pool.loss_before);
                    prop_assert!(pool.loss_used <= pool.result.max(Decimal::ZERO));
                    prop_assert_eq!(pool.base, pool.result.max(Decimal::ZERO) - pool.loss_used);
                }
                prop_assert!(report.capital_gain.tax >= Decimal::ZERO);
                prop_assert!(report.withholding.used >= Decimal::ZERO);
                prop_assert!(report.withholding.carried_out >= Decimal::ZERO);
                for darf in &report.darfs {
                    prop_assert!(darf.tax >= Decimal::ZERO);
                    prop_assert!(darf.amount >= Decimal::ZERO);
                    prop_assert!(darf.pending_after >= Decimal::ZERO);
                    prop_assert!(darf.pending_after < dec!(10));
                    if darf.payable {
                        prop_assert!(darf.amount >= dec!(10));
                    }
                }
            }
        }

        #[test]
        fn every_exchange_result_is_taxed_or_exempt(steps in steps()) {
            let (operations, informed) = build(steps);
            let assessment = assess(&input_with(&operations, &informed), 2026).unwrap();
            for report in &assessment.months {
                let realized: Decimal = report
                    .sales
                    .iter()
                    .filter(|line| line.instrument != Crypto)
                    .map(|line| line.common_result + line.day_trade_result)
                    .sum();
                let pooled: Decimal = report.pools.iter().map(|pool| pool.result).sum();
                let exempt = exemption(report, ExemptionId::StockSales).exempt_gain;
                let difference = (realized - pooled - exempt).abs();
                // Cada linha é arredondada a centavos em separado.
                prop_assert!(difference <= dec!(0.10), "{} != {} + {}", realized, pooled, exempt);
            }
        }

        #[test]
        fn exempt_month_has_no_common_stock_darf(
            sales in prop::collection::vec((1i64..600, 1u32..200, 1u32..10_000), 1..30)
        ) {
            // Só ações, compra única em 2/1/2025, vendas depois, cada mês com
            // no máximo R$ 20 mil vendidos.
            let mut operations = vec![buy(date!(2025 - 01 - 02), "AAAA3", Stock, dec!(100000), dec!(1))];
            let mut per_month: BTreeMap<YearMonth, Decimal> = BTreeMap::new();
            let mut sorted = sales;
            sorted.sort_by_key(|sale| sale.0);
            for (day, quantity, price) in sorted {
                let date = date!(2025 - 01 - 02) + time::Duration::days(day);
                let quantity = Decimal::from(quantity);
                let price = Decimal::new(i64::from(price), 2);
                let total = per_month.entry(YearMonth::of(date)).or_default();
                if *total + quantity * price > dec!(20000) {
                    continue;
                }
                *total += quantity * price;
                operations.push(sell(date, "AAAA3", Stock, quantity, price));
            }
            let assessment = assess(&input(&operations), 2026).unwrap();
            for report in &assessment.months {
                prop_assert!(exemption(report, ExemptionId::StockSales).exempt);
                prop_assert_eq!(pool(report, Pool::Common).tax, Decimal::ZERO);
                prop_assert!(!darf(report, Regime::ExchangeNetGain).payable);
            }
        }
    }

    #[test]
    fn events_are_ignored_by_the_monthly_buckets() {
        let operations = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(100), dec!(10)),
            Operation {
                date: date!(2025 - 02 - 03),
                symbol: "AAAA3".to_owned(),
                instrument: Stock,
                broker: None,
                action: Action::Split { factor: dec!(2) },
            },
            sell(date!(2025 - 03 - 03), "AAAA3", Stock, dec!(200), dec!(6)),
        ];
        let assessment = assess(&input(&operations), 2025).unwrap();
        // 200 × 6 − 1.000 = 200 de lucro, isento.
        assert_eq!(
            exemption(month(&assessment, 2025, 3), ExemptionId::StockSales).exempt_gain,
            dec!(200)
        );
    }
}
