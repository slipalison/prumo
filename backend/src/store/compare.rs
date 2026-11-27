//! Desdobramentos dos símbolos que o comparador consulta (migração 0030).

use std::collections::HashMap;

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};

use super::{Store, StoreError};

impl Store {
    /// Os desdobramentos e bonificações guardados dos símbolos, por data.
    pub async fn price_splits(
        &self,
        symbols: &[String],
    ) -> Result<HashMap<String, Vec<(Date, Decimal)>>, StoreError> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, Date, Decimal)> = sqlx::query_as(
            "select symbol, ex_date, factor from price_splits
              where symbol = any($1) order by symbol, ex_date",
        )
        .bind(symbols)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<String, Vec<(Date, Decimal)>> = HashMap::new();
        for (symbol, date, factor) in rows {
            out.entry(symbol)
                .or_default()
                .push((date, factor.normalize()));
        }
        Ok(out)
    }

    /// Desde quando e até quando a fonte já foi conferida para os
    /// desdobramentos do símbolo.
    pub async fn split_check(
        &self,
        symbol: &str,
    ) -> Result<Option<(Date, OffsetDateTime)>, StoreError> {
        Ok(sqlx::query_as(
            "select checked_from, checked_at from price_split_checks where symbol = $1",
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Grava os desdobramentos que a fonte informou para a janela a partir de
    /// `from` e marca a conferência, numa transação. Um evento antigo que a
    /// fonte já não traz dentro da janela sai (correção da fonte).
    pub async fn save_price_splits(
        &self,
        symbol: &str,
        from: Date,
        splits: &[(Date, Decimal)],
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        let dates: Vec<Date> = splits.iter().map(|(d, _)| *d).collect();
        let factors: Vec<Decimal> = splits.iter().map(|(_, f)| *f).collect();
        sqlx::query(
            "delete from price_splits where symbol = $1 and ex_date >= $2 and ex_date <> all($3::date[])",
        )
        .bind(symbol)
        .bind(from)
        .bind(&dates)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "insert into price_splits (symbol, ex_date, factor)
             select $1, d, f from unnest($2::date[], $3::numeric[]) as t (d, f)
             on conflict (symbol, ex_date) do update set factor = excluded.factor",
        )
        .bind(symbol)
        .bind(&dates)
        .bind(&factors)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "insert into price_split_checks (symbol, checked_from, checked_at) values ($1, $2, $3)
             on conflict (symbol) do update set
                checked_from = least(price_split_checks.checked_from, excluded.checked_from),
                checked_at = excluded.checked_at",
        )
        .bind(symbol)
        .bind(from)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
