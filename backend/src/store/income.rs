//! Proventos, a meta de renda mensal das rotas de `/income/goal` (o tipo
//! `Objective` mora em `store/objectives.rs`) e o cache do histórico de
//! dividendos por cota.
//! Tudo o que é da pessoa filtra pelo dono; o cache de dividendos é
//! compartilhado, como `quotes`.

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::objectives::{objective_columns, Objective, ObjectiveRow};
use super::{decode_error, is_unique_violation, Store, StoreError};
use crate::dividends::{DividendEvent, DividendHistory};
use crate::domain::income::{status, Entry, IncomeAmounts, IncomeKind, IncomeSource, IncomeStatus};

/// Um provento gravado.
#[derive(Debug, Clone, PartialEq)]
pub struct IncomeEvent {
    pub id: Uuid,
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub display_name: String,
    pub kind: IncomeKind,
    pub ex_date: Option<Date>,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub gross: Decimal,
    pub withholding: Decimal,
    pub net: Decimal,
    pub source: IncomeSource,
    pub external_ref: Option<String>,
}

impl IncomeEvent {
    pub fn status(&self, today: Date) -> IncomeStatus {
        status(self.pay_date, today)
    }

    /// O provento como o resumo de renda o lê.
    pub fn entry(&self) -> Entry {
        Entry {
            pay_date: self.pay_date,
            net: self.net,
            payer_key: self.payer_key(),
            payer_label: self.payer_label(),
        }
    }

    /// A chave do pagador: o ativo, ou o símbolo/nome quando o ativo já foi
    /// apagado da carteira.
    pub fn payer_key(&self) -> String {
        match (self.asset_id, &self.symbol) {
            (Some(id), _) => id.to_string(),
            (None, Some(symbol)) => symbol.clone(),
            (None, None) => self.display_name.clone(),
        }
    }

    /// O que a tela chama o pagador: o símbolo, ou o nome do que não tem.
    pub fn payer_label(&self) -> String {
        self.symbol
            .clone()
            .unwrap_or_else(|| self.display_name.clone())
    }
}

/// O que se grava num provento novo (valores já calculados pelo domínio).
#[derive(Debug, Clone, PartialEq)]
pub struct NewIncomeEvent {
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub display_name: String,
    pub kind: IncomeKind,
    pub ex_date: Option<Date>,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub amounts: IncomeAmounts,
    pub source: IncomeSource,
    pub external_ref: Option<String>,
}

/// O que se pode mudar num provento lançado à mão.
#[derive(Debug, Clone, PartialEq)]
pub struct IncomeFields {
    pub kind: IncomeKind,
    pub ex_date: Option<Date>,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub amounts: IncomeAmounts,
}

/// Filtros de `GET /income`. `today` fecha o que "recebido" quer dizer.
#[derive(Debug, Clone, Copy)]
pub struct IncomeFilter {
    pub from: Option<Date>,
    pub to: Option<Date>,
    pub asset: Option<Uuid>,
    pub status: Option<IncomeStatus>,
    pub today: Date,
    pub limit: i64,
}

#[derive(FromRow)]
struct IncomeRow {
    id: Uuid,
    asset_id: Option<Uuid>,
    symbol: Option<String>,
    display_name: String,
    kind: String,
    ex_date: Option<Date>,
    pay_date: Date,
    amount_per_unit: Decimal,
    quantity: Decimal,
    gross: Decimal,
    withholding: Decimal,
    net: Decimal,
    source: String,
    external_ref: Option<String>,
}

macro_rules! income_columns {
    () => {
        "id, asset_id, symbol, display_name, kind, ex_date, pay_date, amount_per_unit, quantity, gross, withholding, net, source, external_ref"
    };
}

impl TryFrom<IncomeRow> for IncomeEvent {
    type Error = sqlx::Error;

    fn try_from(row: IncomeRow) -> Result<Self, Self::Error> {
        Ok(IncomeEvent {
            id: row.id,
            asset_id: row.asset_id,
            symbol: row.symbol,
            display_name: row.display_name,
            kind: IncomeKind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?,
            ex_date: row.ex_date,
            pay_date: row.pay_date,
            amount_per_unit: row.amount_per_unit.normalize(),
            quantity: row.quantity.normalize(),
            gross: row.gross,
            withholding: row.withholding,
            net: row.net,
            source: IncomeSource::parse(&row.source).ok_or_else(|| decode_error(&row.source))?,
            external_ref: row.external_ref,
        })
    }
}

/// O histórico de dividendos de um símbolo, como está no cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedDividends {
    pub currency: String,
    pub fetched_at: OffsetDateTime,
    pub events: Vec<DividendEvent>,
}

impl Store {
    /// Os proventos da pessoa, do pagamento mais novo para o mais velho.
    pub async fn income_events(
        &self,
        user: Uuid,
        filter: &IncomeFilter,
    ) -> Result<Vec<IncomeEvent>, StoreError> {
        let rows: Vec<IncomeRow> = sqlx::query_as(concat!(
            "select ",
            income_columns!(),
            " from income_events
              where user_id = $1
                and ($2::date is null or pay_date >= $2)
                and ($3::date is null or pay_date <= $3)
                and ($4::uuid is null or asset_id = $4)
                and ($5::text is null
                     or ($5 = 'received' and pay_date <= $6)
                     or ($5 = 'upcoming' and pay_date > $6))
              order by pay_date desc, created_at desc, id
              limit $7"
        ))
        .bind(user)
        .bind(filter.from)
        .bind(filter.to)
        .bind(filter.asset)
        .bind(filter.status.map(IncomeStatus::as_str))
        .bind(filter.today)
        .bind(filter.limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(IncomeEvent::try_from)
            .collect::<Result<_, _>>()?)
    }

    /// Todo o livro de proventos, do pagamento mais velho para o mais novo —
    /// o que a tela Renda soma.
    pub async fn income_ledger(&self, user: Uuid) -> Result<Vec<IncomeEvent>, StoreError> {
        let rows: Vec<IncomeRow> = sqlx::query_as(concat!(
            "select ",
            income_columns!(),
            " from income_events where user_id = $1 order by pay_date, created_at, id"
        ))
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(IncomeEvent::try_from)
            .collect::<Result<_, _>>()?)
    }

    pub async fn income_event(
        &self,
        user: Uuid,
        id: Uuid,
    ) -> Result<Option<IncomeEvent>, StoreError> {
        let row: Option<IncomeRow> = sqlx::query_as(concat!(
            "select ",
            income_columns!(),
            " from income_events where user_id = $1 and id = $2"
        ))
        .bind(user)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(IncomeEvent::try_from).transpose()?)
    }

    /// Grava um provento. `Duplicate` quando o `external_ref` já existe para a
    /// pessoa (a importação repetida não duplica).
    pub async fn insert_income_event(
        &self,
        user: Uuid,
        new: &NewIncomeEvent,
    ) -> Result<IncomeEvent, StoreError> {
        let result: Result<IncomeRow, sqlx::Error> = sqlx::query_as(concat!(
            "insert into income_events
                (id, user_id, asset_id, symbol, display_name, kind, ex_date, pay_date,
                 amount_per_unit, quantity, gross, withholding, net, source, external_ref)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             returning ",
            income_columns!()
        ))
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(new.asset_id)
        .bind(&new.symbol)
        .bind(&new.display_name)
        .bind(new.kind.as_str())
        .bind(new.ex_date)
        .bind(new.pay_date)
        .bind(new.amount_per_unit)
        .bind(new.quantity)
        .bind(new.amounts.gross)
        .bind(new.amounts.withholding)
        .bind(new.amounts.net)
        .bind(new.source.as_str())
        .bind(&new.external_ref)
        .fetch_one(&self.pool)
        .await;
        match result {
            Ok(row) => Ok(IncomeEvent::try_from(row)?),
            Err(error) if is_unique_violation(&error) => Err(StoreError::Duplicate),
            Err(error) => Err(error.into()),
        }
    }

    /// Troca os campos de um provento lançado à mão. Só `manual` muda: o que
    /// veio de extrato é o que o extrato diz.
    pub async fn update_manual_income_event(
        &self,
        user: Uuid,
        id: Uuid,
        fields: &IncomeFields,
    ) -> Result<Option<IncomeEvent>, StoreError> {
        let row: Option<IncomeRow> = sqlx::query_as(concat!(
            "update income_events set
                kind = $3, ex_date = $4, pay_date = $5, amount_per_unit = $6,
                quantity = $7, gross = $8, withholding = $9, net = $10
              where user_id = $1 and id = $2 and source = 'manual'
              returning ",
            income_columns!()
        ))
        .bind(user)
        .bind(id)
        .bind(fields.kind.as_str())
        .bind(fields.ex_date)
        .bind(fields.pay_date)
        .bind(fields.amount_per_unit)
        .bind(fields.quantity)
        .bind(fields.amounts.gross)
        .bind(fields.amounts.withholding)
        .bind(fields.amounts.net)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(IncomeEvent::try_from).transpose()?)
    }

    /// Apaga um provento. O automático apagado (`b3_announcement`) fica
    /// lembrado na mesma instrução, para a próxima rodada não o recriar.
    pub async fn delete_income_event(&self, user: Uuid, id: Uuid) -> Result<bool, StoreError> {
        let (deleted,): (i64,) = sqlx::query_as(
            "with gone as (
                 delete from income_events where user_id = $1 and id = $2
                 returning source, external_ref
             ), remembered as (
                 insert into income_auto_dismissed (user_id, external_ref)
                 select $1, external_ref from gone
                  where source = 'b3_announcement' and external_ref is not null
                 on conflict do nothing
             )
             select count(*) from gone",
        )
        .bind(user)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(deleted == 1)
    }

    /// A meta de renda mensal da pessoa, se houver.
    pub async fn income_goal(&self, user: Uuid) -> Result<Option<Objective>, StoreError> {
        let row: Option<ObjectiveRow> = sqlx::query_as(concat!(
            "select ",
            objective_columns!(),
            " from objectives where user_id = $1 and kind = 'monthly_income'
              order by created_at desc, id limit 1"
        ))
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Objective::try_from).transpose()?)
    }

    /// Cria a meta de renda mensal ou troca o valor da que existe (e zera a
    /// data de conquista: meta nova, conquista nova). Uma meta por pessoa; o
    /// lock da transação impede dois `PUT` simultâneos de criarem duas.
    pub async fn set_income_goal(
        &self,
        user: Uuid,
        target: Decimal,
    ) -> Result<Objective, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(format!("income_goal:{user}"))
            .execute(&mut *tx)
            .await?;
        let updated: Option<ObjectiveRow> = sqlx::query_as(concat!(
            "update objectives set target = $2, achieved_at = null
              where user_id = $1 and kind = 'monthly_income'
              returning ",
            objective_columns!()
        ))
        .bind(user)
        .bind(target)
        .fetch_optional(&mut *tx)
        .await?;
        let row = match updated {
            Some(row) => row,
            None => {
                sqlx::query_as(concat!(
                    "insert into objectives (id, user_id, kind, target)
                     values ($1, $2, 'monthly_income', $3) returning ",
                    objective_columns!()
                ))
                .bind(Uuid::new_v4())
                .bind(user)
                .bind(target)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;
        Ok(Objective::try_from(row)?)
    }

    pub async fn delete_income_goal(&self, user: Uuid) -> Result<bool, StoreError> {
        let done =
            sqlx::query("delete from objectives where user_id = $1 and kind = 'monthly_income'")
                .bind(user)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Registra a data em que o objetivo foi atingido, uma vez só.
    pub async fn mark_objective_achieved(
        &self,
        user: Uuid,
        id: Uuid,
        at: OffsetDateTime,
    ) -> Result<Option<OffsetDateTime>, StoreError> {
        let row: Option<(OffsetDateTime,)> = sqlx::query_as(
            "update objectives set achieved_at = coalesce(achieved_at, $3)
              where user_id = $1 and id = $2 returning achieved_at",
        )
        .bind(user)
        .bind(id)
        .bind(at)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(at,)| at))
    }

    /// O cache de dividendos dos símbolos pedidos (os que nunca foram
    /// buscados não aparecem).
    pub async fn dividend_cache(
        &self,
        symbols: &[String],
    ) -> Result<HashMap<String, CachedDividends>, StoreError> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }
        let fetches: Vec<(String, String, OffsetDateTime)> = sqlx::query_as(
            "select symbol, currency, fetched_at from dividend_fetches where symbol = any($1)",
        )
        .bind(symbols)
        .fetch_all(&self.pool)
        .await?;
        let points: Vec<(String, Date, Decimal)> = sqlx::query_as(
            "select symbol, ex_date, amount from dividend_history
              where symbol = any($1) order by ex_date",
        )
        .bind(symbols)
        .fetch_all(&self.pool)
        .await?;
        let mut cache: HashMap<String, CachedDividends> = fetches
            .into_iter()
            .map(|(symbol, currency, fetched_at)| {
                (
                    symbol,
                    CachedDividends {
                        currency,
                        fetched_at,
                        events: Vec::new(),
                    },
                )
            })
            .collect();
        for (symbol, ex_date, amount) in points {
            if let Some(entry) = cache.get_mut(&symbol) {
                entry.events.push(DividendEvent {
                    ex_date,
                    amount: amount.normalize(),
                });
            }
        }
        Ok(cache)
    }

    /// Troca o histórico de um símbolo pelo que o provedor acabou de dizer.
    /// Idempotente: duas réplicas gravando o mesmo símbolo ao mesmo tempo
    /// terminam no mesmo estado (o lock da transação as põe em fila, e a
    /// gravação é um upsert).
    pub async fn save_dividends(
        &self,
        symbol: &str,
        history: &DividendHistory,
        fetched_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(format!("dividends:{symbol}"))
            .execute(&mut *tx)
            .await?;
        let dates: Vec<Date> = history.events.iter().map(|e| e.ex_date).collect();
        let amounts: Vec<Decimal> = history.events.iter().map(|e| e.amount).collect();
        // O que o provedor não traz mais (dividendo corrigido ou fora da janela) sai.
        sqlx::query(
            "delete from dividend_history where symbol = $1 and ex_date <> all($2::date[])",
        )
        .bind(symbol)
        .bind(&dates)
        .execute(&mut *tx)
        .await?;
        if !history.events.is_empty() {
            sqlx::query(
                "insert into dividend_history (symbol, ex_date, amount)
                 select $1, d, a from unnest($2::date[], $3::numeric[]) as t (d, a)
                 on conflict (symbol, ex_date) do update set amount = excluded.amount",
            )
            .bind(symbol)
            .bind(&dates)
            .bind(&amounts)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "insert into dividend_fetches (symbol, currency, fetched_at) values ($1, $2, $3)
             on conflict (symbol) do update
                set currency = excluded.currency, fetched_at = excluded.fetched_at",
        )
        .bind(symbol)
        .bind(&history.currency)
        .bind(fetched_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
