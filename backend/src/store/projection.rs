//! As medianas do Focus que a projeção usa (`focus_expectations`) e a vez da
//! busca diária (`focus_sync`). A busca não segura conexão durante a rede:
//! reivindica a vez com um `update` condicional, busca e grava numa transação
//! curta.

use rust_decimal::Decimal;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};

use super::{decode_error, Store, StoreError};
use crate::domain::projection::rates::{Expectation, Indicator, Period};

#[derive(FromRow)]
struct ExpectationRow {
    indicator: String,
    period: String,
    reference: Date,
    median: Decimal,
    survey_date: Date,
}

impl TryFrom<ExpectationRow> for Expectation {
    type Error = sqlx::Error;

    fn try_from(row: ExpectationRow) -> Result<Self, Self::Error> {
        Ok(Expectation {
            indicator: Indicator::parse(&row.indicator)
                .ok_or_else(|| decode_error(&row.indicator))?,
            period: Period::parse(&row.period).ok_or_else(|| decode_error(&row.period))?,
            reference: row.reference,
            median: row.median.normalize(),
            survey_date: row.survey_date,
        })
    }
}

impl Store {
    /// Todas as medianas guardadas.
    pub async fn focus_expectations(&self) -> Result<Vec<Expectation>, StoreError> {
        let rows: Vec<ExpectationRow> = sqlx::query_as(
            "select indicator, period, reference, median, survey_date
               from focus_expectations
              order by indicator, period, reference",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(Expectation::try_from)
            .collect::<Result<_, _>>()?)
    }

    /// Quando a última busca deu certo.
    pub async fn focus_fetched_at(&self) -> Result<Option<OffsetDateTime>, StoreError> {
        let row: Option<(Option<OffsetDateTime>,)> =
            sqlx::query_as("select fetched_at from focus_sync where singleton")
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(at,)| at))
    }

    /// Reivindica a busca: só quando a última é de antes de `stale_before` (ou
    /// não houve) e nenhuma réplica tem a vez agora. `true`: a vez é desta.
    pub async fn claim_focus_sync(
        &self,
        now: OffsetDateTime,
        stale_before: OffsetDateTime,
        until: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let claimed: Option<(bool,)> = sqlx::query_as(
            "insert into focus_sync as s (singleton, claimed_until) values (true, $3)
             on conflict (singleton) do update set claimed_until = excluded.claimed_until
              where (s.fetched_at is null or s.fetched_at < $2)
                and (s.claimed_until is null or s.claimed_until < $1)
             returning true",
        )
        .bind(now)
        .bind(stale_before)
        .bind(until)
        .fetch_optional(&self.pool)
        .await?;
        Ok(claimed.is_some())
    }

    /// Devolve a vez sem gravar (a busca falhou).
    pub async fn release_focus_claim(&self) -> Result<(), StoreError> {
        sqlx::query("update focus_sync set claimed_until = null where singleton")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Troca as medianas de cada indicador e período que vieram (o que não
    /// veio fica como estava) e marca a busca como feita.
    pub async fn save_focus(
        &self,
        rows: &[Expectation],
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended('focus_expectations', 0))")
            .execute(&mut *tx)
            .await?;
        let mut pairs: Vec<(Indicator, Period)> =
            rows.iter().map(|r| (r.indicator, r.period)).collect();
        pairs.sort_unstable();
        pairs.dedup();
        for (indicator, period) in pairs {
            sqlx::query("delete from focus_expectations where indicator = $1 and period = $2")
                .bind(indicator.as_str())
                .bind(period.as_str())
                .execute(&mut *tx)
                .await?;
            let chosen: Vec<&Expectation> = rows
                .iter()
                .filter(|r| r.indicator == indicator && r.period == period)
                .collect();
            let references: Vec<Date> = chosen.iter().map(|r| r.reference).collect();
            let medians: Vec<Decimal> = chosen.iter().map(|r| r.median).collect();
            let surveys: Vec<Date> = chosen.iter().map(|r| r.survey_date).collect();
            sqlx::query(
                "insert into focus_expectations (indicator, period, reference, median, survey_date)
                 select $1, $2, r, m, s from unnest($3::date[], $4::numeric[], $5::date[]) as t (r, m, s)
                 on conflict (indicator, period, reference) do update
                    set median = excluded.median, survey_date = excluded.survey_date",
            )
            .bind(indicator.as_str())
            .bind(period.as_str())
            .bind(&references)
            .bind(&medians)
            .bind(&surveys)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "insert into focus_sync (singleton, fetched_at, claimed_until) values (true, $1, null)
             on conflict (singleton) do update set fetched_at = $1, claimed_until = null",
        )
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
