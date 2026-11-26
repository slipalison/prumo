//! As regras de saúde que cada pessoa ligou, desligou ou ajustou.

use uuid::Uuid;

use super::{Store, StoreError};
use crate::domain::health::{HealthConfig, IncomeHistory, RuleId, RuleSetting};
use crate::domain::income::Entry;
use time::Date;

impl Store {
    /// As sete regras originais da pessoa (as da Saúde). A configuração é a
    /// mesma da Análise: as regras novas ficam de fora.
    pub async fn health_config(&self, user: Uuid) -> Result<HealthConfig, StoreError> {
        Ok(self.analysis_config(user).await?.health())
    }

    /// Os proventos da pessoa como as regras de renda os leem: o livro todo,
    /// do mais velho ao mais novo, com `today` para separar o recebido.
    pub async fn income_history(
        &self,
        user: Uuid,
        today: Date,
    ) -> Result<IncomeHistory, StoreError> {
        let entries = self
            .income_ledger(user)
            .await?
            .iter()
            .map(|event| Entry {
                pay_date: event.pay_date,
                net: event.net,
                payer_key: event.payer_key(),
                payer_label: event.payer_label(),
            })
            .collect();
        Ok(IncomeHistory { today, entries })
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
