//! Os resultados trimestrais de uma companhia: a linha de cima (receita; em
//! banco e seguradora, o resultado bruto da atividade), o lucro líquido e a
//! margem de cada trimestre, e a comparação do mais novo com o mesmo trimestre
//! do ano anterior e com o trimestre anterior.
//!
//! **De onde sai cada trimestre.** O ITR traz o resultado duas vezes, o do
//! trimestre e o acumulado do ano, e o Prumo guarda o acumulado (é com ele que
//! monta os 12 meses, ver `company.rs`). O trimestre sai da diferença entre
//! acumulados do mesmo exercício — a mesma conta que a companhia faz:
//!
//! | trimestre | conta |
//! |---|---|
//! | 1º | o acumulado de 3 meses do 1º ITR (é o próprio trimestre) |
//! | 2º | acumulado de 6 meses − acumulado de 3 meses |
//! | 3º | acumulado de 9 meses − acumulado de 6 meses |
//! | 4º | o exercício da DFP − acumulado de 9 meses do 3º ITR (não há ITR do 4º) |
//!
//! O acumulado de um período vem da coluna do documento que o fecha (o ITR do
//! trimestre, a DFP do exercício) e, sem ele, da coluna de comparação do
//! documento do ano seguinte — a mesma preferência da série anual.
//!
//! **Lucro dos controladores, na mesma base nas duas pontas.** A diferença usa
//! a parte dos controladores quando os dois acumulados dividem o lucro de
//! verdade; se um deles entrega a divisão toda em zero (divisão não preenchida,
//! a regra de `company.rs`), as duas pontas usam o total. Misturar as bases
//! daria um trimestre que nenhuma demonstração tem.
//!
//! **Prejuízo tem rótulo, não percentual.** Variação sobre base negativa engana
//! (prejuízo que diminui sairia como queda; lucro depois de prejuízo, como
//! "−300%"). [`Change`] diz "saiu do prejuízo", "entrou no prejuízo",
//! "prejuízo menor" ou "prejuízo maior"; só há percentual com a base positiva.

use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;
use serde::Serialize;
use time::Date;

use super::company::{reference, Columnar, ProfitParts};
use super::{
    fold, Column, Figure, Filing, FilingKind, FilingRef, Gap, Statement, Template, RATIO_DP,
};

/// Quantos trimestres a série mostra (dois anos).
pub const SERIES_QUARTERS: usize = 8;

/// O que a linha de cima é para o plano de contas da companhia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopLine {
    /// Plano comercial: a receita líquida (DRE 3.01).
    Revenue,
    /// Banco: o resultado bruto da intermediação financeira (DRE 3.03), o que
    /// sobra das receitas de crédito e tesouraria depois do custo de captação.
    /// A "receita" do banco (3.01) é bruta e não se compara com a de indústria.
    IntermediationResult,
    /// Seguradora: o resultado bruto das operações de seguro (DRE 3.03).
    InsuranceResult,
    /// Plano que o Prumo não reconhece: só o lucro.
    Unknown,
}

impl TopLine {
    fn of(template: Template) -> Self {
        match template {
            Template::Commercial => TopLine::Revenue,
            Template::Bank => TopLine::IntermediationResult,
            Template::Insurer => TopLine::InsuranceResult,
            Template::Unknown => TopLine::Unknown,
        }
    }
}

/// Como o trimestre foi montado a partir dos acumulados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuarterMethod {
    /// O 1º trimestre: o acumulado de três meses, como o ITR traz.
    FirstQuarter,
    /// 2º e 3º: o acumulado do ITR menos o do trimestre anterior.
    YearToDateDifference,
    /// 4º: o exercício da DFP menos o acumulado de nove meses do 3º ITR.
    AnnualLessNineMonths,
}

/// Um trimestre da companhia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quarter {
    /// 1 a 4, dentro do exercício (o exercício pode não ser o ano civil).
    pub fiscal_quarter: u8,
    pub period_start: Date,
    pub period_end: Date,
    pub method: QuarterMethod,
    /// Receita, ou o resultado bruto em banco e seguradora (ver [`TopLine`]).
    pub top_line: Figure,
    /// Lucro líquido dos controladores (o total, sem a divisão).
    pub net_income: Figure,
    /// Lucro ÷ receita; [`Gap::Holding`] na holding e
    /// [`Gap::FinancialTemplate`] em banco e seguradora.
    pub net_margin: Figure,
    /// O documento que fecha o trimestre: o ITR dele, ou a DFP no 4º.
    pub source: FilingRef,
    /// Quando a CVM recebeu esse documento (a entrega que a Agenda lê do índice
    /// dos arquivos, `cvm_filing_receipts`); `None` quando o acumulado veio da
    /// coluna de comparação de um documento posterior, ou sem a entrega lida.
    pub received_on: Option<Date>,
}

/// A variação de uma medida entre dois trimestres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Base positiva: `agora ÷ antes − 1` (fração, seis casas).
    Percent { value: Decimal },
    /// Era prejuízo (negativo) e agora é lucro ou zero.
    LeftLoss,
    /// Era lucro e agora é prejuízo.
    EnteredLoss,
    /// Prejuízo nos dois, menor agora.
    SmallerLoss,
    /// Prejuízo nos dois, maior agora.
    LargerLoss,
    /// O mesmo número nos dois trimestres.
    Unchanged,
    /// A base é zero: não há variação a dizer.
    NoBase,
    /// Falta o número num dos trimestres; o motivo.
    Missing { gap: Gap },
}

impl Change {
    /// De `before` para `now`.
    pub fn between(before: Figure, now: Figure) -> Change {
        let (b, n) = match (before.value(), now.value()) {
            (Some(b), Some(n)) => (b, n),
            (_, None) => {
                return Change::Missing {
                    gap: now.gap().unwrap_or(Gap::NotReported),
                }
            }
            (None, _) => {
                return Change::Missing {
                    gap: before.gap().unwrap_or(Gap::NotReported),
                }
            }
        };
        if b.is_zero() {
            return Change::NoBase;
        }
        if n == b {
            return Change::Unchanged;
        }
        if b > Decimal::ZERO {
            if n < Decimal::ZERO {
                return Change::EnteredLoss;
            }
            return n
                .checked_div(b)
                .map_or(Change::NoBase, |r| Change::Percent {
                    value: (r - Decimal::ONE).round_dp(RATIO_DP).normalize(),
                });
        }
        if n >= Decimal::ZERO {
            Change::LeftLoss
        } else if n.abs() < b.abs() {
            Change::SmallerLoss
        } else {
            Change::LargerLoss
        }
    }

    /// O percentual, quando há.
    pub fn percent(self) -> Option<Decimal> {
        match self {
            Change::Percent { value } => Some(value),
            Change::Unchanged => Some(Decimal::ZERO),
            _ => None,
        }
    }
}

/// O trimestre mais novo contra outro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comparison {
    /// O fim do trimestre comparado.
    pub period_end: Date,
    pub top_line: Change,
    pub net_income: Change,
    /// Margem de agora − margem de antes, em fração (0,012 = 1,2 ponto).
    pub net_margin_points: Figure,
}

impl Comparison {
    fn of(before: &Quarter, now: &Quarter) -> Self {
        let points = match (before.net_margin.value(), now.net_margin.value()) {
            (Some(b), Some(n)) => Figure::known((n - b).round_dp(RATIO_DP).normalize()),
            _ => now
                .net_margin
                .gap()
                .or(before.net_margin.gap())
                .map_or(Figure::missing(Gap::NotReported), Figure::missing),
        };
        Self {
            period_end: before.period_end,
            top_line: Change::between(before.top_line, now.top_line),
            net_income: Change::between(before.net_income, now.net_income),
            net_margin_points: points,
        }
    }
}

/// Os trimestres de uma companhia e a comparação do mais novo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarterlyResults {
    /// O que a linha de cima é (pelo plano do trimestre mais novo).
    pub top_line: TopLine,
    /// Até [`SERIES_QUARTERS`] trimestres, do mais velho ao mais novo.
    pub quarters: Vec<Quarter>,
    /// O mais novo contra o mesmo trimestre do ano anterior.
    pub year_ago: Option<Comparison>,
    /// O mais novo contra o trimestre imediatamente anterior.
    pub previous_quarter: Option<Comparison>,
}

/// O acumulado do exercício até uma data, de uma coluna de um documento.
#[derive(Debug, Clone)]
struct YearToDate {
    template: Template,
    start: Date,
    end: Date,
    /// 3, 6, 9 ou 12.
    months: u8,
    revenue: Option<Decimal>,
    /// DRE 3.03 "Resultado Bruto..." (a linha de cima de banco e seguradora).
    gross_result: Option<Decimal>,
    profit: Option<ProfitParts>,
    source: FilingRef,
    received_on: Option<Date>,
}

/// Quantos meses vão de `start` a `end` (inclusive), se for um número inteiro
/// de trimestres de um exercício: 3, 6, 9 ou 12.
fn quarter_months(start: Date, end: Date) -> Option<u8> {
    let next = end.next_day()?;
    if next.day() != start.day() {
        return None;
    }
    let months = (next.year() - start.year()) * 12
        + (i32::from(u8::from(next.month())) - i32::from(u8::from(start.month())));
    match months {
        3 | 6 | 9 | 12 => u8::try_from(months).ok(),
        _ => None,
    }
}

/// Quando a CVM recebeu cada documento: (DFP ou ITR, data de referência) →
/// data de entrega.
pub type Receipts = HashMap<(FilingKind, Date), Date>;

fn year_to_date(filing: &Filing, column: Column, received: &Receipts) -> Option<YearToDate> {
    let view = Columnar::of(filing, column);
    let revenue_line = view.find(Statement::Income, "3.01");
    let anchor = view
        .lines
        .iter()
        .copied()
        .find(|l| l.statement == Statement::Income && l.period_start.is_some())?;
    let start = anchor.period_start?;
    let end = anchor.period_end;
    let months = quarter_months(start, end)?;
    Some(YearToDate {
        template: Template::from_revenue(revenue_line.map(|l| l.description.as_str())),
        start,
        end,
        months,
        revenue: revenue_line.map(|l| l.value),
        gross_result: view
            .find(Statement::Income, "3.03")
            .filter(|l| fold(&l.description).starts_with("resultado bruto"))
            .map(|l| l.value),
        profit: view.profit_parts(),
        source: reference(filing, column),
        received_on: (column == Column::Current)
            .then(|| received.get(&(filing.kind, filing.reference_date)).copied())
            .flatten(),
    })
}

impl YearToDate {
    fn top_line(&self) -> Option<Decimal> {
        match self.template {
            Template::Commercial => self.revenue,
            Template::Bank | Template::Insurer => self.gross_result,
            Template::Unknown => None,
        }
    }
}

/// `later − earlier`, quando os dois têm o número.
fn difference(later: Option<Decimal>, earlier: Option<Decimal>) -> Figure {
    match (later, earlier) {
        (Some(a), Some(b)) => Figure::known(a - b),
        _ => Figure::missing(Gap::NotReported),
    }
}

/// O lucro dos controladores do trimestre, na mesma base nas duas pontas.
fn profit_difference(later: Option<ProfitParts>, earlier: Option<ProfitParts>) -> Figure {
    match (later, earlier) {
        (Some(a), Some(b)) => match (a.controlling, b.controlling) {
            (Some(x), Some(y)) => Figure::known(x - y),
            _ => Figure::known(a.total - b.total),
        },
        _ => Figure::missing(Gap::NotReported),
    }
}

fn quarter_of(ytd: &YearToDate, earlier: Option<&YearToDate>, holding: bool) -> Option<Quarter> {
    let fiscal_quarter = ytd.months / 3;
    let (period_start, method, top_line, net_income) = match earlier {
        None if fiscal_quarter == 1 => (
            ytd.start,
            QuarterMethod::FirstQuarter,
            ytd.top_line()
                .map_or(Figure::missing(Gap::NotReported), Figure::known),
            ytd.profit.map_or(Figure::missing(Gap::NotReported), |p| {
                Figure::known(p.attributable())
            }),
        ),
        None => return None,
        Some(before) => {
            let method = if fiscal_quarter == 4 {
                QuarterMethod::AnnualLessNineMonths
            } else {
                QuarterMethod::YearToDateDifference
            };
            // Plano trocado entre os dois acumulados: a conta de cima não é a
            // mesma, e a diferença não seria um trimestre.
            let top = if before.template == ytd.template {
                difference(ytd.top_line(), before.top_line())
            } else {
                Figure::missing(Gap::NotReported)
            };
            (
                before.end.next_day()?,
                method,
                top,
                profit_difference(ytd.profit, before.profit),
            )
        }
    };
    let (top_line, net_margin) = match ytd.template {
        Template::Commercial => {
            let margin = if holding {
                Figure::missing(Gap::Holding)
            } else {
                net_income.ratio(top_line)
            };
            (top_line, margin)
        }
        Template::Bank | Template::Insurer => (top_line, Figure::missing(Gap::FinancialTemplate)),
        Template::Unknown => (
            Figure::missing(Gap::UnknownTemplate),
            Figure::missing(Gap::UnknownTemplate),
        ),
    };
    Some(Quarter {
        fiscal_quarter,
        period_start,
        period_end: ytd.end,
        method,
        top_line,
        net_income,
        net_margin,
        source: ytd.source,
        received_on: ytd.received_on,
    })
}

/// Os trimestres da companhia a partir dos documentos dela (DFP e ITR) e da
/// data em que a CVM recebeu cada um. `holding` vem de
/// [`super::CompanyMetrics::holding`]: na holding a margem não se aplica.
pub fn quarterly_results(
    filings: &[Filing],
    received: &Receipts,
    holding: bool,
) -> QuarterlyResults {
    // Um acumulado por período (início, fim): a coluna do documento que o fecha
    // vence a de comparação do ano seguinte; entre iguais, o documento mais novo.
    type Key = (bool, Date, i32);
    let mut by_period: BTreeMap<(Date, Date), (Key, YearToDate)> = BTreeMap::new();
    for filing in filings {
        for column in [Column::Current, Column::Previous] {
            let Some(ytd) = year_to_date(filing, column, received) else {
                continue;
            };
            // Só o ITR tem acumulado de 3, 6 e 9 meses; só a DFP tem o de 12.
            let fits = match filing.kind {
                FilingKind::Annual => ytd.months == 12,
                FilingKind::Quarterly => ytd.months < 12,
            };
            if !fits {
                continue;
            }
            let key = (
                column == Column::Current,
                filing.reference_date,
                filing.version,
            );
            let period = (ytd.start, ytd.end);
            if by_period.get(&period).is_some_and(|(k, _)| *k >= key) {
                continue;
            }
            by_period.insert(period, (key, ytd));
        }
    }
    let by_length: BTreeMap<(Date, u8), &YearToDate> = by_period
        .values()
        .map(|(_, ytd)| ((ytd.start, ytd.months), ytd))
        .collect();
    let mut quarters: Vec<Quarter> = by_period
        .values()
        .filter_map(|(_, ytd)| {
            let earlier = (ytd.months > 3)
                .then(|| by_length.get(&(ytd.start, ytd.months - 3)).copied())
                .flatten();
            if ytd.months > 3 && earlier.is_none() {
                return None;
            }
            quarter_of(ytd, earlier, holding)
        })
        .collect();
    quarters.sort_by_key(|q| q.period_end);

    let latest = quarters.last();
    let year_ago = latest.and_then(|now| {
        quarters
            .iter()
            .find(|q| {
                q.fiscal_quarter == now.fiscal_quarter
                    && q.period_end.year() + 1 == now.period_end.year()
                    && q.period_end.month() == now.period_end.month()
            })
            .map(|before| Comparison::of(before, now))
    });
    let previous_quarter = latest.and_then(|now| {
        quarters
            .iter()
            .find(|q| q.period_end.next_day() == Some(now.period_start))
            .map(|before| Comparison::of(before, now))
    });
    let top_line = by_period
        .values()
        .max_by_key(|(key, ytd)| (ytd.end, *key))
        .map_or(TopLine::Unknown, |(_, ytd)| TopLine::of(ytd.template));
    let skip = quarters.len().saturating_sub(SERIES_QUARTERS);
    QuarterlyResults {
        top_line,
        quarters: quarters.into_iter().skip(skip).collect(),
        year_ago,
        previous_quarter,
    }
}

#[cfg(test)]
mod tests;
