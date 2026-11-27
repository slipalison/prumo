//! Qualidade dos ativos e pontos de atenção: o que os fundamentos da CVM
//! (`domain/fundamentals`, `docs/fundamentos.md`) dizem das empresas e dos FIIs
//! da carteira.
//!
//! Cada regra de Qualidade pergunta **quanto do valor** de um grupo de ativos
//! (as empresas, ou os FIIs) passa num critério — "pelo menos X% do valor em
//! ações vem de empresas com ROE de 10% ou mais" — e só pesa quem tem o dado.
//! Ativo sem número (banco numa conta que o plano dele não tem, ano sem
//! balanço, fundo de papel na vacância) fica **fora** da conta, nunca vira
//! reprovado. Os cortes de cada critério são do Prumo e estão nas constantes
//! abaixo e em `docs/analise.md`.
//!
//! Os pontos de atenção são avisos curtos e factuais por ativo (lucro em
//! queda, ROE baixo, dívida maior que o patrimônio, recuperação judicial...).
//! Não recomendam nada.

use rust_decimal::Decimal;
use uuid::Uuid;

use super::AnalysisAsset;
use crate::domain::allocation::percent;
use crate::domain::asset_class::AssetClass;
use crate::domain::fundamentals::{
    AssetFundamentals, CompanyMetrics, FundMetrics, PeriodFigures, Template,
};
use crate::domain::health::{self, IncomeHistory};
use crate::domain::income;
use crate::domain::performance::months_before;

/// ROE mínimo (fração) de uma empresa "boa" na regra de ROE e no ponto de atenção.
pub const MIN_ROE: Decimal = Decimal::from_parts(10, 0, 0, false, 2);
/// Anos com lucro positivo, entre os 5 da janela, que a regra pede.
pub const MIN_PROFIT_YEARS: u8 = 4;
/// Dívida líquida sobre EBITDA máxima (vezes).
pub const MAX_NET_DEBT_TO_EBITDA: Decimal = Decimal::from_parts(3, 0, 0, false, 0);
/// Quantos exercícios entram na média do caixa operacional.
pub const CASH_YEARS: usize = 4;
/// P/VP máximo de um FII "no preço" (fração: 1,10 é 10% acima do patrimônio).
pub const MAX_FUND_PRICE_TO_BOOK: Decimal = Decimal::from_parts(110, 0, 0, false, 2);
/// Vacância física máxima (fração) de um FII de tijolo.
pub const MAX_FUND_VACANCY: Decimal = Decimal::from_parts(15, 0, 0, false, 2);
/// Crescimento anual composto abaixo disso vira ponto de atenção (fração).
pub const MIN_GROWTH: Decimal = Decimal::from_parts(5, 0, 0, false, 2);
/// Margem líquida abaixo disso vira ponto de atenção (fração).
pub const MIN_NET_MARGIN: Decimal = Decimal::from_parts(10, 0, 0, false, 2);
/// Dívida líquida acima do patrimônio líquido (vezes) vira ponto de atenção.
pub const MAX_NET_DEBT_TO_EQUITY: Decimal = Decimal::ONE;
/// Queda da renda de 12 meses (em %) a partir da qual vira ponto de atenção.
pub const MIN_INCOME_DROP: Decimal = Decimal::from_parts(10, 0, 0, false, 0);

/// Que tipo de empresa é, para dizer onde uma medida **não se aplica**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Indústria, comércio, serviços (e plano que o Prumo não reconhece).
    Operating,
    /// Holding de participações (`CompanyMetrics::holding`): o lucro vem da
    /// equivalência patrimonial, não da receita.
    Holding,
    Bank,
    Insurer,
}

impl Profile {
    pub fn of(metrics: &CompanyMetrics) -> Self {
        match metrics.template {
            Template::Bank => Profile::Bank,
            Template::Insurer => Profile::Insurer,
            _ if metrics.holding => Profile::Holding,
            _ => Profile::Operating,
        }
    }

    /// Banco ou seguradora: o plano da CVM não tem dívida, EBITDA nem receita.
    pub fn financial(self) -> bool {
        matches!(self, Profile::Bank | Profile::Insurer)
    }
}

/// O que a regra sabe de uma empresa. `None` = sem número (o motivo mora nos
/// fundamentos); `profile` diz onde dívida, EBITDA e receita **não se
/// aplicam** (banco, seguradora e holding).
#[derive(Debug, Clone, PartialEq)]
pub struct CompanyFacts {
    pub profile: Profile,
    pub roe: Option<Decimal>,
    /// Exercícios com lucro positivo e quantos têm o número.
    pub profit_years: Option<(u8, u8)>,
    pub net_debt_to_ebitda: Option<Decimal>,
    pub ebitda_margin: Option<Decimal>,
    pub net_margin: Option<Decimal>,
    pub net_debt_to_equity: Option<Decimal>,
    /// Média do caixa operacional dos últimos [`CASH_YEARS`] exercícios; só
    /// com todos conhecidos.
    pub operating_cash_average: Option<Decimal>,
    pub revenue_cagr: Option<Decimal>,
    pub earnings_cagr: Option<Decimal>,
    /// `None`: o cadastro da CVM não tem a companhia.
    pub judicial_recovery: Option<bool>,
    /// Lucro e patrimônio por ação (reais), quando a B3 informou as ações.
    pub per_share: Option<(Decimal, Decimal)>,
}

/// O que a regra sabe de um FII.
#[derive(Debug, Clone, PartialEq)]
pub struct FundFacts {
    pub price_to_book: Option<Decimal>,
    /// Vacância física (fração); `None` no fundo de papel ou sem informe.
    pub vacancy: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Facts {
    Company(CompanyFacts),
    Fund(FundFacts),
}

/// Um ativo da carteira com os fundamentos que a Análise usa.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityAsset {
    pub id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    /// Valor atual em reais: o peso do ativo nas regras.
    pub value: Decimal,
    pub facts: Facts,
}

/// O período que as razões descrevem: os últimos 12 meses ou, sem eles, o
/// exercício mais novo.
fn current_period(metrics: &CompanyMetrics) -> Option<&PeriodFigures> {
    metrics
        .trailing
        .as_ref()
        .or_else(|| metrics.years.iter().rev().find_map(|y| y.figures.as_ref()))
}

impl CompanyFacts {
    pub fn from_metrics(metrics: &CompanyMetrics) -> Self {
        let period = current_period(metrics);
        let ratio = |pick: fn(&PeriodFigures) -> Option<Decimal>| period.and_then(pick);
        let known_cash: Vec<Decimal> = metrics
            .years
            .iter()
            .filter_map(|y| y.figures.as_ref())
            .filter_map(|f| f.operating_cash_flow.value())
            .collect();
        let operating_cash_average = (known_cash.len() >= CASH_YEARS)
            .then(|| &known_cash[known_cash.len() - CASH_YEARS..])
            .map(|last| last.iter().sum::<Decimal>() / Decimal::from(CASH_YEARS));
        let years = metrics.profitable_years;
        Self {
            profile: Profile::of(metrics),
            roe: ratio(|p| p.roe.value()),
            profit_years: (years.known > 0).then_some((years.positive, years.known)),
            net_debt_to_ebitda: ratio(|p| p.net_debt_to_ebitda.value()),
            ebitda_margin: ratio(|p| p.ebitda_margin.value()),
            net_margin: ratio(|p| p.net_margin.value()),
            net_debt_to_equity: ratio(|p| p.net_debt_to_equity.value()),
            operating_cash_average,
            revenue_cagr: metrics.revenue_cagr.value(),
            earnings_cagr: metrics.earnings_cagr.value(),
            judicial_recovery: metrics.judicial_recovery,
            per_share: super::references::company_per_share(metrics),
        }
    }
}

impl FundFacts {
    pub fn from_metrics(metrics: &FundMetrics) -> Self {
        Self {
            price_to_book: metrics.price_to_book.value(),
            vacancy: metrics.vacancy.value(),
        }
    }
}

/// Junta a carteira e os fundamentos de cada símbolo. Ativo sem valor, sem
/// símbolo ou sem fundamentos (ETF, BDR, exterior, ainda não buscado) fica de
/// fora.
pub fn quality_assets(
    assets: &[AnalysisAsset],
    fundamentals: &std::collections::HashMap<String, AssetFundamentals>,
) -> Vec<QualityAsset> {
    assets
        .iter()
        .filter(|a| a.value > Decimal::ZERO)
        .filter_map(|asset| {
            let found = fundamentals.get(asset.symbol.as_deref()?)?;
            let facts = match found {
                AssetFundamentals::Company(metrics) => {
                    Facts::Company(CompanyFacts::from_metrics(metrics))
                }
                AssetFundamentals::Fund(metrics) => Facts::Fund(FundFacts::from_metrics(metrics)),
                AssetFundamentals::Unsupported(_) => return None,
            };
            Some(QualityAsset {
                id: asset.id,
                label: asset.label.clone(),
                asset_class: asset.asset_class,
                value: asset.value,
                facts,
            })
        })
        .collect()
}

// ------------------------------------------------------------------ regras --

/// O critério de uma regra de Qualidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    Roe,
    ProfitYears,
    Leverage,
    EbitdaMargin,
    OperatingCash,
    FundPriceToBook,
    FundVacancy,
}

/// Por que a medida do critério não se aplica à empresa (`None`: se aplica).
/// Dívida sobre EBITDA e margem EBITDA não existem no plano de banco e
/// seguradora e, na holding, o EBITDA leva a equivalência patrimonial; o caixa
/// operacional não existe no plano de banco e seguradora.
pub fn not_applicable(criterion: Criterion, facts: &Facts) -> Option<Profile> {
    let Facts::Company(c) = facts else {
        return None;
    };
    match (criterion, c.profile) {
        (Criterion::Leverage | Criterion::EbitdaMargin, Profile::Operating) => None,
        (Criterion::Leverage | Criterion::EbitdaMargin, profile) => Some(profile),
        (Criterion::OperatingCash, profile) if profile.financial() => Some(profile),
        _ => None,
    }
}

/// Passa (`Some(true)`), reprova (`Some(false)`) ou não entra na conta
/// (`None`: sem número, ou a medida não se aplica ao ativo).
pub fn verdict(criterion: Criterion, facts: &Facts) -> Option<bool> {
    if not_applicable(criterion, facts).is_some() {
        return None;
    }
    match (criterion, facts) {
        (Criterion::Roe, Facts::Company(c)) => c.roe.map(|v| v >= MIN_ROE),
        (Criterion::ProfitYears, Facts::Company(c)) => c
            .profit_years
            .filter(|(_, known)| *known >= MIN_PROFIT_YEARS)
            .map(|(positive, _)| positive >= MIN_PROFIT_YEARS),
        (Criterion::Leverage, Facts::Company(c)) => {
            c.net_debt_to_ebitda.map(|v| v <= MAX_NET_DEBT_TO_EBITDA)
        }
        (Criterion::EbitdaMargin, Facts::Company(c)) => c.ebitda_margin.map(|v| v > Decimal::ZERO),
        (Criterion::OperatingCash, Facts::Company(c)) => {
            c.operating_cash_average.map(|v| v > Decimal::ZERO)
        }
        (Criterion::FundPriceToBook, Facts::Fund(f)) => {
            f.price_to_book.map(|v| v <= MAX_FUND_PRICE_TO_BOOK)
        }
        (Criterion::FundVacancy, Facts::Fund(f)) => f.vacancy.map(|v| v <= MAX_FUND_VACANCY),
        _ => None,
    }
}

/// O resultado de uma regra ponderada por valor.
#[derive(Debug, Clone, PartialEq)]
pub struct Weighted {
    /// Percentual do valor (de quem tem o dado) que passa; duas casas.
    pub share: Decimal,
    /// Ativos que entraram na conta e quantos o grupo tem.
    pub covered: u32,
    pub total: u32,
    /// Os que reprovaram, do maior valor para o menor.
    pub failing: Vec<String>,
    /// Os do grupo em que a medida não se aplica (banco, seguradora, holding),
    /// do maior valor para o menor: ficam fora da conta, e a tela diz por quê.
    pub not_applicable: Vec<(String, Profile)>,
}

fn is_company(a: &QualityAsset) -> bool {
    matches!(a.facts, Facts::Company(_))
}

/// Pondera o critério pelo valor. `None` quando nenhum ativo do grupo tem o dado.
pub fn weighted(assets: &[QualityAsset], criterion: Criterion) -> Option<Weighted> {
    let in_group = |a: &&QualityAsset| match criterion {
        Criterion::FundPriceToBook | Criterion::FundVacancy => !is_company(a),
        _ => is_company(a),
    };
    let mut group: Vec<&QualityAsset> = assets.iter().filter(in_group).collect();
    group.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    let mut judged: Vec<(&QualityAsset, bool)> = group
        .iter()
        .filter_map(|a| verdict(criterion, &a.facts).map(|pass| (*a, pass)))
        .collect();
    if judged.is_empty() {
        return None;
    }
    judged.sort_by(|(a, _), (b, _)| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    let whole: Decimal = judged.iter().map(|(a, _)| a.value).sum();
    let passing: Decimal = judged
        .iter()
        .filter(|(_, p)| *p)
        .map(|(a, _)| a.value)
        .sum();
    Some(Weighted {
        share: percent(passing, whole),
        covered: u32::try_from(judged.len()).unwrap_or(u32::MAX),
        total: u32::try_from(group.len()).unwrap_or(u32::MAX),
        failing: judged
            .iter()
            .filter(|(_, pass)| !*pass)
            .map(|(a, _)| a.label.clone())
            .collect(),
        not_applicable: group
            .iter()
            .filter_map(|a| not_applicable(criterion, &a.facts).map(|p| (a.label.clone(), p)))
            .collect(),
    })
}

/// As empresas em recuperação judicial. `None` se nenhuma empresa da carteira
/// tem a informação (cadastro sem a companhia).
pub fn judicial_recovery(assets: &[QualityAsset]) -> Option<Weighted> {
    let known: Vec<(&QualityAsset, bool)> = assets
        .iter()
        .filter_map(|a| match &a.facts {
            Facts::Company(c) => c.judicial_recovery.map(|v| (a, v)),
            Facts::Fund(_) => None,
        })
        .collect();
    if known.is_empty() {
        return None;
    }
    let failing: Vec<String> = known
        .iter()
        .filter(|(_, in_recovery)| *in_recovery)
        .map(|(a, _)| a.label.clone())
        .collect();
    let total = assets.iter().filter(|a| is_company(a)).count();
    Some(Weighted {
        share: Decimal::ZERO,
        covered: u32::try_from(known.len()).unwrap_or(u32::MAX),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        failing,
        not_applicable: Vec::new(),
    })
}

// ---------------------------------------------------------------- atenção --

/// Um aviso curto e factual sobre um ativo. Os números são percentuais (ou
/// vezes, em `DebtAboveEquity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    /// A renda de 12 meses do ativo caiu `percent` em relação aos 12 anteriores.
    IncomeDrop {
        percent: Decimal,
    },
    LowRevenueGrowth {
        percent: Decimal,
    },
    LowEarningsGrowth {
        percent: Decimal,
    },
    LowRoe {
        percent: Decimal,
    },
    LowNetMargin {
        percent: Decimal,
    },
    /// Dívida líquida sobre patrimônio líquido (vezes), acima de 1.
    DebtAboveEquity {
        times: Decimal,
    },
    JudicialRecovery,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetAttention {
    pub id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attention {
    /// Ativos com fundamentos (empresa ou FII).
    pub assessed: u32,
    /// Ações e FIIs com valor que ainda não têm fundamentos.
    pub without_data: u32,
    /// Do maior valor para o menor; só quem tem ponto.
    pub items: Vec<AssetAttention>,
}

fn pct(fraction: Decimal) -> Decimal {
    (fraction * Decimal::ONE_HUNDRED).round_dp(2)
}

/// Quanto cada ativo pagou nos 12 meses que terminam no mês de hoje e nos 12
/// anteriores; `None` se o histórico não cobre as duas janelas.
fn income_windows(
    income: &IncomeHistory,
) -> Option<(income::IncomeSummary, income::IncomeSummary)> {
    let received = health::received_entries(income);
    if health::history_months(&received, income.today) < super::MIN_GROWTH_HISTORY_MONTHS {
        return None;
    }
    Some((
        income::summarize(&income.entries, income.today),
        income::summarize(&income.entries, months_before(income.today, 12)),
    ))
}

fn company_points(facts: &CompanyFacts) -> Vec<Point> {
    let mut points = Vec::new();
    if facts.judicial_recovery == Some(true) {
        points.push(Point::JudicialRecovery);
    }
    if let Some(roe) = facts.roe.filter(|v| *v < MIN_ROE) {
        points.push(Point::LowRoe { percent: pct(roe) });
    }
    if let Some(margin) = facts.net_margin.filter(|v| *v < MIN_NET_MARGIN) {
        points.push(Point::LowNetMargin {
            percent: pct(margin),
        });
    }
    if let Some(times) = facts
        .net_debt_to_equity
        .filter(|v| *v > MAX_NET_DEBT_TO_EQUITY)
    {
        points.push(Point::DebtAboveEquity {
            times: times.round_dp(2),
        });
    }
    if let Some(cagr) = facts.revenue_cagr.filter(|v| *v < MIN_GROWTH) {
        points.push(Point::LowRevenueGrowth { percent: pct(cagr) });
    }
    if let Some(cagr) = facts.earnings_cagr.filter(|v| *v < MIN_GROWTH) {
        points.push(Point::LowEarningsGrowth { percent: pct(cagr) });
    }
    points
}

/// Os pontos de atenção da carteira: fundamentos das empresas e a queda da
/// renda de cada ativo.
pub fn attention(
    assets: &[AnalysisAsset],
    quality: &[QualityAsset],
    income: &IncomeHistory,
) -> Attention {
    let windows = income_windows(income);
    let total_of = |summary: &income::IncomeSummary, key: &str| {
        summary
            .by_payer
            .iter()
            .find(|p| p.key == key)
            .map_or(Decimal::ZERO, |p| p.total)
    };
    let mut owned: Vec<&AnalysisAsset> =
        assets.iter().filter(|a| a.value > Decimal::ZERO).collect();
    owned.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));

    let mut items = Vec::new();
    for asset in &owned {
        let mut points = Vec::new();
        if let Some((now, before)) = &windows {
            let key = asset.id.to_string();
            let (current, previous) = (total_of(now, &key), total_of(before, &key));
            if previous > Decimal::ZERO && current < previous {
                let drop = percent(previous - current, previous);
                if drop >= MIN_INCOME_DROP {
                    points.push(Point::IncomeDrop { percent: drop });
                }
            }
        }
        if let Some(Facts::Company(facts)) =
            quality.iter().find(|q| q.id == asset.id).map(|q| &q.facts)
        {
            points.extend(company_points(facts));
        }
        if !points.is_empty() {
            items.push(AssetAttention {
                id: asset.id,
                label: asset.label.clone(),
                asset_class: asset.asset_class,
                points,
            });
        }
    }
    let covered = |a: &&AnalysisAsset| quality.iter().any(|q| q.id == a.id);
    let without_data = owned
        .iter()
        .filter(|a| matches!(a.asset_class, AssetClass::BrStocks | AssetClass::BrReits))
        .filter(|a| !covered(a))
        .count();
    Attention {
        assessed: u32::try_from(quality.len()).unwrap_or(u32::MAX),
        without_data: u32::try_from(without_data).unwrap_or(u32::MAX),
        items,
    }
}

#[cfg(test)]
mod tests;
