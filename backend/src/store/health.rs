//! As regras de saúde que cada pessoa ligou, desligou ou ajustou.

use rust_decimal::Decimal;
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::health::{HealthConfig, RuleId, RuleSetting};

impl Store {
    /// As cinco regras da pessoa: o que ela salvou por cima do padrão.
    pub async fn health_config(&self, user: Uuid) -> Result<HealthConfig, StoreError> {
        let rows: Vec<(String, bool, Option<Decimal>)> =
            sqlx::query_as("select rule, enabled, threshold from health_rules where user_id = $1")
                .bind(user)
                .fetch_all(&self.pool)
                .await?;
        let mut config = HealthConfig::default();
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

    /// Grava as regras dadas (as outras ficam como estão), tudo ou nada.
    pub async fn save_health_rules(
        &self,
        user: Uuid,
        rules: &[(RuleId, RuleSetting)],
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
        tx.commit().await?;
        Ok(())
    }
}
