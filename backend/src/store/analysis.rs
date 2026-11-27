//! O que a Análise guarda da pessoa: as regras que ela mexeu (a mesma tabela
//! `health_rules` da Saúde, agora com as regras de dividendos e de
//! rentabilidade), o rendimento-alvo da referência de Bazin e os prêmios do
//! rendimento-alvo dos FIIs.

use rust_decimal::Decimal;
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::analysis::fund_references::Premiums;
use crate::domain::analysis::references::DEFAULT_BAZIN_YIELD;
use crate::domain::analysis::{AnalysisConfig, RuleId};
use crate::domain::health::RuleSetting;

impl Store {
    /// As regras da pessoa: o que ela salvou por cima do padrão.
    pub async fn analysis_config(&self, user: Uuid) -> Result<AnalysisConfig, StoreError> {
        let rows: Vec<(String, bool, Option<Decimal>)> =
            sqlx::query_as("select rule, enabled, threshold from health_rules where user_id = $1")
                .bind(user)
                .fetch_all(&self.pool)
                .await?;
        let mut config = AnalysisConfig::default();
        for (rule, enabled, threshold) in rows {
            let rule = RuleId::parse(&rule).ok_or_else(|| decode_error(&rule))?;
            config.set(
                rule,
                RuleSetting {
                    enabled,
                    // Sem limite salvo (regra que não tem, ou linha sem ele), vale o padrão.
                    threshold: threshold
                        .map(|t| t.normalize())
                        .or_else(|| rule.default_threshold()),
                },
            );
        }
        Ok(config)
    }

    /// Grava as regras dadas (as outras ficam como estão) e, do que vier, o
    /// rendimento-alvo do Bazin e os prêmios dos FIIs, tudo ou nada.
    pub async fn save_analysis(
        &self,
        user: Uuid,
        rules: &[(RuleId, RuleSetting)],
        settings: SettingsChange,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for (rule, setting) in rules {
            sqlx::query(
                "insert into health_rules (user_id, rule, enabled, threshold)
                 values ($1, $2, $3, $4)
                 on conflict (user_id, rule) do update set
                    enabled = excluded.enabled, threshold = excluded.threshold",
            )
            .bind(user)
            .bind(rule.as_str())
            .bind(setting.enabled)
            .bind(setting.threshold)
            .execute(&mut *tx)
            .await?;
        }
        if settings != SettingsChange::default() {
            sqlx::query(
                "insert into analysis_settings (user_id, bazin_yield, brick_premium, paper_premium)
                 values ($1, $2, $3, $4)
                 on conflict (user_id) do update set
                    bazin_yield = coalesce(excluded.bazin_yield, analysis_settings.bazin_yield),
                    brick_premium = coalesce(excluded.brick_premium, analysis_settings.brick_premium),
                    paper_premium = coalesce(excluded.paper_premium, analysis_settings.paper_premium)",
            )
            .bind(user)
            .bind(settings.bazin_yield)
            .bind(settings.brick_premium)
            .bind(settings.paper_premium)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// O rendimento-alvo do Bazin e os prêmios dos FIIs da pessoa; o que ela
    /// não mexeu vale o padrão do código.
    pub async fn reference_settings(&self, user: Uuid) -> Result<ReferenceSettings, StoreError> {
        let row: Option<(Option<Decimal>, Option<Decimal>, Option<Decimal>)> = sqlx::query_as(
            "select bazin_yield, brick_premium, paper_premium
               from analysis_settings where user_id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        let (bazin_yield, brick, paper) = row.unwrap_or_default();
        let defaults = Premiums::default();
        Ok(ReferenceSettings {
            bazin_yield: bazin_yield.map_or(DEFAULT_BAZIN_YIELD, |v| v.normalize()),
            premiums: Premiums {
                brick: brick.map_or(defaults.brick, |v| v.normalize()),
                paper: paper.map_or(defaults.paper, |v| v.normalize()),
            },
        })
    }
}

/// O que as referências de preço pedem da pessoa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceSettings {
    /// O rendimento-alvo do Bazin, em % ao ano.
    pub bazin_yield: Decimal,
    pub premiums: Premiums,
}

/// O que mudar nas configurações das referências; `None` fica como está.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsChange {
    pub bazin_yield: Option<Decimal>,
    pub brick_premium: Option<Decimal>,
    pub paper_premium: Option<Decimal>,
}
