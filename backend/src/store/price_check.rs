//! O filtro de fechamento ruim na entrada de `price_history` e a limpeza do que
//! já estava gravado. A regra está em [`crate::domain::price_check`]; aqui, o
//! que ela precisa do banco: os fechamentos guardados em volta, os
//! desdobramentos e quem tem o ativo (a série dessas pessoas é refeita quando
//! um fechamento guardado sai).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use super::{Store, StoreError};
use crate::domain::price_check::{bad_closes, revise_from, settled};
use crate::history::Series;

/// Quanto do que já está guardado, antes e depois dos fechamentos que chegam,
/// entra na conferência: 20 dias corridos são mais que os 5 pregões do trecho
/// e o da volta, mesmo com feriado.
const NEIGHBOURS: Duration = Duration::days(20);

impl Store {
    /// O caminho de gravação do que a fonte acabou de dizer, para todo mundo
    /// (sincronia diária, reconstrução, comparador, Tesouro):
    ///
    /// - só entra o que já é fechamento ([`settled`]): a barra de um pregão
    ///   ainda aberto (o preço do meio do dia) não é gravada;
    /// - o fechamento ruim da fonte fica de fora ([`Store::screen_closes`]);
    /// - data nova entra; o fechamento já guardado dos últimos dias
    ///   ([`revise_from`]) que a fonte mudou é corrigido, e a série de quem tem
    ///   o ativo é marcada para refazer desde ele. Antes disso o guardado não
    ///   muda;
    /// - o volume financeiro (`traded`, migração 0037) vai junto, só para os
    ///   fechamentos que entraram ou já estavam guardados: preenche o que está
    ///   nulo e, nos últimos dias, corrige o que a fonte mudou. Volume não mexe
    ///   na série de ninguém.
    ///
    /// Devolve as datas novas, em ordem.
    pub async fn record_prices(
        &self,
        symbol: &str,
        closes: &Series,
        traded: &[(Date, Decimal)],
        now: OffsetDateTime,
    ) -> Result<Vec<Date>, StoreError> {
        let closed: Series = closes
            .iter()
            .filter(|(date, _)| settled(symbol, *date, now))
            .copied()
            .collect();
        let open = closes.len() - closed.len();
        if open > 0 {
            tracing::debug!(symbol = %symbol, open, "barra de pregao aberto nao gravada");
        }
        let screened = self.screen_closes(symbol, &closed).await?;
        let inserted = self.insert_closes(symbol, &screened).await?;
        let since = revise_from(now);
        let accepted: BTreeSet<Date> = screened.iter().map(|(d, _)| *d).collect();
        self.record_traded(symbol, traded, &accepted, since).await?;
        let recent: Series = screened
            .into_iter()
            .filter(|(date, _)| *date >= since && inserted.binary_search(date).is_err())
            .collect();
        if !recent.is_empty() {
            let dates: Vec<Date> = recent.iter().map(|(d, _)| *d).collect();
            let values: Vec<Decimal> = recent.iter().map(|(_, v)| *v).collect();
            let changed: Vec<(Date,)> = sqlx::query_as(
                "update price_history p set close = round(t.c, 8)
                   from unnest($2::date[], $3::numeric[]) as t (d, c)
                  where p.symbol = $1 and p.date = t.d and p.close <> round(t.c, 8)
                 returning p.date",
            )
            .bind(symbol)
            .bind(&dates)
            .bind(&values)
            .fetch_all(&self.pool)
            .await?;
            if let Some(first) = changed.iter().map(|(d,)| *d).min() {
                tracing::info!(
                    symbol = %symbol,
                    revised = changed.len(),
                    since = %first,
                    "fechamentos recentes corrigidos pela fonte"
                );
                self.mark_symbol_holders(symbol, first).await?;
            }
        }
        Ok(inserted)
    }

    /// Desde quando o volume financeiro de `symbol` já foi pedido à fonte, de
    /// ponta a ponta até o dia do pedido (`traded_value_coverage`, migração
    /// 0037). `None`: símbolo guardado antes do volume.
    pub async fn traded_covered_from(&self, symbol: &str) -> Result<Option<Date>, StoreError> {
        let row: Option<(Date,)> =
            sqlx::query_as("select covered_from from traded_value_coverage where symbol = $1")
                .bind(symbol)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(d,)| d))
    }

    /// O volume de `symbol` foi pedido de `from` até hoje (a menor data vence).
    pub async fn set_traded_coverage(&self, symbol: &str, from: Date) -> Result<(), StoreError> {
        sqlx::query(
            "insert into traded_value_coverage (symbol, covered_from) values ($1, $2)
             on conflict (symbol) do update
                set covered_from = least(traded_value_coverage.covered_from, excluded.covered_from)",
        )
        .bind(symbol)
        .bind(from)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O volume financeiro de cada pregão dos símbolos, de `from` a `to`, em
    /// ordem (só os pregões com volume).
    pub async fn traded_values(
        &self,
        symbols: &[String],
        from: Date,
        to: Date,
    ) -> Result<HashMap<String, Series>, StoreError> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, Date, Decimal)> = sqlx::query_as(
            "select symbol, date, traded_value from price_history
              where symbol = any($1) and date between $2 and $3 and traded_value is not null
              order by symbol, date",
        )
        .bind(symbols)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<String, Series> = HashMap::new();
        for (symbol, date, value) in rows {
            out.entry(symbol)
                .or_default()
                .push((date, value.normalize()));
        }
        Ok(out)
    }

    /// O volume financeiro dos pregões cujo fechamento passou (`accepted`): o
    /// nulo é preenchido; desde `since`, o que a fonte mudou é corrigido.
    async fn record_traded(
        &self,
        symbol: &str,
        traded: &[(Date, Decimal)],
        accepted: &BTreeSet<Date>,
        since: Date,
    ) -> Result<(), StoreError> {
        let (dates, values): (Vec<Date>, Vec<Decimal>) = traded
            .iter()
            .filter(|(date, value)| accepted.contains(date) && *value >= Decimal::ZERO)
            .copied()
            .unzip();
        if dates.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "update price_history p set traded_value = round(t.v, 2)
               from unnest($2::date[], $3::numeric[]) as t (d, v)
              where p.symbol = $1 and p.date = t.d
                and (p.traded_value is null
                     or (p.date >= $4 and p.traded_value <> round(t.v, 2)))",
        )
        .bind(symbol)
        .bind(&dates)
        .bind(&values)
        .bind(since)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

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
        self.mark_symbol_holders(symbol, since).await
    }

    /// Marca como suja, desde `since`, a série de quem tem um ativo cotado por
    /// `symbol` (o símbolo da fonte: `KNRI11.SA`, `AAPL`, `TD-…`).
    async fn mark_symbol_holders(&self, symbol: &str, since: Date) -> Result<(), StoreError> {
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
