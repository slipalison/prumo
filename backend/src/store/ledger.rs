//! O livro de lançamentos no Postgres.
//!
//! Toda escrita segue o mesmo caminho, numa transação só: trava o ativo
//! (`for update`), grava/edita/apaga o lançamento, refaz a posição a partir do
//! livro inteiro daquele ativo (`domain::ledger`) e grava o resultado no cache
//! de `assets` (`quantity`, `cost_basis`, `realized_gain`). Se o livro recusar
//! — uma venda que passa da posição em alguma data, inclusive por causa de um
//! lançamento antigo editado ou apagado — a transação volta e nada muda.
//!
//! A leitura da carteira (e o stream ao vivo, que a recalcula a cada preço)
//! só lê o cache: o livro não é refeito na leitura.

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::{FromRow, Postgres, Transaction as DbTransaction};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{decode_error, Asset, AssetRow, NewAsset, Store, StoreError};
use crate::domain::asset_class::AssetClass;
use crate::domain::ledger::{self, Entry, Kind, Position};

/// De onde veio o lançamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// Digitado pela pessoa (folha do ativo, cadastro, edição da quantidade).
    Manual,
    /// O "Já comprei" do plano.
    Plan,
    /// Importação do extrato da B3.
    B3Import,
    /// Criado pela migração 0004 a partir dos dados de antes do livro.
    Migration,
}

impl Source {
    pub const ALL: [Source; 4] = [
        Source::Manual,
        Source::Plan,
        Source::B3Import,
        Source::Migration,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Source::Manual => "manual",
            Source::Plan => "plan",
            Source::B3Import => "b3_import",
            Source::Migration => "migration",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Source::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// Um lançamento gravado.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub id: Uuid,
    /// Ordem de entrada no livro.
    pub seq: i64,
    /// `None` depois que o ativo foi removido (o lançamento fica de histórico).
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub asset_class: AssetClass,
    pub display_name: String,
    pub fields: TransactionFields,
    pub source: Source,
    pub external_ref: Option<String>,
    pub contribution_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
}

impl Transaction {
    /// O lançamento como o domínio o vê.
    pub fn entry(&self) -> Entry {
        self.fields.entry(self.seq)
    }
}

/// O que se escreve num lançamento (e o que o `PATCH` troca).
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionFields {
    pub kind: Kind,
    pub trade_date: Date,
    pub quantity: Decimal,
    pub unit_price: Option<Decimal>,
    pub fees: Decimal,
    /// Moeda do preço e das taxas (`BRL`, `USD`…).
    pub currency: String,
    /// Reais por unidade de `currency`; `None` em reais.
    pub fx_rate: Option<Decimal>,
}

impl TransactionFields {
    /// Lançamento em reais, sem taxa.
    pub fn brl(
        kind: Kind,
        trade_date: Date,
        quantity: Decimal,
        unit_price: Option<Decimal>,
    ) -> Self {
        Self {
            kind,
            trade_date,
            quantity,
            unit_price,
            fees: Decimal::ZERO,
            currency: "BRL".into(),
            fx_rate: None,
        }
    }

    pub fn entry(&self, seq: i64) -> Entry {
        Entry {
            kind: self.kind,
            trade_date: self.trade_date,
            seq,
            quantity: self.quantity,
            unit_price: self.unit_price,
            fees: self.fees,
            fx_rate: self.fx_rate.unwrap_or(Decimal::ONE),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewTransaction {
    pub asset_id: Uuid,
    pub fields: TransactionFields,
    pub source: Source,
    pub external_ref: Option<String>,
    pub contribution_id: Option<Uuid>,
}

impl NewTransaction {
    pub fn manual(asset_id: Uuid, fields: TransactionFields) -> Self {
        Self {
            asset_id,
            fields,
            source: Source::Manual,
            external_ref: None,
            contribution_id: None,
        }
    }
}

/// Filtro da listagem; campos ausentes não filtram. As datas incluem as pontas.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransactionFilter {
    pub asset_id: Option<Uuid>,
    pub from: Option<Date>,
    pub to: Option<Date>,
}

/// O custo do saldo inicial de um ativo novo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpeningCost {
    /// A pessoa não disse: custo desconhecido.
    #[default]
    Unknown,
    /// Preço médio em reais, por unidade.
    AveragePrice(Decimal),
    /// Renda fixa por valor: quanto foi aplicado. O saldo inicial é o
    /// aplicado (preço 1) e a diferença até o valor de hoje é um ajuste —
    /// o rendimento, que não é custo.
    Invested(Decimal),
}

/// Teto da listagem: o livro de anos de B3 cabe; mais que isso, filtre por período.
pub const LIST_LIMIT: i64 = 5000;

macro_rules! transaction_columns {
    () => {
        "id, seq, asset_id, symbol, asset_class, display_name, kind, trade_date, quantity, unit_price, \
         fees, currency, fx_rate, source, external_ref, contribution_id, created_at"
    };
}

#[derive(FromRow)]
struct TransactionRow {
    id: Uuid,
    seq: i64,
    asset_id: Option<Uuid>,
    symbol: Option<String>,
    asset_class: String,
    display_name: String,
    kind: String,
    trade_date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    fees: Decimal,
    currency: String,
    fx_rate: Option<Decimal>,
    source: String,
    external_ref: Option<String>,
    contribution_id: Option<Uuid>,
    created_at: OffsetDateTime,
}

impl TryFrom<TransactionRow> for Transaction {
    type Error = sqlx::Error;

    fn try_from(row: TransactionRow) -> Result<Self, Self::Error> {
        Ok(Transaction {
            id: row.id,
            seq: row.seq,
            asset_id: row.asset_id,
            symbol: row.symbol,
            asset_class: AssetClass::parse(&row.asset_class)
                .ok_or_else(|| decode_error(&row.asset_class))?,
            display_name: row.display_name,
            fields: TransactionFields {
                kind: Kind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?,
                trade_date: row.trade_date,
                quantity: row.quantity.normalize(),
                unit_price: row.unit_price.map(|p| p.normalize()),
                fees: row.fees,
                currency: row.currency,
                fx_rate: row.fx_rate.map(|r| r.normalize()),
            },
            source: Source::parse(&row.source).ok_or_else(|| decode_error(&row.source))?,
            external_ref: row.external_ref,
            contribution_id: row.contribution_id,
            created_at: row.created_at,
        })
    }
}

#[derive(FromRow)]
struct EntryRow {
    seq: i64,
    kind: String,
    trade_date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    fees: Decimal,
    fx_rate: Option<Decimal>,
}

/// Hoje, no horário de Brasília — a data do negócio do que o Prumo grava
/// sozinho (cadastro, ajuste, "Já comprei"). A mesma conta da migração.
pub(crate) async fn today(tx: &mut DbTransaction<'_, Postgres>) -> Result<Date, StoreError> {
    let (today,): (Date,) = sqlx::query_as("select (now() at time zone 'America/Sao_Paulo')::date")
        .fetch_one(&mut **tx)
        .await?;
    Ok(today)
}

/// O ativo da pessoa, travado até o fim da transação.
pub(crate) async fn lock_asset(
    tx: &mut DbTransaction<'_, Postgres>,
    user: Uuid,
    id: Uuid,
) -> Result<Option<Asset>, StoreError> {
    let row: Option<AssetRow> = sqlx::query_as(concat!(
        "select ",
        asset_columns!(),
        " from assets where user_id = $1 and id = $2 for update"
    ))
    .bind(user)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(Asset::try_from).transpose()?)
}

/// Grava um lançamento do ativo (já travado). Não refaz a posição: quem chama
/// termina com [`rebuild`].
pub(crate) async fn insert_row(
    tx: &mut DbTransaction<'_, Postgres>,
    user: Uuid,
    asset: &Asset,
    new: &NewTransaction,
) -> Result<Transaction, StoreError> {
    let f = &new.fields;
    let result: Result<TransactionRow, sqlx::Error> = sqlx::query_as(concat!(
        "insert into transactions
            (id, user_id, asset_id, symbol, asset_class, display_name, kind, trade_date, quantity,
             unit_price, fees, currency, fx_rate, source, external_ref, contribution_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         returning ",
        transaction_columns!()
    ))
    .bind(Uuid::new_v4())
    .bind(user)
    .bind(asset.id)
    .bind(&asset.symbol)
    .bind(asset.asset_class.as_str())
    .bind(&asset.display_name)
    .bind(f.kind.as_str())
    .bind(f.trade_date)
    .bind(f.quantity)
    .bind(f.unit_price)
    .bind(f.fees)
    .bind(&f.currency)
    .bind(f.fx_rate)
    .bind(new.source.as_str())
    .bind(&new.external_ref)
    .bind(new.contribution_id)
    .fetch_one(&mut **tx)
    .await;
    match result {
        Ok(row) => Ok(Transaction::try_from(row)?),
        Err(error) if super::is_unique_violation(&error) => Err(StoreError::Duplicate),
        Err(error) => Err(error.into()),
    }
}

/// Refaz a posição do ativo a partir do livro inteiro e grava o cache. Erro
/// do livro desfaz a transação de quem chamou.
pub(crate) async fn rebuild(
    tx: &mut DbTransaction<'_, Postgres>,
    asset_id: Uuid,
) -> Result<Position, StoreError> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "select seq, kind, trade_date, quantity, unit_price, fees, fx_rate
           from transactions where asset_id = $1 order by trade_date, seq",
    )
    .bind(asset_id)
    .fetch_all(&mut **tx)
    .await?;
    let entries = rows
        .into_iter()
        .map(|r| {
            Ok(Entry {
                kind: Kind::parse(&r.kind).ok_or_else(|| decode_error(&r.kind))?,
                trade_date: r.trade_date,
                seq: r.seq,
                quantity: r.quantity,
                unit_price: r.unit_price,
                fees: r.fees,
                fx_rate: r.fx_rate.unwrap_or(Decimal::ONE),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let position = ledger::position(&entries).map_err(StoreError::Ledger)?;
    sqlx::query(
        "update assets set quantity = $2, cost_basis = $3, realized_gain = $4, updated_at = now()
          where id = $1",
    )
    .bind(asset_id)
    .bind(position.quantity)
    .bind(position.cost_basis)
    .bind(position.realized_gain)
    .execute(&mut **tx)
    .await?;
    Ok(position)
}

/// Data no futuro não é negócio que aconteceu.
async fn check_date(tx: &mut DbTransaction<'_, Postgres>, date: Date) -> Result<(), StoreError> {
    if date > today(tx).await? {
        return Err(StoreError::Invalid(
            "A data do lancamento nao pode ser no futuro.".into(),
        ));
    }
    Ok(())
}

/// O ajuste que leva a quantidade do ativo (travado) a `quantity`, datado
/// hoje. Nada se a quantidade já é essa.
pub(crate) async fn adjust_to(
    tx: &mut DbTransaction<'_, Postgres>,
    user: Uuid,
    asset: &Asset,
    quantity: Decimal,
) -> Result<bool, StoreError> {
    let delta = quantity - asset.quantity;
    if delta.is_zero() {
        return Ok(false);
    }
    let date = today(tx).await?;
    insert_row(
        tx,
        user,
        asset,
        &NewTransaction::manual(
            asset.id,
            TransactionFields::brl(Kind::Adjustment, date, delta, None),
        ),
    )
    .await?;
    Ok(true)
}

impl Store {
    /// Lançamentos da pessoa, do mais novo para o mais antigo (data, depois
    /// ordem de entrada), até [`LIST_LIMIT`].
    pub async fn transactions(
        &self,
        user: Uuid,
        filter: TransactionFilter,
    ) -> Result<Vec<Transaction>, StoreError> {
        let rows: Vec<TransactionRow> = sqlx::query_as(concat!(
            "select ",
            transaction_columns!(),
            " from transactions
              where user_id = $1
                and ($2::uuid is null or asset_id = $2)
                and ($3::date is null or trade_date >= $3)
                and ($4::date is null or trade_date <= $4)
              order by trade_date desc, seq desc
              limit $5"
        ))
        .bind(user)
        .bind(filter.asset_id)
        .bind(filter.from)
        .bind(filter.to)
        .bind(LIST_LIMIT)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(Transaction::try_from)
            .collect::<Result<_, _>>()?)
    }

    pub async fn transaction(
        &self,
        user: Uuid,
        id: Uuid,
    ) -> Result<Option<Transaction>, StoreError> {
        let row: Option<TransactionRow> = sqlx::query_as(concat!(
            "select ",
            transaction_columns!(),
            " from transactions where user_id = $1 and id = $2"
        ))
        .bind(user)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Transaction::try_from).transpose()?)
    }

    /// A posição em cache de cada ativo da pessoa. Uma leitura indexada de
    /// `assets`, sem tocar no livro: é o que a carteira (e o stream) usa.
    pub async fn positions(&self, user: Uuid) -> Result<HashMap<Uuid, Position>, StoreError> {
        let rows: Vec<(Uuid, Decimal, Option<Decimal>, Option<Decimal>)> = sqlx::query_as(
            "select id, quantity, cost_basis, realized_gain from assets where user_id = $1",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, quantity, cost_basis, realized_gain)| {
                (
                    id,
                    Position {
                        quantity: quantity.normalize(),
                        cost_basis: cost_basis.map(|c| c.normalize()),
                        realized_gain: realized_gain.map(|r| r.normalize()),
                    },
                )
            })
            .collect())
    }

    /// Grava um lançamento e refaz a posição. `NotFound` se o ativo não é da
    /// pessoa; [`StoreError::Ledger`] se o livro não fecha.
    pub async fn insert_transaction(
        &self,
        user: Uuid,
        new: &NewTransaction,
    ) -> Result<Transaction, StoreError> {
        let mut tx = self.pool.begin().await?;
        let asset = lock_asset(&mut tx, user, new.asset_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        check_date(&mut tx, new.fields.trade_date).await?;
        let stored = insert_row(&mut tx, user, &asset, new).await?;
        rebuild(&mut tx, asset.id).await?;
        tx.commit().await?;
        Ok(stored)
    }

    /// Troca os campos de um lançamento e refaz a posição desde o começo:
    /// mexer num lançamento antigo muda tudo o que vem depois dele.
    pub async fn update_transaction(
        &self,
        user: Uuid,
        id: Uuid,
        fields: &TransactionFields,
    ) -> Result<Option<Transaction>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let owner: Option<(Option<Uuid>,)> =
            sqlx::query_as("select asset_id from transactions where user_id = $1 and id = $2")
                .bind(user)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((asset_id,)) = owner else {
            return Ok(None);
        };
        let asset_id = asset_id.ok_or_else(|| {
            StoreError::Invalid("O ativo deste lancamento foi removido; so da para apagar.".into())
        })?;
        // Trava o ativo antes de tocar no lançamento: duas edições do mesmo
        // ativo entram uma de cada vez.
        lock_asset(&mut tx, user, asset_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        check_date(&mut tx, fields.trade_date).await?;
        let row: TransactionRow = sqlx::query_as(concat!(
            "update transactions set
                kind = $3, trade_date = $4, quantity = $5, unit_price = $6, fees = $7,
                currency = $8, fx_rate = $9
              where user_id = $1 and id = $2
              returning ",
            transaction_columns!()
        ))
        .bind(user)
        .bind(id)
        .bind(fields.kind.as_str())
        .bind(fields.trade_date)
        .bind(fields.quantity)
        .bind(fields.unit_price)
        .bind(fields.fees)
        .bind(&fields.currency)
        .bind(fields.fx_rate)
        .fetch_one(&mut *tx)
        .await?;
        rebuild(&mut tx, asset_id).await?;
        tx.commit().await?;
        Ok(Some(Transaction::try_from(row)?))
    }

    /// Apaga um lançamento e refaz a posição. Apagar a compra que sustenta
    /// uma venda posterior é recusado.
    pub async fn delete_transaction(&self, user: Uuid, id: Uuid) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let owner: Option<(Option<Uuid>,)> =
            sqlx::query_as("select asset_id from transactions where user_id = $1 and id = $2")
                .bind(user)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((asset_id,)) = owner else {
            return Ok(false);
        };
        if let Some(asset_id) = asset_id {
            lock_asset(&mut tx, user, asset_id).await?;
        }
        sqlx::query("delete from transactions where user_id = $1 and id = $2")
            .bind(user)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if let Some(asset_id) = asset_id {
            rebuild(&mut tx, asset_id).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Cadastra o ativo e, com quantidade, o saldo inicial dele no livro.
    pub async fn insert_asset_with_opening(
        &self,
        user: Uuid,
        new: &NewAsset,
        cost: OpeningCost,
    ) -> Result<Asset, StoreError> {
        let mut tx = self.pool.begin().await?;
        let asset = super::insert_asset_row(&mut tx, user, new).await?;
        if new.quantity.is_zero() {
            tx.commit().await?;
            return Ok(asset);
        }
        let date = today(&mut tx).await?;
        let opening = |quantity, price| {
            NewTransaction::manual(
                asset.id,
                TransactionFields::brl(Kind::Opening, date, quantity, price),
            )
        };
        match cost {
            OpeningCost::Unknown => {
                insert_row(&mut tx, user, &asset, &opening(new.quantity, None)).await?;
            }
            OpeningCost::AveragePrice(price) => {
                insert_row(&mut tx, user, &asset, &opening(new.quantity, Some(price))).await?;
            }
            OpeningCost::Invested(invested) => {
                insert_row(
                    &mut tx,
                    user,
                    &asset,
                    &opening(invested, Some(Decimal::ONE)),
                )
                .await?;
                let growth = new.quantity - invested;
                if !growth.is_zero() {
                    let adjustment = NewTransaction::manual(
                        asset.id,
                        TransactionFields::brl(Kind::Adjustment, date, growth, None),
                    );
                    insert_row(&mut tx, user, &asset, &adjustment).await?;
                }
            }
        }
        let position = rebuild(&mut tx, asset.id).await?;
        tx.commit().await?;
        Ok(Asset {
            quantity: position.quantity,
            ..asset
        })
    }
}
