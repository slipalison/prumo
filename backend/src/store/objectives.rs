//! Objetivos (patrimônio, valor numa classe e a meta de renda mensal), as
//! premissas da projeção de cada um e o que a projeção lê do livro: as
//! compras dos últimos 12 meses. A meta de renda é uma por pessoa e também
//! tem as rotas de `/income/goal` (`store/income.rs`).

use rust_decimal::Decimal;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{decode_error, Store, StoreError};
use crate::domain::asset_class::AssetClass;
use crate::domain::income::ObjectiveKind;
use crate::domain::projection::goal::IncomeMode;

/// As premissas que a pessoa informou; `None` = o padrão da projeção
/// (`docs/projecao.md`). Taxas em % ao ano.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ObjectivePremises {
    /// Reais de hoje por mês.
    pub monthly_contribution: Option<Decimal>,
    /// Rentabilidade real total.
    pub real_return: Option<Decimal>,
    pub inflation: Option<Decimal>,
    pub reinvest_income: Option<bool>,
    /// Crescimento real dos proventos.
    pub income_growth: Option<Decimal>,
    /// Rendimento líquido do dinheiro novo.
    pub income_yield: Option<Decimal>,
    /// Amplitude dos cenários.
    pub scenario_spread: Option<Decimal>,
}

/// Um objetivo da pessoa.
#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub id: Uuid,
    pub kind: ObjectiveKind,
    pub asset_class: Option<AssetClass>,
    pub target: Decimal,
    pub premises: ObjectivePremises,
    /// Só na meta de renda; `None` = média.
    pub income_mode: Option<IncomeMode>,
    pub achieved_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

#[derive(FromRow)]
pub(super) struct ObjectiveRow {
    id: Uuid,
    kind: String,
    asset_class: Option<String>,
    target: Decimal,
    monthly_contribution: Option<Decimal>,
    real_return: Option<Decimal>,
    inflation: Option<Decimal>,
    reinvest_income: Option<bool>,
    income_growth: Option<Decimal>,
    income_yield: Option<Decimal>,
    scenario_spread: Option<Decimal>,
    income_mode: Option<String>,
    achieved_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
}

macro_rules! objective_columns {
    () => {
        "id, kind, asset_class, target, monthly_contribution, real_return, inflation, reinvest_income, income_growth, income_yield, scenario_spread, income_mode, achieved_at, created_at"
    };
}
pub(super) use objective_columns;

impl TryFrom<ObjectiveRow> for Objective {
    type Error = sqlx::Error;

    fn try_from(row: ObjectiveRow) -> Result<Self, Self::Error> {
        let normalize = |value: Option<Decimal>| value.map(|v| v.normalize());
        Ok(Objective {
            id: row.id,
            kind: ObjectiveKind::parse(&row.kind).ok_or_else(|| decode_error(&row.kind))?,
            asset_class: row
                .asset_class
                .map(|c| AssetClass::parse(&c).ok_or_else(|| decode_error(&c)))
                .transpose()?,
            target: row.target.normalize(),
            premises: ObjectivePremises {
                monthly_contribution: normalize(row.monthly_contribution),
                real_return: normalize(row.real_return),
                inflation: normalize(row.inflation),
                reinvest_income: row.reinvest_income,
                income_growth: normalize(row.income_growth),
                income_yield: normalize(row.income_yield),
                scenario_spread: normalize(row.scenario_spread),
            },
            income_mode: row
                .income_mode
                .map(|m| IncomeMode::parse(&m).ok_or_else(|| decode_error(&m)))
                .transpose()?,
            achieved_at: row.achieved_at,
            created_at: row.created_at,
        })
    }
}

/// Um objetivo novo.
#[derive(Debug, Clone, PartialEq)]
pub struct NewObjective {
    pub kind: ObjectiveKind,
    pub asset_class: Option<AssetClass>,
    pub target: Decimal,
    pub premises: ObjectivePremises,
    pub income_mode: Option<IncomeMode>,
}

/// O que o `PATCH` troca; o campo ausente fica como está e `Some(None)` volta
/// ao padrão.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ObjectiveChanges {
    pub target: Option<Decimal>,
    pub monthly_contribution: Option<Option<Decimal>>,
    pub real_return: Option<Option<Decimal>>,
    pub inflation: Option<Option<Decimal>>,
    pub reinvest_income: Option<Option<bool>>,
    pub income_growth: Option<Option<Decimal>>,
    pub income_yield: Option<Option<Decimal>>,
    pub scenario_spread: Option<Option<Decimal>>,
    pub income_mode: Option<Option<IncomeMode>>,
}

impl Store {
    /// Os objetivos da pessoa, do mais antigo para o mais novo, a meta de
    /// renda mensal inclusive.
    pub async fn objectives(&self, user: Uuid) -> Result<Vec<Objective>, StoreError> {
        let rows: Vec<ObjectiveRow> = sqlx::query_as(concat!(
            "select ",
            objective_columns!(),
            " from objectives where user_id = $1 order by created_at, id"
        ))
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(Objective::try_from)
            .collect::<Result<_, _>>()?)
    }

    /// Grava um objetivo. A meta de renda é uma por pessoa: se já existe,
    /// `Duplicate` (o lock é o mesmo de `set_income_goal`, então dois pedidos
    /// ao mesmo tempo não criam duas).
    pub async fn insert_objective(
        &self,
        user: Uuid,
        new: &NewObjective,
    ) -> Result<Objective, StoreError> {
        let mut tx = self.pool.begin().await?;
        if new.kind == ObjectiveKind::MonthlyIncome {
            sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 0))")
                .bind(format!("income_goal:{user}"))
                .execute(&mut *tx)
                .await?;
            let (exists,): (bool,) = sqlx::query_as(
                "select exists (select 1 from objectives where user_id = $1 and kind = 'monthly_income')",
            )
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                return Err(StoreError::Duplicate);
            }
        }
        let p = &new.premises;
        let row: ObjectiveRow = sqlx::query_as(concat!(
            "insert into objectives
                 (id, user_id, kind, asset_class, target, monthly_contribution, real_return,
                  inflation, reinvest_income, income_growth, income_yield, scenario_spread,
                  income_mode)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             returning ",
            objective_columns!()
        ))
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(new.kind.as_str())
        .bind(new.asset_class.map(AssetClass::as_str))
        .bind(new.target)
        .bind(p.monthly_contribution)
        .bind(p.real_return)
        .bind(p.inflation)
        .bind(p.reinvest_income)
        .bind(p.income_growth)
        .bind(p.income_yield)
        .bind(p.scenario_spread)
        .bind(new.income_mode.map(IncomeMode::as_str))
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Objective::try_from(row)?)
    }

    /// Troca os campos dados. Meta nova é conquista nova: mudar o valor da
    /// meta, ou o que a meta de renda mede, apaga o `achieved_at`. `None` se o
    /// objetivo não existe ou é de outra pessoa.
    pub async fn update_objective(
        &self,
        user: Uuid,
        id: Uuid,
        changes: &ObjectiveChanges,
    ) -> Result<Option<Objective>, StoreError> {
        let mode = changes.income_mode.map(|m| m.map(IncomeMode::as_str));
        let row: Option<ObjectiveRow> = sqlx::query_as(concat!(
            "update objectives set
                 achieved_at = case
                     when $3::numeric is not null and $3 <> target then null
                     when $18 and coalesce($19::text, 'average') <> coalesce(income_mode, 'average') then null
                     else achieved_at end,
                 target = coalesce($3, target),
                 monthly_contribution = case when $4 then $5 else monthly_contribution end,
                 real_return = case when $6 then $7 else real_return end,
                 inflation = case when $8 then $9 else inflation end,
                 reinvest_income = case when $10 then $11 else reinvest_income end,
                 income_growth = case when $12 then $13 else income_growth end,
                 income_yield = case when $14 then $15 else income_yield end,
                 scenario_spread = case when $16 then $17 else scenario_spread end,
                 income_mode = case when $18 then $19 else income_mode end
              where user_id = $1 and id = $2
              returning ",
            objective_columns!()
        ))
        .bind(user)
        .bind(id)
        .bind(changes.target)
        .bind(changes.monthly_contribution.is_some())
        .bind(changes.monthly_contribution.flatten())
        .bind(changes.real_return.is_some())
        .bind(changes.real_return.flatten())
        .bind(changes.inflation.is_some())
        .bind(changes.inflation.flatten())
        .bind(changes.reinvest_income.is_some())
        .bind(changes.reinvest_income.flatten())
        .bind(changes.income_growth.is_some())
        .bind(changes.income_growth.flatten())
        .bind(changes.income_yield.is_some())
        .bind(changes.income_yield.flatten())
        .bind(changes.scenario_spread.is_some())
        .bind(changes.scenario_spread.flatten())
        .bind(mode.is_some())
        .bind(mode.flatten())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Objective::try_from).transpose()?)
    }

    pub async fn delete_objective(&self, user: Uuid, id: Uuid) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from objectives where user_id = $1 and id = $2")
            .bind(user)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// As compras do livro desde `since`, em reais, uma linha por lançamento
    /// (valor mais taxas, no câmbio do lançamento), e a data da primeira compra
    /// de todas. `asset_class` filtra pela classe do lançamento; sem ela entra
    /// tudo.
    ///
    /// "Compra" é o lançamento `buy`: saldo inicial, bonificação e ajuste não
    /// são dinheiro que a pessoa pôs na carteira; venda e resgate não abatem.
    pub async fn buys_since(
        &self,
        user: Uuid,
        asset_class: Option<AssetClass>,
        since: Date,
    ) -> Result<(Vec<(Date, Decimal)>, Option<Date>), StoreError> {
        let class = asset_class.map(AssetClass::as_str);
        let buys: Vec<(Date, Decimal)> = sqlx::query_as(
            "select trade_date,
                    (quantity * unit_price + fees) * coalesce(fx_rate, 1)
               from transactions
              where user_id = $1 and kind = 'buy' and trade_date >= $2
                and ($3::text is null or asset_class = $3)
              order by trade_date, seq",
        )
        .bind(user)
        .bind(since)
        .bind(class)
        .fetch_all(&self.pool)
        .await?;
        let (first,): (Option<Date>,) = sqlx::query_as(
            "select min(trade_date) from transactions
              where user_id = $1 and kind = 'buy' and ($2::text is null or asset_class = $2)",
        )
        .bind(user)
        .bind(class)
        .fetch_one(&self.pool)
        .await?;
        Ok((buys, first))
    }

    /// Os objetivos que ainda não foram atingidos, de todos os tipos (a meta
    /// de renda mensal inclusive).
    pub async fn open_objectives(&self, user: Uuid) -> Result<Vec<Objective>, StoreError> {
        let rows: Vec<ObjectiveRow> = sqlx::query_as(concat!(
            "select ",
            objective_columns!(),
            " from objectives
              where user_id = $1 and achieved_at is null
              order by created_at, id"
        ))
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(Objective::try_from)
            .collect::<Result<_, _>>()?)
    }

    /// As pessoas com algum objetivo ainda aberto.
    pub async fn users_with_open_objectives(&self) -> Result<Vec<Uuid>, StoreError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "select distinct user_id from objectives where achieved_at is null order by user_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Grava a data de conquista, só se ainda não houver uma. `true` se esta
    /// chamada foi a que gravou: duas réplicas na mesma hora gravam uma vez só.
    pub async fn achieve_objective(
        &self,
        user: Uuid,
        id: Uuid,
        at: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update objectives set achieved_at = $3
              where user_id = $1 and id = $2 and achieved_at is null",
        )
        .bind(user)
        .bind(id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }
}
