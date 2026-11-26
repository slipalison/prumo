//! Análise da carteira: as regras do Prumo organizadas em dimensões —
//! Diversificação, Dividendos, Rentabilidade e Qualidade dos ativos.
//!
//! Cada regra é uma pergunta de sim ou não com o **número de agora**, o limite
//! que a pessoa escolhe (quando a regra tem) e, quando dá para dizer, quanto
//! aportar para voltar ao limite. A pontuação de uma dimensão é a mais simples
//! possível: o **percentual das regras ligadas, com dado, que estão ok**, com
//! o número exato à mostra ("5 de 8"). Não há peso, nota composta nem
//! metodologia de terceiros; o desenho e os limiares estão em `docs/analise.md`.
//!
//! Regra **sem dado não é regra falhada**: série curta, poucos meses de
//! provento, classe sem fonte de setor... a regra fica `no_data`, fora da
//! pontuação e fora da contagem de pontos de atenção.
//!
//! As sete regras que já existiam na Saúde da carteira (`domain/health.rs`)
//! continuam sendo avaliadas por lá — o motor de avisos e os alertas do
//! Telegram dependem dele — e entram aqui nas dimensões certas.
//!
//! **Qualidade dos ativos** lê os fundamentos da CVM (`domain/fundamentals`):
//! cada regra pergunta quanto do **valor** de um grupo de ativos passa num
//! critério, e só pesa quem tem o dado ([`quality`]).
//!
//! Tudo aqui é puro: recebe fatos já prontos e devolve resultados.

pub mod highlights;
pub mod profile;
pub mod quality;
pub mod references;
pub mod returns;

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::allocation::{percent, round_cents, Targets};
use super::asset_class::AssetClass;
use super::health::{self, HealthAsset, HealthConfig, IncomeHistory, RuleSetting, Unit, Warning};
use super::income;
use super::performance::{months_before, to_percent};
use quality::{Criterion, QualityAsset, Weighted};
use returns::{Benchmark, Comparison, Horizon, ReturnsFacts, YearsFact};

const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

/// Menos meses de proventos registrados que isso e as regras que olham 12
/// meses de renda (yield, quem pagou) ficam sem dado: a janela de um ano
/// estaria pela metade e a conta sairia falsamente baixa.
pub const MIN_YIELD_HISTORY_MONTHS: i32 = 12;
/// Para comparar a renda de 12 meses com a dos 12 anteriores, o histórico
/// precisa cobrir as duas janelas.
pub const MIN_GROWTH_HISTORY_MONTHS: i32 = 24;
/// O tipo de FII que protege da inflação (o mesmo texto que a fonte da CVM
/// grava em `sector_reference`).
const BRICK_SECTOR: &str = "Tijolo";

/// Maior limite aceito para as regras de contagem.
/// Percentual do valor que precisa passar nas regras de Qualidade, de fábrica.
const DEFAULT_QUALITY_SHARE: i64 = 70;
const MAX_PORTFOLIO_ASSETS: u32 = 100;
const MAX_CLASSES: u32 = 7;
const MAX_SECTORS: u32 = 30;

// ------------------------------------------------------------- dimensões --

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Diversification,
    Dividends,
    Returns,
    Quality,
}

impl Dimension {
    /// A ordem da tela.
    pub const ALL: [Dimension; 4] = [
        Dimension::Diversification,
        Dimension::Dividends,
        Dimension::Returns,
        Dimension::Quality,
    ];
}

/// O que é "bom" no número da regra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// O número precisa ficar em ou abaixo do limite.
    AtMost,
    /// O número precisa ficar em ou acima do limite.
    AtLeast,
}

// ---------------------------------------------------------------- regras --

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RuleId {
    // Diversificação.
    #[serde(rename = "max_asset")]
    MaxAsset,
    #[serde(rename = "top_five")]
    TopFive,
    #[serde(rename = "max_sector")]
    MaxSector,
    #[serde(rename = "min_assets")]
    MinAssets,
    #[serde(rename = "empty_class")]
    EmptyClass,
    #[serde(rename = "min_portfolio_assets")]
    MinPortfolioAssets,
    #[serde(rename = "min_classes")]
    MinClasses,
    #[serde(rename = "min_sectors")]
    MinSectors,
    #[serde(rename = "min_abroad")]
    MinAbroad,
    #[serde(rename = "min_low_risk")]
    MinLowRisk,
    // Dividendos.
    #[serde(rename = "min_yield")]
    MinYield,
    #[serde(rename = "min_paying_share")]
    MinPayingShare,
    #[serde(rename = "income_growth")]
    IncomeGrowth,
    #[serde(rename = "income_every_month")]
    IncomeEveryMonth,
    #[serde(rename = "max_payer")]
    MaxPayer,
    #[serde(rename = "brick_reit")]
    BrickReit,
    // Rentabilidade.
    #[serde(rename = "beat_cdi_12m")]
    BeatCdi12m,
    #[serde(rename = "beat_cdi_5y")]
    BeatCdi5y,
    #[serde(rename = "beat_ibov_12m")]
    BeatIbov12m,
    #[serde(rename = "beat_ibov_5y")]
    BeatIbov5y,
    #[serde(rename = "beat_ipca_12m")]
    BeatIpca12m,
    #[serde(rename = "beat_ipca_5y")]
    BeatIpca5y,
    #[serde(rename = "min_positive_years")]
    MinPositiveYears,
    #[serde(rename = "max_volatility")]
    MaxVolatility,
    #[serde(rename = "max_loss")]
    MaxLoss,
    // Qualidade dos ativos.
    #[serde(rename = "quality_roe")]
    QualityRoe,
    #[serde(rename = "quality_profit")]
    QualityProfit,
    #[serde(rename = "quality_leverage")]
    QualityLeverage,
    #[serde(rename = "quality_margin")]
    QualityMargin,
    #[serde(rename = "quality_cash")]
    QualityCash,
    #[serde(rename = "quality_recovery")]
    QualityRecovery,
    #[serde(rename = "quality_fii_pvp")]
    QualityFiiPvp,
    #[serde(rename = "quality_fii_vacancy")]
    QualityFiiVacancy,
}

/// Quantas regras existem.
pub const RULE_COUNT: usize = 33;

impl RuleId {
    /// A ordem da tela, dimensão por dimensão.
    pub const ALL: [RuleId; RULE_COUNT] = [
        RuleId::MaxAsset,
        RuleId::TopFive,
        RuleId::MaxSector,
        RuleId::MinAssets,
        RuleId::EmptyClass,
        RuleId::MinPortfolioAssets,
        RuleId::MinClasses,
        RuleId::MinSectors,
        RuleId::MinAbroad,
        RuleId::MinLowRisk,
        RuleId::MinYield,
        RuleId::MinPayingShare,
        RuleId::IncomeGrowth,
        RuleId::IncomeEveryMonth,
        RuleId::MaxPayer,
        RuleId::BrickReit,
        RuleId::BeatCdi12m,
        RuleId::BeatCdi5y,
        RuleId::BeatIbov12m,
        RuleId::BeatIbov5y,
        RuleId::BeatIpca12m,
        RuleId::BeatIpca5y,
        RuleId::MinPositiveYears,
        RuleId::MaxVolatility,
        RuleId::MaxLoss,
        RuleId::QualityRoe,
        RuleId::QualityProfit,
        RuleId::QualityLeverage,
        RuleId::QualityMargin,
        RuleId::QualityCash,
        RuleId::QualityRecovery,
        RuleId::QualityFiiPvp,
        RuleId::QualityFiiVacancy,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RuleId::MaxAsset => "max_asset",
            RuleId::TopFive => "top_five",
            RuleId::MaxSector => "max_sector",
            RuleId::MinAssets => "min_assets",
            RuleId::EmptyClass => "empty_class",
            RuleId::MinPortfolioAssets => "min_portfolio_assets",
            RuleId::MinClasses => "min_classes",
            RuleId::MinSectors => "min_sectors",
            RuleId::MinAbroad => "min_abroad",
            RuleId::MinLowRisk => "min_low_risk",
            RuleId::MinYield => "min_yield",
            RuleId::MinPayingShare => "min_paying_share",
            RuleId::IncomeGrowth => "income_growth",
            RuleId::IncomeEveryMonth => "income_every_month",
            RuleId::MaxPayer => "max_payer",
            RuleId::BrickReit => "brick_reit",
            RuleId::BeatCdi12m => "beat_cdi_12m",
            RuleId::BeatCdi5y => "beat_cdi_5y",
            RuleId::BeatIbov12m => "beat_ibov_12m",
            RuleId::BeatIbov5y => "beat_ibov_5y",
            RuleId::BeatIpca12m => "beat_ipca_12m",
            RuleId::BeatIpca5y => "beat_ipca_5y",
            RuleId::MinPositiveYears => "min_positive_years",
            RuleId::MaxVolatility => "max_volatility",
            RuleId::MaxLoss => "max_loss",
            RuleId::QualityRoe => "quality_roe",
            RuleId::QualityProfit => "quality_profit",
            RuleId::QualityLeverage => "quality_leverage",
            RuleId::QualityMargin => "quality_margin",
            RuleId::QualityCash => "quality_cash",
            RuleId::QualityRecovery => "quality_recovery",
            RuleId::QualityFiiPvp => "quality_fii_pvp",
            RuleId::QualityFiiVacancy => "quality_fii_vacancy",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        RuleId::ALL.into_iter().find(|r| r.as_str() == value)
    }

    pub fn dimension(self) -> Dimension {
        match self {
            RuleId::MaxAsset
            | RuleId::TopFive
            | RuleId::MaxSector
            | RuleId::MinAssets
            | RuleId::EmptyClass
            | RuleId::MinPortfolioAssets
            | RuleId::MinClasses
            | RuleId::MinSectors
            | RuleId::MinAbroad
            | RuleId::MinLowRisk => Dimension::Diversification,
            RuleId::MinYield
            | RuleId::MinPayingShare
            | RuleId::IncomeGrowth
            | RuleId::IncomeEveryMonth
            | RuleId::MaxPayer
            | RuleId::BrickReit => Dimension::Dividends,
            RuleId::BeatCdi12m
            | RuleId::BeatCdi5y
            | RuleId::BeatIbov12m
            | RuleId::BeatIbov5y
            | RuleId::BeatIpca12m
            | RuleId::BeatIpca5y
            | RuleId::MinPositiveYears
            | RuleId::MaxVolatility
            | RuleId::MaxLoss => Dimension::Returns,
            RuleId::QualityRoe
            | RuleId::QualityProfit
            | RuleId::QualityLeverage
            | RuleId::QualityMargin
            | RuleId::QualityCash
            | RuleId::QualityRecovery
            | RuleId::QualityFiiPvp
            | RuleId::QualityFiiVacancy => Dimension::Quality,
        }
    }

    /// A regra que já existia na Saúde da carteira, se for uma delas.
    pub fn legacy(self) -> Option<health::RuleId> {
        match self {
            RuleId::MaxAsset => Some(health::RuleId::MaxAsset),
            RuleId::TopFive => Some(health::RuleId::TopFive),
            RuleId::MaxSector => Some(health::RuleId::MaxSector),
            RuleId::MinAssets => Some(health::RuleId::MinAssets),
            RuleId::EmptyClass => Some(health::RuleId::EmptyClass),
            RuleId::IncomeEveryMonth => Some(health::RuleId::IncomeEveryMonth),
            RuleId::MaxPayer => Some(health::RuleId::MaxPayer),
            _ => None,
        }
    }

    pub fn from_legacy(rule: health::RuleId) -> Self {
        match rule {
            health::RuleId::MaxAsset => RuleId::MaxAsset,
            health::RuleId::TopFive => RuleId::TopFive,
            health::RuleId::MaxSector => RuleId::MaxSector,
            health::RuleId::MinAssets => RuleId::MinAssets,
            health::RuleId::EmptyClass => RuleId::EmptyClass,
            health::RuleId::IncomeEveryMonth => RuleId::IncomeEveryMonth,
            health::RuleId::MaxPayer => RuleId::MaxPayer,
        }
    }

    /// Com o que a regra compara a carteira, nas de rentabilidade contra índice.
    pub fn benchmark(self) -> Option<(Benchmark, Horizon)> {
        match self {
            RuleId::BeatCdi12m => Some((Benchmark::Cdi, Horizon::OneYear)),
            RuleId::BeatCdi5y => Some((Benchmark::Cdi, Horizon::FiveYears)),
            RuleId::BeatIbov12m => Some((Benchmark::Ibov, Horizon::OneYear)),
            RuleId::BeatIbov5y => Some((Benchmark::Ibov, Horizon::FiveYears)),
            RuleId::BeatIpca12m => Some((Benchmark::Ipca, Horizon::OneYear)),
            RuleId::BeatIpca5y => Some((Benchmark::Ipca, Horizon::FiveYears)),
            _ => None,
        }
    }

    /// O critério de uma regra de Qualidade ponderada por valor.
    pub fn criterion(self) -> Option<Criterion> {
        match self {
            RuleId::QualityRoe => Some(Criterion::Roe),
            RuleId::QualityProfit => Some(Criterion::ProfitYears),
            RuleId::QualityLeverage => Some(Criterion::Leverage),
            RuleId::QualityMargin => Some(Criterion::EbitdaMargin),
            RuleId::QualityCash => Some(Criterion::OperatingCash),
            RuleId::QualityFiiPvp => Some(Criterion::FundPriceToBook),
            RuleId::QualityFiiVacancy => Some(Criterion::FundVacancy),
            _ => None,
        }
    }

    /// Em que o número da regra (e o limite) é medido.
    pub fn value_unit(self) -> Unit {
        match self {
            RuleId::MinAssets
            | RuleId::EmptyClass
            | RuleId::MinPortfolioAssets
            | RuleId::MinClasses
            | RuleId::MinSectors
            | RuleId::IncomeEveryMonth
            | RuleId::BrickReit
            | RuleId::QualityRecovery => Unit::Count,
            _ => Unit::Percent,
        }
    }

    /// O limite de fábrica; `None` na regra que não tem limite editável.
    pub fn default_threshold(self) -> Option<Decimal> {
        if let Some(legacy) = self.legacy() {
            return legacy.default_threshold();
        }
        let value = match self {
            RuleId::MinPortfolioAssets => 10,
            RuleId::MinClasses => 3,
            RuleId::MinSectors => 4,
            RuleId::MinAbroad | RuleId::MinLowRisk => 10,
            RuleId::MinYield => 5,
            RuleId::MinPayingShare | RuleId::MinPositiveYears => 70,
            RuleId::MaxVolatility => 20,
            RuleId::MaxLoss => 30,
            _ if self.criterion().is_some() => DEFAULT_QUALITY_SHARE,
            _ => return None,
        };
        Some(Decimal::from(value))
    }

    /// O limite é editável?
    pub fn editable(self) -> bool {
        self.default_threshold().is_some()
    }

    /// O que é "bom" no número; `None` nas regras sem limite.
    pub fn direction(self) -> Option<Direction> {
        match self {
            RuleId::MaxAsset
            | RuleId::TopFive
            | RuleId::MaxSector
            | RuleId::MaxPayer
            | RuleId::MaxVolatility
            | RuleId::MaxLoss => Some(Direction::AtMost),
            RuleId::MinAssets
            | RuleId::MinPortfolioAssets
            | RuleId::MinClasses
            | RuleId::MinSectors
            | RuleId::MinAbroad
            | RuleId::MinLowRisk
            | RuleId::MinYield
            | RuleId::MinPayingShare
            | RuleId::IncomeEveryMonth
            | RuleId::MinPositiveYears
            | RuleId::BeatCdi12m
            | RuleId::BeatCdi5y
            | RuleId::BeatIbov12m
            | RuleId::BeatIbov5y
            | RuleId::BeatIpca12m
            | RuleId::BeatIpca5y
            | RuleId::QualityRoe
            | RuleId::QualityProfit
            | RuleId::QualityLeverage
            | RuleId::QualityMargin
            | RuleId::QualityCash
            | RuleId::QualityFiiPvp
            | RuleId::QualityFiiVacancy => Some(Direction::AtLeast),
            RuleId::EmptyClass
            | RuleId::IncomeGrowth
            | RuleId::BrickReit
            | RuleId::QualityRecovery => None,
        }
    }

    /// Confere o limite que a pessoa mandou e o devolve normalizado. A
    /// mensagem de erro vai sem acento (atravessa log e JSON de erro).
    pub fn validate_threshold(self, value: Decimal) -> Result<Decimal, String> {
        if let Some(legacy) = self.legacy() {
            return legacy.validate_threshold(value);
        }
        if !self.editable() {
            return Err("Essa regra nao tem limite.".to_owned());
        }
        match self.value_unit() {
            Unit::Percent => {
                if value > Decimal::ZERO && value <= HUNDRED && value.normalize().scale() <= 2 {
                    Ok(value.normalize())
                } else {
                    Err("O limite precisa ser um percentual maior que 0 e ate 100, com ate duas casas."
                        .to_owned())
                }
            }
            Unit::Count => {
                let (max, what) = match self {
                    RuleId::MinClasses => (MAX_CLASSES, "classes"),
                    RuleId::MinSectors => (MAX_SECTORS, "setores"),
                    _ => (MAX_PORTFOLIO_ASSETS, "ativos"),
                };
                if value.fract().is_zero() && value >= Decimal::ONE && value <= Decimal::from(max) {
                    Ok(value.normalize())
                } else {
                    Err(format!(
                        "O minimo de {what} precisa ser um numero inteiro de 1 a {max}."
                    ))
                }
            }
        }
    }
}

/// As regras como a pessoa as deixou. O padrão tem todas ligadas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisConfig {
    settings: BTreeMap<RuleId, RuleSetting>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            settings: RuleId::ALL
                .into_iter()
                .map(|rule| {
                    (
                        rule,
                        RuleSetting {
                            enabled: true,
                            threshold: rule.default_threshold(),
                        },
                    )
                })
                .collect(),
        }
    }
}

impl AnalysisConfig {
    pub fn get(&self, rule: RuleId) -> RuleSetting {
        self.settings.get(&rule).copied().unwrap_or(RuleSetting {
            enabled: true,
            threshold: rule.default_threshold(),
        })
    }

    pub fn set(&mut self, rule: RuleId, setting: RuleSetting) {
        self.settings.insert(rule, setting);
    }

    /// As sete regras originais, para o motor de avisos da Saúde.
    pub fn health(&self) -> HealthConfig {
        let mut config = HealthConfig::default();
        for rule in health::RuleId::ALL {
            config.set(rule, self.get(RuleId::from_legacy(rule)));
        }
        config
    }
}

// ----------------------------------------------------------------- entrada --

/// Um ativo como a análise o enxerga.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisAsset {
    pub id: Uuid,
    /// Símbolo ou nome, para o aviso.
    pub label: String,
    pub asset_class: AssetClass,
    /// O símbolo; `None` na renda fixa por valor. Liga o ativo aos fundamentos.
    pub symbol: Option<String>,
    /// Valor atual em reais.
    pub value: Decimal,
    /// Setor da fonte pública; `None` quando não há.
    pub sector: Option<String>,
    /// Custo total pelo livro, em reais; `None` quando o custo é desconhecido.
    pub cost_basis: Option<Decimal>,
}

impl AnalysisAsset {
    fn health(&self) -> HealthAsset {
        HealthAsset {
            id: self.id,
            label: self.label.clone(),
            asset_class: self.asset_class,
            value: self.value,
            sector: self.sector.clone(),
        }
    }
}

/// Tudo que as regras leem.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    pub assets: &'a [AnalysisAsset],
    pub targets: Targets,
    pub income: &'a IncomeHistory,
    pub returns: &'a ReturnsFacts,
    /// Os ativos com fundamentos da CVM (empresas e FIIs).
    pub quality: &'a [QualityAsset],
}

// ---------------------------------------------------------------- saída --

/// Como uma regra terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Desligada pela pessoa.
    Off,
    /// Ligada, mas ainda não há dado para medir. Não é falha.
    NoData,
    Ok,
    Warning,
}

/// Um ativo com perda desde a compra.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetLoss {
    pub id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    /// Percentual de perda sobre o custo (sempre positivo).
    pub loss_percent: Decimal,
}

/// O que a regra encontrou, além do número.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    None,
    /// Os avisos de uma regra original da Saúde (quem, quanto, aportar).
    Findings(Vec<Warning>),
    Classes(Vec<AssetClass>),
    Sectors(Vec<String>),
    Yield {
        income_12m: Decimal,
        base_value: Decimal,
    },
    Paying {
        paying: u32,
        total: u32,
        /// Os que não pagaram nada em 12 meses.
        silent: Vec<String>,
    },
    IncomeGrowth {
        current_12m: Decimal,
        previous_12m: Decimal,
    },
    Brick {
        reits: u32,
        brick: u32,
    },
    Benchmark(Comparison),
    Years(YearsFact),
    Losses(Vec<AssetLoss>),
    /// Regra de Qualidade: quantos ativos do grupo entraram na conta e quais
    /// reprovaram.
    Quality(Weighted),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleResult {
    pub rule: RuleId,
    pub setting: RuleSetting,
    pub status: Status,
    /// O número de agora, na unidade de [`RuleId::value_unit`]. `None`: desligada,
    /// sem dado, ou regra cujo número não se calcula (renda que cresceu sobre zero).
    pub current: Option<Decimal>,
    /// O que o número precisa alcançar: o limite da pessoa ou, nas regras
    /// contra índice, o que o índice rendeu.
    pub limit: Option<Decimal>,
    /// Aporte (em reais) que traz o número de volta ao limite sem vender nada.
    pub needed_contribution: Option<Decimal>,
    pub detail: Detail,
}

/// A pontuação de uma dimensão: o percentual de regras ligadas, **com dado**,
/// que estão ok.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    pub ok: u32,
    /// Regras ligadas com dado (ok + aviso).
    pub measured: u32,
    /// Regras ligadas sem dado: ficam fora da conta.
    pub no_data: u32,
    /// `ok ÷ measured`, de 0 a 100 com uma casa; `None` sem nenhuma medida.
    pub percent: Option<Decimal>,
}

pub fn score(results: &[RuleResult]) -> Score {
    let count = |status: Status| {
        u32::try_from(results.iter().filter(|r| r.status == status).count()).unwrap_or(u32::MAX)
    };
    let ok = count(Status::Ok);
    let measured = ok + count(Status::Warning);
    Score {
        ok,
        measured,
        no_data: count(Status::NoData),
        percent: (measured > 0)
            .then(|| (Decimal::from(ok) * HUNDRED / Decimal::from(measured)).round_dp(1)),
    }
}

/// Quantas regras estão em aviso: "N pontos a ver".
pub fn attention_count(results: &[RuleResult]) -> u32 {
    u32::try_from(
        results
            .iter()
            .filter(|r| r.status == Status::Warning)
            .count(),
    )
    .unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DimensionReport {
    pub dimension: Dimension,
    pub score: Score,
    pub rules: Vec<RuleResult>,
}

/// Agrupa os resultados por dimensão, na ordem da tela. As quatro dimensões
/// sempre aparecem.
pub fn dimensions(results: Vec<RuleResult>) -> Vec<DimensionReport> {
    Dimension::ALL
        .into_iter()
        .map(|dimension| {
            let rules: Vec<RuleResult> = results
                .iter()
                .filter(|r| r.rule.dimension() == dimension)
                .cloned()
                .collect();
            DimensionReport {
                dimension,
                score: score(&rules),
                rules,
            }
        })
        .collect()
}

// ---------------------------------------------------------------- avaliar --

/// O que uma regra nova mediu; `None` = sem dado.
struct Measured {
    current: Option<Decimal>,
    limit: Option<Decimal>,
    ok: bool,
    needed: Option<Decimal>,
    detail: Detail,
}

impl Measured {
    fn simple(current: Decimal, limit: Decimal, ok: bool) -> Self {
        Self {
            current: Some(current),
            limit: Some(limit),
            ok,
            needed: None,
            detail: Detail::None,
        }
    }
}

/// Os ativos com valor, do maior para o menor, e o total.
struct Portfolio<'a> {
    owned: Vec<&'a AnalysisAsset>,
    total: Decimal,
}

impl<'a> Portfolio<'a> {
    fn new(assets: &'a [AnalysisAsset]) -> Self {
        let mut owned: Vec<&AnalysisAsset> =
            assets.iter().filter(|a| a.value > Decimal::ZERO).collect();
        owned.sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
        let total = owned.iter().map(|a| a.value).sum();
        Self { owned, total }
    }

    fn value_in(&self, keep: impl Fn(AssetClass) -> bool) -> Decimal {
        self.owned
            .iter()
            .filter(|a| keep(a.asset_class))
            .map(|a| a.value)
            .sum()
    }
}

/// As classes de exposição ao exterior: ações, REITs e renda fixa de fora (o
/// BDR e o ETF de exterior negociados na B3 ficam em Ações Internacionais).
pub fn is_abroad(class: AssetClass) -> bool {
    matches!(
        class,
        AssetClass::IntlStocks | AssetClass::UsReits | AssetClass::IntlFixedIncome
    )
}

/// As classes que pagam provento (dividendo, JCP, rendimento de FII).
pub fn pays_income(class: AssetClass) -> bool {
    matches!(
        class,
        AssetClass::BrStocks | AssetClass::BrReits | AssetClass::IntlStocks | AssetClass::UsReits
    )
}

/// Aporte, só na parte `part` de um total `whole`, que leva a fatia a `limit`%:
/// `(p + x) / (T + x) = L / 100  =>  x = (L·T − 100·p) / (100 − L)`. Arredonda
/// para cima, em centavos, para o aporte de fato alcançar o limite. `None`
/// quando o limite é 100% (nenhum aporte chega lá sem zerar o resto).
pub fn contribution_to_grow(part: Decimal, whole: Decimal, limit: Decimal) -> Option<Decimal> {
    if limit >= HUNDRED {
        return None;
    }
    let needed = (limit * whole - HUNDRED * part) / (HUNDRED - limit);
    Some(
        needed
            .max(Decimal::ZERO)
            .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::AwayFromZero),
    )
}

/// Roda as regras ligadas, na ordem de [`RuleId::ALL`].
pub fn evaluate(config: &AnalysisConfig, input: &Input<'_>) -> Vec<RuleResult> {
    let health_assets: Vec<HealthAsset> = input.assets.iter().map(AnalysisAsset::health).collect();
    let legacy = health::evaluate(
        &config.health(),
        &health_assets,
        input.targets,
        input.income,
    );
    let portfolio = Portfolio::new(input.assets);

    RuleId::ALL
        .into_iter()
        .map(|rule| {
            let setting = config.get(rule);
            if !setting.enabled {
                return RuleResult {
                    rule,
                    setting,
                    status: Status::Off,
                    current: None,
                    limit: None,
                    needed_contribution: None,
                    detail: Detail::None,
                };
            }
            if let Some(original) = rule.legacy() {
                return from_legacy(rule, original, setting, &legacy);
            }
            let limit = setting.threshold.or_else(|| rule.default_threshold());
            match measure(rule, limit, &portfolio, input) {
                None => RuleResult {
                    rule,
                    setting,
                    status: Status::NoData,
                    current: None,
                    limit: None,
                    needed_contribution: None,
                    detail: Detail::None,
                },
                Some(m) => RuleResult {
                    rule,
                    setting,
                    status: if m.ok { Status::Ok } else { Status::Warning },
                    current: m.current,
                    limit: m.limit,
                    needed_contribution: m.needed.filter(|_| !m.ok),
                    detail: m.detail,
                },
            }
        })
        .collect()
}

fn from_legacy(
    rule: RuleId,
    original: health::RuleId,
    setting: RuleSetting,
    legacy: &health::Evaluation,
) -> RuleResult {
    let report = legacy.rules.iter().find(|r| r.rule == original);
    let status = match report.map(|r| r.status) {
        Some(health::RuleStatus::Ok) => Status::Ok,
        Some(health::RuleStatus::Warning) => Status::Warning,
        Some(health::RuleStatus::Off) => Status::Off,
        Some(health::RuleStatus::NotApplicable) | None => Status::NoData,
    };
    let findings: Vec<Warning> = legacy
        .warnings
        .iter()
        .filter(|w| w.rule == original)
        .cloned()
        .collect();
    RuleResult {
        rule,
        setting,
        status,
        current: report.and_then(|r| r.current),
        limit: setting.threshold.filter(|_| status != Status::NoData),
        needed_contribution: None,
        detail: if findings.is_empty() {
            Detail::None
        } else {
            Detail::Findings(findings)
        },
    }
}

fn measure(
    rule: RuleId,
    limit: Option<Decimal>,
    portfolio: &Portfolio<'_>,
    input: &Input<'_>,
) -> Option<Measured> {
    // As regras sem limite editável.
    match rule {
        RuleId::IncomeGrowth => return income_growth(input.income),
        RuleId::BrickReit => return brick_reit(portfolio),
        _ => {}
    }
    if let Some((benchmark, horizon)) = rule.benchmark() {
        return beat_index(input.returns, benchmark, horizon);
    }
    if rule == RuleId::QualityRecovery {
        return no_judicial_recovery(input.quality);
    }
    let limit = limit?;
    if let Some(criterion) = rule.criterion() {
        return quality_share(input.quality, criterion, limit);
    }
    match rule {
        RuleId::MinPortfolioAssets => min_portfolio_assets(portfolio, limit),
        RuleId::MinClasses => min_classes(portfolio, limit),
        RuleId::MinSectors => min_sectors(portfolio, limit),
        RuleId::MinAbroad => min_share(portfolio, limit, is_abroad),
        RuleId::MinLowRisk => min_share(portfolio, limit, profile::is_fixed_income),
        RuleId::MinYield => min_yield(portfolio, input.income, limit),
        RuleId::MinPayingShare => min_paying_share(portfolio, input.income, limit),
        RuleId::MinPositiveYears => min_positive_years(input.returns, limit),
        RuleId::MaxVolatility => max_volatility(input.returns, limit),
        RuleId::MaxLoss => max_loss(portfolio, limit),
        // As originais da Saúde nunca chegam aqui; as sem limite e as de índice
        // já saíram acima.
        _ => None,
    }
}

/// Pelo menos `limit`% do valor do grupo (empresas ou FIIs) passa no critério.
fn quality_share(
    quality: &[QualityAsset],
    criterion: Criterion,
    limit: Decimal,
) -> Option<Measured> {
    let found = quality::weighted(quality, criterion)?;
    Some(Measured {
        current: Some(found.share),
        limit: Some(limit),
        ok: found.share >= limit,
        needed: None,
        detail: Detail::Quality(found),
    })
}

/// Nenhuma empresa da carteira está em recuperação judicial.
fn no_judicial_recovery(quality: &[QualityAsset]) -> Option<Measured> {
    let found = quality::judicial_recovery(quality)?;
    Some(Measured {
        current: Some(Decimal::from(found.failing.len())),
        limit: None,
        ok: found.failing.is_empty(),
        needed: None,
        detail: Detail::Quality(found),
    })
}

fn min_portfolio_assets(portfolio: &Portfolio<'_>, limit: Decimal) -> Option<Measured> {
    if portfolio.owned.is_empty() {
        return None;
    }
    let count = Decimal::from(portfolio.owned.len());
    Some(Measured::simple(count, limit, count >= limit))
}

fn min_classes(portfolio: &Portfolio<'_>, limit: Decimal) -> Option<Measured> {
    if portfolio.owned.is_empty() {
        return None;
    }
    let classes: BTreeSet<AssetClass> = portfolio.owned.iter().map(|a| a.asset_class).collect();
    let count = Decimal::from(classes.len());
    Some(Measured {
        detail: Detail::Classes(classes.into_iter().collect()),
        ..Measured::simple(count, limit, count >= limit)
    })
}

/// Setores distintos entre as Ações Nacionais. Sem ações, ou sem nenhuma com
/// setor na fonte, não há o que contar.
fn min_sectors(portfolio: &Portfolio<'_>, limit: Decimal) -> Option<Measured> {
    let sectors: BTreeSet<String> = portfolio
        .owned
        .iter()
        .filter(|a| a.asset_class == AssetClass::BrStocks)
        .filter_map(|a| a.sector.clone())
        .collect();
    if sectors.is_empty() {
        return None;
    }
    let count = Decimal::from(sectors.len());
    Some(Measured {
        detail: Detail::Sectors(sectors.into_iter().collect()),
        ..Measured::simple(count, limit, count >= limit)
    })
}

/// Fatia mínima de um grupo de classes (exterior, renda fixa) na carteira.
fn min_share(
    portfolio: &Portfolio<'_>,
    limit: Decimal,
    group: impl Fn(AssetClass) -> bool,
) -> Option<Measured> {
    if portfolio.total <= Decimal::ZERO {
        return None;
    }
    let part = portfolio.value_in(group);
    let share = percent(part, portfolio.total);
    Some(Measured {
        needed: contribution_to_grow(part, portfolio.total, limit),
        ..Measured::simple(share, limit, share >= limit)
    })
}

/// Os pagamentos recebidos e quantos meses de calendário o histórico cobre.
fn history_months(income: &IncomeHistory) -> i32 {
    health::history_months(&health::received_entries(income), income.today)
}

fn min_yield(
    portfolio: &Portfolio<'_>,
    income: &IncomeHistory,
    limit: Decimal,
) -> Option<Measured> {
    if history_months(income) < MIN_YIELD_HISTORY_MONTHS {
        return None;
    }
    let base = portfolio.value_in(pays_income);
    if base <= Decimal::ZERO {
        return None;
    }
    let received = income::summarize(&income.entries, income.today).total_12m;
    let current = percent(received, base);
    Some(Measured {
        detail: Detail::Yield {
            income_12m: received,
            base_value: round_cents(base),
        },
        ..Measured::simple(current, limit, current >= limit)
    })
}

fn min_paying_share(
    portfolio: &Portfolio<'_>,
    income: &IncomeHistory,
    limit: Decimal,
) -> Option<Measured> {
    if history_months(income) < MIN_YIELD_HISTORY_MONTHS {
        return None;
    }
    let payers: Vec<&&AnalysisAsset> = portfolio
        .owned
        .iter()
        .filter(|a| pays_income(a.asset_class))
        .collect();
    if payers.is_empty() {
        return None;
    }
    let summary = income::summarize(&income.entries, income.today);
    let paid: BTreeSet<&str> = summary
        .by_payer
        .iter()
        .filter(|p| p.total > Decimal::ZERO)
        .map(|p| p.key.as_str())
        .collect();
    let mut silent: Vec<String> = payers
        .iter()
        .filter(|a| !paid.contains(a.id.to_string().as_str()))
        .map(|a| a.label.clone())
        .collect();
    silent.sort();
    let total = u32::try_from(payers.len()).unwrap_or(u32::MAX);
    let paying = total - u32::try_from(silent.len()).unwrap_or(total);
    let share = percent(Decimal::from(paying), Decimal::from(total));
    Some(Measured {
        detail: Detail::Paying {
            paying,
            total,
            silent,
        },
        ..Measured::simple(share, limit, share >= limit)
    })
}

/// A renda dos últimos 12 meses contra a dos 12 anteriores (as duas janelas
/// são as da tela Renda: meses de calendário que terminam no mês de hoje).
fn income_growth(income: &IncomeHistory) -> Option<Measured> {
    if history_months(income) < MIN_GROWTH_HISTORY_MONTHS {
        return None;
    }
    let current = income::summarize(&income.entries, income.today).total_12m;
    let previous = income::summarize(&income.entries, months_before(income.today, 12)).total_12m;
    if current.is_zero() && previous.is_zero() {
        return None;
    }
    Some(Measured {
        current: (previous > Decimal::ZERO).then(|| percent(current - previous, previous)),
        limit: None,
        ok: current > previous,
        needed: None,
        detail: Detail::IncomeGrowth {
            current_12m: current,
            previous_12m: previous,
        },
    })
}

/// Se há FII na carteira, ao menos um de tijolo (renda atrelada a aluguel, que
/// acompanha a inflação). Sem FII, ou sem tipo conhecido, não há o que medir.
fn brick_reit(portfolio: &Portfolio<'_>) -> Option<Measured> {
    let reits: Vec<&&AnalysisAsset> = portfolio
        .owned
        .iter()
        .filter(|a| a.asset_class == AssetClass::BrReits)
        .collect();
    if reits.iter().all(|a| a.sector.is_none()) {
        return None;
    }
    let brick = reits
        .iter()
        .filter(|a| a.sector.as_deref() == Some(BRICK_SECTOR))
        .count();
    Some(Measured {
        current: Some(Decimal::from(brick)),
        limit: None,
        ok: brick >= 1,
        needed: None,
        detail: Detail::Brick {
            reits: u32::try_from(reits.len()).unwrap_or(u32::MAX),
            brick: u32::try_from(brick).unwrap_or(u32::MAX),
        },
    })
}

fn beat_index(returns: &ReturnsFacts, benchmark: Benchmark, horizon: Horizon) -> Option<Measured> {
    let comparison = returns.comparison(benchmark, horizon)?;
    let portfolio = to_percent(comparison.portfolio, 2);
    let index = to_percent(comparison.benchmark, 2);
    Some(Measured {
        detail: Detail::Benchmark(*comparison),
        ..Measured::simple(portfolio, index, portfolio >= index)
    })
}

fn min_positive_years(returns: &ReturnsFacts, limit: Decimal) -> Option<Measured> {
    let years = returns.positive_years?;
    let share = percent(Decimal::from(years.positive), Decimal::from(years.total));
    Some(Measured {
        detail: Detail::Years(years),
        ..Measured::simple(share, limit, share >= limit)
    })
}

fn max_volatility(returns: &ReturnsFacts, limit: Decimal) -> Option<Measured> {
    let volatility = returns.volatility_percent?;
    Some(Measured::simple(volatility, limit, volatility <= limit))
}

/// A maior perda desde a compra: valor de hoje contra o custo do livro (preço
/// médio × quantidade). Ativo sem custo conhecido não entra.
fn max_loss(portfolio: &Portfolio<'_>, limit: Decimal) -> Option<Measured> {
    let mut known = false;
    let mut losses: Vec<AssetLoss> = Vec::new();
    let mut biggest = Decimal::ZERO;
    for asset in &portfolio.owned {
        let Some(cost) = asset.cost_basis.filter(|c| *c > Decimal::ZERO) else {
            continue;
        };
        known = true;
        if asset.value < cost {
            let loss = percent(cost - asset.value, cost);
            biggest = biggest.max(loss);
            if loss > limit {
                losses.push(AssetLoss {
                    id: asset.id,
                    label: asset.label.clone(),
                    asset_class: asset.asset_class,
                    loss_percent: loss,
                });
            }
        }
    }
    if !known {
        return None;
    }
    losses.sort_by(|a, b| {
        b.loss_percent
            .cmp(&a.loss_percent)
            .then_with(|| a.label.cmp(&b.label))
    });
    Some(Measured {
        detail: if losses.is_empty() {
            Detail::None
        } else {
            Detail::Losses(losses)
        },
        ..Measured::simple(biggest, limit, biggest <= limit)
    })
}

#[cfg(test)]
mod tests;
