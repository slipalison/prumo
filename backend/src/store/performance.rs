//! Séries de referência, fechamentos e as fotos diárias da carteira.

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::pool::PoolConnection;
use sqlx::{FromRow, Postgres};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{Store, StoreError};
use crate::domain::asset_class::UnitKind;
use crate::domain::performance::{Position, Purchase, Quota};
use crate::history::{IndexCode, Series};

/// A foto de um dia.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub date: Date,
    pub market_value: Decimal,
    pub net_flow: Decimal,
    pub quota: Quota,
    pub positions: Vec<Position>,
    pub taken_at: OffsetDateTime,
}

/// A foto sem as posições: o que as telas leem.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapshotPoint {
    pub date: Date,
    pub market_value: Decimal,
    pub net_flow: Decimal,
    pub quota_value: Decimal,
}

#[derive(FromRow)]
struct SnapshotRow {
    date: Date,
    market_value: Decimal,
    net_flow: Decimal,
    quota_value: Decimal,
    quotas: Decimal,
    positions: String,
    taken_at: OffsetDateTime,
}

impl TryFrom<SnapshotRow> for Snapshot {
    type Error = sqlx::Error;

    fn try_from(row: SnapshotRow) -> Result<Self, Self::Error> {
        let positions: Vec<Position> = serde_json::from_str(&row.positions)
            .map_err(|e| sqlx::Error::Decode(format!("posicoes da foto: {e}").into()))?;
        Ok(Snapshot {
            date: row.date,
            market_value: row.market_value,
            net_flow: row.net_flow,
            quota: Quota {
                quotas: row.quotas.normalize(),
                value: row.quota_value.normalize(),
            },
            positions,
            taken_at: row.taken_at,
        })
    }
}

/// O advisory lock da tarefa diária, preso a uma conexão. Duas réplicas
/// pedem o mesmo número; só uma leva. Se o processo cair, o Postgres solta o
/// lock quando a conexão fecha.
pub struct JobLock {
    key: i64,
    conn: Option<PoolConnection<Postgres>>,
}

impl JobLock {
    /// Solta o lock e devolve a conexão ao pool. Se o `unlock` falhar, a
    /// conexão é fechada (e o lock vai junto).
    pub async fn release(mut self) {
        if let Some(mut conn) = self.conn.take() {
            let unlocked: Result<(bool,), sqlx::Error> =
                sqlx::query_as("select pg_advisory_unlock($1)")
                    .bind(self.key)
                    .fetch_one(&mut *conn)
                    .await;
            if !matches!(unlocked, Ok((true,))) {
                drop(conn.detach());
            }
        }
    }
}

impl Drop for JobLock {
    /// Sem `release` (pânico, tarefa cancelada): a conexão não volta ao pool
    /// com o lock preso — ela é fechada.
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn.detach());
        }
    }
}

impl Store {
    /// Tenta o advisory lock `key` sem esperar. `None`: outra réplica está
    /// rodando a tarefa.
    pub async fn try_job_lock(&self, key: i64) -> Result<Option<JobLock>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let (taken,): (bool,) = sqlx::query_as("select pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut *conn)
            .await?;
        Ok(taken.then(|| JobLock {
            key,
            conn: Some(conn),
        }))
    }

    /// A primeira e a última data guardadas de um índice.
    pub async fn index_bounds(&self, code: IndexCode) -> Result<Option<(Date, Date)>, StoreError> {
        let (first, last): (Option<Date>, Option<Date>) =
            sqlx::query_as("select min(date), max(date) from index_history where code = $1")
                .bind(code.as_str())
                .fetch_one(&self.pool)
                .await?;
        Ok(first.zip(last))
    }

    /// Grava (ou corrige) os valores de um índice.
    pub async fn upsert_index(&self, code: IndexCode, values: &Series) -> Result<(), StoreError> {
        if values.is_empty() {
            return Ok(());
        }
        let dates: Vec<Date> = values.iter().map(|(d, _)| *d).collect();
        let numbers: Vec<Decimal> = values.iter().map(|(_, v)| *v).collect();
        sqlx::query(
            "insert into index_history (code, date, value)
             select $1, d, v from unnest($2::date[], $3::numeric[]) as t (d, v)
             on conflict (code, date) do update set value = excluded.value",
        )
        .bind(code.as_str())
        .bind(&dates)
        .bind(&numbers)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O índice de `from` a `to`, inclusive, em ordem.
    pub async fn index_series(
        &self,
        code: IndexCode,
        from: Date,
        to: Date,
    ) -> Result<Series, StoreError> {
        let rows: Vec<(Date, Decimal)> = sqlx::query_as(
            "select date, value from index_history
              where code = $1 and date between $2 and $3 order by date",
        )
        .bind(code.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(d, v)| (d, v.normalize())).collect())
    }

    pub async fn last_price_date(&self, symbol: &str) -> Result<Option<Date>, StoreError> {
        let (date,): (Option<Date>,) =
            sqlx::query_as("select max(date) from price_history where symbol = $1")
                .bind(symbol)
                .fetch_one(&self.pool)
                .await?;
        Ok(date)
    }

    /// Grava fechamentos. Data que já existe não muda: o cache guarda o preço
    /// como era no dia (o Yahoo reescreve o passado num desdobramento).
    pub async fn insert_prices(&self, symbol: &str, closes: &Series) -> Result<(), StoreError> {
        if closes.is_empty() {
            return Ok(());
        }
        let dates: Vec<Date> = closes.iter().map(|(d, _)| *d).collect();
        let values: Vec<Decimal> = closes.iter().map(|(_, v)| *v).collect();
        sqlx::query(
            "insert into price_history (symbol, date, close)
             select $1, d, c from unnest($2::date[], $3::numeric[]) as t (d, c)
             on conflict (symbol, date) do nothing",
        )
        .bind(symbol)
        .bind(&dates)
        .bind(&values)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O fechamento de cada símbolo exatamente em `date` (quem não tem, fica
    /// de fora).
    pub async fn closes_on(
        &self,
        symbols: &[String],
        date: Date,
    ) -> Result<HashMap<String, Decimal>, StoreError> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, Decimal)> = sqlx::query_as(
            "select symbol, close from price_history where date = $1 and symbol = any($2)",
        )
        .bind(date)
        .bind(symbols)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(s, c)| (s, c.normalize())).collect())
    }

    /// As moedas que aparecem nas cotações dos símbolos dados.
    pub async fn quote_currencies(&self, symbols: &[String]) -> Result<Vec<String>, StoreError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "select distinct currency from quotes where symbol = any($1) order by currency",
        )
        .bind(symbols)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(c,)| c).collect())
    }

    /// Quem entra na tarefa diária: quem tem ativo ou já tem série.
    pub async fn snapshot_users(&self) -> Result<Vec<Uuid>, StoreError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "select id from users u
              where exists (select 1 from assets a where a.user_id = u.id)
                 or exists (select 1 from portfolio_snapshots s where s.user_id = u.id)
              order by id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// A primeira foto de qualquer carteira: de onde as séries de referência
    /// precisam começar.
    pub async fn first_snapshot_date(&self) -> Result<Option<Date>, StoreError> {
        let (date,): (Option<Date>,) = sqlx::query_as("select min(date) from portfolio_snapshots")
            .fetch_one(&self.pool)
            .await?;
        Ok(date)
    }

    /// A última foto da pessoa antes de `date`.
    pub async fn snapshot_before(
        &self,
        user: Uuid,
        date: Date,
    ) -> Result<Option<Snapshot>, StoreError> {
        let row: Option<SnapshotRow> = sqlx::query_as(
            "select date, market_value, net_flow, quota_value, quotas, positions::text as positions, taken_at
               from portfolio_snapshots where user_id = $1 and date < $2
              order by date desc limit 1",
        )
        .bind(user)
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Snapshot::try_from).transpose()?)
    }

    /// Grava a foto do dia, ou refaz a que já existe (a tarefa é idempotente).
    pub async fn upsert_snapshot(&self, user: Uuid, snapshot: &Snapshot) -> Result<(), StoreError> {
        let positions = serde_json::to_string(&snapshot.positions)
            .map_err(|e| StoreError::Invalid(format!("posicoes da foto: {e}")))?;
        sqlx::query(
            "insert into portfolio_snapshots
                (user_id, date, market_value, net_flow, quota_value, quotas, positions, taken_at)
             values ($1, $2, $3, $4, $5, $6, $7::jsonb, $8)
             on conflict (user_id, date) do update set
                market_value = excluded.market_value,
                net_flow = excluded.net_flow,
                quota_value = excluded.quota_value,
                quotas = excluded.quotas,
                positions = excluded.positions,
                taken_at = excluded.taken_at",
        )
        .bind(user)
        .bind(snapshot.date)
        .bind(snapshot.market_value)
        .bind(snapshot.net_flow)
        .bind(snapshot.quota.value)
        .bind(snapshot.quota.quotas)
        .bind(positions)
        .bind(snapshot.taken_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A série inteira da pessoa, em ordem.
    pub async fn snapshot_points(&self, user: Uuid) -> Result<Vec<SnapshotPoint>, StoreError> {
        let rows: Vec<(Date, Decimal, Decimal, Decimal)> = sqlx::query_as(
            "select date, market_value, net_flow, quota_value from portfolio_snapshots
              where user_id = $1 order by date",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(date, market_value, net_flow, quota_value)| SnapshotPoint {
                    date,
                    market_value,
                    net_flow,
                    quota_value: quota_value.normalize(),
                },
            )
            .collect())
    }

    /// O que os aportes registrados compraram em `(after, until]`, por item.
    /// Num ativo por valor, a quantidade comprada é o próprio valor.
    pub async fn purchases_between(
        &self,
        user: Uuid,
        after: OffsetDateTime,
        until: OffsetDateTime,
    ) -> Result<Vec<Purchase>, StoreError> {
        let rows: Vec<(Uuid, Decimal, Decimal, String, bool)> = sqlx::query_as(
            "select i.asset_id, i.quantity, i.value, a.asset_class, a.listed_on_b3
               from contribution_items i
               join contributions c on c.id = i.contribution_id
               join assets a on a.id = i.asset_id
              where c.user_id = $1 and c.created_at > $2 and c.created_at <= $3",
        )
        .bind(user)
        .bind(after)
        .bind(until)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(asset_id, quantity, value, class, listed_on_b3)| {
                let by_value = crate::domain::asset_class::AssetClass::parse(&class)
                    .is_some_and(|c| c.market(listed_on_b3).unit_kind() == UnitKind::Value);
                Purchase {
                    asset_id,
                    quantity: if by_value { value } else { quantity },
                    value,
                }
            })
            .collect())
    }
}
