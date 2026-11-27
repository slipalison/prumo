//! As taxas de mercado do rendimento-alvo dos FIIs: o juro real que a rodada
//! do Tesouro escolhe (`treasury_real_rate`, uma linha só) e o CDI de 12 meses,
//! que sai do `index_history` já guardado pelo Desempenho.

use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime};

use super::{decode_error, Store, StoreError};
use crate::domain::market_rates::{cdi_12m, Cdi12m, RealRate, CDI_LOOKBACK_DAYS};
use crate::domain::treasury::Bond;
use crate::history::IndexCode;

impl Store {
    /// Grava o juro real de referência (troca o anterior).
    pub async fn save_real_rate(
        &self,
        rate: &RealRate,
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into treasury_real_rate (singleton, bond_symbol, rate, base_date, updated_at)
             values (true, $1, $2, $3, $4)
             on conflict (singleton) do update set
                bond_symbol = excluded.bond_symbol,
                rate = excluded.rate,
                base_date = excluded.base_date,
                updated_at = excluded.updated_at",
        )
        .bind(rate.bond.symbol())
        .bind(rate.percent)
        .bind(rate.base_date)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// O juro real de referência guardado; `None` antes da primeira rodada do
    /// Tesouro que trouxe um título IPCA+.
    pub async fn real_rate(&self) -> Result<Option<RealRate>, StoreError> {
        let row: Option<(String, Decimal, Date)> =
            sqlx::query_as("select bond_symbol, rate, base_date from treasury_real_rate")
                .fetch_optional(&self.pool)
                .await?;
        let rate = row
            .map(|(symbol, rate, base_date)| {
                let bond = Bond::from_symbol(&symbol).ok_or_else(|| decode_error(&symbol))?;
                Ok::<_, sqlx::Error>(RealRate {
                    bond,
                    percent: rate.normalize(),
                    base_date,
                })
            })
            .transpose()?;
        Ok(rate)
    }

    /// O CDI acumulado nos 12 meses que terminam na última taxa guardada até
    /// `today`; `None` se o guardado não cobre a janela.
    pub async fn cdi_12m(&self, today: Date) -> Result<Option<Cdi12m>, StoreError> {
        let series = self
            .index_series(
                IndexCode::Cdi,
                today - Duration::days(CDI_LOOKBACK_DAYS),
                today,
            )
            .await?;
        Ok(cdi_12m(&series))
    }
}
