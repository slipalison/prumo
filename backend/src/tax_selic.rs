//! A Selic do mês para os juros do DARF pago em atraso (docs/ir.md § 2.11).
//!
//! A Receita soma a Selic de cada mês ("acumulada mensalmente", Lei 9.430/1996,
//! art. 5, par. 3, e art. 61, par. 3). A série certa é a **4390** do SGS do
//! Banco Central (Selic acumulada no mês, % a.m.), a mesma da tabela de
//! acréscimos legais que a Receita publica: conferida em 27/11/2026 contra a
//! tabela de juros para pagamento em julho de 2026 (teste em
//! `domain::tax::late`). A resposta inteira, de janeiro de 2025 até hoje, tem
//! menos de 1 KB.
//!
//! O SGS publica o mês corrente acumulado até a véspera (em 27/11/2026, o de
//! novembro dizia 0,87), e a taxa do último dia útil sai no dia útil seguinte.
//! Por isso só entra mês **fechado** há [`SETTLE_DAYS`] dias: um mês ainda
//! aberto fica de fora e a calculadora diz que falta, em vez de somar uma taxa
//! pela metade.
//!
//! Roda em segundo plano, fora das requisições. Falha de rede não derruba nada:
//! o que já está no banco continua valendo, e a próxima rodada tenta de novo.

use std::sync::Arc;
use std::time::Duration;

use rust_decimal::Decimal;
use time::{Date, Month, OffsetDateTime};

use crate::domain::income::today_in_brazil;
use crate::domain::tax::rules::covered_years;
use crate::history::bcb::BcbClient;
use crate::history::HistoryError;
use crate::store::{Store, StoreError};

/// A série do SGS: Selic acumulada no mês, % a.m.
pub const SERIES: u32 = 4390;

/// De quanto em quanto tempo a rodada confere se falta mês (sem mês novo
/// fechado, não vai à rede).
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Dias depois do fim do mês até ele entrar: cobre fim de semana e feriado
/// entre o último dia útil e a publicação da taxa dele.
pub const SETTLE_DAYS: i64 = 4;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("selic indisponivel: {0}")]
    Source(#[from] HistoryError),
    #[error("banco: {0}")]
    Store(#[from] StoreError),
}

/// O primeiro mês que a conta pode pedir: janeiro do primeiro ano com regra
/// de IR.
pub fn first_month() -> Date {
    let year = covered_years().into_iter().min().unwrap_or(2025);
    Date::from_calendar_date(year, Month::January, 1).unwrap_or(Date::MIN)
}

/// O último mês fechado em `today`: o mês M entra a partir de
/// [`SETTLE_DAYS`] dias depois do último dia dele.
pub fn last_closed_month(today: Date) -> Option<Date> {
    // O mês M fecha quando o último dia dele fica a SETTLE_DAYS de hoje: o
    // mês do dia seguinte a `settled` ainda não fechou; o anterior, sim.
    let settled = today - time::Duration::days(SETTLE_DAYS);
    let open = settled.next_day()?.replace_day(1).ok()?;
    open.previous_day()?.replace_day(1).ok()
}

/// Só os meses fechados em `today`, datados no dia 1º.
pub fn closed_months(series: &[(Date, Decimal)], today: Date) -> Vec<(Date, Decimal)> {
    let Some(last) = last_closed_month(today) else {
        return Vec::new();
    };
    series
        .iter()
        .filter(|(date, _)| date.day() == 1 && *date <= last)
        .copied()
        .collect()
}

pub struct TaxSelicSync {
    store: Store,
    bcb: BcbClient,
}

impl TaxSelicSync {
    pub fn new(store: Store, http: reqwest::Client) -> Self {
        Self {
            store,
            bcb: BcbClient::new(http),
        }
    }

    pub fn with_base(store: Store, http: reqwest::Client, base: &str) -> Self {
        Self {
            store,
            bcb: BcbClient::with_base(http, base),
        }
    }

    /// Uma rodada: se falta algum mês fechado, baixa a série desde o primeiro
    /// ano com regra e grava os meses fechados. Devolve quantos meses gravou.
    pub async fn run_once(&self, today: Date) -> Result<usize, SyncError> {
        let Some(last) = last_closed_month(today) else {
            return Ok(0);
        };
        if self
            .store
            .tax_selic_last_month()
            .await?
            .is_some_and(|stored| stored >= last)
        {
            return Ok(0);
        }
        let series = self.bcb.series(SERIES, first_month(), today).await?;
        let closed = closed_months(&series, today);
        self.store.upsert_tax_selic(&closed).await?;
        Ok(closed.len())
    }

    /// Roda já e depois a cada `every`, até o processo encerrar.
    pub fn spawn(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let today = today_in_brazil(OffsetDateTime::now_utc());
                match self.run_once(today).await {
                    Ok(0) => {}
                    Ok(months) => tracing::info!(months, "selic do mes atualizada"),
                    Err(error) => tracing::warn!(error = %error, "selic do mes nao atualizada"),
                }
                tokio::time::sleep(every).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    #[test]
    fn a_month_enters_only_after_it_settles() {
        // Em 27/11/2026 o último mês fechado é outubro; em 3/12, ainda
        // outubro (novembro precisa de 4 dias); em 4/12, novembro.
        assert_eq!(
            last_closed_month(date!(2026 - 11 - 27)),
            Some(date!(2026 - 10 - 01))
        );
        assert_eq!(
            last_closed_month(date!(2026 - 12 - 03)),
            Some(date!(2026 - 10 - 01))
        );
        assert_eq!(
            last_closed_month(date!(2026 - 12 - 04)),
            Some(date!(2026 - 11 - 01))
        );
        assert_eq!(
            last_closed_month(date!(2027 - 01 - 05)),
            Some(date!(2026 - 12 - 01))
        );
    }

    #[test]
    fn the_open_month_is_left_out() {
        let series = [
            (date!(2026 - 09 - 01), dec!(1.08)),
            (date!(2026 - 10 - 01), dec!(1.07)),
            // O SGS traz o mês corrente acumulado até a véspera.
            (date!(2026 - 11 - 01), dec!(0.87)),
        ];
        assert_eq!(
            closed_months(&series, date!(2026 - 11 - 27)),
            vec![
                (date!(2026 - 09 - 01), dec!(1.08)),
                (date!(2026 - 10 - 01), dec!(1.07))
            ]
        );
        assert_eq!(first_month(), date!(2025 - 01 - 01));
    }
}
