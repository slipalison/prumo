//! O que o IR lê e grava no Postgres: o livro inteiro com o instrumento fiscal
//! de cada ativo, o saldo de prejuízo de antes do Prumo e os DARFs pagos.
//! Tudo filtra pelo dono.

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::asset_class::AssetClass;
use crate::domain::ledger::{Entry, Kind};
use crate::domain::tax_book::{LedgerLine, TaxInstrument};

/// O saldo que a pessoa traz de antes do primeiro ano apurado, como estava em
/// 1º de janeiro de `year`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredCarryforward {
    pub year: i32,
    pub common_loss: Decimal,
    pub day_trade_loss: Decimal,
    pub real_estate_fund_loss: Decimal,
    pub agribusiness_fund_loss: Decimal,
    pub pending_exchange_darf: Decimal,
    pub pending_capital_gain_darf: Decimal,
}

/// Um DARF marcado como pago.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DarfPayment {
    pub year: i32,
    pub month: u8,
    pub code: String,
    /// O valor que a tela mostrava quando a pessoa marcou.
    pub amount: Decimal,
    pub paid_at: OffsetDateTime,
}

#[derive(FromRow)]
struct LedgerRow {
    seq: i64,
    kind: String,
    trade_date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    fees: Decimal,
    fx_rate: Option<Decimal>,
    symbol: Option<String>,
    display_name: String,
    asset_class: String,
    tax_instrument: Option<String>,
}

#[derive(FromRow)]
struct CarryforwardRow {
    year: i32,
    common_loss: Decimal,
    day_trade_loss: Decimal,
    real_estate_fund_loss: Decimal,
    agribusiness_fund_loss: Decimal,
    pending_exchange_darf: Decimal,
    pending_capital_gain_darf: Decimal,
}

#[derive(FromRow)]
struct PaymentRow {
    year: i32,
    month: i16,
    code: String,
    amount: Decimal,
    paid_at: OffsetDateTime,
}

impl Store {
    /// O livro inteiro da pessoa (sem o teto da listagem), cada lançamento com
    /// o instrumento fiscal do ativo. Lançamento de ativo já removido usa o
    /// padrão da foto que ele guardou (classe, símbolo e nome); fora da B3 só
    /// as classes da B3.
    pub async fn tax_ledger(&self, user: Uuid) -> Result<Vec<LedgerLine>, StoreError> {
        let rows: Vec<LedgerRow> = sqlx::query_as(
            "select t.seq, t.kind, t.trade_date, t.quantity, t.unit_price, t.fees, t.fx_rate,
                    coalesce(a.symbol, t.symbol) as symbol,
                    coalesce(a.display_name, t.display_name) as display_name,
                    coalesce(a.asset_class, t.asset_class) as asset_class,
                    a.tax_instrument
               from transactions t
               left join assets a on a.id = t.asset_id and a.user_id = t.user_id
              where t.user_id = $1
              order by t.trade_date, t.seq",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        // O ticker que ainda está na carteira dá o instrumento também aos
        // lançamentos antigos dele (de um cadastro removido e refeito).
        let mut live: HashMap<String, TaxInstrument> = HashMap::new();
        for row in &rows {
            if let (Some(text), Some(symbol)) = (&row.tax_instrument, &row.symbol) {
                let instrument = TaxInstrument::parse(text).ok_or_else(|| decode_error(text))?;
                live.entry(symbol.clone()).or_insert(instrument);
            }
        }
        let mut lines = Vec::with_capacity(rows.len());
        for row in rows {
            let class = AssetClass::parse(&row.asset_class)
                .ok_or_else(|| decode_error(&row.asset_class))?;
            let key = row
                .symbol
                .clone()
                .unwrap_or_else(|| row.display_name.clone());
            let instrument = match &row.tax_instrument {
                Some(text) => TaxInstrument::parse(text).ok_or_else(|| decode_error(text))?,
                None => match live.get(&key) {
                    Some(instrument) => *instrument,
                    None => TaxInstrument::default_for(
                        class,
                        !class.accepts_b3_listing(),
                        row.symbol.as_deref(),
                        &row.display_name,
                    ),
                },
            };
            lines.push(LedgerLine {
                key,
                instrument,
                entry: Entry {
                    kind: Kind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?,
                    trade_date: row.trade_date,
                    seq: row.seq,
                    quantity: row.quantity.normalize(),
                    unit_price: row.unit_price.map(|p| p.normalize()),
                    fees: row.fees,
                    fx_rate: row.fx_rate.unwrap_or(Decimal::ONE),
                },
            });
        }
        Ok(lines)
    }

    pub async fn tax_carryforward(
        &self,
        user: Uuid,
    ) -> Result<Option<StoredCarryforward>, StoreError> {
        let row: Option<CarryforwardRow> = sqlx::query_as(
            "select year, common_loss, day_trade_loss, real_estate_fund_loss, agribusiness_fund_loss,
                    pending_exchange_darf, pending_capital_gain_darf
               from tax_carryforward where user_id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| StoredCarryforward {
            year: r.year,
            common_loss: r.common_loss,
            day_trade_loss: r.day_trade_loss,
            real_estate_fund_loss: r.real_estate_fund_loss,
            agribusiness_fund_loss: r.agribusiness_fund_loss,
            pending_exchange_darf: r.pending_exchange_darf,
            pending_capital_gain_darf: r.pending_capital_gain_darf,
        }))
    }

    pub async fn set_tax_carryforward(
        &self,
        user: Uuid,
        value: &StoredCarryforward,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into tax_carryforward
                (user_id, year, common_loss, day_trade_loss, real_estate_fund_loss, agribusiness_fund_loss,
                 pending_exchange_darf, pending_capital_gain_darf, updated_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, now())
             on conflict (user_id) do update set
                year = excluded.year,
                common_loss = excluded.common_loss,
                day_trade_loss = excluded.day_trade_loss,
                real_estate_fund_loss = excluded.real_estate_fund_loss,
                agribusiness_fund_loss = excluded.agribusiness_fund_loss,
                pending_exchange_darf = excluded.pending_exchange_darf,
                pending_capital_gain_darf = excluded.pending_capital_gain_darf,
                updated_at = now()",
        )
        .bind(user)
        .bind(value.year)
        .bind(value.common_loss)
        .bind(value.day_trade_loss)
        .bind(value.real_estate_fund_loss)
        .bind(value.agribusiness_fund_loss)
        .bind(value.pending_exchange_darf)
        .bind(value.pending_capital_gain_darf)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Os DARFs que a pessoa marcou como pagos num ano.
    pub async fn darf_payments(
        &self,
        user: Uuid,
        year: i32,
    ) -> Result<Vec<DarfPayment>, StoreError> {
        let rows: Vec<PaymentRow> = sqlx::query_as(
            "select year, month, code, amount, paid_at from tax_darf_payments
              where user_id = $1 and year = $2 order by month, code",
        )
        .bind(user)
        .bind(year)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DarfPayment {
                year: r.year,
                month: u8::try_from(r.month).unwrap_or_default(),
                code: r.code,
                amount: r.amount,
                paid_at: r.paid_at,
            })
            .collect())
    }

    /// Marca (ou marca de novo, com o valor de agora) um DARF como pago.
    pub async fn mark_darf_paid(
        &self,
        user: Uuid,
        year: i32,
        month: u8,
        code: &str,
        amount: Decimal,
    ) -> Result<DarfPayment, StoreError> {
        let row: PaymentRow = sqlx::query_as(
            "insert into tax_darf_payments (user_id, year, month, code, amount)
             values ($1, $2, $3, $4, $5)
             on conflict (user_id, year, month, code) do update set
                amount = excluded.amount, paid_at = now()
             returning year, month, code, amount, paid_at",
        )
        .bind(user)
        .bind(year)
        .bind(i16::from(month))
        .bind(code)
        .bind(amount)
        .fetch_one(&self.pool)
        .await?;
        Ok(DarfPayment {
            year: row.year,
            month,
            code: row.code,
            amount: row.amount,
            paid_at: row.paid_at,
        })
    }

    /// Tira a marca de pago. `false` se não havia.
    pub async fn unmark_darf_paid(
        &self,
        user: Uuid,
        year: i32,
        month: u8,
        code: &str,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "delete from tax_darf_payments where user_id = $1 and year = $2 and month = $3 and code = $4",
        )
        .bind(user)
        .bind(year)
        .bind(i16::from(month))
        .bind(code)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }
}
