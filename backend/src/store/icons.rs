//! Cache dos logos dos ativos (`asset_icons`, migração 0019): o SVG já saneado
//! de cada símbolo e o estado da última busca.
//!
//! A reserva dos símbolos a buscar ([`Store::claim_icons`]) é uma transação
//! curta sob um advisory lock transacional: o lock cai no commit e nenhuma
//! conexão fica presa durante a ida à rede. Quem chega depois enxerga a reserva
//! e pula o símbolo.

use std::collections::HashMap;

use time::OffsetDateTime;

use super::{decode_error, Store, StoreError};

/// Como terminou a última tentativa de um símbolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconOutcome {
    /// Reservado por uma réplica que ainda busca (ou que caiu no meio).
    Pending,
    Found,
    /// A fonte não tem logo para o símbolo (ou o arquivo foi recusado).
    Missing,
    /// Rede, limite ou token: tenta de novo em breve.
    Failed,
}

impl IconOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            IconOutcome::Pending => "pending",
            IconOutcome::Found => "found",
            IconOutcome::Missing => "missing",
            IconOutcome::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            IconOutcome::Pending,
            IconOutcome::Found,
            IconOutcome::Missing,
            IconOutcome::Failed,
        ]
        .into_iter()
        .find(|o| o.as_str() == value)
    }
}

/// O que se sabe de um símbolo para decidir se busca de novo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconState {
    pub outcome: IconOutcome,
    pub attempted_at: Option<OffsetDateTime>,
    pub claimed_at: Option<OffsetDateTime>,
}

/// Um logo guardado, pronto para servir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredIcon {
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub etag: String,
}

type StateRow = (
    String,
    String,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
);

fn state_of(row: StateRow) -> Result<(String, IconState), sqlx::Error> {
    let (symbol, outcome, attempted_at, claimed_at) = row;
    let outcome = IconOutcome::parse(&outcome).ok_or_else(|| decode_error(&outcome))?;
    Ok((
        symbol,
        IconState {
            outcome,
            attempted_at,
            claimed_at,
        },
    ))
}

impl Store {
    /// Reserva, numa transação só, os símbolos de `symbols` que `is_due` manda
    /// buscar, e devolve só esses. Se outra réplica está reservando neste
    /// instante, devolve vazio (a próxima rodada confere de novo).
    pub async fn claim_icons(
        &self,
        symbols: &[String],
        lock_key: i64,
        now: OffsetDateTime,
        is_due: impl Fn(Option<&IconState>) -> bool,
    ) -> Result<Vec<String>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let (locked,): (bool,) = sqlx::query_as("select pg_try_advisory_xact_lock($1)")
            .bind(lock_key)
            .fetch_one(&mut *tx)
            .await?;
        if !locked {
            return Ok(Vec::new());
        }
        let rows: Vec<StateRow> = sqlx::query_as(
            "select symbol, outcome, attempted_at, claimed_at
               from asset_icons where symbol = any($1)",
        )
        .bind(symbols)
        .fetch_all(&mut *tx)
        .await?;
        let states: HashMap<String, IconState> =
            rows.into_iter().map(state_of).collect::<Result<_, _>>()?;
        let due: Vec<String> = symbols
            .iter()
            .filter(|s| is_due(states.get(*s)))
            .cloned()
            .collect();
        if !due.is_empty() {
            sqlx::query(
                "insert into asset_icons (symbol, outcome, claimed_at)
                 select s, 'pending', $2 from unnest($1::text[]) as t (s)
                 on conflict (symbol) do update set claimed_at = excluded.claimed_at",
            )
            .bind(&due)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(due)
    }

    /// Larga a reserva dos símbolos que a rodada desistiu de buscar.
    pub async fn release_icon_claims(&self, symbols: &[String]) -> Result<(), StoreError> {
        sqlx::query("update asset_icons set claimed_at = null where symbol = any($1)")
            .bind(symbols)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Grava o logo (já saneado) e encerra a reserva.
    pub async fn save_icon(
        &self,
        symbol: &str,
        svg: &[u8],
        source_url: &str,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into asset_icons
                 (symbol, outcome, content_type, bytes, source_url, fetched_at, attempted_at)
             values ($1, 'found', 'image/svg+xml', $2, $3, $4, $4)
             on conflict (symbol) do update set
                 outcome = 'found', content_type = 'image/svg+xml', bytes = excluded.bytes,
                 source_url = excluded.source_url, fetched_at = excluded.fetched_at,
                 attempted_at = excluded.attempted_at, claimed_at = null, error = null",
        )
        .bind(symbol)
        .bind(svg)
        .bind(source_url)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A fonte não tem (ou recusamos) o logo: apaga o que houvesse e encerra a
    /// reserva.
    pub async fn save_icon_missing(
        &self,
        symbol: &str,
        reason: &str,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let reason: String = reason.chars().take(300).collect();
        sqlx::query(
            "insert into asset_icons (symbol, outcome, attempted_at, error)
             values ($1, 'missing', $2, $3)
             on conflict (symbol) do update set
                 outcome = 'missing', content_type = null, bytes = null, source_url = null,
                 attempted_at = excluded.attempted_at, claimed_at = null, error = excluded.error",
        )
        .bind(symbol)
        .bind(now)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A busca falhou: guarda o motivo e encerra a reserva, sem tocar no logo
    /// que já houvesse (um logo velho vale mais que nenhum).
    pub async fn save_icon_failure(
        &self,
        symbol: &str,
        reason: &str,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let reason: String = reason.chars().take(300).collect();
        sqlx::query(
            "insert into asset_icons (symbol, outcome, attempted_at, error)
             values ($1, 'failed', $2, $3)
             on conflict (symbol) do update set
                 outcome = 'failed', attempted_at = excluded.attempted_at,
                 claimed_at = null, error = excluded.error",
        )
        .bind(symbol)
        .bind(now)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O logo guardado do símbolo, se há.
    pub async fn icon(&self, symbol: &str) -> Result<Option<StoredIcon>, StoreError> {
        let row: Option<(String, Vec<u8>, String)> = sqlx::query_as(
            "select content_type, bytes, etag from asset_icons
              where symbol = $1 and bytes is not null",
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(content_type, bytes, etag)| StoredIcon {
            content_type,
            bytes,
            etag,
        }))
    }

    /// Os símbolos com logo e a versão de cada um (o `ETag`), sem os bytes.
    pub async fn icon_versions(&self) -> Result<Vec<(String, String)>, StoreError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "select symbol, etag from asset_icons where bytes is not null order by symbol",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// O estado de um símbolo (o que a próxima rodada vai olhar).
    pub async fn icon_state(&self, symbol: &str) -> Result<Option<IconState>, StoreError> {
        let row: Option<StateRow> = sqlx::query_as(
            "select symbol, outcome, attempted_at, claimed_at from asset_icons where symbol = $1",
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(state_of).transpose()?.map(|(_, state)| state))
    }
}
