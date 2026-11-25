//! O que vai para a declaração anual (IRPF) de um ano-calendário:
//!
//! - **Bens e Direitos**: posição em 31/12 por ativo, pelo custo de aquisição
//!   (preço médio da Receita), com grupo e código da ficha;
//! - **Rendimentos isentos**: dividendos, rendimentos de FII e Fiagro, lucro
//!   isento de ações (vendas até R$ 20 mil no mês) e de cripto (até R$ 35 mil);
//! - **Tributação exclusiva**: JCP;
//! - **Renda variável**: os meses apurados, o prejuízo que fica para o ano
//!   seguinte e o IR retido que sobrou.
//!
//! Os proventos entram como dado (o valor bruto e o retido que a empresa
//! informa); o Prumo não deduz a retenção, só soma.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::Date;

use super::monthly::{assess, MonthReport, TaxInput};
use super::rules::{rules_for, ExemptionId, IrpfCode, Pool, Regime, Sourced};
use super::{cents, Instrument, TaxError};

/// Tipo de provento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeKind {
    /// Dividendo (isento; a partir de 2026 pode vir com retenção de 10%).
    Dividend,
    /// Juros sobre capital próprio (tributação exclusiva na fonte).
    InterestOnEquity,
    /// Rendimento distribuído por FII ou Fiagro (isento nas condições da lei).
    FundIncome,
}

/// Um provento recebido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Income {
    pub pay_date: Date,
    /// Quem pagou (o ativo).
    pub symbol: String,
    pub kind: IncomeKind,
    pub gross: Decimal,
    pub withheld: Decimal,
}

/// Uma linha da ficha Bens e Direitos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetLine {
    pub symbol: String,
    pub instrument: Instrument,
    pub quantity: Decimal,
    /// "Situação em 31/12": custo de aquisição, não valor de mercado.
    pub total_cost: Decimal,
    pub average_price: Decimal,
    pub code: IrpfCode,
    pub code_source: &'static str,
}

/// Quanto cada pagador somou numa linha de rendimento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PayerAmount {
    pub symbol: String,
    pub gross: Decimal,
    pub withheld: Decimal,
}

/// Uma linha das fichas de rendimentos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncomeLine {
    pub code: IrpfCode,
    pub code_source: &'static str,
    pub gross: Decimal,
    pub withheld: Decimal,
    /// Bruto menos retido (o valor líquido do informe).
    pub net: Decimal,
    pub by_payer: Vec<PayerAmount>,
}

/// A declaração do ano, do ponto de vista da carteira.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnnualReport {
    pub year: i32,
    pub assets: Vec<AssetLine>,
    /// Dividendos, rendimentos de fundos, lucro isento de ações e de cripto.
    pub exempt_income: Vec<IncomeLine>,
    /// JCP.
    pub exclusive_income: Vec<IncomeLine>,
    pub months: Vec<MonthReport>,
    /// Prejuízo a compensar em 31/12, por grupo (vai para o ano seguinte).
    pub losses_at_year_end: BTreeMap<Pool, Decimal>,
    /// Imposto abaixo do mínimo ainda não pago em 31/12, por regime.
    pub pending_darf_at_year_end: BTreeMap<Regime, Decimal>,
    /// IR retido em bolsa que sobrou no ano (compensa na declaração ou pede
    /// restituição).
    pub withholding_left: Decimal,
    /// Soma dos DARFs a pagar nos meses do ano.
    pub darf_total: Decimal,
}

/// Fecha o ano-calendário `year`, apurando desde `input.since_year`.
pub fn annual_report(
    input: &TaxInput<'_>,
    incomes: &[Income],
    year: i32,
) -> Result<AnnualReport, TaxError> {
    let rules = rules_for(year)?;
    let assessment = assess(input, year)?;

    let mut assets = Vec::new();
    for holding in assessment.holdings.values() {
        let (code, code_source) = if holding.instrument == Instrument::Crypto {
            (
                rules.crypto_asset_code(&holding.symbol),
                rules.crypto_codes.source,
            )
        } else {
            let rule = rules.instrument(holding.instrument)?;
            (rule.asset_code.value, rule.asset_code.source)
        };
        assets.push(AssetLine {
            symbol: holding.symbol.clone(),
            instrument: holding.instrument,
            quantity: holding.quantity,
            total_cost: cents(holding.total_cost),
            average_price: holding.average_price(),
            code,
            code_source,
        });
    }

    for income in incomes {
        if income.gross < Decimal::ZERO
            || income.withheld < Decimal::ZERO
            || income.withheld > income.gross
        {
            return Err(TaxError::InvalidOperation {
                symbol: income.symbol.clone(),
                date: income.pay_date,
                reason: "provento com valor ou retencao invalidos",
            });
        }
    }
    let of_year = |kind: IncomeKind| {
        incomes
            .iter()
            .filter(move |income| income.kind == kind && income.pay_date.year() == year)
    };
    let codes = &rules.income_codes;

    let months: Vec<MonthReport> = assessment
        .months
        .into_iter()
        .filter(|report| report.period.year == year)
        .collect();
    let exempt_gain = |id: ExemptionId| -> Decimal {
        months
            .iter()
            .flat_map(|report| &report.exemptions)
            .filter(|status| status.id == id)
            .map(|status| status.exempt_gain)
            .sum()
    };

    let exempt_income = vec![
        income_line(&codes.dividends, of_year(IncomeKind::Dividend)),
        income_line(&codes.fund_income, of_year(IncomeKind::FundIncome)),
        gain_line(
            &codes.exempt_stock_gains,
            exempt_gain(ExemptionId::StockSales),
        ),
        gain_line(
            &codes.exempt_crypto_gains,
            exempt_gain(ExemptionId::CryptoSales),
        ),
    ];
    let exclusive_income = vec![income_line(
        &codes.interest_on_equity,
        of_year(IncomeKind::InterestOnEquity),
    )];

    let darf_total = months
        .iter()
        .flat_map(|report| &report.darfs)
        .map(|darf| darf.amount)
        .sum();
    Ok(AnnualReport {
        year,
        assets,
        exempt_income,
        exclusive_income,
        months,
        losses_at_year_end: assessment.closing.losses,
        pending_darf_at_year_end: assessment.closing.pending_darf,
        withholding_left: assessment
            .withholding_left
            .get(&year)
            .copied()
            .unwrap_or_default(),
        darf_total,
    })
}

fn income_line<'a>(
    code: &Sourced<IrpfCode>,
    incomes: impl Iterator<Item = &'a Income>,
) -> IncomeLine {
    let mut by_payer: BTreeMap<&str, (Decimal, Decimal)> = BTreeMap::new();
    for income in incomes {
        let entry = by_payer.entry(income.symbol.as_str()).or_default();
        entry.0 += income.gross;
        entry.1 += income.withheld;
    }
    let by_payer: Vec<PayerAmount> = by_payer
        .into_iter()
        .map(|(symbol, (gross, withheld))| PayerAmount {
            symbol: symbol.to_owned(),
            gross: cents(gross),
            withheld: cents(withheld),
        })
        .collect();
    let gross: Decimal = by_payer.iter().map(|payer| payer.gross).sum();
    let withheld: Decimal = by_payer.iter().map(|payer| payer.withheld).sum();
    IncomeLine {
        code: code.value,
        code_source: code.source,
        gross,
        withheld,
        net: gross - withheld,
        by_payer,
    }
}

fn gain_line(code: &Sourced<IrpfCode>, gain: Decimal) -> IncomeLine {
    IncomeLine {
        code: code.value,
        code_source: code.source,
        gross: gain,
        withheld: Decimal::ZERO,
        net: gain,
        by_payer: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tax::monthly::Carryforward;
    use crate::domain::tax::positions::tests::{buy, sell};
    use crate::domain::tax::positions::{OpeningPosition, Operation};
    use rust_decimal_macros::dec;
    use time::macros::date;
    use Instrument::{Bdr, Crypto, Etf, RealEstateFund, Stock};

    fn input<'a>(openings: &'a [OpeningPosition], operations: &'a [Operation]) -> TaxInput<'a> {
        TaxInput {
            openings,
            operations,
            since_year: 2025,
            carryforward: Carryforward::default(),
        }
    }

    fn income(
        pay_date: Date,
        symbol: &str,
        kind: IncomeKind,
        gross: Decimal,
        withheld: Decimal,
    ) -> Income {
        Income {
            pay_date,
            symbol: symbol.to_owned(),
            kind,
            gross,
            withheld,
        }
    }

    #[test]
    fn assets_are_declared_at_cost_with_their_codes() {
        let openings = [OpeningPosition {
            symbol: "AAAA3".to_owned(),
            instrument: Stock,
            quantity: dec!(100),
            total_cost: dec!(1000),
        }];
        let operations = [
            buy(date!(2025 - 03 - 03), "AAAA3", Stock, dec!(100), dec!(12)),
            buy(
                date!(2025 - 03 - 03),
                "FFFF11",
                RealEstateFund,
                dec!(10),
                dec!(100),
            ),
            buy(date!(2025 - 03 - 03), "EEEE11", Etf, dec!(10), dec!(100)),
            buy(date!(2025 - 03 - 03), "BBBB34", Bdr, dec!(10), dec!(50)),
            buy(
                date!(2025 - 03 - 03),
                "BTC",
                Crypto,
                dec!(0.01),
                dec!(500000),
            ),
            buy(date!(2025 - 03 - 03), "USDT", Crypto, dec!(100), dec!(5.5)),
            // Comprado em 2026: não entra na posição de 31/12/2025.
            buy(date!(2026 - 01 - 05), "ZZZZ3", Stock, dec!(1), dec!(1)),
        ];
        let report = annual_report(&input(&openings, &operations), &[], 2025).unwrap();
        let line = |symbol: &str| report.assets.iter().find(|a| a.symbol == symbol).unwrap();
        let stock = line("AAAA3");
        assert_eq!(stock.total_cost, dec!(2200));
        assert_eq!(stock.average_price, dec!(11));
        assert_eq!((stock.code.group, stock.code.code), ("03", "01"));
        assert_eq!(
            (line("FFFF11").code.group, line("FFFF11").code.code),
            ("07", "03")
        );
        assert_eq!(
            (line("EEEE11").code.group, line("EEEE11").code.code),
            ("07", "09")
        );
        assert_eq!(
            (line("BBBB34").code.group, line("BBBB34").code.code),
            ("04", "04")
        );
        assert_eq!(line("BTC").code.code, "01");
        assert_eq!(line("BTC").total_cost, dec!(5000));
        assert_eq!(line("USDT").code.code, "03");
        assert!(report.assets.iter().all(|a| a.symbol != "ZZZZ3"));
    }

    #[test]
    fn income_is_summed_per_line_and_payer() {
        let incomes = [
            income(
                date!(2025 - 03 - 14),
                "AAAA3",
                IncomeKind::Dividend,
                dec!(120),
                dec!(0),
            ),
            income(
                date!(2025 - 09 - 14),
                "AAAA3",
                IncomeKind::Dividend,
                dec!(80),
                dec!(0),
            ),
            income(
                date!(2025 - 05 - 15),
                "FFFF11",
                IncomeKind::FundIncome,
                dec!(55.5),
                dec!(0),
            ),
            income(
                date!(2025 - 06 - 30),
                "AAAA3",
                IncomeKind::InterestOnEquity,
                dec!(100),
                dec!(15),
            ),
            // Pago em 2026: fica fora de 2025.
            income(
                date!(2026 - 01 - 15),
                "AAAA3",
                IncomeKind::Dividend,
                dec!(999),
                dec!(0),
            ),
        ];
        let report = annual_report(&input(&[], &[]), &incomes, 2025).unwrap();
        let dividends = &report.exempt_income[0];
        assert_eq!(dividends.code.code, "09");
        assert_eq!(dividends.gross, dec!(200));
        assert_eq!(dividends.by_payer.len(), 1);
        assert_eq!(report.exempt_income[1].gross, dec!(55.5));
        let jcp = &report.exclusive_income[0];
        assert_eq!(jcp.code.code, "10");
        assert_eq!(jcp.gross, dec!(100));
        assert_eq!(jcp.withheld, dec!(15));
        assert_eq!(jcp.net, dec!(85));
    }

    #[test]
    fn exempt_gains_and_year_end_balances_are_reported() {
        let operations = [
            buy(date!(2025 - 01 - 06), "AAAA3", Stock, dec!(1000), dec!(10)),
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            buy(date!(2025 - 01 - 06), "BTC", Crypto, dec!(1), dec!(100000)),
            sell(date!(2025 - 02 - 03), "AAAA3", Stock, dec!(1000), dec!(12)),
            sell(
                date!(2025 - 04 - 03),
                "BTC",
                Crypto,
                dec!(0.1),
                dec!(150000),
            ),
            sell(date!(2025 - 12 - 03), "EEEE11", Etf, dec!(100), dec!(90)),
        ];
        let report = annual_report(&input(&[], &operations), &[], 2025).unwrap();
        assert_eq!(report.exempt_income[2].code.code, "20");
        assert_eq!(report.exempt_income[2].gross, dec!(2000));
        assert_eq!(report.exempt_income[3].code.code, "05");
        assert_eq!(report.exempt_income[3].gross, dec!(5000));
        assert_eq!(report.losses_at_year_end[&Pool::Common], dec!(1000));
        assert_eq!(report.months.len(), 12);
        assert_eq!(report.darf_total, dec!(0));
        assert_eq!(report.withholding_left, dec!(0));
    }

    #[test]
    fn second_year_report_carries_the_first() {
        let operations = [
            buy(date!(2025 - 01 - 06), "EEEE11", Etf, dec!(100), dec!(100)),
            sell(date!(2025 - 12 - 03), "EEEE11", Etf, dec!(50), dec!(90)),
            sell(date!(2026 - 03 - 03), "EEEE11", Etf, dec!(50), dec!(130)),
        ];
        let report = annual_report(&input(&[], &operations), &[], 2026).unwrap();
        assert_eq!(report.year, 2026);
        assert!(report.months.iter().all(|m| m.period.year == 2026));
        // 1.500 de lucro − 500 de prejuízo de 2025 = 1.000 × 15%.
        assert_eq!(report.darf_total, dec!(150));
        assert!(report.assets.is_empty());
        assert_eq!(
            report.pending_darf_at_year_end[&Regime::ExchangeNetGain],
            dec!(0)
        );
    }

    #[test]
    fn invalid_income_and_missing_rules_are_errors() {
        let bad = [income(
            date!(2025 - 03 - 14),
            "AAAA3",
            IncomeKind::Dividend,
            dec!(10),
            dec!(11),
        )];
        assert!(matches!(
            annual_report(&input(&[], &[]), &bad, 2025),
            Err(TaxError::InvalidOperation { .. })
        ));
        assert_eq!(
            annual_report(&input(&[], &[]), &[], 2027),
            Err(TaxError::NoRulesForYear(2027))
        );
    }
}
