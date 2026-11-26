//! O caixa no Postgres: lê os movimentos (proventos recebidos, vendas do livro
//! e o caixa que cada aporte consumiu) e deixa o domínio refazer o saldo. Não
//! há tabela de saldo — ver `migrations/0012_cash.sql`.
//!
//! O registro do aporte que usa o caixa confere o saldo **dentro da mesma
//! transação** que o grava, sob um lock por pessoa: dois aportes ao mesmo
//! tempo não gastam o mesmo dinheiro duas vezes.

use std::collections::HashMap;

use rust_decimal::Decimal;
use sqlx::{FromRow, PgConnection};
use time::Date;
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::cash::{
    check_use, replay, sale_proceeds, CashState, CashUseError, Movement, MovementKind,
};
use crate::domain::ledger::{Entry, Kind};

/// Quantos movimentos o detalhe do caixa mostra (os mais novos).
pub const RECENT_SHOWN: usize = 30;

/// O caixa da pessoa: o saldo, de onde ele veio e os movimentos mais novos.
#[derive(Debug, Clone, PartialEq)]
pub struct CashReport {
    /// O dia (Brasília) a partir do qual o caixa conta.
    pub since: Date,
    pub state: CashState,
    /// Do mais novo para o mais velho, até [`RECENT_SHOWN`].
    pub recent: Vec<Movement>,
}

#[derive(FromRow)]
struct SaleRow {
    seq: i64,
    kind: String,
    trade_date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    fees: Decimal,
    fx_rate: Option<Decimal>,
    label: String,
}

#[derive(FromRow)]
struct BookRow {
    asset_id: Uuid,
    seq: i64,
    kind: String,
    trade_date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    fees: Decimal,
    fx_rate: Option<Decimal>,
}

/// O livro de cada ativo da pessoa, na ordem do livro, numa conexão (ou
/// transação) que quem chama já tem.
pub(super) async fn entries_by_asset_in(
    conn: &mut PgConnection,
    user: Uuid,
) -> Result<HashMap<Uuid, Vec<Entry>>, StoreError> {
    let rows: Vec<BookRow> = sqlx::query_as(
        "select asset_id, seq, kind, trade_date, quantity, unit_price, fees, fx_rate
           from transactions where user_id = $1 and asset_id is not null
          order by trade_date, seq",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    let mut by_asset: HashMap<Uuid, Vec<Entry>> = HashMap::new();
    for row in rows {
        by_asset.entry(row.asset_id).or_default().push(Entry {
            kind: Kind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?,
            trade_date: row.trade_date,
            seq: row.seq,
            quantity: row.quantity,
            unit_price: row.unit_price,
            fees: row.fees,
            fx_rate: row.fx_rate.unwrap_or(Decimal::ONE),
        });
    }
    Ok(by_asset)
}

/// O dia (Brasília) em que o caixa da pessoa começa a contar.
async fn cash_since(conn: &mut PgConnection, user: Uuid) -> Result<Date, StoreError> {
    let (since,): (Date,) = sqlx::query_as("select cash_since from users where id = $1")
        .bind(user)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(StoreError::NotFound)?;
    Ok(since)
}

/// Todo movimento de caixa de `since` até `today`, sem ordem.
async fn load_movements(
    conn: &mut PgConnection,
    user: Uuid,
    since: Date,
    today: Date,
) -> Result<Vec<Movement>, StoreError> {
    let mut movements = Vec::new();

    let income: Vec<(Date, Decimal, String)> = sqlx::query_as(
        "select pay_date, net, coalesce(symbol, display_name) from income_events
          where user_id = $1 and pay_date >= $2 and pay_date <= $3 and net > 0",
    )
    .bind(user)
    .bind(since)
    .bind(today)
    .fetch_all(&mut *conn)
    .await?;
    movements.extend(income.into_iter().map(|(date, net, label)| Movement {
        date,
        kind: MovementKind::Income,
        amount: net,
        label,
    }));

    let sales: Vec<SaleRow> = sqlx::query_as(
        "select seq, kind, trade_date, quantity, unit_price, fees, fx_rate,
                coalesce(symbol, display_name) as label
           from transactions
          where user_id = $1 and kind in ('sell', 'redemption')
            and trade_date >= $2 and trade_date <= $3",
    )
    .bind(user)
    .bind(since)
    .bind(today)
    .fetch_all(&mut *conn)
    .await?;
    for row in sales {
        let entry = Entry {
            kind: Kind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?,
            trade_date: row.trade_date,
            seq: row.seq,
            quantity: row.quantity,
            unit_price: row.unit_price,
            fees: row.fees,
            fx_rate: row.fx_rate.unwrap_or(Decimal::ONE),
        };
        if let Some(amount) = sale_proceeds(&entry).filter(|a| *a > Decimal::ZERO) {
            movements.push(Movement {
                date: row.trade_date,
                kind: MovementKind::Sale,
                amount,
                label: row.label,
            });
        }
    }

    let contributions: Vec<(Date, Decimal)> = sqlx::query_as(
        "select (created_at at time zone 'America/Sao_Paulo')::date, cash_used
           from contributions
          where user_id = $1 and cash_used > 0
            and (created_at at time zone 'America/Sao_Paulo')::date >= $2",
    )
    .bind(user)
    .bind(since)
    .fetch_all(&mut *conn)
    .await?;
    movements.extend(contributions.into_iter().map(|(date, used)| Movement {
        date,
        kind: MovementKind::Contribution,
        amount: used,
        label: "Aporte".to_owned(),
    }));
    Ok(movements)
}

/// O caixa da pessoa agora (`today` é o dia de Brasília).
async fn report(
    conn: &mut PgConnection,
    user: Uuid,
    today: Date,
) -> Result<CashReport, StoreError> {
    let since = cash_since(conn, user).await?;
    let movements = load_movements(conn, user, since, today).await?;
    let state = replay(&movements, since);
    let mut recent = movements;
    // Mais novos primeiro; no mesmo dia, o que entra antes do que sai.
    recent.sort_by_key(|m| (std::cmp::Reverse(m.date), u8::from(!m.kind.is_inflow())));
    recent.truncate(RECENT_SHOWN);
    Ok(CashReport {
        since,
        state,
        recent,
    })
}

/// O texto de um valor em reais para mensagem de erro (sem acento, vírgula).
fn reais(value: Decimal) -> String {
    format!("R$ {:.2}", value.round_dp(2)).replace('.', ",")
}

/// Confere, dentro da transação do aporte, que o caixa cobre `cash_used`.
/// Trava a pessoa: o saldo lido aqui é o saldo do commit.
pub(crate) async fn reserve(
    conn: &mut PgConnection,
    user: Uuid,
    amount: Decimal,
    cash_used: Decimal,
    today: Date,
) -> Result<(), StoreError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(format!("cash:{user}"))
        .execute(&mut *conn)
        .await?;
    let balance = report(conn, user, today).await?.state.balance;
    check_use(amount, cash_used, balance).map_err(|error| {
        StoreError::Invalid(match error {
            CashUseError::AboveAmount => {
                "O caixa usado nao pode passar do valor do aporte.".to_owned()
            }
            CashUseError::AboveBalance => format!(
                "O caixa tem {}; nao da para usar {} dele.",
                reais(balance),
                reais(cash_used)
            ),
        })
    })
}

impl Store {
    /// O caixa da pessoa: saldo, de onde veio e os movimentos mais novos.
    pub async fn cash(&self, user: Uuid, today: Date) -> Result<CashReport, StoreError> {
        let mut conn = self.pool.acquire().await?;
        report(&mut conn, user, today).await
    }

    /// Quanto de provento líquido a pessoa já recebeu (pagamento até `today`)
    /// e se há venda de ativo já removido da carteira — o lucro total precisa
    /// dos dois.
    pub async fn profit_inputs(
        &self,
        user: Uuid,
        today: Date,
    ) -> Result<(Decimal, bool), StoreError> {
        let (income, removed): (Decimal, bool) = sqlx::query_as(
            "select coalesce(sum(net), 0),
                    exists (select 1 from transactions
                             where user_id = $1 and asset_id is null
                               and kind in ('sell', 'redemption', 'amortization'))
               from income_events where user_id = $1 and pay_date <= $2",
        )
        .bind(user)
        .bind(today)
        .fetch_one(&self.pool)
        .await?;
        Ok((income, removed))
    }

    /// O livro de cada ativo da pessoa, na ordem do livro: o que a quantidade
    /// na data com e a estimativa de renda precisam.
    pub async fn entries_by_asset(
        &self,
        user: Uuid,
    ) -> Result<HashMap<Uuid, Vec<Entry>>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        entries_by_asset_in(&mut conn, user).await
    }

    /// O livro de um ativo da pessoa (vazio se o ativo não existe ou não tem lançamento).
    pub async fn asset_entries(&self, user: Uuid, asset: Uuid) -> Result<Vec<Entry>, StoreError> {
        let mut all = self.entries_by_asset(user).await?;
        Ok(all.remove(&asset).unwrap_or_default())
    }
}
