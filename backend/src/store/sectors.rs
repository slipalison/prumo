//! Referência de setor (compartilhada entre as carteiras) e o estado da última
//! busca de cada fonte.

use std::collections::HashMap;

use rust_decimal::Decimal;
use time::OffsetDateTime;

use super::{decode_error, Store, StoreError};
use crate::domain::sector::{FundComposition, SectorInfo, SectorSource};
use crate::sectors::SectorEntry;

/// O que a última rodada deixou sobre uma fonte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    pub last_attempt_at: OffsetDateTime,
    pub last_success_at: Option<OffsetDateTime>,
    /// Quantas linhas a última busca boa gravou.
    pub entries: usize,
    pub last_error: Option<String>,
}

type SyncRow = (
    String,
    OffsetDateTime,
    Option<OffsetDateTime>,
    i32,
    Option<String>,
);

type ReferenceRow = (String, String, String, Option<String>, Option<String>);

fn sync_state(row: SyncRow) -> Result<(SectorSource, SyncState), sqlx::Error> {
    let (source, last_attempt_at, last_success_at, entries, last_error) = row;
    let source = SectorSource::parse(&source).ok_or_else(|| decode_error(&source))?;
    Ok((
        source,
        SyncState {
            last_attempt_at,
            last_success_at,
            entries: usize::try_from(entries).unwrap_or(0),
            last_error,
        },
    ))
}

impl Store {
    pub async fn sector_sync_state(
        &self,
        source: SectorSource,
    ) -> Result<Option<SyncState>, StoreError> {
        let row: Option<SyncRow> = sqlx::query_as(
            "select source, last_attempt_at, last_success_at, entries, last_error
               from sector_sync where source = $1",
        )
        .bind(source.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(sync_state).transpose()?.map(|(_, state)| state))
    }

    /// O estado de todas as fontes que já foram buscadas alguma vez.
    pub async fn sector_sync_states(&self) -> Result<Vec<(SectorSource, SyncState)>, StoreError> {
        let rows: Vec<SyncRow> = sqlx::query_as(
            "select source, last_attempt_at, last_success_at, entries, last_error
               from sector_sync order by source",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(sync_state).collect::<Result<_, _>>()?)
    }

    /// Registra uma busca que falhou, sem tocar na referência nem na data do
    /// último sucesso.
    pub async fn record_sector_sync_failure(
        &self,
        source: SectorSource,
        at: OffsetDateTime,
        error: &str,
    ) -> Result<(), StoreError> {
        let error: String = error.chars().take(300).collect();
        sqlx::query(
            "insert into sector_sync (source, last_attempt_at, last_error)
             values ($1, $2, $3)
             on conflict (source) do update set
                last_attempt_at = excluded.last_attempt_at,
                last_error = excluded.last_error",
        )
        .bind(source.as_str())
        .bind(at)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Troca a referência da fonte pela lista nova, de uma vez: quem lê nunca
    /// vê a tabela pela metade.
    pub async fn replace_sector_reference(
        &self,
        source: SectorSource,
        entries: &[SectorEntry],
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let codes: Vec<&str> = entries.iter().map(|e| e.code.as_str()).collect();
        let sectors: Vec<&str> = entries.iter().map(|e| e.sector.as_str()).collect();
        let subsectors: Vec<Option<&str>> =
            entries.iter().map(|e| e.subsector.as_deref()).collect();
        let segments: Vec<Option<&str>> = entries.iter().map(|e| e.segment.as_deref()).collect();
        let share = |pick: fn(&FundComposition) -> Decimal| -> Vec<Option<Decimal>> {
            entries
                .iter()
                .map(|e| e.composition.as_ref().map(pick))
                .collect()
        };
        let brick = share(|c| c.brick);
        let paper = share(|c| c.paper);
        let funds = share(|c| c.funds);

        let mut tx = self.pool.begin().await?;
        sqlx::query("delete from sector_reference where source = $1")
            .bind(source.as_str())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "insert into sector_reference
                    (source, code, sector, subsector, segment, brick_share, paper_share, funds_share, updated_at)
             select $1, code, sector, subsector, segment, brick, paper, funds, $6
               from unnest($2::text[], $3::text[], $4::text[], $5::text[],
                           $7::numeric[], $8::numeric[], $9::numeric[])
                    as e (code, sector, subsector, segment, brick, paper, funds)",
        )
        .bind(source.as_str())
        .bind(&codes)
        .bind(&sectors)
        .bind(&subsectors)
        .bind(&segments)
        .bind(at)
        .bind(&brick)
        .bind(&paper)
        .bind(&funds)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "insert into sector_sync (source, last_attempt_at, last_success_at, entries, last_error)
             values ($1, $2, $2, $3, null)
             on conflict (source) do update set
                last_attempt_at = excluded.last_attempt_at,
                last_success_at = excluded.last_success_at,
                entries = excluded.entries,
                last_error = null",
        )
        .bind(source.as_str())
        .bind(at)
        .bind(i32::try_from(entries.len()).unwrap_or(i32::MAX))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// A classificação dos pares (fonte, código) que existem na referência.
    pub async fn sector_reference(
        &self,
        keys: &[(SectorSource, String)],
    ) -> Result<HashMap<(SectorSource, String), SectorInfo>, StoreError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let sources: Vec<&str> = keys.iter().map(|(s, _)| s.as_str()).collect();
        let codes: Vec<&str> = keys.iter().map(|(_, c)| c.as_str()).collect();
        let rows: Vec<ReferenceRow> = sqlx::query_as(
            "select r.source, r.code, r.sector, r.subsector, r.segment
                   from sector_reference r
                   join unnest($1::text[], $2::text[]) as k (source, code)
                     on k.source = r.source and k.code = r.code",
        )
        .bind(&sources)
        .bind(&codes)
        .fetch_all(&self.pool)
        .await?;
        let mut found = HashMap::with_capacity(rows.len());
        for (source, code, sector, subsector, segment) in rows {
            let source = SectorSource::parse(&source).ok_or_else(|| decode_error(&source))?;
            found.insert(
                (source, code),
                SectorInfo {
                    sector,
                    subsector,
                    segment,
                },
            );
        }
        Ok(found)
    }

    /// A composição da carteira dos FIIs (código do emissor → frações), dos
    /// que a CVM classificou com ela.
    pub async fn fund_compositions(
        &self,
        codes: &[String],
    ) -> Result<HashMap<String, FundComposition>, StoreError> {
        if codes.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, Decimal, Decimal, Decimal)> = sqlx::query_as(
            "select code, brick_share, paper_share, funds_share
               from sector_reference
              where source = $1 and code = any($2) and brick_share is not null",
        )
        .bind(SectorSource::CvmReits.as_str())
        .bind(codes)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(code, brick, paper, funds)| {
                Some((code, FundComposition::from_shares(brick, paper, funds)?))
            })
            .collect())
    }
}
