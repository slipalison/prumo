//! Alertas: o canal vinculado, os tipos ligados, o que já foi avisado e o
//! código de vinculação (docs/alertas.md).

use std::collections::HashSet;

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime, Time};
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::alerts::{AlertConfig, AlertKind, TypeSetting, DEFAULT_SEND_AT};
use crate::domain::income::ObjectiveKind;

/// O canal e as preferências de envio de uma pessoa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertSettings {
    pub chat_id: Option<i64>,
    pub linked_at: Option<OffsetDateTime>,
    /// Horário de Brasília do resumo.
    pub send_at: Time,
    pub hide_values: bool,
    pub last_run_on: Option<Date>,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            chat_id: None,
            linked_at: None,
            send_at: DEFAULT_SEND_AT,
            hide_values: false,
            last_run_on: None,
        }
    }
}

/// Um objetivo atingido.
#[derive(Debug, Clone, PartialEq)]
pub struct AchievedObjective {
    pub id: Uuid,
    pub kind: ObjectiveKind,
    pub target: Decimal,
}

#[derive(sqlx::FromRow)]
struct AlertSettingsRow {
    telegram_chat_id: Option<i64>,
    linked_at: Option<OffsetDateTime>,
    send_at: Time,
    hide_values: bool,
    last_run_on: Option<Date>,
}

impl Store {
    pub async fn alert_settings(&self, user: Uuid) -> Result<AlertSettings, StoreError> {
        let row: Option<AlertSettingsRow> = sqlx::query_as(
            "select telegram_chat_id, linked_at, send_at, hide_values, last_run_on
               from alert_settings where user_id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        Ok(
            row.map_or_else(AlertSettings::default, |row| AlertSettings {
                chat_id: row.telegram_chat_id,
                linked_at: row.linked_at,
                send_at: row.send_at,
                hide_values: row.hide_values,
                last_run_on: row.last_run_on,
            }),
        )
    }

    /// Grava o horário e/ou "sem valores"; o que vem `None` fica como está.
    pub async fn save_alert_settings(
        &self,
        user: Uuid,
        send_at: Option<Time>,
        hide_values: Option<bool>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into alert_settings (user_id, send_at, hide_values)
             values ($1, coalesce($2, $4), coalesce($3, false))
             on conflict (user_id) do update set
                send_at = coalesce($2, alert_settings.send_at),
                hide_values = coalesce($3, alert_settings.hide_values),
                updated_at = now()",
        )
        .bind(user)
        .bind(send_at)
        .bind(hide_values)
        .bind(DEFAULT_SEND_AT)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Os tipos da pessoa (o que ela salvou por cima do padrão) e quais já
    /// tiveram a primeira apuração.
    pub async fn alert_config(
        &self,
        user: Uuid,
    ) -> Result<(AlertConfig, HashSet<AlertKind>), StoreError> {
        let rows: Vec<(String, bool, Option<Decimal>, bool)> = sqlx::query_as(
            "select kind, enabled, threshold, seeded from alert_types where user_id = $1",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        let mut config = AlertConfig::default();
        let mut seeded = HashSet::new();
        for (kind, enabled, threshold, was_seeded) in rows {
            let kind = AlertKind::parse(&kind).ok_or_else(|| decode_error(&kind))?;
            config.set(
                kind,
                TypeSetting {
                    enabled,
                    threshold: threshold
                        .map(|t| t.normalize())
                        .or_else(|| kind.default_threshold()),
                },
            );
            if was_seeded {
                seeded.insert(kind);
            }
        }
        Ok((config, seeded))
    }

    /// Grava os tipos dados, tudo ou nada. Ligar um tipo, ou mudar o limite
    /// dele, refaz a primeira apuração (ver `alert_types.seeded` na migração).
    pub async fn save_alert_types(
        &self,
        user: Uuid,
        types: &[(AlertKind, TypeSetting)],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for (kind, setting) in types {
            sqlx::query(
                "insert into alert_types (user_id, kind, enabled, threshold, seeded)
                 values ($1, $2, $3, $4, false)
                 on conflict (user_id, kind) do update set
                    enabled = excluded.enabled,
                    threshold = excluded.threshold,
                    seeded = alert_types.seeded
                             and alert_types.enabled and excluded.enabled
                             and alert_types.threshold is not distinct from excluded.threshold",
            )
            .bind(user)
            .bind(kind.as_str())
            .bind(setting.enabled)
            .bind(setting.threshold)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// O que já valia na última apuração do tipo.
    pub async fn alert_seen(
        &self,
        user: Uuid,
        kind: AlertKind,
    ) -> Result<HashSet<String>, StoreError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("select key from alert_seen where user_id = $1 and kind = $2")
                .bind(user)
                .bind(kind.as_str())
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(key,)| key).collect())
    }

    /// Troca o que já valia por `keys` e marca o tipo como apurado.
    pub async fn replace_alert_seen(
        &self,
        user: Uuid,
        kind: AlertKind,
        setting: TypeSetting,
        keys: &[String],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("delete from alert_seen where user_id = $1 and kind = $2")
            .bind(user)
            .bind(kind.as_str())
            .execute(&mut *tx)
            .await?;
        if !keys.is_empty() {
            sqlx::query(
                "insert into alert_seen (user_id, kind, key)
                 select $1, $2, unnest($3::text[]) on conflict do nothing",
            )
            .bind(user)
            .bind(kind.as_str())
            .bind(keys)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "insert into alert_types (user_id, kind, enabled, threshold, seeded)
             values ($1, $2, $3, $4, true)
             on conflict (user_id, kind) do update set seeded = true",
        )
        .bind(user)
        .bind(kind.as_str())
        .bind(setting.enabled)
        .bind(setting.threshold)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Os objetivos já atingidos (`achieved_at` preenchido).
    pub async fn achieved_objectives(
        &self,
        user: Uuid,
    ) -> Result<Vec<AchievedObjective>, StoreError> {
        let rows: Vec<(Uuid, String, Decimal)> = sqlx::query_as(
            "select id, kind, target from objectives
              where user_id = $1 and achieved_at is not null order by created_at, id",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(id, kind, target)| {
                Ok(AchievedObjective {
                    id,
                    kind: ObjectiveKind::parse(&kind).ok_or_else(|| decode_error(&kind))?,
                    target,
                })
            })
            .collect()
    }

    /// Quem já passou do horário do resumo hoje e ainda não o recebeu: tem chat
    /// vinculado, o horário já chegou e o último dia apurado é anterior a hoje.
    pub async fn alert_users_due(&self, today: Date, now: Time) -> Result<Vec<Uuid>, StoreError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "select user_id from alert_settings
              where telegram_chat_id is not null
                and send_at <= $2
                and (last_run_on is null or last_run_on < $1)
              order by user_id",
        )
        .bind(today)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn mark_alert_run(&self, user: Uuid, day: Date) -> Result<(), StoreError> {
        sqlx::query("update alert_settings set last_run_on = $2 where user_id = $1")
            .bind(user)
            .bind(day)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ------------------------------------------------------- vinculação --

    /// Guarda um código novo da pessoa; o anterior dela deixa de valer.
    /// `Duplicate` se o código já existe (quem chama sorteia outro).
    pub async fn create_link_code(
        &self,
        user: Uuid,
        code: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("delete from alert_link_codes where user_id = $1 or expires_at <= now()")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        let inserted = sqlx::query(
            "insert into alert_link_codes (code, user_id, expires_at) values ($1, $2, $3)
             on conflict do nothing",
        )
        .bind(code)
        .bind(user)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(StoreError::Duplicate);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Gasta o código: devolve a pessoa dele se ainda vale (e o apaga, uso
    /// único); `None` se não existe, já foi usado ou venceu.
    pub async fn consume_link_code(
        &self,
        code: &str,
        now: OffsetDateTime,
    ) -> Result<Option<Uuid>, StoreError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "delete from alert_link_codes where code = $1 and expires_at > $2 returning user_id",
        )
        .bind(code)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(user,)| user))
    }

    /// Liga o chat à pessoa. Um chat é de uma pessoa só: quem o tinha perde.
    /// Devolve a pessoa que perdeu o chat, se havia outra.
    pub async fn link_alert_chat(
        &self,
        user: Uuid,
        chat_id: i64,
        now: OffsetDateTime,
    ) -> Result<Option<Uuid>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let previous: Option<(Uuid,)> = sqlx::query_as(
            "update alert_settings set telegram_chat_id = null, linked_at = null, updated_at = now()
              where telegram_chat_id = $1 and user_id <> $2 returning user_id",
        )
        .bind(chat_id)
        .bind(user)
        .fetch_optional(&mut *tx)
        .await?;
        sqlx::query(
            "insert into alert_settings (user_id, telegram_chat_id, linked_at)
             values ($1, $2, $3)
             on conflict (user_id) do update set
                telegram_chat_id = excluded.telegram_chat_id,
                linked_at = excluded.linked_at,
                updated_at = now()",
        )
        .bind(user)
        .bind(chat_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(previous.map(|(id,)| id))
    }

    /// Desvincula a pessoa. `true` se havia chat.
    pub async fn unlink_alert_chat(&self, user: Uuid) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update alert_settings set telegram_chat_id = null, linked_at = null, updated_at = now()
              where user_id = $1 and telegram_chat_id is not null",
        )
        .bind(user)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Desvincula o chat (a pessoa mandou `/stop`, ou o Telegram recusou o
    /// envio). `true` se o chat estava vinculado.
    pub async fn unlink_alert_chat_by_id(&self, chat_id: i64) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update alert_settings set telegram_chat_id = null, linked_at = null, updated_at = now()
              where telegram_chat_id = $1",
        )
        .bind(chat_id)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    // ---------------------------------------------------------- polling --

    /// De onde o `getUpdates` continua (0 na primeira vez).
    pub async fn telegram_offset(&self) -> Result<i64, StoreError> {
        let row: Option<(i64,)> =
            sqlx::query_as("select next_offset from telegram_poll_state where id = 1")
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map_or(0, |(offset,)| offset))
    }

    pub async fn save_telegram_offset(&self, offset: i64) -> Result<(), StoreError> {
        sqlx::query(
            "insert into telegram_poll_state (id, next_offset) values (1, $1)
             on conflict (id) do update set next_offset = greatest(telegram_poll_state.next_offset, $1)",
        )
        .bind(offset)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
