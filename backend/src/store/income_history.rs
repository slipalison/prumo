//! O patrimônio médio de cada mês, das fotos diárias da carteira: a base do DY
//! do mapa de calor da Renda (`domain::income_history::heatmap`).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use uuid::Uuid;

use super::{Store, StoreError};
use crate::domain::income::YearMonth;

impl Store {
    /// A média do patrimônio das fotos de cada mês da pessoa (uma foto por dia
    /// corrido, desde o primeiro lançamento do livro). Mês sem foto não aparece.
    pub async fn monthly_market_values(
        &self,
        user: Uuid,
    ) -> Result<BTreeMap<YearMonth, Decimal>, StoreError> {
        let rows: Vec<(i32, i32, Decimal)> = sqlx::query_as(
            "select extract(year from date)::int, extract(month from date)::int,
                    round(avg(market_value), 6)
               from portfolio_snapshots
              where user_id = $1
              group by 1, 2",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(year, month, value)| {
                let month = u8::try_from(month).ok()?;
                Some((YearMonth { year, month }, value.normalize()))
            })
            .collect())
    }
}
