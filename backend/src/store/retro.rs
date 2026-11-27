//! O que a série retroativa lê e marca: o livro de cada pessoa, os proventos
//! recebidos, os preços guardados, as marcas de "sujo desde" e até onde o
//! cache de preços já foi pedido para trás.

use std::collections::HashMap;

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::ledger::{Entry, Kind};
use crate::history::Series;

/// A série de uma pessoa precisa ser refeita desde `since`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtySeries {
    pub user: Uuid,
    pub since: Date,
    /// Quando a marca foi feita: só ela é apagada depois de refazer.
    pub marked_at: OffsetDateTime,
}

/// Um ativo com livro, como a sincronia de preços o vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldSymbol {
    pub user: Uuid,
    pub market: Market,
    pub symbol: String,
    pub manual_price: bool,
    /// O lançamento mais antigo do ativo: o preço é preciso desde aqui.
    pub first_trade: Date,
}

impl Store {
    /// As séries sujas com a marca feita até `marked_until`. A espera deixa a
    /// escrita em lote (a importação da B3) terminar antes de refazer.
    pub async fn dirty_series(
        &self,
        marked_until: OffsetDateTime,
    ) -> Result<Vec<DirtySeries>, StoreError> {
        let rows: Vec<(Uuid, Date, OffsetDateTime)> = sqlx::query_as(
            "select user_id, dirty_since, marked_at from performance_rebuilds
              where marked_at <= $1 order by marked_at",
        )
        .bind(marked_until)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(user, since, marked_at)| DirtySeries {
                user,
                since,
                marked_at,
            })
            .collect())
    }

    /// A marca da pessoa, se houver.
    pub async fn dirty_since(&self, user: Uuid) -> Result<Option<DirtySeries>, StoreError> {
        let row: Option<(Date, OffsetDateTime)> = sqlx::query_as(
            "select dirty_since, marked_at from performance_rebuilds where user_id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(since, marked_at)| DirtySeries {
            user,
            since,
            marked_at,
        }))
    }

    /// Apaga a marca, se ela ainda é a que foi lida. Uma marca nova no meio da
    /// reconstrução fica para a próxima rodada.
    pub async fn clear_dirty(&self, dirty: &DirtySeries) -> Result<bool, StoreError> {
        let done =
            sqlx::query("delete from performance_rebuilds where user_id = $1 and marked_at = $2")
                .bind(dirty.user)
                .bind(dirty.marked_at)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Marca as séries como sujas desde `since` (preço antigo que chegou
    /// depois, por exemplo).
    pub async fn mark_dirty(&self, users: &[Uuid], since: Date) -> Result<(), StoreError> {
        for user in users {
            sqlx::query("select performance_mark_dirty($1, $2)")
                .bind(user)
                .bind(since)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// O livro de cada ativo da pessoa (os lançamentos de ativo removido não
    /// entram: a série é a dos ativos que existem).
    pub async fn ledger_entries(
        &self,
        user: Uuid,
    ) -> Result<HashMap<Uuid, Vec<Entry>>, StoreError> {
        type Row = (
            Uuid,
            i64,
            String,
            Date,
            Decimal,
            Option<Decimal>,
            Decimal,
            Option<Decimal>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "select t.asset_id, t.seq, t.kind, t.trade_date, t.quantity, t.unit_price, t.fees, t.fx_rate
               from transactions t
               join assets a on a.id = t.asset_id and a.user_id = t.user_id
              where t.user_id = $1
              order by t.trade_date, t.seq",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<Uuid, Vec<Entry>> = HashMap::new();
        for (asset, seq, kind, trade_date, quantity, unit_price, fees, fx_rate) in rows {
            out.entry(asset).or_default().push(Entry {
                kind: Kind::parse(&kind).ok_or_else(|| decode_error(&kind))?,
                trade_date,
                seq,
                quantity: quantity.normalize(),
                unit_price: unit_price.map(|p| p.normalize()),
                fees,
                fx_rate: fx_rate.unwrap_or(Decimal::ONE),
            });
        }
        Ok(out)
    }

    /// O lançamento mais antigo dos ativos da pessoa: o primeiro dia da série.
    pub async fn first_trade_date(&self, user: Uuid) -> Result<Option<Date>, StoreError> {
        let (date,): (Option<Date>,) = sqlx::query_as(
            "select min(t.trade_date) from transactions t
               join assets a on a.id = t.asset_id and a.user_id = t.user_id
              where t.user_id = $1",
        )
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        Ok(date)
    }

    /// Proventos líquidos recebidos por dia de pagamento, de `from` a `to`, a
    /// soma de [`Store::received_income_by_asset`] em cada dia.
    pub async fn received_income(
        &self,
        user: Uuid,
        from: Date,
        to: Date,
    ) -> Result<Series, StoreError> {
        let mut days: std::collections::BTreeMap<Date, Decimal> = std::collections::BTreeMap::new();
        for series in self
            .received_income_by_asset(user, from, to)
            .await?
            .into_values()
        {
            for (date, value) in series {
                *days.entry(date).or_default() += value;
            }
        }
        Ok(days.into_iter().collect())
    }

    /// Proventos líquidos recebidos de cada ativo por dia de pagamento, de
    /// `from` a `to`. A estimativa não é dinheiro recebido e fica de fora; a
    /// amortização que já está no livro (mesmo ativo, mesma data) também, para
    /// não contar duas vezes o mesmo dinheiro. Ativo removido não entra.
    pub async fn received_income_by_asset(
        &self,
        user: Uuid,
        from: Date,
        to: Date,
    ) -> Result<HashMap<Uuid, Series>, StoreError> {
        let rows: Vec<(Uuid, Date, Decimal)> = sqlx::query_as(
            "select e.asset_id, e.pay_date, sum(e.net)
               from income_events e
               join assets a on a.id = e.asset_id and a.user_id = e.user_id
              where e.user_id = $1
                and e.source <> 'estimate'
                and e.pay_date between $2 and $3
                and not (e.kind = 'amortization' and exists (
                    select 1 from transactions t
                     where t.asset_id = e.asset_id and t.kind = 'amortization'
                       and t.trade_date = e.pay_date))
              group by e.asset_id, e.pay_date
              order by e.asset_id, e.pay_date",
        )
        .bind(user)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<Uuid, Series> = HashMap::new();
        for (asset, date, value) in rows {
            out.entry(asset).or_default().push((date, value));
        }
        Ok(out)
    }

    /// Os fechamentos guardados de cada símbolo de `from` a `to`, mais o
    /// último antes de `from` (o preço que vale no começo da janela).
    pub async fn price_series(
        &self,
        symbols: &[String],
        from: Date,
        to: Date,
    ) -> Result<HashMap<String, Series>, StoreError> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(String, Date, Decimal)> = sqlx::query_as(
            "select symbol, date, close from price_history
              where symbol = any($1) and date between $2 and $3
             union all
             (select distinct on (symbol) symbol, date, close from price_history
               where symbol = any($1) and date < $2
               order by symbol, date desc)",
        )
        .bind(symbols)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<String, Series> = HashMap::new();
        for (symbol, date, close) in rows {
            out.entry(symbol)
                .or_default()
                .push((date, close.normalize()));
        }
        for series in out.values_mut() {
            series.sort_by_key(|(d, _)| *d);
        }
        Ok(out)
    }

    /// O preço que cada foto guardou de cada ativo cotado, de `from` a `to`,
    /// mais a foto de antes de `from`. É a segunda fonte de preço: vale onde o
    /// `price_history` não tem a data (o Tesouro antes do histórico, o ativo de
    /// preço manual, a cotação do momento de uma foto antiga).
    pub async fn snapshot_prices(
        &self,
        user: Uuid,
        from: Date,
        to: Date,
    ) -> Result<HashMap<Uuid, Series>, StoreError> {
        let rows: Vec<(Uuid, Date, Decimal)> = sqlx::query_as(
            "select (p ->> 'asset_id')::uuid, s.date, (p ->> 'unit_price')::numeric
               from portfolio_snapshots s
               cross join lateral jsonb_array_elements(s.positions) as p
              where s.user_id = $1
                and s.date <= $3
                and s.date >= coalesce(
                    (select max(date) from portfolio_snapshots where user_id = $1 and date < $2), $2)
                and p ->> 'unit_price' is not null
                and not coalesce((p ->> 'by_value')::boolean, false)
              order by s.date",
        )
        .bind(user)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        let mut out: HashMap<Uuid, Series> = HashMap::new();
        for (asset, date, price) in rows {
            out.entry(asset)
                .or_default()
                .push((date, price.normalize()));
        }
        Ok(out)
    }

    /// A data da última foto da pessoa.
    pub async fn last_snapshot_date(&self, user: Uuid) -> Result<Option<Date>, StoreError> {
        let (date,): (Option<Date>,) =
            sqlx::query_as("select max(date) from portfolio_snapshots where user_id = $1")
                .bind(user)
                .fetch_one(&self.pool)
                .await?;
        Ok(date)
    }

    /// Cada ativo com símbolo e livro, de qualquer carteira, com a data do
    /// lançamento mais antigo: de onde o histórico de preço precisa começar.
    pub async fn held_symbols(&self) -> Result<Vec<HeldSymbol>, StoreError> {
        let rows: Vec<(Uuid, String, bool, bool, String, bool, Date)> = sqlx::query_as(
            "select a.user_id, a.asset_class, a.listed_on_b3, a.treasury, a.symbol,
                    a.manual_price is not null, min(t.trade_date)
               from assets a
               join transactions t on t.asset_id = a.id
              where a.symbol is not null
              group by a.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(
                |(user, class, listed_on_b3, treasury, symbol, manual_price, first_trade)| {
                    let market = AssetClass::parse(&class)?.market_of(listed_on_b3, treasury);
                    Some(HeldSymbol {
                        user,
                        market,
                        symbol,
                        manual_price,
                        first_trade,
                    })
                },
            )
            .collect())
    }

    /// A moeda de cada símbolo, pela cotação guardada.
    pub async fn quote_currency_of(
        &self,
        symbols: &[String],
    ) -> Result<HashMap<String, String>, StoreError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("select symbol, currency from quotes where symbol = any($1)")
                .bind(symbols)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().collect())
    }

    /// Desde quando o cache do símbolo está completo: a marca de até onde já
    /// se pediu à fonte ou, sem ela, o primeiro dia guardado.
    pub async fn price_covered_from(&self, symbol: &str) -> Result<Option<Date>, StoreError> {
        let (date,): (Option<Date>,) = sqlx::query_as(
            "select coalesce(
                 (select covered_from from price_coverage where symbol = $1),
                 (select min(date) from price_history where symbol = $1))",
        )
        .bind(symbol)
        .fetch_one(&self.pool)
        .await?;
        Ok(date)
    }

    /// A fonte já foi consultada desde `from` (tenha ou não dado tão antigo).
    pub async fn set_price_coverage(&self, symbol: &str, from: Date) -> Result<(), StoreError> {
        sqlx::query(
            "insert into price_coverage (symbol, covered_from) values ($1, $2)
             on conflict (symbol) do update
                set covered_from = least(price_coverage.covered_from, excluded.covered_from)",
        )
        .bind(symbol)
        .bind(from)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
