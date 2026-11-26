//! Os indicadores de uma companhia aberta a partir das demonstrações que ela
//! entrega à CVM: a DFP (anual) e o ITR (trimestral).
//!
//! **De onde sai cada número** (contas do plano da CVM; as fixas têm o mesmo
//! código em toda empresa do plano comercial e industrial):
//!
//! | medida | conta |
//! |---|---|
//! | receita líquida | DRE 3.01 |
//! | EBIT | DRE 3.05 (resultado antes do resultado financeiro e dos tributos) |
//! | lucro líquido | DRE "Lucro/Prejuízo ... do Período" (3.11 no plano comercial), a parte atribuída aos controladores quando há |
//! | patrimônio líquido | BPP "Patrimônio Líquido" (2.03 no plano comercial) |
//! | dívida bruta | BPP 2.01.04 + 2.02.01 (empréstimos e financiamentos, curto e longo prazo) |
//! | caixa e aplicações | BPA 1.01.01 + 1.01.02 |
//! | D&A | DVA 7.04.01 (depreciação, amortização e exaustão), em módulo |
//! | caixa operacional | DFC 6.01 |
//!
//! O lucro e o patrimônio são achados pela **descrição** da conta de nível 1,
//! porque o código muda de plano para plano (3.09 e 3.11 nos bancos, 3.13 nas
//! seguradoras; 2.07 e 2.08 no patrimônio dos bancos). O resto só existe no
//! plano comercial: em banco e seguradora é [`Gap::FinancialTemplate`].
//!
//! A explicação inteira, com as fórmulas, está em `docs/fundamentos.md`.

use std::collections::BTreeMap;

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, OffsetDateTime};

use super::{fold, Figure, Gap, RATIO_DP};

/// Quantos exercícios a série anual cobre (o último entregue e os quatro de antes).
pub const WINDOW_YEARS: i32 = 5;

/// A demonstração de onde vem a linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Statement {
    /// Balanço patrimonial ativo (BPA).
    Assets,
    /// Balanço patrimonial passivo, com o patrimônio líquido (BPP).
    Liabilities,
    /// Demonstração do resultado (DRE).
    Income,
    /// Demonstração do fluxo de caixa, método direto ou indireto (DFC).
    CashFlow,
    /// Demonstração do valor adicionado (DVA).
    ValueAdded,
}

impl Statement {
    pub const ALL: [Statement; 5] = [
        Statement::Assets,
        Statement::Liabilities,
        Statement::Income,
        Statement::CashFlow,
        Statement::ValueAdded,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Statement::Assets => "bpa",
            Statement::Liabilities => "bpp",
            Statement::Income => "dre",
            Statement::CashFlow => "dfc",
            Statement::ValueAdded => "dva",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Statement::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// DFP (anual) ou ITR (trimestral).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FilingKind {
    Annual,
    Quarterly,
}

impl FilingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FilingKind::Annual => "annual",
            FilingKind::Quarterly => "quarterly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [FilingKind::Annual, FilingKind::Quarterly]
            .into_iter()
            .find(|k| k.as_str() == value)
    }
}

/// Demonstração consolidada (o grupo) ou individual (só a controladora).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Consolidated,
    Individual,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Consolidated => "consolidated",
            Scope::Individual => "individual",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Scope::Consolidated, Scope::Individual]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

/// A coluna da demonstração: o exercício entregue (`ÚLTIMO`) ou o de
/// comparação (`PENÚLTIMO`), como a CVM publica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    Current,
    Previous,
}

impl Column {
    pub fn as_str(self) -> &'static str {
        match self {
            Column::Current => "current",
            Column::Previous => "previous",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Column::Current, Column::Previous]
            .into_iter()
            .find(|c| c.as_str() == value)
    }
}

/// Uma conta de uma demonstração, já em reais (a escala `MIL` multiplicada).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub statement: Statement,
    pub column: Column,
    /// Início do período (resultado, fluxo de caixa, valor adicionado). `None`
    /// no balanço, que é uma posição numa data.
    pub period_start: Option<Date>,
    pub period_end: Date,
    pub code: String,
    pub description: String,
    pub value: Decimal,
}

/// Um documento entregue (DFP ou ITR de uma data), na versão mais alta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filing {
    pub cvm_code: i32,
    pub kind: FilingKind,
    pub reference_date: Date,
    pub version: i32,
    pub scope: Scope,
    pub company_name: String,
    pub lines: Vec<Line>,
}

/// O nível da conta: `3` é 0, `3.01` é 1, `3.11.01` é 2.
pub fn depth(code: &str) -> usize {
    code.matches('.').count()
}

fn is_profit(description: &str) -> bool {
    let d = fold(description);
    d.starts_with("lucro") && d.contains("periodo")
}

fn is_equity(description: &str) -> bool {
    fold(description).starts_with("patrimonio liquido")
}

fn is_controlling(description: &str) -> bool {
    let d = fold(description);
    d.contains("controlador") && !d.contains("nao controlador")
}

fn is_non_controlling(description: &str) -> bool {
    fold(description).contains("nao controlador")
}

/// As contas que o Prumo guarda de cada demonstração: só as que a conta usa
/// (e as de nível 1 do resultado e do passivo, onde estão o lucro, o
/// patrimônio e o tipo de plano). O resto do arquivo nem é lido para o banco.
pub fn keeps_line(statement: Statement, code: &str, description: &str) -> bool {
    match statement {
        Statement::Income => {
            !code.starts_with("3.99")
                && (depth(code) == 1
                    || (depth(code) == 2 && fold(description).contains("controlador")))
        }
        Statement::Liabilities => {
            depth(code) == 1
                || code == "2.01.04"
                || code == "2.02.01"
                || (depth(code) == 2 && fold(description).contains("controlador"))
        }
        Statement::Assets => code == "1.01.01" || code == "1.01.02",
        Statement::CashFlow => code == "6.01",
        Statement::ValueAdded => code == "7.04.01",
    }
}

/// O plano de contas da demonstração, pela descrição da conta 3.01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    /// Comercial, industrial e de serviços: o plano com todas as contas.
    Commercial,
    /// Instituição financeira (3.01 "Receitas da Intermediação Financeira").
    Bank,
    /// Seguradora, resseguradora, previdência e capitalização (3.01 "Receitas
    /// das Atividades Seguradoras", ou "Receitas das Operações" no plano antigo).
    Insurer,
    /// Outro plano: só lucro e patrimônio, achados pela descrição.
    Unknown,
}

impl Template {
    fn from_revenue(description: Option<&str>) -> Self {
        let Some(d) = description.map(fold) else {
            return Template::Unknown;
        };
        if d.starts_with("receita de venda") {
            Template::Commercial
        } else if d.contains("intermediacao financeira") {
            Template::Bank
        } else if d.contains("segurad")
            || d.contains("previdencia")
            || d.contains("capitalizacao")
            // O plano das seguradoras antes do IFRS 17 (DFP até 2022).
            || d == "receitas das operacoes"
        {
            Template::Insurer
        } else {
            Template::Unknown
        }
    }

    /// O motivo de faltar uma conta que só o plano comercial tem.
    fn absent(self) -> Option<Gap> {
        match self {
            Template::Commercial => None,
            Template::Bank | Template::Insurer => Some(Gap::FinancialTemplate),
            Template::Unknown => Some(Gap::UnknownTemplate),
        }
    }
}

/// O plano de contas de um documento (pela coluna do exercício entregue).
pub fn template_of(filing: &Filing) -> Template {
    Template::from_revenue(
        filing
            .lines
            .iter()
            .find(|l| {
                l.statement == Statement::Income && l.column == Column::Current && l.code == "3.01"
            })
            .map(|l| l.description.as_str()),
    )
}

/// De onde saiu um período: o documento e a coluna.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilingRef {
    pub kind: FilingKind,
    pub reference_date: Date,
    pub version: i32,
    pub scope: Scope,
    pub column: Column,
}

/// Os números de um período de 12 meses (um exercício, ou os últimos 12 meses
/// pela soma DFP + ITR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodFigures {
    pub period_start: Date,
    pub period_end: Date,
    pub template: Template,
    /// O documento (ou os dois, nos 12 meses: a DFP e o ITR).
    pub sources: Vec<FilingRef>,
    pub revenue: Figure,
    /// Lucro líquido atribuído aos controladores (o consolidado, quando a
    /// demonstração não separa).
    pub net_income: Figure,
    /// Patrimônio líquido total (com a participação dos não controladores).
    pub equity: Figure,
    /// Patrimônio líquido dos controladores: a base do ROE e do VPA.
    pub equity_controlling: Figure,
    pub gross_debt: Figure,
    pub cash: Figure,
    /// Dívida bruta menos caixa e aplicações; negativa = caixa líquido.
    pub net_debt: Figure,
    pub ebit: Figure,
    pub depreciation_amortization: Figure,
    /// EBIT + D&A: uma APROXIMAÇÃO do EBITDA (ver `docs/fundamentos.md`).
    pub ebitda: Figure,
    pub operating_cash_flow: Figure,
    /// Lucro dos controladores ÷ patrimônio dos controladores no fim do período.
    pub roe: Figure,
    pub net_margin: Figure,
    pub ebitda_margin: Figure,
    pub net_debt_to_ebitda: Figure,
    pub net_debt_to_equity: Figure,
}

/// Um exercício da série. `figures` é `None` quando não há balanço dele nos
/// arquivos da CVM (ano faltando).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalYear {
    pub year: i32,
    pub figures: Option<PeriodFigures>,
}

/// Quantos exercícios da janela tiveram a medida positiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YearCount {
    pub positive: u8,
    /// Exercícios com o número conhecido.
    pub known: u8,
    /// Exercícios na janela (até [`WINDOW_YEARS`]).
    pub window: u8,
    /// Por que nenhum exercício tem o número (`None` se algum tem).
    pub gap: Option<Gap>,
}

/// O cadastro da companhia na CVM (`cad_cia_aberta.csv`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub cvm_code: i32,
    pub cnpj: String,
    pub name: String,
    /// `SIT`: ATIVO, CANCELADA, SUSPENSO(A).
    pub status: String,
    /// `SIT_EMISSOR`: FASE OPERACIONAL, EM RECUPERAÇÃO JUDICIAL OU
    /// EQUIVALENTE, FALIDA...
    pub issuer_status: Option<String>,
    pub sector: Option<String>,
}

impl Registration {
    pub fn in_judicial_recovery(&self) -> bool {
        self.issuer_status
            .as_deref()
            .is_some_and(|s| fold(s).starts_with("em recuperacao judicial"))
    }
}

/// Os indicadores de uma companhia. Contrato estável: ver
/// `docs/fundamentos.md` e `docs/api.md` (`GET /api/v1/fundamentals`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyMetrics {
    pub cvm_code: i32,
    pub name: String,
    /// O plano de contas do documento mais novo.
    pub template: Template,
    /// Do cadastro; `None` quando o cadastro não tem a companhia.
    pub judicial_recovery: Option<bool>,
    /// `SIT_EMISSOR` como a CVM escreve (falida, liquidação...).
    pub issuer_status: Option<String>,
    /// `SIT` como a CVM escreve (ativo, cancelada, suspensa).
    pub registration_status: Option<String>,
    /// A janela de [`WINDOW_YEARS`] exercícios, do mais velho ao mais novo.
    pub years: Vec<FiscalYear>,
    /// Os últimos 12 meses: DFP + ITR acumulado − ITR do ano anterior quando
    /// há trimestre depois do último exercício; senão, o último exercício. As
    /// datas dizem qual.
    pub trailing: Option<PeriodFigures>,
    pub profitable_years: YearCount,
    pub positive_operating_cash_years: YearCount,
    /// Crescimento anual composto da receita entre as pontas da janela.
    pub revenue_cagr: Figure,
    pub earnings_cagr: Figure,
    /// O documento mais novo lido.
    pub latest_filing: Option<FilingRef>,
    /// Total de ações emitidas (ON + PN), da página do emissor na B3.
    pub shares: Option<Decimal>,
    /// LPA: lucro dos controladores dos últimos 12 meses ÷ ações.
    pub earnings_per_share: Figure,
    /// VPA: patrimônio dos controladores no fim dos 12 meses ÷ ações.
    pub book_value_per_share: Figure,
    /// Quando o Prumo leu a CVM pela última vez com sucesso.
    pub synced_at: Option<OffsetDateTime>,
}

/// As contas de uma coluna de um documento, ainda sem as razões.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Raw {
    template: Template,
    start: Date,
    end: Date,
    revenue: Figure,
    ebit: Figure,
    depreciation: Figure,
    net_income: Figure,
    operating_cash: Figure,
    equity: Figure,
    equity_controlling: Figure,
    gross_debt: Figure,
    cash: Figure,
}

struct Columnar<'a> {
    lines: Vec<&'a Line>,
}

impl<'a> Columnar<'a> {
    fn find(&self, statement: Statement, code: &str) -> Option<&'a Line> {
        self.lines
            .iter()
            .copied()
            .find(|l| l.statement == statement && l.code == code)
    }

    /// A conta `code`, desde que a descrição tenha `needle` (o código fixo com
    /// outro conteúdo não vale).
    fn checked(&self, statement: Statement, code: &str, needle: &str) -> Figure {
        match self.find(statement, code) {
            Some(line) if fold(&line.description).contains(needle) => Figure::known(line.value),
            _ => Figure::missing(Gap::NotReported),
        }
    }

    fn children(&self, statement: Statement, parent: &str) -> impl Iterator<Item = &'a Line> + '_ {
        let prefix = format!("{parent}.");
        self.lines.iter().copied().filter(move |l| {
            l.statement == statement && depth(&l.code) == 2 && l.code.starts_with(&prefix)
        })
    }

    fn net_income(&self) -> Figure {
        let Some(profit) = self.lines.iter().copied().find(|l| {
            l.statement == Statement::Income && depth(&l.code) == 1 && is_profit(&l.description)
        }) else {
            return Figure::missing(Gap::NotReported);
        };
        let children: Vec<&Line> = self.children(Statement::Income, &profit.code).collect();
        let controlling = children.iter().find(|l| is_controlling(&l.description));
        let minority = children.iter().find(|l| is_non_controlling(&l.description));
        // Há companhia sem minoritários que preenche só o total e entrega as
        // duas linhas da divisão zeradas (a Sabesp, de 2023 em diante). Divisão
        // toda em zero com total diferente de zero é divisão não preenchida, e
        // não lucro zero dos controladores.
        let split_blank = !profit.value.is_zero()
            && controlling.is_none_or(|l| l.value.is_zero())
            && minority.is_none_or(|l| l.value.is_zero());
        match controlling {
            Some(own) if !split_blank => Figure::known(own.value),
            _ => Figure::known(profit.value),
        }
    }

    /// O patrimônio total e o dos controladores.
    fn equity(&self) -> (Figure, Figure) {
        let Some(total) = self.lines.iter().copied().find(|l| {
            l.statement == Statement::Liabilities
                && depth(&l.code) == 1
                && is_equity(&l.description)
        }) else {
            return (
                Figure::missing(Gap::NotReported),
                Figure::missing(Gap::NotReported),
            );
        };
        let children: Vec<&Line> = self.children(Statement::Liabilities, &total.code).collect();
        let controlling = if let Some(own) =
            children.iter().find(|l| is_controlling(&l.description))
        {
            own.value
        } else if let Some(minority) = children.iter().find(|l| is_non_controlling(&l.description))
        {
            total.value - minority.value
        } else {
            total.value
        };
        (Figure::known(total.value), Figure::known(controlling))
    }
}

/// As contas de uma coluna. `None` quando a coluna não tem resultado (a DFP
/// da companhia recém-aberta não traz exercício anterior).
fn raw(filing: &Filing, column: Column) -> Option<Raw> {
    let view = Columnar {
        lines: filing.lines.iter().filter(|l| l.column == column).collect(),
    };
    let anchor = view
        .lines
        .iter()
        .copied()
        .find(|l| {
            l.statement == Statement::Income && depth(&l.code) == 1 && is_profit(&l.description)
        })
        .or_else(|| view.find(Statement::Income, "3.01"))?;
    let start = anchor.period_start?;
    let end = anchor.period_end;
    let template = Template::from_revenue(
        view.find(Statement::Income, "3.01")
            .map(|l| l.description.as_str()),
    );
    let (equity, equity_controlling) = view.equity();
    let commercial = |f: &dyn Fn() -> Figure| match template.absent() {
        None => f(),
        Some(gap) => Figure::missing(gap),
    };
    Some(Raw {
        template,
        start,
        end,
        revenue: commercial(&|| {
            view.find(Statement::Income, "3.01")
                .map_or(Figure::missing(Gap::NotReported), |l| {
                    Figure::known(l.value)
                })
        }),
        ebit: commercial(&|| {
            view.checked(
                Statement::Income,
                "3.05",
                "resultado antes do resultado financeiro",
            )
        }),
        depreciation: commercial(&|| {
            let d = view.checked(Statement::ValueAdded, "7.04.01", "deprecia");
            d.value().map_or(d, |v| Figure::known(v.abs()))
        }),
        net_income: view.net_income(),
        operating_cash: commercial(&|| view.checked(Statement::CashFlow, "6.01", "operaciona")),
        equity,
        equity_controlling,
        gross_debt: commercial(&|| {
            view.checked(Statement::Liabilities, "2.01.04", "emprestimo")
                .add(view.checked(Statement::Liabilities, "2.02.01", "emprestimo"))
        }),
        cash: commercial(&|| {
            view.checked(Statement::Assets, "1.01.01", "caixa")
                .add(view.checked(Statement::Assets, "1.01.02", "aplica"))
        }),
    })
}

fn derive(raw: &Raw, sources: Vec<FilingRef>) -> PeriodFigures {
    let ebitda = raw.ebit.add(raw.depreciation);
    let net_debt = raw.gross_debt.sub(raw.cash);
    PeriodFigures {
        period_start: raw.start,
        period_end: raw.end,
        template: raw.template,
        sources,
        revenue: raw.revenue,
        net_income: raw.net_income,
        equity: raw.equity,
        equity_controlling: raw.equity_controlling,
        gross_debt: raw.gross_debt,
        cash: raw.cash,
        net_debt,
        ebit: raw.ebit,
        depreciation_amortization: raw.depreciation,
        ebitda,
        operating_cash_flow: raw.operating_cash,
        roe: raw.net_income.ratio(raw.equity_controlling),
        net_margin: raw.net_income.ratio(raw.revenue),
        ebitda_margin: ebitda.ratio(raw.revenue),
        net_debt_to_ebitda: net_debt.ratio(ebitda),
        net_debt_to_equity: net_debt.ratio(raw.equity),
    }
}

fn reference(filing: &Filing, column: Column) -> FilingRef {
    FilingRef {
        kind: filing.kind,
        reference_date: filing.reference_date,
        version: filing.version,
        scope: filing.scope,
        column,
    }
}

/// Os últimos 12 meses: exercício + acumulado do ano − acumulado do ano
/// anterior, se o ITR mais novo é posterior ao exercício e os períodos se
/// encaixam; senão, o próprio exercício.
fn trailing(filings: &[Filing], annual: &(Raw, FilingRef)) -> PeriodFigures {
    let (a, a_ref) = annual;
    let quarter = filings
        .iter()
        .filter(|f| f.kind == FilingKind::Quarterly && f.reference_date > a.end)
        .max_by_key(|f| (f.reference_date, f.version));
    if let Some(q) = quarter {
        if let (Some(c), Some(p)) = (raw(q, Column::Current), raw(q, Column::Previous)) {
            let fits = a.end.next_day() == Some(c.start)
                && p.start == a.start
                && c.end == q.reference_date
                && p.end.month() == c.end.month()
                && p.end.day() == c.end.day()
                && p.end.year() + 1 == c.end.year();
            if fits {
                let flow = |annual: Figure, current: Figure, previous: Figure| {
                    annual.add(current).sub(previous)
                };
                let ttm = Raw {
                    template: c.template,
                    start: p.end.next_day().unwrap_or(p.end),
                    end: c.end,
                    revenue: flow(a.revenue, c.revenue, p.revenue),
                    ebit: flow(a.ebit, c.ebit, p.ebit),
                    depreciation: flow(a.depreciation, c.depreciation, p.depreciation),
                    net_income: flow(a.net_income, c.net_income, p.net_income),
                    operating_cash: flow(a.operating_cash, c.operating_cash, p.operating_cash),
                    equity: c.equity,
                    equity_controlling: c.equity_controlling,
                    gross_debt: c.gross_debt,
                    cash: c.cash,
                };
                return derive(
                    &ttm,
                    vec![
                        *a_ref,
                        reference(q, Column::Current),
                        reference(q, Column::Previous),
                    ],
                );
            }
        }
    }
    derive(a, vec![*a_ref])
}

fn count(years: &[FiscalYear], pick: impl Fn(&PeriodFigures) -> Figure) -> YearCount {
    let (mut positive, mut known, mut gap) = (0u8, 0u8, None);
    for figures in years.iter().filter_map(|y| y.figures.as_ref()) {
        let figure = pick(figures);
        match figure.value() {
            Some(v) => {
                known += 1;
                if v > Decimal::ZERO {
                    positive += 1;
                }
            }
            None => {
                gap = gap.or(figure.gap());
            }
        }
    }
    YearCount {
        positive,
        known,
        window: u8::try_from(years.len()).unwrap_or(u8::MAX),
        gap: (known == 0).then(|| gap.unwrap_or(Gap::NoFiling)),
    }
}

/// Crescimento anual composto entre o primeiro e o último exercício da janela.
fn cagr(years: &[FiscalYear], pick: impl Fn(&PeriodFigures) -> Figure) -> Figure {
    let ends = (years.first(), years.last());
    let (Some(first), Some(last)) = ends else {
        return Figure::missing(Gap::InsufficientHistory);
    };
    let (Some(a), Some(b)) = (first.figures.as_ref(), last.figures.as_ref()) else {
        return Figure::missing(Gap::InsufficientHistory);
    };
    let periods = last.year - first.year;
    if periods < WINDOW_YEARS - 1 {
        return Figure::missing(Gap::InsufficientHistory);
    }
    pick(a).zip(pick(b), |start, end| {
        if start <= Decimal::ZERO || end <= Decimal::ZERO {
            return Figure::missing(Gap::NonPositiveEndpoint);
        }
        let rate = (end / start)
            .to_f64()
            .map(|r| r.powf(1.0 / f64::from(periods)) - 1.0)
            .and_then(Decimal::from_f64);
        rate.map_or(Figure::missing(Gap::NonPositiveEndpoint), |r| {
            Figure::known(r.round_dp(RATIO_DP).normalize())
        })
    })
}

/// Os indicadores da companhia `cvm_code` a partir dos documentos dela.
/// `None` sem documento nenhum.
pub fn company_metrics(
    cvm_code: i32,
    filings: &[Filing],
    registration: Option<&Registration>,
    shares: Option<Decimal>,
    synced_at: Option<OffsetDateTime>,
) -> Option<CompanyMetrics> {
    let latest = filings
        .iter()
        .max_by_key(|f| (f.reference_date, f.kind == FilingKind::Annual, f.version))?;

    // Um candidato por exercício: o da coluna do exercício entregue vence o da
    // coluna de comparação do ano seguinte; entre iguais, o documento mais novo.
    type Key = (bool, Date, i32);
    let mut by_year: BTreeMap<i32, (Key, Raw, FilingRef)> = BTreeMap::new();
    for filing in filings.iter().filter(|f| f.kind == FilingKind::Annual) {
        for column in [Column::Current, Column::Previous] {
            let Some(r) = raw(filing, column) else {
                continue;
            };
            let key = (
                column == Column::Current,
                filing.reference_date,
                filing.version,
            );
            let year = r.end.year();
            if by_year.get(&year).is_some_and(|(k, _, _)| *k >= key) {
                continue;
            }
            by_year.insert(year, (key, r, reference(filing, column)));
        }
    }
    let last_year = by_year
        .iter()
        .filter(|(_, (key, _, _))| key.0)
        .map(|(year, _)| *year)
        .max();
    let years: Vec<FiscalYear> = last_year
        .map(|last| {
            (last - WINDOW_YEARS + 1..=last)
                .map(|year| FiscalYear {
                    year,
                    figures: by_year.get(&year).map(|(_, r, s)| derive(r, vec![*s])),
                })
                .collect()
        })
        .unwrap_or_default();
    let trailing = last_year
        .and_then(|year| by_year.get(&year))
        .map(|(_, r, s)| trailing(filings, &(r.clone(), *s)));
    let per_share = |pick: fn(&PeriodFigures) -> Figure| {
        let divisor = shares
            .filter(|s| *s > Decimal::ZERO)
            .map_or(Figure::missing(Gap::NoShares), Figure::known);
        match &trailing {
            Some(period) => pick(period).ratio(divisor),
            None => Figure::missing(Gap::NoFiling),
        }
    };
    let earnings_per_share = per_share(|p| p.net_income);
    let book_value_per_share = per_share(|p| p.equity_controlling);

    Some(CompanyMetrics {
        cvm_code,
        name: registration
            .map(|r| r.name.clone())
            .unwrap_or_else(|| latest.company_name.clone()),
        template: template_of(latest),
        judicial_recovery: registration.map(Registration::in_judicial_recovery),
        issuer_status: registration.and_then(|r| r.issuer_status.clone()),
        registration_status: registration.map(|r| r.status.clone()),
        profitable_years: count(&years, |p| p.net_income),
        positive_operating_cash_years: count(&years, |p| p.operating_cash_flow),
        revenue_cagr: cagr(&years, |p| p.revenue),
        earnings_cagr: cagr(&years, |p| p.net_income),
        years,
        trailing,
        latest_filing: Some(reference(latest, Column::Current)),
        shares,
        earnings_per_share,
        book_value_per_share,
        synced_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    /// Uma linha de fluxo (resultado, DFC, DVA) de um exercício de calendário.
    fn flow(
        statement: Statement,
        column: Column,
        year: i32,
        code: &str,
        desc: &str,
        value: Decimal,
    ) -> Line {
        Line {
            statement,
            column,
            period_start: Date::from_calendar_date(year, time::Month::January, 1).ok(),
            period_end: Date::from_calendar_date(year, time::Month::December, 31).unwrap(),
            code: code.to_owned(),
            description: desc.to_owned(),
            value,
        }
    }

    fn balance(
        statement: Statement,
        column: Column,
        end: Date,
        code: &str,
        desc: &str,
        value: Decimal,
    ) -> Line {
        Line {
            statement,
            column,
            period_start: None,
            period_end: end,
            code: code.to_owned(),
            description: desc.to_owned(),
            value,
        }
    }

    /// Os números fictícios de um exercício de uma empresa industrial.
    #[derive(Clone, Copy)]
    struct Year {
        revenue: Decimal,
        ebit: Decimal,
        da: Decimal,
        profit: Decimal,
        controlling: Decimal,
        equity: Decimal,
        minority: Decimal,
        debt_short: Decimal,
        debt_long: Decimal,
        cash: Decimal,
        investments: Decimal,
        operating: Decimal,
    }

    fn year_of(revenue: Decimal, profit: Decimal) -> Year {
        Year {
            revenue,
            ebit: profit * dec!(1.5),
            da: dec!(100),
            profit,
            controlling: profit,
            equity: dec!(5000),
            minority: dec!(0),
            debt_short: dec!(300),
            debt_long: dec!(700),
            cash: dec!(200),
            investments: dec!(100),
            operating: profit + dec!(50),
        }
    }

    fn commercial_lines(column: Column, year: i32, y: &Year) -> Vec<Line> {
        let end = Date::from_calendar_date(year, time::Month::December, 31).unwrap();
        vec![
            flow(
                Statement::Income,
                column,
                year,
                "3.01",
                "Receita de Venda de Bens e/ou Serviços",
                y.revenue,
            ),
            flow(
                Statement::Income,
                column,
                year,
                "3.05",
                "Resultado Antes do Resultado Financeiro e dos Tributos",
                y.ebit,
            ),
            flow(
                Statement::Income,
                column,
                year,
                "3.11",
                "Lucro/Prejuízo Consolidado do Período",
                y.profit,
            ),
            flow(
                Statement::Income,
                column,
                year,
                "3.11.01",
                "Atribuído a Sócios da Empresa Controladora",
                y.controlling,
            ),
            flow(
                Statement::Income,
                column,
                year,
                "3.11.02",
                "Atribuído a Sócios Não Controladores",
                y.profit - y.controlling,
            ),
            flow(
                Statement::ValueAdded,
                column,
                year,
                "7.04.01",
                "Depreciação, Amortização e Exaustão",
                -y.da,
            ),
            flow(
                Statement::CashFlow,
                column,
                year,
                "6.01",
                "Caixa Líquido Atividades Operacionais",
                y.operating,
            ),
            balance(
                Statement::Liabilities,
                column,
                end,
                "2.03",
                "Patrimônio Líquido Consolidado",
                y.equity,
            ),
            balance(
                Statement::Liabilities,
                column,
                end,
                "2.03.09",
                "Participação dos Acionistas Não Controladores",
                y.minority,
            ),
            balance(
                Statement::Liabilities,
                column,
                end,
                "2.01.04",
                "Empréstimos e Financiamentos",
                y.debt_short,
            ),
            balance(
                Statement::Liabilities,
                column,
                end,
                "2.02.01",
                "Empréstimos e Financiamentos",
                y.debt_long,
            ),
            balance(
                Statement::Assets,
                column,
                end,
                "1.01.01",
                "Caixa e Equivalentes de Caixa",
                y.cash,
            ),
            balance(
                Statement::Assets,
                column,
                end,
                "1.01.02",
                "Aplicações Financeiras",
                y.investments,
            ),
        ]
    }

    fn dfp(year: i32, current: &Year, previous: Option<&Year>, version: i32) -> Filing {
        let mut lines = commercial_lines(Column::Current, year, current);
        if let Some(p) = previous {
            lines.extend(commercial_lines(Column::Previous, year - 1, p));
        }
        Filing {
            cvm_code: 99901,
            kind: FilingKind::Annual,
            reference_date: Date::from_calendar_date(year, time::Month::December, 31).unwrap(),
            version,
            scope: Scope::Consolidated,
            company_name: "INDUSTRIA FICTICIA SA".into(),
            lines,
        }
    }

    /// Um ITR de 30/09 com o acumulado do ano e o do ano anterior.
    fn itr(year: i32, ytd: &Year, previous_ytd: &Year) -> Filing {
        let shift = |lines: Vec<Line>, y: i32| -> Vec<Line> {
            lines
                .into_iter()
                .map(|mut l| {
                    l.period_end = Date::from_calendar_date(y, time::Month::September, 30).unwrap();
                    l
                })
                .collect()
        };
        let mut lines = shift(commercial_lines(Column::Current, year, ytd), year);
        lines.extend(shift(
            commercial_lines(Column::Previous, year - 1, previous_ytd),
            year - 1,
        ));
        Filing {
            cvm_code: 99901,
            kind: FilingKind::Quarterly,
            reference_date: Date::from_calendar_date(year, time::Month::September, 30).unwrap(),
            version: 1,
            scope: Scope::Consolidated,
            company_name: "INDUSTRIA FICTICIA SA".into(),
            lines,
        }
    }

    fn five_years() -> Vec<Filing> {
        let revenues = [dec!(1000), dec!(1100), dec!(1210), dec!(1331), dec!(1464.1)];
        let profits = [dec!(100), dec!(110), dec!(121), dec!(133.1), dec!(146.41)];
        (0..5)
            .map(|i| {
                let current = year_of(revenues[i], profits[i]);
                let previous = (i > 0).then(|| year_of(revenues[i - 1], profits[i - 1]));
                dfp(2021 + i as i32, &current, previous.as_ref(), 1)
            })
            .collect()
    }

    fn registration(issuer_status: &str) -> Registration {
        Registration {
            cvm_code: 99901,
            cnpj: "00.000.000/0001-00".into(),
            name: "INDUSTRIA FICTICIA S.A.".into(),
            status: "ATIVO".into(),
            issuer_status: Some(issuer_status.into()),
            sector: Some("Máquinas".into()),
        }
    }

    #[test]
    fn five_years_of_an_industrial_company() {
        let m = company_metrics(
            99901,
            &five_years(),
            Some(&registration("FASE OPERACIONAL")),
            None,
            None,
        )
        .unwrap();
        assert_eq!(m.template, Template::Commercial);
        assert_eq!(m.name, "INDUSTRIA FICTICIA S.A.");
        assert_eq!(m.judicial_recovery, Some(false));
        assert_eq!(
            m.years.iter().map(|y| y.year).collect::<Vec<_>>(),
            vec![2021, 2022, 2023, 2024, 2025]
        );
        let last = m.years[4].figures.as_ref().unwrap();
        assert_eq!(last.revenue.value(), Some(dec!(1464.1)));
        assert_eq!(last.net_income.value(), Some(dec!(146.41)));
        assert_eq!(last.gross_debt.value(), Some(dec!(1000)));
        assert_eq!(last.cash.value(), Some(dec!(300)));
        assert_eq!(last.net_debt.value(), Some(dec!(700)));
        assert_eq!(last.depreciation_amortization.value(), Some(dec!(100)));
        // EBIT 146,41 × 1,5 = 219,615; + D&A 100.
        assert_eq!(last.ebitda.value(), Some(dec!(319.615)));
        assert_eq!(last.roe.value(), Some(dec!(0.029282)));
        assert_eq!(last.net_margin.value(), Some(dec!(0.1)));
        assert_eq!(last.net_debt_to_equity.value(), Some(dec!(0.14)));
        assert_eq!(last.net_debt_to_ebitda.value(), Some(dec!(2.190135)));
        assert_eq!(last.operating_cash_flow.value(), Some(dec!(196.41)));
        // 10% ao ano, quatro passos de 2021 a 2025.
        assert_eq!(m.revenue_cagr.value(), Some(dec!(0.1)));
        assert_eq!(m.earnings_cagr.value(), Some(dec!(0.1)));
        assert_eq!(
            m.profitable_years,
            YearCount {
                positive: 5,
                known: 5,
                window: 5,
                gap: None
            }
        );
        assert_eq!(m.positive_operating_cash_years.positive, 5);
        // Sem ITR, os 12 meses são o último exercício.
        let ttm = m.trailing.unwrap();
        assert_eq!(ttm.period_end, date!(2025 - 12 - 31));
        assert_eq!(ttm.sources.len(), 1);
        assert_eq!(
            m.latest_filing.unwrap().reference_date,
            date!(2025 - 12 - 31)
        );
    }

    #[test]
    fn the_last_twelve_months_add_the_quarters_after_the_year() {
        let mut filings = five_years();
        let ytd = Year {
            operating: dec!(90),
            ..year_of(dec!(1200), dec!(130))
        };
        let previous_ytd = Year {
            operating: dec!(70),
            ..year_of(dec!(1000), dec!(100))
        };
        filings.push(itr(2026, &ytd, &previous_ytd));
        let m = company_metrics(99901, &filings, None, None, None).unwrap();
        let ttm = m.trailing.unwrap();
        assert_eq!(ttm.period_start, date!(2025 - 10 - 01));
        assert_eq!(ttm.period_end, date!(2026 - 09 - 30));
        // 1464,1 + 1200 − 1000
        assert_eq!(ttm.revenue.value(), Some(dec!(1664.1)));
        assert_eq!(ttm.net_income.value(), Some(dec!(176.41)));
        assert_eq!(ttm.operating_cash_flow.value(), Some(dec!(216.41)));
        // O balanço é o do trimestre.
        assert_eq!(ttm.equity.value(), Some(dec!(5000)));
        assert_eq!(ttm.sources.len(), 3);
        assert_eq!(ttm.sources[1].kind, FilingKind::Quarterly);
        // Sem cadastro: nome do documento, recuperação desconhecida.
        assert_eq!(m.name, "INDUSTRIA FICTICIA SA");
        assert_eq!(m.judicial_recovery, None);
        assert_eq!(m.latest_filing.unwrap().kind, FilingKind::Quarterly);
    }

    #[test]
    fn a_quarter_that_does_not_fit_falls_back_to_the_year() {
        let mut filings = five_years();
        // ITR de 2027: o ano de 2026 inteiro falta entre a DFP e ele.
        filings.push(itr(
            2027,
            &year_of(dec!(1), dec!(1)),
            &year_of(dec!(1), dec!(1)),
        ));
        let ttm = company_metrics(99901, &filings, None, None, None)
            .unwrap()
            .trailing
            .unwrap();
        assert_eq!(ttm.period_end, date!(2025 - 12 - 31));
        assert_eq!(ttm.revenue.value(), Some(dec!(1464.1)));
    }

    #[test]
    fn a_bank_has_profit_equity_and_roe_only() {
        let lines = vec![
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.01",
                "Receitas da Intermediação Financeira",
                dec!(9000),
            ),
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.05",
                "Resultado Antes dos Tributos sobre o Lucro",
                dec!(1500),
            ),
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.09",
                "Lucro/Prejuízo Consolidado do Período",
                dec!(1200),
            ),
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.09.01",
                "Atribuído a Sócios da Empresa Controladora",
                dec!(1000),
            ),
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.09.02",
                "Atribuído a Sócios Não Controladores",
                dec!(200),
            ),
            flow(
                Statement::CashFlow,
                Column::Current,
                2025,
                "6.01",
                "Caixa Líquido Atividades Operacionais",
                dec!(-50000),
            ),
            balance(
                Statement::Liabilities,
                Column::Current,
                date!(2025 - 12 - 31),
                "2.03",
                "Passivos Financeiros ao Custo Amortizado",
                dec!(900000),
            ),
            balance(
                Statement::Liabilities,
                Column::Current,
                date!(2025 - 12 - 31),
                "2.07",
                "Patrimônio Líquido Consolidado",
                dec!(11000),
            ),
            balance(
                Statement::Liabilities,
                Column::Current,
                date!(2025 - 12 - 31),
                "2.07.01",
                "Patrimônio Líquido Atribuído ao Controlador",
                dec!(10000),
            ),
            balance(
                Statement::Liabilities,
                Column::Current,
                date!(2025 - 12 - 31),
                "2.07.02",
                "Patrimônio Líquido Atribuído aos Não Controladores",
                dec!(1000),
            ),
        ];
        let filing = Filing {
            cvm_code: 99902,
            kind: FilingKind::Annual,
            reference_date: date!(2025 - 12 - 31),
            version: 2,
            scope: Scope::Consolidated,
            company_name: "BANCO FICTICIO SA".into(),
            lines,
        };
        let m = company_metrics(99902, &[filing], None, None, None).unwrap();
        assert_eq!(m.template, Template::Bank);
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.net_income.value(), Some(dec!(1000)));
        assert_eq!(y.equity.value(), Some(dec!(11000)));
        // Lucro e patrimônio dos controladores.
        assert_eq!(y.roe.value(), Some(dec!(0.1)));
        for figure in [
            y.revenue,
            y.ebit,
            y.ebitda,
            y.gross_debt,
            y.net_debt,
            y.operating_cash_flow,
            y.net_margin,
            y.net_debt_to_ebitda,
        ] {
            assert_eq!(figure.gap(), Some(Gap::FinancialTemplate));
        }
        assert_eq!(
            m.positive_operating_cash_years.gap,
            Some(Gap::FinancialTemplate)
        );
        assert_eq!(m.revenue_cagr.gap(), Some(Gap::InsufficientHistory));
        // Só um exercício: os outros quatro da janela faltam.
        assert_eq!(
            m.profitable_years,
            YearCount {
                positive: 1,
                known: 1,
                window: 5,
                gap: None
            }
        );
        assert!(m.years[..4].iter().all(|y| y.figures.is_none()));
    }

    #[test]
    fn an_insurer_finds_its_profit_by_description() {
        let lines = vec![
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.01",
                "Receitas das Atividades Seguradoras/Resseguradoras",
                dec!(0),
            ),
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.11",
                "Resultado Líquido das Operações Continuadas",
                dec!(900),
            ),
            flow(
                Statement::Income,
                Column::Current,
                2025,
                "3.13",
                "Lucro/Prejuízo Consolidado do Período",
                dec!(900),
            ),
            balance(
                Statement::Liabilities,
                Column::Current,
                date!(2025 - 12 - 31),
                "2.03",
                "Patrimônio Líquido Consolidado",
                dec!(9000),
            ),
        ];
        let filing = Filing {
            cvm_code: 99903,
            kind: FilingKind::Annual,
            reference_date: date!(2025 - 12 - 31),
            version: 1,
            scope: Scope::Consolidated,
            company_name: "SEGURADORA FICTICIA SA".into(),
            lines,
        };
        let m = company_metrics(99903, &[filing], None, None, None).unwrap();
        assert_eq!(m.template, Template::Insurer);
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.net_income.value(), Some(dec!(900)));
        assert_eq!(y.roe.value(), Some(dec!(0.1)));
        assert_eq!(y.revenue.gap(), Some(Gap::FinancialTemplate));
    }

    #[test]
    fn losses_and_negative_equity() {
        let mut filings = five_years();
        let bad = Year {
            equity: dec!(-800),
            ..year_of(dec!(900), dec!(-300))
        };
        filings[4] = dfp(2025, &bad, Some(&year_of(dec!(1331), dec!(133.1))), 1);
        let m = company_metrics(
            99901,
            &filings,
            Some(&registration("EM RECUPERAÇÃO JUDICIAL OU EQUIVALENTE")),
            None,
            None,
        )
        .unwrap();
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.net_income.value(), Some(dec!(-300)));
        assert_eq!(y.roe.gap(), Some(Gap::NonPositiveBase));
        assert_eq!(y.net_debt_to_equity.gap(), Some(Gap::NonPositiveBase));
        // EBIT −450 + D&A 100: EBITDA negativo não divide.
        assert_eq!(y.ebitda.value(), Some(dec!(-350)));
        assert_eq!(y.net_debt_to_ebitda.gap(), Some(Gap::NonPositiveBase));
        // Margem negativa é número.
        assert_eq!(y.net_margin.value(), Some(dec!(-0.333333)));
        assert_eq!(m.earnings_cagr.gap(), Some(Gap::NonPositiveEndpoint));
        assert!(m.revenue_cagr.value().is_some());
        assert_eq!(m.profitable_years.positive, 4);
        assert_eq!(m.positive_operating_cash_years.positive, 4);
        assert_eq!(m.judicial_recovery, Some(true));
    }

    #[test]
    fn a_missing_year_is_filled_by_the_next_comparison_column() {
        let mut filings = five_years();
        // Sem a DFP de 2023: a coluna de comparação da DFP de 2024 cobre.
        filings.remove(2);
        let m = company_metrics(99901, &filings, None, None, None).unwrap();
        let y2023 = m.years[2].figures.as_ref().unwrap();
        assert_eq!(y2023.revenue.value(), Some(dec!(1210)));
        assert_eq!(y2023.sources[0].column, Column::Previous);
        assert_eq!(y2023.sources[0].reference_date, date!(2024 - 12 - 31));
        // Sem 2023 e sem 2024: 2023 fica vazio.
        let mut filings = five_years();
        filings.remove(3);
        filings.remove(2);
        let m = company_metrics(99901, &filings, None, None, None).unwrap();
        assert!(m.years[2].figures.is_none());
        assert!(m.years[3].figures.is_some());
        assert_eq!(m.profitable_years.known, 4);
        // A primeira ponta da janela faltando: sem CAGR.
        let filings: Vec<Filing> = five_years().into_iter().skip(2).collect();
        let m = company_metrics(99901, &filings, None, None, None).unwrap();
        assert_eq!(m.years[0].year, 2021);
        assert!(m.years[0].figures.is_none());
        // 2022 vem da comparação da DFP de 2023; 2021 não tem fonte.
        assert!(m.years[1].figures.is_some());
        assert_eq!(m.revenue_cagr.gap(), Some(Gap::InsufficientHistory));
    }

    #[test]
    fn the_current_column_beats_the_restated_comparison() {
        // A DFP de 2025 reapresenta 2024 com outro número na comparação; o
        // exercício 2024 vale como foi entregue na DFP dele.
        let mut filings = five_years();
        filings[4] = dfp(
            2025,
            &year_of(dec!(1464.1), dec!(146.41)),
            Some(&year_of(dec!(9999), dec!(1))),
            1,
        );
        let m = company_metrics(99901, &filings, None, None, None).unwrap();
        assert_eq!(
            m.years[3].figures.as_ref().unwrap().revenue.value(),
            Some(dec!(1331))
        );
    }

    #[test]
    fn missing_accounts_are_not_reported_never_zero() {
        let mut filing = five_years().remove(4);
        filing
            .lines
            .retain(|l| l.code != "7.04.01" && l.code != "2.02.01" && l.code != "1.01.02");
        // Conta fixa com outro conteúdo também não vale.
        for line in &mut filing.lines {
            if line.code == "6.01" {
                line.description = "Outra coisa".into();
            }
        }
        let m = company_metrics(99901, &[filing], None, None, None).unwrap();
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.depreciation_amortization.gap(), Some(Gap::NotReported));
        assert_eq!(y.ebitda.gap(), Some(Gap::NotReported));
        assert_eq!(y.gross_debt.gap(), Some(Gap::NotReported));
        assert_eq!(y.cash.gap(), Some(Gap::NotReported));
        assert_eq!(y.net_debt.gap(), Some(Gap::NotReported));
        assert_eq!(y.operating_cash_flow.gap(), Some(Gap::NotReported));
        assert_eq!(m.positive_operating_cash_years.gap, Some(Gap::NotReported));
        assert_eq!(y.revenue.value(), Some(dec!(1464.1)));
    }

    #[test]
    fn without_a_controlling_split_the_consolidated_profit_counts() {
        let mut filing = five_years().remove(4);
        filing
            .lines
            .retain(|l| !l.code.starts_with("3.11.") && !l.code.starts_with("2.03."));
        let m = company_metrics(99901, &[filing], None, None, None).unwrap();
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.net_income.value(), Some(dec!(146.41)));
        assert_eq!(y.roe.value(), Some(dec!(0.029282)));
    }

    #[test]
    fn a_split_filed_as_zeros_is_a_split_not_filled() {
        let mut filing = five_years().remove(4);
        for line in &mut filing.lines {
            if line.code.starts_with("3.11.") {
                line.value = Decimal::ZERO;
            }
        }
        let m = company_metrics(99901, &[filing], None, None, None).unwrap();
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.net_income.value(), Some(dec!(146.41)));
    }

    #[test]
    fn a_real_loss_with_a_filled_split_stays_a_loss_of_the_controllers() {
        let mut filing = five_years().remove(4);
        for line in &mut filing.lines {
            match line.code.as_str() {
                "3.11" => line.value = dec!(-50),
                "3.11.01" => line.value = dec!(-60),
                "3.11.02" => line.value = dec!(10),
                _ => {}
            }
        }
        let m = company_metrics(99901, &[filing], None, None, None).unwrap();
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(y.net_income.value(), Some(dec!(-60)));
    }

    #[test]
    fn roe_uses_the_controlling_shareholders_equity() {
        let y = Year {
            controlling: dec!(80),
            minority: dec!(1000),
            ..year_of(dec!(1000), dec!(100))
        };
        let m = company_metrics(99901, &[dfp(2025, &y, None, 1)], None, None, None).unwrap();
        let f = m.years[4].figures.as_ref().unwrap();
        assert_eq!(f.net_income.value(), Some(dec!(80)));
        // 80 ÷ (5000 − 1000)
        assert_eq!(f.roe.value(), Some(dec!(0.02)));
        // A dívida líquida é do grupo: divide pelo patrimônio total.
        assert_eq!(f.net_debt_to_equity.value(), Some(dec!(0.14)));
    }

    #[test]
    fn the_template_comes_from_the_revenue_account() {
        assert_eq!(
            Template::from_revenue(Some("Receita de Venda de Bens e/ou Serviços")),
            Template::Commercial
        );
        assert_eq!(
            Template::from_revenue(Some("Receitas de Intermediação Financeira")),
            Template::Bank
        );
        assert_eq!(
            Template::from_revenue(Some("Receitas da Intermediação Financeira")),
            Template::Bank
        );
        assert_eq!(
            Template::from_revenue(Some("Receitas das Atividades Seguradoras/Resseguradoras")),
            Template::Insurer
        );
        assert_eq!(
            Template::from_revenue(Some("Receitas das Operações")),
            Template::Insurer
        );
        assert_eq!(Template::from_revenue(Some("Algo novo")), Template::Unknown);
        assert_eq!(Template::from_revenue(None), Template::Unknown);
        let mut filing = five_years().remove(4);
        for line in &mut filing.lines {
            if line.code == "3.01" {
                line.description = "Plano novo".into();
            }
        }
        let m = company_metrics(99901, &[filing], None, None, None).unwrap();
        let y = m.years[4].figures.as_ref().unwrap();
        assert_eq!(m.template, Template::Unknown);
        assert_eq!(y.revenue.gap(), Some(Gap::UnknownTemplate));
        assert_eq!(y.net_income.value(), Some(dec!(146.41)));
    }

    #[test]
    fn only_the_accounts_in_use_are_kept() {
        assert!(keeps_line(Statement::Income, "3.01", "Receita"));
        assert!(keeps_line(
            Statement::Income,
            "3.11.01",
            "Atribuído a Sócios da Empresa Controladora"
        ));
        assert!(!keeps_line(
            Statement::Income,
            "3.04.01",
            "Despesas com Vendas"
        ));
        assert!(!keeps_line(
            Statement::Income,
            "3.99.01",
            "Lucro Básico por Ação controlador"
        ));
        assert!(keeps_line(
            Statement::Liabilities,
            "2.03",
            "Patrimônio Líquido Consolidado"
        ));
        assert!(keeps_line(
            Statement::Liabilities,
            "2.03.09",
            "Participação dos Acionistas Não Controladores"
        ));
        assert!(keeps_line(
            Statement::Liabilities,
            "2.01.04",
            "Empréstimos e Financiamentos"
        ));
        assert!(!keeps_line(
            Statement::Liabilities,
            "2.01.01",
            "Obrigações Sociais"
        ));
        assert!(keeps_line(Statement::Assets, "1.01.02", "Aplicações"));
        assert!(!keeps_line(Statement::Assets, "1.01", "Ativo Circulante"));
        assert!(keeps_line(Statement::CashFlow, "6.01", "Caixa"));
        assert!(!keeps_line(Statement::CashFlow, "6.01.01", "Caixa"));
        assert!(keeps_line(Statement::ValueAdded, "7.04.01", "Depreciação"));
        assert!(!keeps_line(Statement::ValueAdded, "7.01", "Receitas"));
    }

    #[test]
    fn per_share_figures_use_the_b3_share_count() {
        let y = Year {
            controlling: dec!(120),
            minority: dec!(1000),
            ..year_of(dec!(1000), dec!(146.41))
        };
        let m = company_metrics(
            99901,
            &[dfp(2025, &y, None, 1)],
            None,
            Some(dec!(100)),
            None,
        )
        .unwrap();
        assert_eq!(m.shares, Some(dec!(100)));
        // Lucro e patrimônio dos controladores: 120 ÷ 100 e (5000 − 1000) ÷ 100.
        assert_eq!(m.earnings_per_share.value(), Some(dec!(1.2)));
        assert_eq!(m.book_value_per_share.value(), Some(dec!(40)));
        let f = m.years[4].figures.as_ref().unwrap();
        assert_eq!(f.equity_controlling.value(), Some(dec!(4000)));
        // Prejuízo dá LPA negativo; sem ações, o motivo.
        let loss = company_metrics(
            99901,
            &[dfp(2025, &year_of(dec!(1), dec!(-50)), None, 1)],
            None,
            Some(dec!(10)),
            None,
        )
        .unwrap();
        assert_eq!(loss.earnings_per_share.value(), Some(dec!(-5)));
        let none = company_metrics(99901, &[dfp(2025, &y, None, 1)], None, None, None).unwrap();
        assert_eq!(none.earnings_per_share.gap(), Some(Gap::NoShares));
        assert_eq!(none.book_value_per_share.gap(), Some(Gap::NoShares));
        let zero =
            company_metrics(99901, &[dfp(2025, &y, None, 1)], None, Some(dec!(0)), None).unwrap();
        assert_eq!(zero.earnings_per_share.gap(), Some(Gap::NoShares));
        // Só ITR: sem 12 meses, sem LPA.
        let quarter =
            company_metrics(99901, &[itr(2026, &y, &y)], None, Some(dec!(10)), None).unwrap();
        assert_eq!(quarter.earnings_per_share.gap(), Some(Gap::NoFiling));
    }

    #[test]
    fn no_filings_no_metrics() {
        assert!(company_metrics(1, &[], None, None, None).is_none());
        // Só ITR: sem exercício, sem série nem 12 meses.
        let m = company_metrics(
            99901,
            &[itr(
                2026,
                &year_of(dec!(1), dec!(1)),
                &year_of(dec!(1), dec!(1)),
            )],
            None,
            None,
            None,
        )
        .unwrap();
        assert!(m.years.is_empty());
        assert!(m.trailing.is_none());
        assert_eq!(m.profitable_years.gap, Some(Gap::NoFiling));
        assert_eq!(m.revenue_cagr.gap(), Some(Gap::InsufficientHistory));
    }

    #[test]
    fn statement_and_enum_codes_round_trip() {
        for s in Statement::ALL {
            assert_eq!(Statement::parse(s.as_str()), Some(s));
        }
        for k in [FilingKind::Annual, FilingKind::Quarterly] {
            assert_eq!(FilingKind::parse(k.as_str()), Some(k));
        }
        for s in [Scope::Consolidated, Scope::Individual] {
            assert_eq!(Scope::parse(s.as_str()), Some(s));
        }
        for c in [Column::Current, Column::Previous] {
            assert_eq!(Column::parse(c.as_str()), Some(c));
        }
        assert_eq!(Statement::parse("dmpl"), None);
    }

    proptest! {
        /// Com o acumulado do ano igual ao do ano anterior, os 12 meses são o
        /// exercício.
        #[test]
        fn equal_quarters_keep_the_year(revenue in 1i64..10_000_000, profit in -1_000_000i64..1_000_000, ytd in 0i64..5_000_000) {
            let mut filings = vec![dfp(2025, &year_of(Decimal::from(revenue), Decimal::from(profit)), None, 1)];
            let same = year_of(Decimal::from(ytd), Decimal::from(ytd / 10));
            filings.push(itr(2026, &same, &same));
            let m = company_metrics(99901, &filings, None, None, None).unwrap();
            let ttm = m.trailing.unwrap();
            prop_assert_eq!(ttm.revenue.value(), Some(Decimal::from(revenue)));
            prop_assert_eq!(ttm.net_income.value(), Some(Decimal::from(profit)));
            prop_assert_eq!(ttm.period_end, date!(2026 - 09 - 30));
        }

        /// O CAGR devolve a taxa que gerou a série.
        #[test]
        fn cagr_recovers_the_growth(start in 1i64..1_000_000, rate in -50i64..100) {
            let g = Decimal::from(rate) / dec!(100);
            let start = Decimal::from(start);
            let filings: Vec<Filing> = (0..5)
                .map(|i| {
                    let mut v = start;
                    for _ in 0..i { v *= Decimal::ONE + g; }
                    dfp(2021 + i, &year_of(v, v), None, 1)
                })
                .collect();
            let m = company_metrics(99901, &filings, None, None, None).unwrap();
            let got = m.revenue_cagr.value().unwrap();
            prop_assert!((got - g).abs() <= dec!(0.00001), "{} contra {}", got, g);
        }

        /// ROE tem o sinal do lucro quando o patrimônio é positivo.
        #[test]
        fn roe_has_the_sign_of_the_profit(profit in -1_000_000i64..1_000_000, equity in 1i64..10_000_000) {
            let y = Year { equity: Decimal::from(equity), ..year_of(dec!(1000), Decimal::from(profit)) };
            let m = company_metrics(99901, &[dfp(2025, &y, None, 1)], None, None, None).unwrap();
            let roe = m.years[4].figures.as_ref().unwrap().roe.value().unwrap();
            prop_assert_eq!(roe.is_sign_negative() && !roe.is_zero(), profit < 0 && roe != Decimal::ZERO);
        }
    }
}
