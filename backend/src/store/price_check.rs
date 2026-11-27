//! O filtro de fechamento ruim na entrada de `price_history` e a limpeza do que
//! já estava gravado. A regra está em [`crate::domain::price_check`]; aqui, o
//! que ela precisa do banco: os fechamentos guardados em volta, os
//! desdobramentos e quem tem o ativo (a série dessas pessoas é refeita quando
//! um fechamento guardado sai).

use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;
use time::{Date, Duration};
use uuid::Uuid;

use super::{Store, StoreError};
use crate::domain::price_check::bad_closes;
use crate::history::Series;

/// Quanto do que já está guardado, antes e depois dos fechamentos que chegam,
/// entra na conferência: 20 dias corridos são mais que os 5 pregões do trecho
/// e o da volta, mesmo com feriado.
const NEIGHBOURS: Duration = Duration::days(20);

impl Store {
    /// Os fechamentos que chegam e podem entrar. Cada um é conferido junto com
    /// os guardados em volta (o guardado vale na mesma data, como no `on
    /// conflict do nothing`): o que faz parte de um trecho ruim fica de fora,
    /// e o guardado que a chegada revelou ruim (a volta chegou agora) sai, com a
    /// série de quem tem o ativo marcada para refazer desde ele.
    pub(super) async fn screen_closes(
        &self,
        symbol: &str,
        closes: &Series,
    ) -> Result<Series, StoreError> {
        let (Some(first), Some(last)) = (
            closes.iter().map(|(d, _)| *d).min(),
            closes.iter().map(|(d, _)| *d).max(),
        ) else {
            return Ok(Vec::new());
        };
        let stored: Vec<(Date, Decimal)> = sqlx::query_as(
            "select date, close from price_history
              where symbol = $1 and date between $2 and $3 order by date",
        )
        .bind(symbol)
        .bind(first - NEIGHBOURS)
        .bind(last + NEIGHBOURS)
        .fetch_all(&self.pool)
        .await?;
        let mut merged: BTreeMap<Date, (Decimal, bool)> = stored
            .into_iter()
            .map(|(date, close)| (date, (close.normalize(), true)))
            .collect();
        for (date, close) in closes {
            merged.entry(*date).or_insert((*close, false));
        }
        let series: Series = merged.iter().map(|(d, (c, _))| (*d, *c)).collect();
        let splits = self.symbol_splits(symbol).await?;
        let bad: BTreeSet<Date> = bad_closes(&series, &splits).into_iter().collect();
        if bad.is_empty() {
            return Ok(closes.clone());
        }
        let removed: Vec<Date> = bad
            .iter()
            .copied()
            .filter(|d| merged.get(d).is_some_and(|(_, stored)| *stored))
            .collect();
        let dropped = bad.len() - removed.len();
        if dropped > 0 {
            tracing::warn!(symbol = %symbol, dropped, "fechamentos ruins da fonte descartados");
        }
        if !removed.is_empty() {
            self.remove_bad_closes(symbol, &removed).await?;
        }
        Ok(closes
            .iter()
            .filter(|(d, _)| !bad.contains(d))
            .copied()
            .collect())
    }

    /// Confere o cache inteiro: para cada símbolo guardado, apaga os trechos de
    /// fechamento ruim e marca a série de quem tem o ativo. Idempotente (a
    /// segunda rodada não acha nada). Devolve quantos fechamentos saíram.
    pub async fn clean_bad_closes(&self) -> Result<usize, StoreError> {
        let symbols: Vec<(String,)> =
            sqlx::query_as("select distinct symbol from price_history order by symbol")
                .fetch_all(&self.pool)
                .await?;
        let mut removed = 0;
        for (symbol,) in symbols {
            let series: Vec<(Date, Decimal)> = sqlx::query_as(
                "select date, close from price_history where symbol = $1 order by date",
            )
            .bind(&symbol)
            .fetch_all(&self.pool)
            .await?;
            let series: Series = series
                .into_iter()
                .map(|(d, c)| (d, c.normalize()))
                .collect();
            let splits = self.symbol_splits(&symbol).await?;
            let bad = bad_closes(&series, &splits);
            if !bad.is_empty() {
                removed += bad.len();
                self.remove_bad_closes(&symbol, &bad).await?;
            }
        }
        Ok(removed)
    }

    async fn symbol_splits(&self, symbol: &str) -> Result<Series, StoreError> {
        Ok(self
            .price_splits(&[symbol.to_owned()])
            .await?
            .remove(symbol)
            .unwrap_or_default())
    }

    /// Apaga fechamentos guardados (é cache da fonte) e marca a série de quem
    /// tem o ativo desde o primeiro deles.
    async fn remove_bad_closes(&self, symbol: &str, dates: &[Date]) -> Result<(), StoreError> {
        let Some(since) = dates.iter().min().copied() else {
            return Ok(());
        };
        sqlx::query("delete from price_history where symbol = $1 and date = any($2)")
            .bind(symbol)
            .bind(dates)
            .execute(&self.pool)
            .await?;
        tracing::warn!(
            symbol = %symbol,
            removed = dates.len(),
            since = %since,
            "fechamentos ruins guardados apagados"
        );
        let holders: Vec<Uuid> = self
            .held_symbols()
            .await?
            .into_iter()
            .filter(|held| held.market.quote_symbol(&held.symbol).as_deref() == Some(symbol))
            .map(|held| held.user)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !holders.is_empty() {
            self.mark_dirty(&holders, since).await?;
        }
        Ok(())
    }
}
