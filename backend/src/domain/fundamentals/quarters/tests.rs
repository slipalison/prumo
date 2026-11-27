use super::*;
use crate::domain::fundamentals::{Line, Scope};
use proptest::prelude::*;
use rust_decimal_macros::dec;
use time::macros::date;
use time::Month;

/// Os números fictícios de um trimestre (em reais).
#[derive(Clone, Copy)]
struct Q {
    revenue: Decimal,
    profit: Decimal,
    controlling: Decimal,
}

fn q(revenue: Decimal, profit: Decimal) -> Q {
    Q {
        revenue,
        profit,
        controlling: profit,
    }
}

fn end_of(year: i32, quarter: u8) -> Date {
    match quarter {
        1 => Date::from_calendar_date(year, Month::March, 31).unwrap(),
        2 => Date::from_calendar_date(year, Month::June, 30).unwrap(),
        3 => Date::from_calendar_date(year, Month::September, 30).unwrap(),
        _ => Date::from_calendar_date(year, Month::December, 31).unwrap(),
    }
}

fn line(column: Column, start: Date, end: Date, code: &str, desc: &str, value: Decimal) -> Line {
    Line {
        statement: Statement::Income,
        column,
        period_start: Some(start),
        period_end: end,
        code: code.to_owned(),
        description: desc.to_owned(),
        value,
    }
}

/// As linhas de resultado de um acumulado (soma dos trimestres dados).
fn ytd_lines(column: Column, year: i32, quarters: &[Q], template: Template) -> Vec<Line> {
    let start = Date::from_calendar_date(year, Month::January, 1).unwrap();
    let end = end_of(year, u8::try_from(quarters.len()).unwrap());
    let revenue: Decimal = quarters.iter().map(|q| q.revenue).sum();
    let profit: Decimal = quarters.iter().map(|q| q.profit).sum();
    let controlling: Decimal = quarters.iter().map(|q| q.controlling).sum();
    let (top, gross, profit_code) = match template {
        Template::Bank => (
            "Receitas da Intermediação Financeira",
            "Resultado Bruto de Intermediação Financeira",
            "3.09",
        ),
        _ => (
            "Receita de Venda de Bens e/ou Serviços",
            "Resultado Bruto",
            "3.11",
        ),
    };
    vec![
        // No banco a 3.01 é a receita bruta (o dobro, aqui); a linha de cima
        // é o resultado bruto (3.03), que recebe a "receita" do trimestre.
        line(
            column,
            start,
            end,
            "3.01",
            top,
            if template == Template::Bank {
                revenue * dec!(2)
            } else {
                revenue
            },
        ),
        line(column, start, end, "3.03", gross, revenue),
        line(
            column,
            start,
            end,
            profit_code,
            "Lucro/Prejuízo Consolidado do Período",
            profit,
        ),
        line(
            column,
            start,
            end,
            &format!("{profit_code}.01"),
            "Atribuído a Sócios da Empresa Controladora",
            controlling,
        ),
        line(
            column,
            start,
            end,
            &format!("{profit_code}.02"),
            "Atribuído a Sócios Não Controladores",
            profit - controlling,
        ),
    ]
}

/// Uma companhia fictícia com os trimestres dados por ano (do 1º ao último
/// entregue). Gera os ITR e as DFP como a CVM publica: cada documento com a
/// coluna do período e a de comparação do ano anterior.
struct Company {
    template: Template,
    years: Vec<(i32, Vec<Q>)>,
}

impl Company {
    fn quarters_of(&self, year: i32) -> Option<&[Q]> {
        self.years
            .iter()
            .find(|(y, _)| *y == year)
            .map(|(_, q)| q.as_slice())
    }

    /// Os documentos dos anos `files` (como o Prumo lê: DFP e ITR desses anos).
    fn filings(&self, files: &[i32]) -> Vec<Filing> {
        let mut out = Vec::new();
        for year in files {
            let Some(current) = self.quarters_of(*year) else {
                continue;
            };
            let previous = self.quarters_of(year - 1);
            for k in 1..=current.len().min(3) {
                let mut lines = ytd_lines(Column::Current, *year, &current[..k], self.template);
                if let Some(p) = previous.filter(|p| p.len() >= k) {
                    lines.extend(ytd_lines(
                        Column::Previous,
                        year - 1,
                        &p[..k],
                        self.template,
                    ));
                }
                out.push(self.filing(
                    FilingKind::Quarterly,
                    end_of(*year, u8::try_from(k).unwrap()),
                    lines,
                ));
            }
            if current.len() == 4 {
                let mut lines = ytd_lines(Column::Current, *year, current, self.template);
                if let Some(p) = previous.filter(|p| p.len() == 4) {
                    lines.extend(ytd_lines(Column::Previous, year - 1, p, self.template));
                }
                out.push(self.filing(FilingKind::Annual, end_of(*year, 4), lines));
            }
        }
        out
    }

    fn filing(&self, kind: FilingKind, reference_date: Date, lines: Vec<Line>) -> Filing {
        Filing {
            cvm_code: 99911,
            kind,
            reference_date,
            version: 1,
            scope: Scope::Consolidated,
            company_name: "CIA FICTICIA SA".into(),
            lines,
        }
    }
}

fn industrial() -> Company {
    Company {
        template: Template::Commercial,
        years: vec![
            (
                2024,
                vec![
                    q(dec!(100), dec!(10)),
                    q(dec!(110), dec!(11)),
                    q(dec!(120), dec!(12)),
                    q(dec!(130), dec!(13)),
                ],
            ),
            (
                2025,
                vec![
                    q(dec!(105), dec!(-4)),
                    q(dec!(115), dec!(9)),
                    q(dec!(125), dec!(15)),
                    q(dec!(140), dec!(14)),
                ],
            ),
            (
                2026,
                vec![
                    q(dec!(120), dec!(-2)),
                    q(dec!(118), dec!(6)),
                    q(dec!(150), dec!(18)),
                ],
            ),
        ],
    }
}

/// A entrega de cada documento: 40 dias depois da data de referência.
fn received(filings: &[Filing]) -> Receipts {
    filings
        .iter()
        .filter_map(|f| {
            f.reference_date
                .checked_add(time::Duration::days(40))
                .map(|d| ((f.kind, f.reference_date), d))
        })
        .collect()
}

fn run(filings: &[Filing], holding: bool) -> QuarterlyResults {
    quarterly_results(filings, &received(filings), holding)
}

fn values(results: &QuarterlyResults) -> Vec<(Date, Option<Decimal>, Option<Decimal>)> {
    results
        .quarters
        .iter()
        .map(|q| (q.period_end, q.top_line.value(), q.net_income.value()))
        .collect()
}

#[test]
fn each_quarter_comes_back_from_the_year_to_date_figures() {
    let company = industrial();
    // O Prumo lê o ITR do ano corrente e do anterior e as DFP dos seis anos.
    let filings = company.filings(&[2024, 2025, 2026]);
    let results = run(&filings, false);
    assert_eq!(results.top_line, TopLine::Revenue);
    // Oito trimestres, do 4º de 2024 ao 3º de 2026, cada um igual ao que a
    // companhia teve no trimestre.
    let expected: Vec<(Date, Option<Decimal>, Option<Decimal>)> = [
        (2024, 4),
        (2025, 1),
        (2025, 2),
        (2025, 3),
        (2025, 4),
        (2026, 1),
        (2026, 2),
        (2026, 3),
    ]
    .iter()
    .map(|(year, k)| {
        let quarter = company.quarters_of(*year).unwrap()[usize::from(*k) - 1];
        (
            end_of(*year, *k),
            Some(quarter.revenue),
            Some(quarter.profit),
        )
    })
    .collect();
    assert_eq!(values(&results), expected);
    let methods: Vec<QuarterMethod> = results.quarters.iter().map(|q| q.method).collect();
    assert_eq!(methods[0], QuarterMethod::AnnualLessNineMonths);
    assert_eq!(methods[1], QuarterMethod::FirstQuarter);
    assert_eq!(methods[2], QuarterMethod::YearToDateDifference);
    let latest = results.quarters.last().unwrap();
    assert_eq!(latest.fiscal_quarter, 3);
    assert_eq!(latest.period_start, date!(2026 - 07 - 01));
    // 18 ÷ 150
    assert_eq!(latest.net_margin.value(), Some(dec!(0.12)));
    assert_eq!(latest.source.kind, FilingKind::Quarterly);
    assert_eq!(latest.received_on, Some(date!(2026 - 11 - 09)));
    // O 4º trimestre sai da DFP, entregue na data dela.
    assert_eq!(results.quarters[4].source.kind, FilingKind::Annual);
    assert_eq!(results.quarters[4].received_on, Some(date!(2026 - 02 - 09)));
}

#[test]
fn the_newest_quarter_against_a_year_ago_and_the_previous_quarter() {
    let results = run(&industrial().filings(&[2024, 2025, 2026]), false);
    let year_ago = results.year_ago.unwrap();
    assert_eq!(year_ago.period_end, date!(2025 - 09 - 30));
    // 150 ÷ 125 − 1 e 18 ÷ 15 − 1.
    assert_eq!(year_ago.top_line, Change::Percent { value: dec!(0.2) });
    assert_eq!(year_ago.net_income, Change::Percent { value: dec!(0.2) });
    // 12% − 12%.
    assert_eq!(year_ago.net_margin_points.value(), Some(dec!(0)));
    let previous = results.previous_quarter.unwrap();
    assert_eq!(previous.period_end, date!(2026 - 06 - 30));
    assert_eq!(
        previous.top_line,
        Change::Percent {
            value: dec!(0.271186)
        }
    );
    assert_eq!(previous.net_income, Change::Percent { value: dec!(2) });
    // 12% − 6 ÷ 118 (5,0847%).
    assert_eq!(previous.net_margin_points.value(), Some(dec!(0.069153)));
}

#[test]
fn a_loss_gets_a_label_never_a_percent_over_a_negative_base() {
    // O 2º trimestre de 2026 contra o 1º (prejuízo): saiu do prejuízo.
    let mut company = industrial();
    company.years[2].1.truncate(2);
    let results = run(&company.filings(&[2025, 2026]), false);
    assert_eq!(
        results.previous_quarter.unwrap().net_income,
        Change::LeftLoss
    );
    // O 1º de 2026 (−2) contra o 1º de 2025 (−4): prejuízo menor.
    company.years[2].1.truncate(1);
    let results = run(&company.filings(&[2025, 2026]), false);
    assert_eq!(results.year_ago.unwrap().net_income, Change::SmallerLoss);
    // E contra o 4º de 2025 (lucro 14): entrou no prejuízo.
    assert_eq!(
        results.previous_quarter.unwrap().net_income,
        Change::EnteredLoss
    );
}

#[test]
fn change_labels() {
    let f = |v: Decimal| Figure::known(v);
    assert_eq!(
        Change::between(f(dec!(10)), f(dec!(15))),
        Change::Percent { value: dec!(0.5) }
    );
    assert_eq!(
        Change::between(f(dec!(10)), f(dec!(0))),
        Change::Percent { value: dec!(-1) }
    );
    assert_eq!(
        Change::between(f(dec!(10)), f(dec!(-1))),
        Change::EnteredLoss
    );
    assert_eq!(Change::between(f(dec!(-10)), f(dec!(5))), Change::LeftLoss);
    assert_eq!(Change::between(f(dec!(-10)), f(dec!(0))), Change::LeftLoss);
    assert_eq!(
        Change::between(f(dec!(-10)), f(dec!(-3))),
        Change::SmallerLoss
    );
    assert_eq!(
        Change::between(f(dec!(-10)), f(dec!(-30))),
        Change::LargerLoss
    );
    assert_eq!(
        Change::between(f(dec!(-10)), f(dec!(-10))),
        Change::Unchanged
    );
    assert_eq!(Change::between(f(dec!(0)), f(dec!(5))), Change::NoBase);
    assert_eq!(
        Change::between(Figure::missing(Gap::Holding), f(dec!(5))),
        Change::Missing { gap: Gap::Holding }
    );
    assert_eq!(
        Change::between(f(dec!(5)), Figure::missing(Gap::NotReported)),
        Change::Missing {
            gap: Gap::NotReported
        }
    );
    assert_eq!(Change::Unchanged.percent(), Some(dec!(0)));
    assert_eq!(Change::LeftLoss.percent(), None);
}

#[test]
fn a_holding_has_no_margin() {
    let results = run(&industrial().filings(&[2025, 2026]), true);
    let latest = results.quarters.last().unwrap();
    assert_eq!(latest.top_line.value(), Some(dec!(150)));
    assert_eq!(latest.net_margin.gap(), Some(Gap::Holding));
    assert_eq!(
        results.year_ago.unwrap().net_margin_points.gap(),
        Some(Gap::Holding)
    );
}

#[test]
fn a_bank_shows_the_gross_intermediation_result() {
    let mut company = industrial();
    company.template = Template::Bank;
    let results = run(&company.filings(&[2025, 2026]), false);
    assert_eq!(results.top_line, TopLine::IntermediationResult);
    let latest = results.quarters.last().unwrap();
    // A 3.03, não a 3.01 (que aqui é o dobro).
    assert_eq!(latest.top_line.value(), Some(dec!(150)));
    assert_eq!(latest.net_income.value(), Some(dec!(18)));
    assert_eq!(latest.net_margin.gap(), Some(Gap::FinancialTemplate));
    assert_eq!(
        results.year_ago.unwrap().top_line,
        Change::Percent { value: dec!(0.2) }
    );
}

#[test]
fn the_controlling_share_uses_the_same_basis_at_both_ends() {
    let mut company = industrial();
    for (_, quarters) in &mut company.years {
        for quarter in quarters.iter_mut() {
            quarter.controlling = quarter.profit - dec!(1);
        }
    }
    let filings = company.filings(&[2025, 2026]);
    let results = run(&filings, false);
    // Divisão preenchida nas duas pontas: a parte dos controladores (18 − 1).
    assert_eq!(
        results.quarters.last().unwrap().net_income.value(),
        Some(dec!(17))
    );
    // O acumulado de 6 meses entrega a divisão toda em zero: as duas pontas
    // usam o total (33 − 4), nunca 33 − 2 (total − controladores).
    let mut blanked = filings.clone();
    for filing in &mut blanked {
        if filing.kind == FilingKind::Quarterly && filing.reference_date == date!(2026 - 06 - 30) {
            for l in &mut filing.lines {
                if l.column == Column::Current && (l.code == "3.11.01" || l.code == "3.11.02") {
                    l.value = Decimal::ZERO;
                }
            }
        }
    }
    let results = run(&blanked, false);
    let q3 = results.quarters.last().unwrap();
    assert_eq!(q3.net_income.value(), Some(dec!(18)));
    let q2 = &results.quarters[results.quarters.len() - 2];
    assert_eq!(q2.net_income.value(), Some(dec!(6)));
}

#[test]
fn a_missing_year_to_date_leaves_the_quarter_out() {
    let mut filings = industrial().filings(&[2025, 2026]);
    // Sem o ITR de setembro de 2025: não há 3º nem 4º trimestre de 2025 (a
    // DFP sozinha não diz o 4º), mas a comparação do ITR de 2026 traz o
    // acumulado de 9 meses de 2025.
    filings.retain(|f| {
        !(f.kind == FilingKind::Quarterly && f.reference_date == date!(2025 - 09 - 30))
    });
    let results = run(&filings, false);
    let ends: Vec<Date> = results.quarters.iter().map(|q| q.period_end).collect();
    assert!(ends.contains(&date!(2025 - 09 - 30)));
    assert!(ends.contains(&date!(2025 - 12 - 31)));
    // O 3º de 2025 veio da coluna de comparação: sem data de entrega.
    let q3 = results
        .quarters
        .iter()
        .find(|q| q.period_end == date!(2025 - 09 - 30))
        .unwrap();
    assert_eq!(q3.source.column, Column::Previous);
    assert_eq!(q3.received_on, None);
    // Sem nenhum acumulado de 9 meses de 2025: nem o 3º nem o 4º.
    filings.retain(|f| f.reference_date != date!(2026 - 09 - 30));
    let results = run(&filings, false);
    let ends: Vec<Date> = results.quarters.iter().map(|q| q.period_end).collect();
    assert!(!ends.contains(&date!(2025 - 09 - 30)));
    assert!(!ends.contains(&date!(2025 - 12 - 31)));
    assert_eq!(ends.last(), Some(&date!(2026 - 06 - 30)));
    // O 2º de 2026 compara com o 2º de 2025; o anterior (1º de 2026) existe.
    assert!(results.year_ago.is_some());
    assert!(results.previous_quarter.is_some());
}

#[test]
fn no_filings_no_quarters() {
    let results = run(&[], false);
    assert!(results.quarters.is_empty());
    assert_eq!(results.top_line, TopLine::Unknown);
    assert!(results.year_ago.is_none());
    assert!(results.previous_quarter.is_none());
}

#[test]
fn a_fiscal_year_off_the_calendar_still_splits_in_quarters() {
    // Exercício de abril a março: o 1º trimestre termina em junho.
    let start = date!(2026 - 04 - 01);
    let ends = [date!(2026 - 06 - 30), date!(2026 - 09 - 30)];
    let filings: Vec<Filing> = ends
        .iter()
        .enumerate()
        .map(|(i, end)| {
            let total = Decimal::from(i + 1) * dec!(100);
            Filing {
                cvm_code: 99912,
                kind: FilingKind::Quarterly,
                reference_date: *end,
                version: 1,
                scope: Scope::Individual,
                company_name: "USINA FICTICIA SA".into(),
                lines: vec![
                    line(
                        Column::Current,
                        start,
                        *end,
                        "3.01",
                        "Receita de Venda de Bens e/ou Serviços",
                        total,
                    ),
                    line(
                        Column::Current,
                        start,
                        *end,
                        "3.11",
                        "Lucro/Prejuízo do Período",
                        total / dec!(10),
                    ),
                ],
            }
        })
        .collect();
    let results = run(&filings, false);
    assert_eq!(results.quarters.len(), 2);
    assert_eq!(results.quarters[1].fiscal_quarter, 2);
    assert_eq!(results.quarters[1].period_start, date!(2026 - 07 - 01));
    assert_eq!(results.quarters[1].top_line.value(), Some(dec!(100)));
    assert!(results.year_ago.is_none());
}

#[test]
fn quarter_lengths() {
    assert_eq!(
        quarter_months(date!(2026 - 01 - 01), date!(2026 - 03 - 31)),
        Some(3)
    );
    assert_eq!(
        quarter_months(date!(2026 - 01 - 01), date!(2026 - 12 - 31)),
        Some(12)
    );
    assert_eq!(
        quarter_months(date!(2026 - 04 - 01), date!(2027 - 03 - 31)),
        Some(12)
    );
    assert_eq!(
        quarter_months(date!(2026 - 01 - 01), date!(2026 - 02 - 28)),
        None
    );
    assert_eq!(
        quarter_months(date!(2026 - 01 - 15), date!(2026 - 03 - 31)),
        None
    );
}

proptest! {
    /// Com quaisquer trimestres, a série devolve cada trimestre como foi.
    #[test]
    fn the_quarters_add_up(revenues in proptest::collection::vec(1i64..1_000_000, 11), profits in proptest::collection::vec(-100_000i64..100_000, 11)) {
        let mut years = Vec::new();
        for (year, range) in [(2024, 0..4), (2025, 4..8), (2026, 8..11)] {
            years.push((year, range.map(|i| q(Decimal::from(revenues[i]), Decimal::from(profits[i]))).collect::<Vec<Q>>()));
        }
        let company = Company { template: Template::Commercial, years };
        let results = run(&company.filings(&[2024, 2025, 2026]), false);
        prop_assert_eq!(results.quarters.len(), SERIES_QUARTERS);
        let flat: Vec<Q> = company.years.iter().flat_map(|(_, q)| q.clone()).collect();
        for (got, want) in results.quarters.iter().zip(flat.iter().skip(3)) {
            prop_assert_eq!(got.top_line.value(), Some(want.revenue));
            prop_assert_eq!(got.net_income.value(), Some(want.profit));
        }
    }

    /// Percentual só com base positiva; o sinal do percentual é o da mudança.
    #[test]
    fn a_percent_needs_a_positive_base(before in -1_000i64..1_000, now in -1_000i64..1_000) {
        let change = Change::between(Figure::known(Decimal::from(before)), Figure::known(Decimal::from(now)));
        if let Change::Percent { value } = change {
            prop_assert!(before > 0);
            prop_assert!(now >= 0);
            prop_assert_eq!(value > Decimal::ZERO, now > before);
        }
        if before < 0 {
            prop_assert!(change.percent().is_none() || now == before);
        }
    }
}
