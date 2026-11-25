//! Objetivos com prazo (patrimônio e valor numa classe) e o que a projeção lê
//! do livro: as compras dos últimos 12 meses. A meta de renda mensal tem as
//! rotas e consultas de `store/income.rs`; aqui ela nunca aparece.

use rust_decimal::Decimal;
use time::Date;
use uuid::Uuid;

use super::income::ObjectiveRow;
use super::{Objective, Store, StoreError};
use crate::domain::asset_class::AssetClass;
use crate::domain::income::ObjectiveKind;
use crate::domain::objectives::Target;

macro_rules! columns {
    () => {
        "id, kind, asset_class, target, monthly_contribution, expected_return, achieved_at,          created_at"
    };
}

/// Um objetivo novo.
#[derive(Debug, Clone, PartialEq)]
pub struct NewObjective {
    pub target_kind: Target,
    pub asset_class: Option<AssetClass>,
    pub target: Decimal,
    pub monthly_contribution: Option<Decimal>,
    /// Em % ao ano.
    pub expected_return: Option<Decimal>,
}

/// O que o `PATCH` troca; o campo ausente fica como está. Nos opcionais,
/// `Some(None)` apaga a premissa (volta ao padrão).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectiveChanges {
    pub target: Option<Decimal>,
    pub monthly_contribution: Option<Option<Decimal>>,
    pub expected_return: Option<Option<Decimal>>,
}

impl Store {
    /// Os objetivos da pessoa, do mais antigo para o mais novo. A meta de
    /// renda mensal não entra.
    pub async fn objectives(&self, user: Uuid) -> Result<Vec<Objective>, StoreError> {
        let rows: Vec<ObjectiveRow> = sqlx::query_as(concat!(
            "select ",
            columns!(),
            " from objectives
              where user_id = $1 and kind <> 'monthly_income'
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

    pub async fn insert_objective(
        &self,
        user: Uuid,
        new: &NewObjective,
    ) -> Result<Objective, StoreError> {
        let row: ObjectiveRow = sqlx::query_as(concat!(
            "insert into objectives
                 (id, user_id, kind, asset_class, target, monthly_contribution, expected_return)
             values ($1, $2, $3, $4, $5, $6, $7)
             returning ",
            columns!()
        ))
        .bind(Uuid::new_v4())
        .bind(user)
        .bind(ObjectiveKind::from(new.target_kind).as_str())
        .bind(new.asset_class.map(AssetClass::as_str))
        .bind(new.target)
        .bind(new.monthly_contribution)
        .bind(new.expected_return)
        .fetch_one(&self.pool)
        .await?;
        Ok(Objective::try_from(row)?)
    }

    /// Troca os campos dados. Meta nova é conquista nova: mudar o valor da
    /// meta apaga o `achieved_at`. `None` se o objetivo não existe, é de
    /// outra pessoa ou é a meta de renda.
    pub async fn update_objective(
        &self,
        user: Uuid,
        id: Uuid,
        changes: &ObjectiveChanges,
    ) -> Result<Option<Objective>, StoreError> {
        let row: Option<ObjectiveRow> = sqlx::query_as(concat!(
            "update objectives set
                 achieved_at = case when $3::numeric is not null and $3 <> target
                                    then null else achieved_at end,
                 target = coalesce($3, target),
                 monthly_contribution = case when $4 then $5 else monthly_contribution end,
                 expected_return = case when $6 then $7 else expected_return end
              where user_id = $1 and id = $2 and kind <> 'monthly_income'
              returning ",
            columns!()
        ))
        .bind(user)
        .bind(id)
        .bind(changes.target)
        .bind(changes.monthly_contribution.is_some())
        .bind(changes.monthly_contribution.flatten())
        .bind(changes.expected_return.is_some())
        .bind(changes.expected_return.flatten())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Objective::try_from).transpose()?)
    }

    pub async fn delete_objective(&self, user: Uuid, id: Uuid) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "delete from objectives where user_id = $1 and id = $2 and kind <> 'monthly_income'",
        )
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
}
