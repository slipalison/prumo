//! Análise da carteira: o painel (`GET /analysis`) e a configuração das regras
//! (`GET` e `PUT /analysis/rules`). Tudo pelo dono.
//!
//! O servidor devolve fatos (qual regra, o número de agora, o limite, o que
//! fazer em reais); o texto que a pessoa lê, com acento, é montado na tela — o
//! mesmo desenho da Saúde e do plano de aporte. As regras, as fórmulas e os
//! limiares estão em `docs/analise.md`.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::optional_decimal;
use super::health::{number, sources_dto, RuleChange, SectorSourceDto, WarningDto};
use super::{ApiJson, AppState, CurrentUser};
use crate::analysis::{self as service, AnalysisReport};
use crate::domain::analysis::highlights::{Mover, Ranking};
use crate::domain::analysis::profile::{self, Mix};
use crate::domain::analysis::quality::{Attention, Point};
use crate::domain::analysis::references::{validate_bazin_yield, Bazin};
use crate::domain::analysis::{
    AnalysisConfig, Detail, Dimension, DimensionReport, Direction, RuleId, RuleResult, Score,
    Status,
};
use crate::domain::asset_class::AssetClass;
use crate::domain::health::{RuleSetting, Unit};
use crate::error::{ApiError, ApiResult};

fn day(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize, PartialEq)]
pub struct ScoreDto {
    pub ok: u32,
    pub measured: u32,
    pub no_data: u32,
    /// `ok ÷ measured`, de 0 a 100; `null` sem nenhuma regra medida.
    #[serde(with = "number::option")]
    pub percent: Option<Decimal>,
}

impl From<Score> for ScoreDto {
    fn from(score: Score) -> Self {
        Self {
            ok: score.ok,
            measured: score.measured,
            no_data: score.no_data,
            percent: score.percent,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LossDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    #[serde(with = "number")]
    pub loss_percent: Decimal,
}

/// O que a regra encontrou, além do número. O `kind` diz qual.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DetailDto {
    /// Os avisos de uma regra original da Saúde.
    Findings {
        warnings: Vec<WarningDto>,
    },
    Classes {
        asset_classes: Vec<AssetClass>,
    },
    Sectors {
        sectors: Vec<String>,
    },
    Yield {
        #[serde(with = "number")]
        income_12m: Decimal,
        #[serde(with = "number")]
        base_value: Decimal,
    },
    Paying {
        paying: u32,
        total: u32,
        silent: Vec<String>,
    },
    IncomeGrowth {
        #[serde(with = "number")]
        current_12m: Decimal,
        #[serde(with = "number")]
        previous_12m: Decimal,
    },
    Brick {
        reits: u32,
        brick: u32,
    },
    Benchmark {
        benchmark: &'static str,
        horizon: &'static str,
        from: String,
        to: String,
        #[serde(with = "number")]
        portfolio_percent: Decimal,
        #[serde(with = "number")]
        benchmark_percent: Decimal,
    },
    Years {
        positive: u32,
        total: u32,
    },
    Losses {
        assets: Vec<LossDto>,
    },
    /// Regra de Qualidade: quantos ativos do grupo entraram na conta (os que
    /// têm o dado) e quais reprovaram.
    Quality {
        covered: u32,
        total: u32,
        failing: Vec<String>,
    },
}

impl DetailDto {
    fn of(rule: RuleId, detail: &Detail) -> Option<Self> {
        Some(match detail {
            Detail::None => return None,
            Detail::Findings(warnings) => DetailDto::Findings {
                warnings: warnings.iter().map(WarningDto::from).collect(),
            },
            Detail::Classes(classes) => DetailDto::Classes {
                asset_classes: classes.clone(),
            },
            Detail::Sectors(sectors) => DetailDto::Sectors {
                sectors: sectors.clone(),
            },
            Detail::Yield {
                income_12m,
                base_value,
            } => DetailDto::Yield {
                income_12m: *income_12m,
                base_value: *base_value,
            },
            Detail::Paying {
                paying,
                total,
                silent,
            } => DetailDto::Paying {
                paying: *paying,
                total: *total,
                silent: silent.clone(),
            },
            Detail::IncomeGrowth {
                current_12m,
                previous_12m,
            } => DetailDto::IncomeGrowth {
                current_12m: *current_12m,
                previous_12m: *previous_12m,
            },
            Detail::Brick { reits, brick } => DetailDto::Brick {
                reits: *reits,
                brick: *brick,
            },
            Detail::Benchmark(c) => {
                let (benchmark, horizon) = rule.benchmark()?;
                DetailDto::Benchmark {
                    benchmark: benchmark.as_str(),
                    horizon: horizon.as_str(),
                    from: day(c.from),
                    to: day(c.to),
                    portfolio_percent: crate::domain::performance::to_percent(c.portfolio, 2),
                    benchmark_percent: crate::domain::performance::to_percent(c.benchmark, 2),
                }
            }
            Detail::Years(years) => DetailDto::Years {
                positive: years.positive,
                total: years.total,
            },
            Detail::Quality(found) => DetailDto::Quality {
                covered: found.covered,
                total: found.total,
                failing: found.failing.clone(),
            },
            Detail::Losses(losses) => DetailDto::Losses {
                assets: losses
                    .iter()
                    .map(|l| LossDto {
                        asset_id: l.id,
                        label: l.label.clone(),
                        asset_class: l.asset_class,
                        loss_percent: l.loss_percent,
                    })
                    .collect(),
            },
        })
    }
}

/// A regra como ela é (e como a pessoa a deixou), sem o resultado.
#[derive(Debug, Serialize)]
pub struct RuleDto {
    pub rule: RuleId,
    pub dimension: Dimension,
    pub enabled: bool,
    /// Em que o número (e o limite) é medido.
    pub unit: Unit,
    /// O que é "bom": em ou abaixo, ou em ou acima do limite.
    pub direction: Option<Direction>,
    /// `null` nas regras que não têm limite editável.
    #[serde(with = "number::option")]
    pub threshold: Option<Decimal>,
    #[serde(with = "number::option")]
    pub default_threshold: Option<Decimal>,
}

impl RuleDto {
    fn new(rule: RuleId, setting: RuleSetting) -> Self {
        Self {
            rule,
            dimension: rule.dimension(),
            enabled: setting.enabled,
            unit: rule.value_unit(),
            direction: rule.direction(),
            threshold: setting.threshold,
            default_threshold: rule.default_threshold(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RuleResultDto {
    #[serde(flatten)]
    pub rule: RuleDto,
    pub status: Status,
    /// O número de agora.
    #[serde(with = "number::option")]
    pub current: Option<Decimal>,
    /// O que o número precisa alcançar: o limite da pessoa ou, nas regras
    /// contra índice, o que o índice rendeu.
    #[serde(with = "number::option")]
    pub limit: Option<Decimal>,
    /// Aporte (R$) que traz o número de volta ao limite sem vender nada.
    #[serde(with = "number::option")]
    pub needed_contribution: Option<Decimal>,
    pub detail: Option<DetailDto>,
}

impl From<&RuleResult> for RuleResultDto {
    fn from(r: &RuleResult) -> Self {
        Self {
            rule: RuleDto::new(r.rule, r.setting),
            status: r.status,
            current: r.current,
            limit: r.limit,
            needed_contribution: r.needed_contribution,
            detail: DetailDto::of(r.rule, &r.detail),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DimensionDto {
    pub id: Dimension,
    pub score: ScoreDto,
    pub rules: Vec<RuleResultDto>,
}

impl From<&DimensionReport> for DimensionDto {
    fn from(d: &DimensionReport) -> Self {
        Self {
            id: d.dimension,
            score: ScoreDto::from(d.score),
            rules: d.rules.iter().map(RuleResultDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MixDto {
    /// Renda fixa e renda variável, em %; somam 100.
    #[serde(with = "number")]
    pub fixed_income: Decimal,
    #[serde(with = "number")]
    pub variable: Decimal,
    pub profile: profile::Profile,
}

impl From<Mix> for MixDto {
    fn from(mix: Mix) -> Self {
        Self {
            fixed_income: mix.fixed_income,
            variable: mix.variable,
            profile: mix.profile,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProfileDto {
    pub current: Option<MixDto>,
    /// O perfil das metas por classe da pessoa.
    pub target: Option<MixDto>,
    /// Até quanto de renda variável (%) o perfil é conservador, e até quanto é
    /// moderado; acima, arrojado.
    #[serde(with = "number")]
    pub conservative_max: Decimal,
    #[serde(with = "number")]
    pub moderate_max: Decimal,
}

#[derive(Debug, Serialize)]
pub struct MoverDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    #[serde(with = "number")]
    pub change_percent: Decimal,
}

impl From<&Mover> for MoverDto {
    fn from(m: &Mover) -> Self {
        Self {
            asset_id: m.asset_id,
            label: m.label.clone(),
            asset_class: m.asset_class,
            change_percent: m.change_percent,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RankingDto {
    pub best: Vec<MoverDto>,
    pub worst: Vec<MoverDto>,
}

impl From<&Ranking> for RankingDto {
    fn from(r: &Ranking) -> Self {
        Self {
            best: r.best.iter().map(MoverDto::from).collect(),
            worst: r.worst.iter().map(MoverDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HighlightsDto {
    pub price_12m: RankingDto,
    pub since_purchase: RankingDto,
}

#[derive(Debug, Serialize)]
pub struct BazinDto {
    /// A referência, em reais por cota.
    #[serde(with = "number")]
    pub price: Decimal,
    /// Dividendo anual médio por cota, em reais.
    #[serde(with = "number")]
    pub annual_average: Decimal,
    /// Quantos anos de 12 meses entraram na média.
    pub years: u32,
}

impl From<Bazin> for BazinDto {
    fn from(b: Bazin) -> Self {
        Self {
            price: b.price,
            annual_average: b.annual_average,
            years: b.years,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReferenceDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    #[serde(with = "number::option")]
    pub unit_price: Option<Decimal>,
    /// O teto de preço que a pessoa pôs no ativo.
    #[serde(with = "number::option")]
    pub max_price: Option<Decimal>,
    pub bazin: Option<BazinDto>,
    #[serde(with = "number::option")]
    pub graham: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct ReferencesDto {
    /// O rendimento-alvo do Bazin, em % ao ano.
    #[serde(with = "number")]
    pub bazin_yield: Decimal,
    pub items: Vec<ReferenceDto>,
}

/// Um ponto de atenção, curto e factual. O `kind` diz qual.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PointDto {
    /// A renda de 12 meses do ativo caiu `percent` contra os 12 anteriores.
    IncomeDrop {
        #[serde(with = "number")]
        percent: Decimal,
    },
    /// Crescimento anual composto em 5 anos, em %.
    LowRevenueGrowth {
        #[serde(with = "number")]
        percent: Decimal,
    },
    LowEarningsGrowth {
        #[serde(with = "number")]
        percent: Decimal,
    },
    LowRoe {
        #[serde(with = "number")]
        percent: Decimal,
    },
    LowNetMargin {
        #[serde(with = "number")]
        percent: Decimal,
    },
    /// Dívida líquida sobre patrimônio líquido, em vezes.
    DebtAboveEquity {
        #[serde(with = "number")]
        times: Decimal,
    },
    JudicialRecovery,
}

impl From<&Point> for PointDto {
    fn from(point: &Point) -> Self {
        match *point {
            Point::IncomeDrop { percent } => PointDto::IncomeDrop { percent },
            Point::LowRevenueGrowth { percent } => PointDto::LowRevenueGrowth { percent },
            Point::LowEarningsGrowth { percent } => PointDto::LowEarningsGrowth { percent },
            Point::LowRoe { percent } => PointDto::LowRoe { percent },
            Point::LowNetMargin { percent } => PointDto::LowNetMargin { percent },
            Point::DebtAboveEquity { times } => PointDto::DebtAboveEquity { times },
            Point::JudicialRecovery => PointDto::JudicialRecovery,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssetAttentionDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    pub points: Vec<PointDto>,
}

#[derive(Debug, Serialize)]
pub struct AttentionDto {
    /// Ativos com fundamentos (empresa ou FII).
    pub assessed: u32,
    /// Ações e FIIs com valor que ainda não têm fundamentos.
    pub without_data: u32,
    pub items: Vec<AssetAttentionDto>,
}

impl From<&Attention> for AttentionDto {
    fn from(attention: &Attention) -> Self {
        Self {
            assessed: attention.assessed,
            without_data: attention.without_data,
            items: attention
                .items
                .iter()
                .map(|item| AssetAttentionDto {
                    asset_id: item.id,
                    label: item.label.clone(),
                    asset_class: item.asset_class,
                    points: item.points.iter().map(PointDto::from).collect(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AnalysisDto {
    pub as_of: String,
    #[serde(with = "number")]
    pub total_value: Decimal,
    /// Quantas regras estão em aviso: "N pontos a ver".
    pub attention_count: u32,
    pub dimensions: Vec<DimensionDto>,
    /// Os pontos de atenção dos ativos.
    pub attention: AttentionDto,
    pub profile: ProfileDto,
    pub highlights: HighlightsDto,
    pub references: ReferencesDto,
    /// Ativos com valor em Ações Nacionais e FIIs que a fonte de setor não classificou.
    pub unclassified_count: usize,
    pub sources: Vec<SectorSourceDto>,
}

impl AnalysisDto {
    fn new(report: &AnalysisReport, sources: Vec<SectorSourceDto>) -> Self {
        Self {
            as_of: day(report.today),
            total_value: report.total_value,
            attention_count: report.attention_count,
            dimensions: report.dimensions.iter().map(DimensionDto::from).collect(),
            attention: AttentionDto::from(&report.asset_attention),
            profile: ProfileDto {
                current: report.profile.current.map(MixDto::from),
                target: report.profile.target.map(MixDto::from),
                conservative_max: profile::CONSERVATIVE_MAX,
                moderate_max: profile::MODERATE_MAX,
            },
            highlights: HighlightsDto {
                price_12m: RankingDto::from(&report.highlights.price_12m),
                since_purchase: RankingDto::from(&report.highlights.since_purchase),
            },
            references: ReferencesDto {
                bazin_yield: report.references.bazin_yield,
                items: report
                    .references
                    .items
                    .iter()
                    .map(|item| ReferenceDto {
                        asset_id: item.asset_id,
                        label: item.label.clone(),
                        asset_class: item.asset_class,
                        unit_price: item.unit_price,
                        max_price: item.max_price,
                        bazin: item.bazin.map(BazinDto::from),
                        graham: item.graham,
                    })
                    .collect(),
            },
            unclassified_count: report.unclassified_count,
            sources,
        }
    }
}

/// A configuração: as regras e o rendimento-alvo do Bazin.
#[derive(Debug, Serialize)]
pub struct ConfigDto {
    pub rules: Vec<RuleDto>,
    #[serde(with = "number")]
    pub bazin_yield: Decimal,
}

impl ConfigDto {
    fn new(config: &AnalysisConfig, bazin_yield: Decimal) -> Self {
        Self {
            rules: RuleId::ALL
                .into_iter()
                .map(|rule| RuleDto::new(rule, config.get(rule)))
                .collect(),
            bazin_yield,
        }
    }
}

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigRequest {
    #[serde(default)]
    pub rules: BTreeMap<RuleId, RuleChange>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub bazin_yield: Option<Decimal>,
}

// --------------------------------------------------------------- handlers --

pub async fn analysis(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<AnalysisDto>> {
    let fundamentals = state.store.fundamentals_for_user(user.id).await?;
    let report = service::report(
        &state.store,
        user.id,
        OffsetDateTime::now_utc(),
        &fundamentals,
    )
    .await?;
    let sources = state.store.sector_sync_states().await?;
    Ok(Json(AnalysisDto::new(&report, sources_dto(&sources))))
}

pub async fn rules(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<ConfigDto>> {
    let config = state.store.analysis_config(user.id).await?;
    let bazin_yield = state.store.bazin_yield(user.id).await?;
    Ok(Json(ConfigDto::new(&config, bazin_yield)))
}

pub async fn save_rules(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<ConfigRequest>,
) -> ApiResult<Json<ConfigDto>> {
    if body.rules.is_empty() && body.bazin_yield.is_none() {
        return Err(ApiError::validation(
            "Informe ao menos uma regra ou o rendimento-alvo do Bazin.",
        ));
    }
    let current = state.store.analysis_config(user.id).await?;
    let mut changes = Vec::with_capacity(body.rules.len());
    for (rule, change) in body.rules {
        if change.enabled.is_none() && change.threshold.is_none() {
            return Err(ApiError::validation(format!(
                "A regra {} precisa de enabled ou threshold.",
                rule.as_str()
            )));
        }
        let before = current.get(rule);
        let threshold =
            match change.threshold {
                None => before.threshold,
                Some(value) => Some(rule.validate_threshold(value).map_err(|detail| {
                    ApiError::validation(format!("{}: {detail}", rule.as_str()))
                })?),
            };
        changes.push((
            rule,
            RuleSetting {
                enabled: change.enabled.unwrap_or(before.enabled),
                threshold,
            },
        ));
    }
    let bazin_yield = body
        .bazin_yield
        .map(|value| {
            validate_bazin_yield(value)
                .map_err(|detail| ApiError::validation(format!("bazin_yield: {detail}")))
        })
        .transpose()?;
    state
        .store
        .save_analysis(user.id, &changes, bazin_yield)
        .await?;
    let config = state.store.analysis_config(user.id).await?;
    let bazin_yield = state.store.bazin_yield(user.id).await?;
    Ok(Json(ConfigDto::new(&config, bazin_yield)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analysis::returns::{Benchmark, Comparison, Horizon};
    use rust_decimal_macros::dec;
    use serde_json::json;
    use time::macros::date;

    #[test]
    fn the_rule_list_has_every_rule_with_its_default_and_direction() {
        let response = ConfigDto::new(&AnalysisConfig::default(), dec!(6));
        let json = serde_json::to_value(&response).unwrap();
        let rules = json["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 33);
        assert_eq!(rules[0]["rule"], "max_asset");
        assert_eq!(rules[0]["dimension"], "diversification");
        assert_eq!(rules[0]["direction"], "at_most");
        assert_eq!(rules[0]["threshold"], 15.0);
        assert_eq!(json["bazin_yield"], 6.0);
        // Sem limite editável: threshold nulo, mas o número tem unidade.
        let cdi = rules.iter().find(|r| r["rule"] == "beat_cdi_12m").unwrap();
        assert!(cdi["threshold"].is_null());
        assert_eq!(cdi["unit"], "percent");
        assert_eq!(cdi["dimension"], "returns");
        let empty = rules.iter().find(|r| r["rule"] == "empty_class").unwrap();
        assert!(empty["direction"].is_null());
        assert_eq!(empty["unit"], "count");
    }

    #[test]
    fn a_result_flattens_the_rule_and_tags_the_detail_by_kind() {
        let result = RuleResult {
            rule: RuleId::BeatCdi12m,
            setting: RuleSetting {
                enabled: true,
                threshold: None,
            },
            status: Status::Warning,
            current: Some(dec!(8.5)),
            limit: Some(dec!(10.25)),
            needed_contribution: None,
            detail: Detail::Benchmark(Comparison {
                from: date!(2025 - 11 - 25),
                to: date!(2026 - 11 - 25),
                portfolio: dec!(0.085),
                benchmark: dec!(0.1025),
            }),
        };
        assert_eq!(
            serde_json::to_value(RuleResultDto::from(&result)).unwrap(),
            json!({
                "rule": "beat_cdi_12m",
                "dimension": "returns",
                "enabled": true,
                "unit": "percent",
                "direction": "at_least",
                "threshold": null,
                "default_threshold": null,
                "status": "warning",
                "current": 8.5,
                "limit": 10.25,
                "needed_contribution": null,
                "detail": {
                    "kind": "benchmark",
                    "benchmark": "cdi",
                    "horizon": "12m",
                    "from": "2025-11-25",
                    "to": "2026-11-25",
                    "portfolio_percent": 8.5,
                    "benchmark_percent": 10.25
                }
            })
        );
    }

    #[test]
    fn details_without_content_are_null_and_losses_list_the_assets() {
        assert!(DetailDto::of(RuleId::MaxLoss, &Detail::None).is_none());
        let id = Uuid::nil();
        let detail = Detail::Losses(vec![crate::domain::analysis::AssetLoss {
            id,
            label: "ABCD3".into(),
            asset_class: AssetClass::BrStocks,
            loss_percent: dec!(41.5),
        }]);
        assert_eq!(
            serde_json::to_value(DetailDto::of(RuleId::MaxLoss, &detail)).unwrap(),
            json!({"kind": "losses", "assets": [
                {"asset_id": id, "label": "ABCD3", "asset_class": "br_stocks", "loss_percent": 41.5}
            ]})
        );
    }

    #[test]
    fn attention_points_are_tagged_by_kind_and_quality_details_list_who_failed() {
        let point = |p: Point| serde_json::to_value(PointDto::from(&p)).unwrap();
        assert_eq!(
            point(Point::LowRoe {
                percent: dec!(4.12)
            }),
            json!({"kind": "low_roe", "percent": 4.12})
        );
        assert_eq!(
            point(Point::DebtAboveEquity { times: dec!(2.9) }),
            json!({"kind": "debt_above_equity", "times": 2.9})
        );
        assert_eq!(
            point(Point::JudicialRecovery),
            json!({"kind": "judicial_recovery"})
        );
        assert_eq!(
            point(Point::IncomeDrop { percent: dec!(40) }),
            json!({"kind": "income_drop", "percent": 40.0})
        );
        let detail = Detail::Quality(crate::domain::analysis::quality::Weighted {
            share: dec!(66.67),
            covered: 2,
            total: 3,
            failing: vec!["FRAC3".into()],
        });
        assert_eq!(
            serde_json::to_value(DetailDto::of(RuleId::QualityRoe, &detail)).unwrap(),
            json!({"kind": "quality", "covered": 2, "total": 3, "failing": ["FRAC3"]})
        );
    }

    #[test]
    fn the_score_serializes_the_exact_numbers() {
        let score = Score {
            ok: 5,
            measured: 8,
            no_data: 2,
            percent: Some(dec!(62.5)),
        };
        assert_eq!(
            serde_json::to_value(ScoreDto::from(score)).unwrap(),
            json!({"ok": 5, "measured": 8, "no_data": 2, "percent": 62.5})
        );
        let empty = Score {
            ok: 0,
            measured: 0,
            no_data: 3,
            percent: None,
        };
        assert!(serde_json::to_value(ScoreDto::from(empty)).unwrap()["percent"].is_null());
    }

    #[test]
    fn the_benchmark_codes_match_the_index_codes() {
        // O contrato usa os mesmos nomes do Desempenho.
        for benchmark in Benchmark::ALL {
            assert!(crate::history::IndexCode::parse(benchmark.as_str()).is_some());
        }
        assert_eq!(Horizon::OneYear.as_str(), "12m");
        assert_eq!(Horizon::FiveYears.as_str(), "5y");
    }

    #[test]
    fn the_request_rejects_unknown_fields_and_rules() {
        assert!(serde_json::from_str::<ConfigRequest>(r#"{"nota": 1}"#).is_err());
        assert!(serde_json::from_str::<ConfigRequest>(r#"{"rules": {"nota_final": {}}}"#).is_err());
        let ok: ConfigRequest = serde_json::from_str(
            r#"{"rules": {"min_abroad": {"threshold": 12.5}}, "bazin_yield": 7}"#,
        )
        .unwrap();
        assert_eq!(ok.rules.len(), 1);
        assert_eq!(ok.bazin_yield, Some(dec!(7)));
    }
}
