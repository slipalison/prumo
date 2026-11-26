//! A importação da B3 no Postgres: a foto do livro que o plano confronta e a
//! gravação do plano, tudo ou nada.
//!
//! A gravação roda numa transação só: segura um advisory lock da pessoa (duas
//! importações dela entram uma de cada vez), trava os ativos dela
//! (`for update`, o mesmo lock do livro de lançamentos), lê o livro, refaz o
//! plano **ali dentro** (quem chama confere o `digest`) e grava. Qualquer erro
//! — o livro recusar um lançamento, um ativo repetido, o banco — desfaz tudo.
//!
//! A série de desempenho não é tocada aqui: os gatilhos da migração 0013
//! marcam a série da pessoa como suja desde a data mais antiga que a gravação
//! escreveu ou apagou (lançamento ou provento), e a tarefa em segundo plano
//! refaz a série inteira dali em diante.
//!
//! Nada daqui escreve conteúdo no log.

use std::collections::HashSet;

use rust_decimal::Decimal;
use sqlx::{FromRow, PgConnection, Postgres, Transaction as DbTransaction};
use time::Date;
use uuid::Uuid;

use super::ledger::{self, NewTransaction, Source, TransactionFields};
use super::{decode_error, insert_asset_row, Asset, NewAsset, Store, StoreError};
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::b3_plan::{
    AssetAction, AssetPlan, Book, BookAsset, BookEntry, BookIncome, IncomeWrite, Plan,
};
use crate::domain::income::IncomeKind;
use crate::domain::ledger::{Entry, Kind};

/// Um ativo que a importação criou: para buscar cotação e histórico de
/// dividendos em segundo plano depois de gravar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedAsset {
    pub id: Uuid,
    pub market: Market,
    pub symbol: Option<String>,
}

/// O plano gravado.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub plan: Plan,
    pub created: Vec<CreatedAsset>,
}

#[derive(FromRow)]
struct AssetRow {
    id: Uuid,
    asset_class: String,
    symbol: Option<String>,
    display_name: String,
    quantity: Decimal,
    listed_on_b3: bool,
    treasury: bool,
}

#[derive(FromRow)]
struct EntryRow {
    id: Uuid,
    asset_id: Uuid,
    seq: i64,
    kind: String,
    trade_date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    fees: Decimal,
    fx_rate: Option<Decimal>,
    external_ref: Option<String>,
}

#[derive(FromRow)]
struct IncomeRow {
    id: Uuid,
    symbol: Option<String>,
    kind: String,
    pay_date: Date,
    amount_per_unit: Decimal,
    net: Decimal,
    source: String,
    provisional: bool,
    external_ref: Option<String>,
}

const ASSETS: &str =
    "select id, asset_class, symbol, display_name, quantity, listed_on_b3, treasury
                        from assets where user_id = $1 order by created_at, id";
const ASSETS_LOCKED: &str =
    "select id, asset_class, symbol, display_name, quantity, listed_on_b3, treasury
                               from assets where user_id = $1 order by created_at, id for update";

/// A foto do livro da pessoa. Com `lock`, os ativos ficam travados até o fim
/// da transação.
async fn load_book(conn: &mut PgConnection, user: Uuid, lock: bool) -> Result<Book, StoreError> {
    let assets: Vec<AssetRow> = sqlx::query_as(if lock { ASSETS_LOCKED } else { ASSETS })
        .bind(user)
        .fetch_all(&mut *conn)
        .await?;
    let entries: Vec<EntryRow> = sqlx::query_as(
        "select id, asset_id, seq, kind, trade_date, quantity, unit_price, fees, fx_rate, external_ref
           from transactions where user_id = $1 and asset_id is not null
          order by trade_date, seq",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let refs: Vec<(String,)> = sqlx::query_as(
        "select external_ref from transactions where user_id = $1 and external_ref is not null",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let income: Vec<IncomeRow> = sqlx::query_as(
        "select id, symbol, kind, pay_date, amount_per_unit, net, source, provisional, external_ref
           from income_events where user_id = $1 order by pay_date, id",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let treasury: Vec<(String,)> =
        sqlx::query_as("select symbol from quotes where symbol like 'TD-%' order by symbol")
            .fetch_all(&mut *conn)
            .await?;
    let (today,): (Date,) = sqlx::query_as("select (now() at time zone 'America/Sao_Paulo')::date")
        .fetch_one(&mut *conn)
        .await?;
    Ok(Book {
        assets: assets
            .into_iter()
            .map(|r| {
                Ok(BookAsset {
                    id: r.id,
                    asset_class: AssetClass::parse(&r.asset_class)
                        .ok_or_else(|| decode_error(&r.asset_class))?,
                    symbol: r.symbol,
                    display_name: r.display_name,
                    quantity: r.quantity.normalize(),
                    listed_on_b3: r.listed_on_b3,
                    treasury: r.treasury,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?,
        entries: entries
            .into_iter()
            .map(|r| {
                Ok(BookEntry {
                    id: r.id,
                    asset_id: r.asset_id,
                    entry: Entry {
                        kind: Kind::parse(&r.kind).ok_or_else(|| decode_error(&r.kind))?,
                        trade_date: r.trade_date,
                        seq: r.seq,
                        quantity: r.quantity.normalize(),
                        unit_price: r.unit_price.map(|p| p.normalize()),
                        fees: r.fees,
                        fx_rate: r.fx_rate.unwrap_or(Decimal::ONE),
                    },
                    external_ref: r.external_ref,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?,
        transaction_refs: refs.into_iter().map(|(r,)| r).collect::<HashSet<_>>(),
        income: income
            .into_iter()
            .map(|r| {
                Ok(BookIncome {
                    id: r.id,
                    symbol: r.symbol,
                    kind: IncomeKind::parse(&r.kind).ok_or_else(|| decode_error(&r.kind))?,
                    pay_date: r.pay_date,
                    amount_per_unit: r.amount_per_unit.normalize(),
                    net: r.net,
                    imported: r.source == "b3_import",
                    provisional: r.provisional,
                    external_ref: r.external_ref,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?,
        treasury_symbols: treasury.into_iter().map(|(s,)| s).collect(),
        today,
    })
}

impl Store {
    /// A foto do livro para a prévia. Só lê.
    pub async fn b3_book(&self, user: Uuid) -> Result<Book, StoreError> {
        let mut conn = self.pool.acquire().await?;
        load_book(&mut conn, user, false).await
    }

    /// Grava a importação. `check` recebe o livro já travado e devolve o plano
    /// a gravar, ou a recusa (`Ok(Err(..))`): prévia vencida, conflito, classe
    /// por confirmar. Recusa e erro não gravam nada.
    pub async fn commit_b3_import<R>(
        &self,
        user: Uuid,
        check: impl FnOnce(&Book) -> Result<Plan, R>,
    ) -> Result<Result<Applied, R>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(format!("b3_import:{user}"))
            .execute(&mut *tx)
            .await?;
        let book = load_book(&mut tx, user, true).await?;
        let plan = match check(&book) {
            Ok(plan) => plan,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let mut created = Vec::new();
        for asset in &plan.assets {
            if let Some(made) = apply_asset(&mut tx, user, asset).await? {
                created.push(made);
            }
        }
        tx.commit().await?;
        Ok(Ok(Applied { plan, created }))
    }
}

/// Grava o plano de um ativo. Devolve o ativo criado, se criou.
async fn apply_asset(
    tx: &mut DbTransaction<'_, Postgres>,
    user: Uuid,
    plan: &AssetPlan,
) -> Result<Option<CreatedAsset>, StoreError> {
    let writes = &plan.writes;
    let (asset, created): (Option<Asset>, bool) = match plan.action {
        AssetAction::Skip => return Ok(None),
        AssetAction::Nothing => (None, false),
        AssetAction::Create => {
            let symbol = plan
                .symbol
                .clone()
                .ok_or_else(|| StoreError::Invalid("ativo sem simbolo".into()))?;
            let new = NewAsset {
                asset_class: plan.asset_class,
                symbol: Some(symbol),
                display_name: plan.display_name.chars().take(120).collect(),
                quantity: Decimal::ZERO,
                manual_price: None,
                score_override: None,
                listed_on_b3: plan.listed_on_b3,
                treasury: plan.treasury,
                max_price: None,
            };
            (Some(insert_asset_row(tx, user, &new).await?), true)
        }
        AssetAction::Update => {
            let id = plan.asset_id.ok_or(StoreError::NotFound)?;
            (
                Some(
                    ledger::lock_asset(tx, user, id)
                        .await?
                        .ok_or(StoreError::NotFound)?,
                ),
                false,
            )
        }
    };
    if let Some(asset) = &asset {
        if !writes.delete_entries.is_empty() {
            sqlx::query("delete from transactions where user_id = $1 and id = any($2)")
                .bind(user)
                .bind(&writes.delete_entries)
                .execute(&mut **tx)
                .await?;
        }
        for (id, external_ref) in &writes.adopt {
            sqlx::query(
                "update transactions set external_ref = $3
                  where user_id = $1 and id = $2 and external_ref is null",
            )
            .bind(user)
            .bind(id)
            .bind(external_ref)
            .execute(&mut **tx)
            .await?;
        }
        for entry in &writes.entries {
            let new = NewTransaction {
                asset_id: asset.id,
                fields: TransactionFields::brl(
                    entry.kind,
                    entry.trade_date,
                    entry.quantity,
                    entry.unit_price,
                ),
                source: Source::B3Import,
                external_ref: entry.external_ref.clone(),
                contribution_id: None,
            };
            ledger::insert_row(tx, user, asset, &new).await?;
        }
        if !writes.entries.is_empty() || !writes.delete_entries.is_empty() {
            ledger::rebuild(tx, asset.id).await?;
        }
    }
    let asset_id = asset.as_ref().map(|a| a.id);
    let symbol = asset
        .as_ref()
        .and_then(|a| a.symbol.clone())
        .or_else(|| plan.symbol.clone());
    let display_name = asset
        .as_ref()
        .map_or_else(|| plan.display_name.clone(), |a| a.display_name.clone());
    for income in &writes.income {
        write_income(tx, user, asset_id, symbol.as_deref(), &display_name, income).await?;
    }
    Ok(asset.filter(|_| created).map(|a| CreatedAsset {
        id: a.id,
        market: a.market(),
        symbol: a.symbol,
    }))
}

async fn write_income(
    tx: &mut DbTransaction<'_, Postgres>,
    user: Uuid,
    asset_id: Option<Uuid>,
    symbol: Option<&str>,
    display_name: &str,
    income: &IncomeWrite,
) -> Result<(), StoreError> {
    let name: String = display_name.chars().take(120).collect();
    let result = match income.replace {
        Some(id) => {
            sqlx::query(
                "update income_events set
                    asset_id = coalesce($3, asset_id), kind = $4, pay_date = $5,
                    amount_per_unit = $6, quantity = $7, gross = $8, withholding = $9, net = $10,
                    provisional = $11, external_ref = $12
                  where user_id = $1 and id = $2",
            )
            .bind(user)
            .bind(id)
            .bind(asset_id)
            .bind(income.kind.as_str())
            .bind(income.pay_date)
            .bind(income.amount_per_unit)
            .bind(income.quantity)
            .bind(income.gross)
            .bind(income.withholding)
            .bind(income.net)
            .bind(income.provisional)
            .bind(&income.external_ref)
            .execute(&mut **tx)
            .await
        }
        None => {
            sqlx::query(
                "insert into income_events
                    (id, user_id, asset_id, symbol, display_name, kind, ex_date, pay_date,
                     amount_per_unit, quantity, gross, withholding, net, source, external_ref, provisional)
                 values ($1, $2, $3, $4, $5, $6, null, $7, $8, $9, $10, $11, $12, 'b3_import', $13, $14)",
            )
            .bind(Uuid::new_v4())
            .bind(user)
            .bind(asset_id)
            .bind(symbol)
            .bind(name)
            .bind(income.kind.as_str())
            .bind(income.pay_date)
            .bind(income.amount_per_unit)
            .bind(income.quantity)
            .bind(income.gross)
            .bind(income.withholding)
            .bind(income.net)
            .bind(&income.external_ref)
            .bind(income.provisional)
            .execute(&mut **tx)
            .await
        }
    };
    match result {
        Ok(_) => Ok(()),
        Err(error) if super::is_unique_violation(&error) => Err(StoreError::Duplicate),
        Err(error) => Err(error.into()),
    }
}
