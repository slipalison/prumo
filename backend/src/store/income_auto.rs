//! Proventos automáticos no Postgres: os anúncios da B3 (compartilhados entre
//! as carteiras), a vez de cada réplica buscar um emissor, a chave da pessoa e
//! a rodada que refaz os automáticos dela.
//!
//! Toda escrita é uma transação curta: nenhuma conexão fica presa enquanto a
//! busca fala com a B3 (o pool é de quatro conexões por réplica).

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;
use sqlx::{FromRow, PgConnection};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::cash::entries_by_asset_in;
use super::{decode_error, Store, StoreError};
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::income::{IncomeKind, IncomeSource};
use crate::domain::income_auto::{
    isin_for, parse_ticker, pending_stock_events, plan, AutoAsset, AutoIncome, AutoPlan,
    CashAnnouncement, KnownIncome, PendingStockEvent, StockAnnouncement, Venue,
};
use crate::income_auto::{issuers_of, Listing, Supplement};

/// O que a rodada de uma pessoa gravou.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AutoOutcome {
    pub inserted: usize,
    pub updated: usize,
    pub deleted: usize,
    /// Anúncios que já estavam lançados à mão ou importados.
    pub covered: usize,
}

/// O que a B3 diz dos proventos de um ativo, para a estimativa de renda
/// (`domain::income::estimate`).
#[derive(Debug, Clone, PartialEq)]
pub struct B3IncomeSource {
    pub isin: String,
    /// A última busca bem-sucedida do emissor.
    pub fetched_at: OffsetDateTime,
    /// Os anúncios em dinheiro do ISIN com data com desde a pedida.
    pub cash: Vec<CashAnnouncement>,
    /// Os eventos em ações do ISIN com data com desde a pedida.
    pub stock: Vec<StockAnnouncement>,
}

/// O que a tela de ajustes mostra.
#[derive(Debug, Clone, PartialEq)]
pub struct IncomeAutoState {
    pub enabled: bool,
    /// A busca mais velha entre os emissores da pessoa (`None`: nenhum buscado).
    pub fetched_at: Option<OffsetDateTime>,
    /// Ativos da B3 da pessoa que a fonte cobre e os que ficaram sem ISIN.
    pub covered_symbols: Vec<String>,
    pub unmatched_symbols: Vec<String>,
    pub pending: Vec<PendingStockEvent>,
}

#[derive(FromRow)]
struct AssetRow {
    id: Uuid,
    asset_class: String,
    symbol: String,
    display_name: String,
    listed_on_b3: bool,
    treasury: bool,
}

#[derive(FromRow)]
struct KnownRow {
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

#[derive(FromRow)]
struct CashRow {
    issuer: String,
    isin: String,
    label: String,
    record_date: Date,
    rate: Decimal,
    pay_date: Option<Date>,
    approved_on: Option<Date>,
    related_to: Option<String>,
}

#[derive(FromRow)]
struct StockRow {
    issuer: String,
    isin: String,
    label: String,
    record_date: Date,
    factor: Decimal,
    approved_on: Option<Date>,
}

impl From<CashRow> for CashAnnouncement {
    fn from(r: CashRow) -> Self {
        CashAnnouncement {
            issuer: r.issuer,
            isin: r.isin,
            label: r.label,
            record_date: r.record_date,
            rate: r.rate.normalize(),
            pay_date: r.pay_date,
            approved_on: r.approved_on,
            related_to: r.related_to,
        }
    }
}

impl From<StockRow> for StockAnnouncement {
    fn from(r: StockRow) -> Self {
        StockAnnouncement {
            issuer: r.issuer,
            isin: r.isin,
            label: r.label,
            record_date: r.record_date,
            factor: r.factor.normalize(),
            approved_on: r.approved_on,
        }
    }
}

/// Um ativo da B3 da pessoa, com o código já lido.
struct B3Asset {
    row: AssetRow,
    class: AssetClass,
    ticker: crate::domain::income_auto::Ticker,
    venue: Venue,
}

/// Os ativos da B3 da pessoa que têm código de negociação.
async fn b3_assets(conn: &mut PgConnection, user: Uuid) -> Result<Vec<B3Asset>, StoreError> {
    let rows: Vec<AssetRow> = sqlx::query_as(
        "select id, asset_class, symbol, display_name, listed_on_b3, treasury
           from assets where user_id = $1 and symbol is not null
          order by id",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let mut assets = Vec::new();
    for row in rows {
        let class =
            AssetClass::parse(&row.asset_class).ok_or_else(|| decode_error(&row.asset_class))?;
        if class.market_of(row.listed_on_b3, row.treasury) != Market::B3 {
            continue;
        }
        let Some(ticker) = parse_ticker(&row.symbol) else {
            continue;
        };
        let venue = Venue::of(class, &ticker);
        assets.push(B3Asset {
            row,
            class,
            ticker,
            venue,
        });
    }
    Ok(assets)
}

/// Os anúncios em dinheiro dos emissores.
async fn cash_announcements(
    conn: &mut PgConnection,
    issuers: &[String],
) -> Result<Vec<CashAnnouncement>, StoreError> {
    let rows: Vec<CashRow> = sqlx::query_as(
        "select issuer, isin, label, record_date, rate, pay_date, approved_on, related_to
           from b3_cash_announcements where issuer = any($1)
          order by isin, record_date, label, rate, pay_date",
    )
    .bind(issuers)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(CashAnnouncement::from).collect())
}

async fn stock_announcements(
    conn: &mut PgConnection,
    issuers: &[String],
) -> Result<Vec<StockAnnouncement>, StoreError> {
    let rows: Vec<StockRow> = sqlx::query_as(
        "select issuer, isin, label, record_date, factor, approved_on
           from b3_stock_announcements where issuer = any($1)
          order by isin, record_date, label",
    )
    .bind(issuers)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(StockAnnouncement::from).collect())
}

/// Os ativos da B3 da pessoa com o ISIN resolvido (cadastro da B3 ou a regra
/// do tipo de papel entre os ISIN que o emissor anunciou) e o livro.
async fn auto_assets(
    conn: &mut PgConnection,
    user: Uuid,
    known_isins: &HashMap<String, Vec<String>>,
    assets: Vec<B3Asset>,
) -> Result<Vec<AutoAsset>, StoreError> {
    let tickers: Vec<String> = assets.iter().map(|a| a.ticker.code.clone()).collect();
    let listings: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "select ticker, isin from b3_listings where ticker = any($1)",
    )
    .bind(&tickers)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let mut entries = entries_by_asset_in(conn, user).await?;
    Ok(assets
        .into_iter()
        .map(|a| {
            let isins: Vec<&str> = known_isins
                .get(&a.ticker.issuer)
                .map(|list| list.iter().map(String::as_str).collect())
                .unwrap_or_default();
            AutoAsset {
                id: a.row.id,
                isin: isin_for(
                    &a.ticker,
                    a.venue,
                    listings.get(&a.ticker.code).map(String::as_str),
                    &isins,
                ),
                symbol: a.row.symbol,
                display_name: a.row.display_name,
                class: a.class,
                entries: entries.remove(&a.row.id).unwrap_or_default(),
            }
        })
        .collect())
}

fn issuers(assets: &[B3Asset]) -> Vec<String> {
    let set: HashSet<&str> = assets.iter().map(|a| a.ticker.issuer.as_str()).collect();
    let mut list: Vec<String> = set.into_iter().map(str::to_owned).collect();
    list.sort();
    list
}

fn isins_by_issuer<'a>(
    isins: impl Iterator<Item = (&'a str, &'a str)>,
) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (issuer, isin) in isins {
        map.entry(issuer.to_owned())
            .or_default()
            .push(isin.to_owned());
    }
    map
}

fn isins_by_issuer_owned(rows: Vec<(String, String)>) -> HashMap<String, Vec<String>> {
    isins_by_issuer(
        rows.iter()
            .map(|(issuer, isin)| (issuer.as_str(), isin.as_str())),
    )
}

async fn known_income(conn: &mut PgConnection, user: Uuid) -> Result<Vec<KnownIncome>, StoreError> {
    let rows: Vec<KnownRow> = sqlx::query_as(
        "select id, asset_id, symbol, display_name, kind, ex_date, pay_date, amount_per_unit,
                quantity, gross, withholding, net, source, external_ref
           from income_events where user_id = $1 and source <> 'estimate'
          order by pay_date, created_at, id",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(KnownIncome {
                id: r.id,
                asset_id: r.asset_id,
                symbol: r.symbol,
                kind: IncomeKind::parse(&r.kind).ok_or_else(|| decode_error(&r.kind))?,
                ex_date: r.ex_date,
                pay_date: r.pay_date,
                amount_per_unit: r.amount_per_unit.normalize(),
                quantity: r.quantity.normalize(),
                gross: r.gross,
                withholding: r.withholding,
                net: r.net,
                source: IncomeSource::parse(&r.source).ok_or_else(|| decode_error(&r.source))?,
                external_ref: r.external_ref,
                display_name: r.display_name,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(StoreError::from)
}

async fn lock_user(conn: &mut PgConnection, user: Uuid) -> Result<(), StoreError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(income_auto_lock(user))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A chave do lock da pessoa: a rodada e a importação da B3 dela entram uma de
/// cada vez (a importação pode trocar um automático pelo importado).
pub(super) fn income_auto_lock(user: Uuid) -> String {
    format!("income_auto:{user}")
}

async fn delete_all_automatic(conn: &mut PgConnection, user: Uuid) -> Result<usize, StoreError> {
    let done =
        sqlx::query("delete from income_events where user_id = $1 and source = 'b3_announcement'")
            .bind(user)
            .execute(&mut *conn)
            .await?;
    Ok(usize::try_from(done.rows_affected()).unwrap_or(usize::MAX))
}

async fn apply_plan(
    conn: &mut PgConnection,
    user: Uuid,
    plan: &AutoPlan,
) -> Result<AutoOutcome, StoreError> {
    let mut outcome = AutoOutcome {
        covered: plan.covered,
        ..AutoOutcome::default()
    };
    if !plan.delete.is_empty() {
        let done = sqlx::query(
            "delete from income_events
              where user_id = $1 and id = any($2) and source = 'b3_announcement'",
        )
        .bind(user)
        .bind(&plan.delete)
        .execute(&mut *conn)
        .await?;
        outcome.deleted = usize::try_from(done.rows_affected()).unwrap_or(usize::MAX);
    }
    for (id, auto) in &plan.update {
        let done = sqlx::query(
            "update income_events set
                asset_id = $3, symbol = $4, display_name = $5, kind = $6, ex_date = $7,
                pay_date = $8, amount_per_unit = $9, quantity = $10, gross = $11,
                withholding = $12, net = $13
              where user_id = $1 and id = $2 and source = 'b3_announcement'",
        )
        .bind(user)
        .bind(id)
        .bind(auto.asset_id)
        .bind(&auto.symbol)
        .bind(name(auto))
        .bind(auto.kind.as_str())
        .bind(auto.ex_date)
        .bind(auto.pay_date)
        .bind(auto.amount_per_unit)
        .bind(auto.quantity)
        .bind(auto.amounts.gross)
        .bind(auto.amounts.withholding)
        .bind(auto.amounts.net)
        .execute(&mut *conn)
        .await?;
        outcome.updated += usize::try_from(done.rows_affected()).unwrap_or(0);
    }
    for auto in &plan.insert {
        let done = sqlx::query(
            "insert into income_events
                (id, user_id, asset_id, symbol, display_name, kind, ex_date, pay_date,
                 amount_per_unit, quantity, gross, withholding, net, source, external_ref)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'b3_announcement', $14)
             on conflict (user_id, external_ref) where external_ref is not null do nothing",
        )
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(auto.asset_id)
        .bind(&auto.symbol)
        .bind(name(auto))
        .bind(auto.kind.as_str())
        .bind(auto.ex_date)
        .bind(auto.pay_date)
        .bind(auto.amount_per_unit)
        .bind(auto.quantity)
        .bind(auto.amounts.gross)
        .bind(auto.amounts.withholding)
        .bind(auto.amounts.net)
        .bind(&auto.external_ref)
        .execute(&mut *conn)
        .await?;
        outcome.inserted += usize::try_from(done.rows_affected()).unwrap_or(0);
    }
    Ok(outcome)
}

fn name(auto: &AutoIncome) -> String {
    auto.display_name.chars().take(120).collect()
}

impl Store {
    /// Os emissores da B3 de todas as carteiras, cada um com a página certa.
    pub async fn income_auto_issuers(&self) -> Result<Vec<(String, Venue)>, StoreError> {
        let rows: Vec<(String, bool, bool, String)> = sqlx::query_as(
            "select distinct asset_class, listed_on_b3, treasury, symbol
               from assets where symbol is not null",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(issuers_of(rows.into_iter().filter_map(
            |(class, listed_on_b3, treasury, symbol)| {
                let class = AssetClass::parse(&class)?;
                if class.market_of(listed_on_b3, treasury) != Market::B3 {
                    return None;
                }
                let ticker = parse_ticker(&symbol)?;
                let venue = Venue::of(class, &ticker);
                Some((ticker.issuer, venue))
            },
        )))
    }

    /// A B3 de cada ativo pedido (`id`, classe, código): só os que a B3 tem, isto
    /// é, cujo emissor já foi buscado com sucesso e cujo ISIN foi achado (o
    /// cadastro da B3 ou a regra do tipo de papel, como nos proventos
    /// automáticos). Os anúncios vêm com data com a partir de `since`. Tudo é
    /// compartilhado entre as carteiras: não depende de quem pede.
    pub async fn b3_income_sources(
        &self,
        assets: &[(Uuid, AssetClass, String)],
        since: Date,
    ) -> Result<HashMap<Uuid, B3IncomeSource>, StoreError> {
        let parsed: Vec<(Uuid, crate::domain::income_auto::Ticker, Venue)> = assets
            .iter()
            .filter_map(|(id, class, symbol)| {
                let ticker = parse_ticker(symbol)?;
                let venue = Venue::of(*class, &ticker);
                Some((*id, ticker, venue))
            })
            .collect();
        if parsed.is_empty() {
            return Ok(HashMap::new());
        }
        let mut issuers: Vec<String> = parsed.iter().map(|(_, t, _)| t.issuer.clone()).collect();
        issuers.sort();
        issuers.dedup();
        let tickers: Vec<String> = parsed.iter().map(|(_, t, _)| t.code.clone()).collect();
        let mut conn = self.pool.acquire().await?;
        let fetched: HashMap<String, OffsetDateTime> =
            sqlx::query_as::<_, (String, OffsetDateTime)>(
                "select issuer, fetched_at from b3_issuer_fetches
              where issuer = any($1) and fetched_at is not null",
            )
            .bind(&issuers)
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();
        let listings: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
            "select ticker, isin from b3_listings where ticker = any($1)",
        )
        .bind(&tickers)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .collect();
        // Os ISIN que cada emissor já anunciou, de qualquer época: a regra de
        // reserva do ISIN (fundo não tem cadastro com `otherCodes`).
        let known = isins_by_issuer_owned(
            sqlx::query_as::<_, (String, String)>(
                "select issuer, isin from b3_cash_announcements where issuer = any($1)
                 union
                 select issuer, isin from b3_stock_announcements where issuer = any($1)",
            )
            .bind(&issuers)
            .fetch_all(&mut *conn)
            .await?,
        );
        let cash: Vec<CashRow> = sqlx::query_as(
            "select issuer, isin, label, record_date, rate, pay_date, approved_on, related_to
               from b3_cash_announcements where issuer = any($1) and record_date >= $2
              order by isin, record_date, label, rate, pay_date",
        )
        .bind(&issuers)
        .bind(since)
        .fetch_all(&mut *conn)
        .await?;
        let stock: Vec<StockRow> = sqlx::query_as(
            "select issuer, isin, label, record_date, factor, approved_on
               from b3_stock_announcements where issuer = any($1) and record_date >= $2
              order by isin, record_date, label",
        )
        .bind(&issuers)
        .bind(since)
        .fetch_all(&mut *conn)
        .await?;
        let cash: Vec<CashAnnouncement> = cash.into_iter().map(CashAnnouncement::from).collect();
        let stock: Vec<StockAnnouncement> =
            stock.into_iter().map(StockAnnouncement::from).collect();

        let mut sources = HashMap::new();
        for (id, ticker, venue) in parsed {
            let Some(fetched_at) = fetched.get(&ticker.issuer).copied() else {
                continue;
            };
            let isins: Vec<&str> = known
                .get(&ticker.issuer)
                .map(|list| list.iter().map(String::as_str).collect())
                .unwrap_or_default();
            let Some(isin) = isin_for(
                &ticker,
                venue,
                listings.get(&ticker.code).map(String::as_str),
                &isins,
            ) else {
                continue;
            };
            sources.insert(
                id,
                B3IncomeSource {
                    cash: cash.iter().filter(|c| c.isin == isin).cloned().collect(),
                    stock: stock.iter().filter(|s| s.isin == isin).cloned().collect(),
                    isin,
                    fetched_at,
                },
            );
        }
        Ok(sources)
    }

    /// A vez de buscar `issuer`: `true` para quem marcou primeiro, e só se a
    /// última busca venceu (`ttl`) e ninguém está buscando (`lease`). Um
    /// `update` condicional só, sem segurar conexão.
    pub async fn claim_issuer_fetch(
        &self,
        issuer: &str,
        now: OffsetDateTime,
        ttl: time::Duration,
        lease: time::Duration,
    ) -> Result<bool, StoreError> {
        let claimed: Option<(String,)> = sqlx::query_as(
            "insert into b3_issuer_fetches as f (issuer, claimed_until) values ($1, $2::timestamptz + $4::interval)
             on conflict (issuer) do update set claimed_until = excluded.claimed_until
              where (f.fetched_at is null or f.fetched_at <= $2::timestamptz - $3::interval)
                and (f.claimed_until is null or f.claimed_until <= $2::timestamptz)
             returning issuer",
        )
        .bind(issuer)
        .bind(now)
        .bind(pg_interval(ttl))
        .bind(pg_interval(lease))
        .fetch_optional(&self.pool)
        .await?;
        Ok(claimed.is_some())
    }

    /// Grava o que a B3 disse de um emissor. O que a B3 deixou de listar
    /// **dentro do período que ela ainda cobre** (data com a partir da mais
    /// velha da resposta) sai — foi corrigido ou cancelado; o mais velho fica,
    /// para o histórico de proventos não encolher quando a janela anda.
    pub async fn save_b3_announcements(
        &self,
        issuer: &str,
        supplement: &Supplement,
        listings: &[Listing],
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(format!("b3_announcements:{issuer}"))
            .execute(&mut *tx)
            .await?;
        let cash = &supplement.cash;
        if let Some(oldest) = cash.iter().map(|c| c.record_date).min() {
            let keys: Vec<String> = cash.iter().map(cash_key).collect();
            sqlx::query(
                "delete from b3_cash_announcements
                  where issuer = $1 and record_date >= $2
                    and (isin || '|' || label || '|' || record_date::text || '|' || rate::text
                         || '|' || coalesce(pay_date::text, '')) <> all($3)",
            )
            .bind(issuer)
            .bind(oldest)
            .bind(&keys)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "insert into b3_cash_announcements
                    (issuer, isin, label, record_date, rate, pay_date, approved_on, related_to)
                 select $1, i, l, r, v, p, a, t
                   from unnest($2::text[], $3::text[], $4::date[], $5::numeric[], $6::date[], $7::date[], $8::text[])
                     as x (i, l, r, v, p, a, t)
                 on conflict (isin, label, record_date, rate, pay_date) do update set
                    issuer = excluded.issuer,
                    approved_on = excluded.approved_on, related_to = excluded.related_to",
            )
            .bind(issuer)
            .bind(cash.iter().map(|c| c.isin.clone()).collect::<Vec<_>>())
            .bind(cash.iter().map(|c| c.label.clone()).collect::<Vec<_>>())
            .bind(cash.iter().map(|c| c.record_date).collect::<Vec<_>>())
            .bind(cash.iter().map(|c| c.rate).collect::<Vec<_>>())
            .bind(cash.iter().map(|c| c.pay_date).collect::<Vec<_>>())
            .bind(cash.iter().map(|c| c.approved_on).collect::<Vec<_>>())
            .bind(cash.iter().map(|c| c.related_to.clone()).collect::<Vec<_>>())
            .execute(&mut *tx)
            .await?;
        }
        let stock = &supplement.stock;
        if let Some(oldest) = stock.iter().map(|s| s.record_date).min() {
            let keys: Vec<String> = stock.iter().map(stock_key).collect();
            sqlx::query(
                "delete from b3_stock_announcements
                  where issuer = $1 and record_date >= $2
                    and (isin || '|' || label || '|' || record_date::text || '|' || factor::text) <> all($3)",
            )
            .bind(issuer)
            .bind(oldest)
            .bind(&keys)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "insert into b3_stock_announcements
                    (issuer, isin, label, record_date, factor, approved_on)
                 select $1, i, l, r, f, a
                   from unnest($2::text[], $3::text[], $4::date[], $5::numeric[], $6::date[])
                     as x (i, l, r, f, a)
                 on conflict (isin, label, record_date, factor) do update set
                    issuer = excluded.issuer, approved_on = excluded.approved_on",
            )
            .bind(issuer)
            .bind(stock.iter().map(|s| s.isin.clone()).collect::<Vec<_>>())
            .bind(stock.iter().map(|s| s.label.clone()).collect::<Vec<_>>())
            .bind(stock.iter().map(|s| s.record_date).collect::<Vec<_>>())
            .bind(stock.iter().map(|s| s.factor).collect::<Vec<_>>())
            .bind(stock.iter().map(|s| s.approved_on).collect::<Vec<_>>())
            .execute(&mut *tx)
            .await?;
        }
        if !listings.is_empty() {
            sqlx::query(
                "insert into b3_listings (ticker, issuer, isin, fetched_at)
                 select t, $1, i, $4 from unnest($2::text[], $3::text[]) as x (t, i)
                 on conflict (ticker) do update set
                    issuer = excluded.issuer, isin = excluded.isin, fetched_at = excluded.fetched_at",
            )
            .bind(issuer)
            .bind(listings.iter().map(|l| l.ticker.clone()).collect::<Vec<_>>())
            .bind(listings.iter().map(|l| l.isin.clone()).collect::<Vec<_>>())
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "insert into b3_issuer_fetches (issuer, fetched_at, claimed_until) values ($1, $2, null)
             on conflict (issuer) do update set fetched_at = excluded.fetched_at, claimed_until = null",
        )
        .bind(issuer)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Quem está com os automáticos ligados e tem algum ativo com código.
    pub async fn income_auto_users(&self) -> Result<Vec<Uuid>, StoreError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "select u.id from users u
              where u.income_auto
                and exists (select 1 from assets a where a.user_id = u.id and a.symbol is not null)
              order by u.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Refaz os automáticos da pessoa: o plano do domínio, gravado numa
    /// transação só, sob o lock da pessoa. Só escreve o que mudou (a série de
    /// desempenho só fica suja quando um provento de fato muda). Desligado:
    /// apaga todos.
    pub async fn regenerate_income_auto(&self, user: Uuid) -> Result<AutoOutcome, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_user(&mut tx, user).await?;
        let enabled: Option<(bool,)> =
            sqlx::query_as("select income_auto from users where id = $1")
                .bind(user)
                .fetch_optional(&mut *tx)
                .await?;
        let outcome = match enabled {
            None => AutoOutcome::default(),
            Some((false,)) => AutoOutcome {
                deleted: delete_all_automatic(&mut tx, user).await?,
                ..AutoOutcome::default()
            },
            Some((true,)) => {
                let assets = b3_assets(&mut tx, user).await?;
                let issuers = issuers(&assets);
                let announcements = cash_announcements(&mut tx, &issuers).await?;
                let known_isins = isins_by_issuer(
                    announcements
                        .iter()
                        .map(|a| (a.issuer.as_str(), a.isin.as_str())),
                );
                let assets = auto_assets(&mut tx, user, &known_isins, assets).await?;
                let known = known_income(&mut tx, user).await?;
                let dismissed: HashSet<String> = sqlx::query_as::<_, (String,)>(
                    "select external_ref from income_auto_dismissed where user_id = $1",
                )
                .bind(user)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|(r,)| r)
                .collect();
                let plan = plan(&assets, &announcements, &known, &dismissed);
                for asset in &plan.skipped {
                    tracing::warn!(user = %user, asset = %asset, "livro nao fecha: automaticos do ativo mantidos");
                }
                apply_plan(&mut tx, user, &plan).await?
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// Liga ou desliga os automáticos da pessoa. Desligar apaga os automáticos
    /// na mesma transação; ligar não grava nada aqui (quem chama refaz).
    pub async fn set_income_auto(&self, user: Uuid, enabled: bool) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_user(&mut tx, user).await?;
        sqlx::query("update users set income_auto = $2 where id = $1")
            .bind(user)
            .bind(enabled)
            .execute(&mut *tx)
            .await?;
        if !enabled {
            delete_all_automatic(&mut tx, user).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// A chave, a data da busca e as pendências de eventos em ações da pessoa.
    pub async fn income_auto_state(
        &self,
        user: Uuid,
        today: Date,
    ) -> Result<IncomeAutoState, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let (enabled,): (bool,) = sqlx::query_as("select income_auto from users where id = $1")
            .bind(user)
            .fetch_one(&mut *conn)
            .await?;
        let assets = b3_assets(&mut conn, user).await?;
        let issuers = issuers(&assets);
        let fetched_at: Option<(Option<OffsetDateTime>, i64)> = sqlx::query_as(
            "select min(fetched_at), count(fetched_at) from b3_issuer_fetches where issuer = any($1)",
        )
        .bind(&issuers)
        .fetch_optional(&mut *conn)
        .await?;
        let cash = cash_announcements(&mut conn, &issuers).await?;
        let stock = stock_announcements(&mut conn, &issuers).await?;
        let known_isins = isins_by_issuer(
            cash.iter()
                .map(|a| (a.issuer.as_str(), a.isin.as_str()))
                .chain(stock.iter().map(|s| (s.issuer.as_str(), s.isin.as_str()))),
        );
        let assets = auto_assets(&mut conn, user, &known_isins, assets).await?;
        let mut pending: Vec<PendingStockEvent> = assets
            .iter()
            .flat_map(|a| pending_stock_events(a, &stock, today))
            .collect();
        pending.sort_by(|a, b| {
            b.record_date
                .cmp(&a.record_date)
                .then_with(|| a.symbol.cmp(&b.symbol))
        });
        let (mut covered, mut unmatched): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        for asset in &assets {
            if asset.isin.is_some() {
                covered.push(asset.symbol.clone());
            } else {
                unmatched.push(asset.symbol.clone());
            }
        }
        covered.sort();
        unmatched.sort();
        Ok(IncomeAutoState {
            enabled,
            fetched_at: fetched_at.and_then(|(at, _)| at),
            covered_symbols: covered,
            unmatched_symbols: unmatched,
            pending,
        })
    }
}

/// A chave de um anúncio em dinheiro, como o `delete` a monta no Postgres. A
/// data de pagamento entra: cada parcela é um anúncio.
fn cash_key(c: &CashAnnouncement) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        c.isin,
        c.label,
        c.record_date,
        numeric_text(c.rate),
        c.pay_date.map(|d| d.to_string()).unwrap_or_default()
    )
}

fn stock_key(s: &StockAnnouncement) -> String {
    format!(
        "{}|{}|{}|{}",
        s.isin,
        s.label,
        s.record_date,
        numeric_text(s.factor)
    )
}

/// Como o Postgres escreve um `numeric(24, 11)`: sempre com 11 casas.
fn numeric_text(value: Decimal) -> String {
    let mut scaled = value;
    scaled.rescale(11);
    scaled.to_string()
}

/// `time::Duration` como intervalo do Postgres.
fn pg_interval(duration: time::Duration) -> String {
    format!("{} seconds", duration.whole_seconds())
}
