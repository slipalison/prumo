//! Saúde da carteira: os avisos (`GET /health`) e a configuração das regras
//! (`GET` e `PUT /health/rules`). Tudo pelo dono.
//!
//! O servidor devolve fatos (qual regra, sobre o quê, o número de agora, o
//! limite, quanto aportar fora dali para voltar ao limite); o texto que a
//! pessoa lê, com acento, é montado na tela — o mesmo desenho dos avisos do
//! plano de aporte.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::dto::optional_decimal;
use super::{ApiJson, AppState, CurrentUser};
use crate::domain::asset_class::AssetClass;
use crate::domain::health::{
    self, HealthAsset, HealthConfig, RuleId, RuleReport, RuleSetting, RuleStatus, Subject, Unit,
    Warning,
};
use crate::domain::income::today_in_brazil;
use crate::domain::sector::SectorSource;
use crate::error::{ApiError, ApiResult};
use crate::service::{self, PortfolioView};
use crate::store::sectors::SyncState;

pub(crate) mod number {
    use super::{Decimal, ToPrimitive};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(value.to_f64().unwrap_or(0.0))
    }

    pub mod option {
        use super::{Decimal, Serializer};

        pub fn serialize<S: Serializer>(
            value: &Option<Decimal>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match value {
                Some(v) => super::serialize(v, serializer),
                None => serializer.serialize_none(),
            }
        }
    }
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubjectDto {
    Asset {
        asset_id: Uuid,
        label: String,
    },
    TopAssets {
        labels: Vec<String>,
    },
    Sector {
        asset_class: AssetClass,
        sector: String,
    },
    Class {
        asset_class: AssetClass,
    },
    /// Meses (`AAAA-MM`) sem provento e quem da carteira paga neles.
    IncomeMonths {
        empty_months: Vec<String>,
        payers: Vec<GapPayerDto>,
    },
    Payer {
        asset_id: Option<Uuid>,
        label: String,
        #[serde(with = "number")]
        received: Decimal,
        #[serde(with = "number")]
        total: Decimal,
    },
}

#[derive(Debug, Serialize, PartialEq)]
pub struct GapPayerDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    /// Meses do calendário (1 a 12) entre os vazios em que ele já pagou.
    pub months: Vec<u8>,
}

impl From<&Subject> for SubjectDto {
    fn from(subject: &Subject) -> Self {
        match subject {
            Subject::Asset { id, label } => SubjectDto::Asset {
                asset_id: *id,
                label: label.clone(),
            },
            Subject::TopAssets { labels } => SubjectDto::TopAssets {
                labels: labels.clone(),
            },
            Subject::Sector { class, sector } => SubjectDto::Sector {
                asset_class: *class,
                sector: sector.clone(),
            },
            Subject::Class(class) => SubjectDto::Class {
                asset_class: *class,
            },
            Subject::IncomeGaps {
                empty_months,
                payers,
            } => SubjectDto::IncomeMonths {
                empty_months: empty_months.iter().map(|m| m.label()).collect(),
                payers: payers
                    .iter()
                    .map(|p| GapPayerDto {
                        asset_id: p.id,
                        label: p.label.clone(),
                        asset_class: p.asset_class,
                        months: p.months.clone(),
                    })
                    .collect(),
            },
            Subject::Payer {
                id,
                label,
                received,
                total,
            } => SubjectDto::Payer {
                asset_id: *id,
                label: label.clone(),
                received: *received,
                total: *total,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WarningDto {
    pub rule: RuleId,
    pub subject: SubjectDto,
    pub unit: Unit,
    #[serde(with = "number")]
    pub current: Decimal,
    #[serde(with = "number")]
    pub limit: Decimal,
    #[serde(with = "number::option")]
    pub needed_contribution: Option<Decimal>,
}

impl From<&Warning> for WarningDto {
    fn from(w: &Warning) -> Self {
        Self {
            rule: w.rule,
            subject: SubjectDto::from(&w.subject),
            // Toda regra que avisa tem limite, e portanto unidade.
            unit: w.rule.unit().unwrap_or(Unit::Count),
            current: w.current,
            limit: w.limit,
            needed_contribution: w.needed_contribution,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RuleDto {
    pub rule: RuleId,
    pub enabled: bool,
    pub unit: Option<Unit>,
    #[serde(with = "number::option")]
    pub threshold: Option<Decimal>,
    #[serde(with = "number::option")]
    pub default_threshold: Option<Decimal>,
}

impl RuleDto {
    fn new(rule: RuleId, setting: RuleSetting) -> Self {
        Self {
            rule,
            enabled: setting.enabled,
            unit: rule.unit(),
            threshold: setting.threshold,
            default_threshold: rule.default_threshold(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RulesResponse {
    pub rules: Vec<RuleDto>,
}

impl RulesResponse {
    fn from_config(config: &HealthConfig) -> Self {
        Self {
            rules: RuleId::ALL
                .into_iter()
                .map(|rule| RuleDto::new(rule, config.get(rule)))
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RuleResultDto {
    #[serde(flatten)]
    pub rule: RuleDto,
    pub status: RuleStatus,
}

/// Uma fonte de setor e o estado dela.
#[derive(Debug, Serialize)]
pub struct SectorSourceDto {
    pub source: SectorSource,
    /// A classe que a fonte classifica.
    pub asset_class: AssetClass,
    /// Da última busca que deu certo; `None` enquanto nunca deu.
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    /// A última tentativa falhou: o setor mostrado é o da busca anterior.
    pub failing: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthDto {
    #[serde(with = "number")]
    pub total_value: Decimal,
    pub warnings: Vec<WarningDto>,
    pub rules: Vec<RuleResultDto>,
    /// Quantos ativos com valor em Ações Nacionais e FIIs ficaram sem setor.
    pub unclassified_count: usize,
    pub sources: Vec<SectorSourceDto>,
}

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleChange {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub threshold: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesRequest {
    pub rules: BTreeMap<RuleId, RuleChange>,
}

// --------------------------------------------------------------- handlers --

/// Os ativos da carteira como as regras os enxergam.
pub(crate) fn health_assets(view: &PortfolioView) -> Vec<HealthAsset> {
    view.assets
        .iter()
        .map(|a| HealthAsset {
            id: a.asset.id,
            label: a.label(),
            asset_class: a.asset.asset_class,
            value: a.current_value,
            sector: a.sector.as_ref().map(|s| s.sector.clone()),
        })
        .collect()
}

pub(crate) fn sources_dto(states: &[(SectorSource, SyncState)]) -> Vec<SectorSourceDto> {
    SectorSource::ALL
        .into_iter()
        .map(|source| {
            let state = states.iter().find(|(s, _)| *s == source).map(|(_, s)| s);
            SectorSourceDto {
                source,
                asset_class: source.class(),
                updated_at: state.and_then(|s| s.last_success_at),
                failing: state.is_some_and(|s| s.last_error.is_some()),
            }
        })
        .collect()
}

pub async fn health(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<HealthDto>> {
    let view = service::portfolio(&state.store, user.id).await?;
    let config = state.store.health_config(user.id).await?;
    let assets = health_assets(&view);
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let income = state.store.income_history(user.id, today).await?;
    let evaluation = health::evaluate(&config, &assets, view.targets, &income);
    let states = state.store.sector_sync_states().await?;

    let unclassified_count = crate::analysis::unclassified_count(&view);

    Ok(Json(HealthDto {
        total_value: view.total_value,
        warnings: evaluation.warnings.iter().map(WarningDto::from).collect(),
        rules: evaluation
            .rules
            .iter()
            .map(|r: &RuleReport| RuleResultDto {
                rule: RuleDto::new(
                    r.rule,
                    RuleSetting {
                        enabled: r.enabled,
                        threshold: r.threshold,
                    },
                ),
                status: r.status,
            })
            .collect(),
        unclassified_count,
        sources: sources_dto(&states),
    }))
}

pub async fn rules(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<RulesResponse>> {
    let config = state.store.health_config(user.id).await?;
    Ok(Json(RulesResponse::from_config(&config)))
}

pub async fn save_rules(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<RulesRequest>,
) -> ApiResult<Json<RulesResponse>> {
    if body.rules.is_empty() {
        return Err(ApiError::validation("Informe ao menos uma regra."));
    }
    let current = state.store.health_config(user.id).await?;
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
    state.store.save_health_rules(user.id, &changes).await?;
    let config = state.store.health_config(user.id).await?;
    Ok(Json(RulesResponse::from_config(&config)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::health::GapPayer;
    use crate::domain::income::YearMonth;
    use rust_decimal_macros::dec;
    use serde_json::json;

    #[test]
    fn subjects_are_tagged_by_kind() {
        let id = Uuid::nil();
        let cases = [
            (
                Subject::Asset {
                    id,
                    label: "KNRI11".into(),
                },
                json!({"kind": "asset", "asset_id": id, "label": "KNRI11"}),
            ),
            (
                Subject::TopAssets {
                    labels: vec!["A".into(), "B".into()],
                },
                json!({"kind": "top_assets", "labels": ["A", "B"]}),
            ),
            (
                Subject::Sector {
                    class: AssetClass::BrStocks,
                    sector: "Financeiro".into(),
                },
                json!({"kind": "sector", "asset_class": "br_stocks", "sector": "Financeiro"}),
            ),
            (
                Subject::Class(AssetClass::Crypto),
                json!({"kind": "class", "asset_class": "crypto"}),
            ),
            (
                Subject::IncomeGaps {
                    empty_months: vec![YearMonth {
                        year: 2026,
                        month: 2,
                    }],
                    payers: vec![GapPayer {
                        id,
                        label: "KNRI11".into(),
                        asset_class: AssetClass::BrReits,
                        months: vec![2],
                    }],
                },
                json!({"kind": "income_months", "empty_months": ["2026-02"],
                    "payers": [{"asset_id": id, "label": "KNRI11", "asset_class": "br_reits", "months": [2]}]}),
            ),
            (
                Subject::Payer {
                    id: None,
                    label: "OLD3".into(),
                    received: dec!(90.5),
                    total: dec!(150),
                },
                json!({"kind": "payer", "asset_id": null, "label": "OLD3", "received": 90.5, "total": 150.0}),
            ),
        ];
        for (subject, expected) in cases {
            assert_eq!(
                serde_json::to_value(SubjectDto::from(&subject)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn a_warning_serializes_numbers_as_json_numbers() {
        let warning = Warning {
            rule: RuleId::MaxAsset,
            subject: Subject::Class(AssetClass::BrReits),
            current: dec!(23.5),
            limit: dec!(15),
            needed_contribution: Some(dec!(1234.56)),
        };
        assert_eq!(
            serde_json::to_value(WarningDto::from(&warning)).unwrap(),
            json!({
                "rule": "max_asset",
                "subject": {"kind": "class", "asset_class": "br_reits"},
                "unit": "percent",
                "current": 23.5,
                "limit": 15.0,
                "needed_contribution": 1234.56,
            })
        );
    }

    #[test]
    fn the_rule_list_has_every_rule_with_its_default() {
        let response = RulesResponse::from_config(&HealthConfig::default());
        let json = serde_json::to_value(&response).unwrap();
        let rules = json["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 7);
        assert_eq!(rules[0]["rule"], "max_asset");
        assert_eq!(rules[0]["threshold"], 15.0);
        assert_eq!(rules[0]["default_threshold"], 15.0);
        assert_eq!(rules[4]["rule"], "empty_class");
        assert!(rules[4]["threshold"].is_null());
        assert!(rules[4]["unit"].is_null());
        assert_eq!(rules[5]["rule"], "income_every_month");
        assert_eq!(rules[5]["unit"], "count");
        assert_eq!(rules[5]["threshold"], 12.0);
        assert_eq!(rules[6]["rule"], "max_payer");
        assert_eq!(rules[6]["unit"], "percent");
        assert_eq!(rules[6]["default_threshold"], 25.0);
    }

    #[test]
    fn sources_report_missing_and_failing_states() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let states = vec![(
            SectorSource::CvmReits,
            SyncState {
                last_attempt_at: at,
                last_success_at: Some(at),
                entries: 3,
                last_error: Some("fora".into()),
            },
        )];
        let sources = sources_dto(&states);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].source, SectorSource::B3Stocks);
        assert!(!sources[0].failing);
        assert_eq!(sources[0].updated_at, None);
        assert!(sources[1].failing);
        assert_eq!(sources[1].updated_at, Some(at));
    }
}
