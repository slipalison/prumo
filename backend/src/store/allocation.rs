//! A configuração do motor do aporte por pessoa (`allocation_settings`).

use uuid::Uuid;

use super::{Store, StoreError};
use crate::domain::allocation::Limits;

impl Store {
    /// Os guarda-corpos da pessoa. Sem linha gravada, tudo desligado.
    pub async fn allocation_limits(&self, user: Uuid) -> Result<Limits, StoreError> {
        let row: Option<(Option<i32>,)> =
            sqlx::query_as("select max_asset_bp from allocation_settings where user_id = $1")
                .bind(user)
                .fetch_optional(&self.pool)
                .await?;
        Ok(Limits {
            max_asset_bp: row
                .and_then(|(bp,)| bp)
                .and_then(|bp| u32::try_from(bp).ok()),
        })
    }

    pub async fn set_allocation_limits(
        &self,
        user: Uuid,
        limits: Limits,
    ) -> Result<(), StoreError> {
        let bp = limits
            .max_asset_bp
            .map(|bp| {
                i32::try_from(bp).map_err(|_| StoreError::Invalid("limite fora da faixa".into()))
            })
            .transpose()?;
        sqlx::query(
            "insert into allocation_settings (user_id, max_asset_bp) values ($1, $2)
             on conflict (user_id) do update set max_asset_bp = excluded.max_asset_bp, updated_at = now()",
        )
        .bind(user)
        .bind(bp)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
