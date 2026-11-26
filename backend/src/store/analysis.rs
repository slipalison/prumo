//! O que a Análise guarda da pessoa: as regras que ela mexeu (a mesma tabela
//! `health_rules` da Saúde, agora com as regras de dividendos e de
//! rentabilidade) e o rendimento-alvo da referência de Bazin.

use rust_decimal::Decimal;
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
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

    /// Grava as regras dadas (as outras ficam como estão) e, se vier, o
    /// rendimento-alvo do Bazin, tudo ou nada.
    pub async fn save_analysis(
        &self,
        user: Uuid,
        rules: &[(RuleId, RuleSetting)],
        bazin_yield: Option<Decimal>,
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
        if let Some(value) = bazin_yield {
            sqlx::query(
                "insert into analysis_settings (user_id, bazin_yield) values ($1, $2)
                 on conflict (user_id) do update set bazin_yield = excluded.bazin_yield",
            )
            .bind(user)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// O rendimento-alvo do Bazin da pessoa, em % ao ano (padrão 6%).
    pub async fn bazin_yield(&self, user: Uuid) -> Result<Decimal, StoreError> {
        let row: Option<(Decimal,)> =
            sqlx::query_as("select bazin_yield from analysis_settings where user_id = $1")
                .bind(user)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map_or(DEFAULT_BAZIN_YIELD, |(value,)| value.normalize()))
    }
}
