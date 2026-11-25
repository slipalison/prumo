//! Séries temporais do Banco Central (SGS), pela API aberta:
//! `GET /dados/serie/bcdata.sgs.{N}/dados?formato=json&dataInicial=dd/mm/aaaa&dataFinal=dd/mm/aaaa`.
//!
//! Medido com `curl` em 25/11/2026:
//!
//! - resposta `[{"data":"18/11/2026","valor":"0.050788"}, ...]`, com o valor
//!   em texto e ponto decimal;
//! - janela sem dado publicado (o IPCA do mês corrente) responde `404` com
//!   `{"erro":{...,"detail":"... Value(s) not found"}}` — é lista vazia;
//! - série diária com janela de mais de 10 anos responde `406` ("no máximo, 10
//!   anos em séries de periodicidade diária"); por isso o cliente pagina em
//!   janelas de [`WINDOW_DAYS`].

use std::time::Duration;

use rust_decimal::Decimal;
use serde::Deserialize;
use time::macros::format_description;
use time::Date;

use super::{HistoryError, Series};

/// Janela de cada pedido: abaixo dos 10 anos que a API aceita.
pub const WINDOW_DAYS: i64 = 3_600;

pub struct BcbClient {
    http: reqwest::Client,
    base: String,
}

#[derive(Deserialize)]
struct Row {
    data: String,
    valor: String,
}

impl BcbClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, "https://api.bcb.gov.br")
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// A série `code` de `from` a `to`, inclusive, em janelas de até
    /// [`WINDOW_DAYS`] dias.
    pub async fn series(&self, code: u32, from: Date, to: Date) -> Result<Series, HistoryError> {
        let mut out: Series = Vec::new();
        let mut start = from;
        while start <= to {
            let end = to.min(start + time::Duration::days(WINDOW_DAYS - 1));
            out.extend(self.window(code, start, end).await?);
            match end.next_day() {
                Some(next) => start = next,
                None => break,
            }
        }
        out.sort_by_key(|(date, _)| *date);
        out.dedup_by_key(|(date, _)| *date);
        Ok(out)
    }

    async fn window(&self, code: u32, from: Date, to: Date) -> Result<Series, HistoryError> {
        let br = format_description!("[day]/[month]/[year]");
        let unavailable = |e: &dyn std::fmt::Display| HistoryError::Unavailable(e.to_string());
        let url = format!("{}/dados/serie/bcdata.sgs.{code}/dados", self.base);
        let response = self
            .http
            .get(url)
            .query(&[
                ("formato", "json".to_owned()),
                ("dataInicial", from.format(br).map_err(|e| unavailable(&e))?),
                ("dataFinal", to.format(br).map_err(|e| unavailable(&e))?),
            ])
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| unavailable(&e))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        let rows: Vec<Row> = response
            .error_for_status()
            .map_err(|e| unavailable(&e))?
            .json()
            .await
            .map_err(|e| unavailable(&e))?;
        rows.into_iter()
            .map(|row| {
                let date = Date::parse(&row.data, br).map_err(|_| {
                    HistoryError::Unavailable(format!("data invalida: {}", row.data))
                })?;
                let value: Decimal = row.valor.trim().parse().map_err(|_| {
                    HistoryError::Unavailable(format!("valor invalido: {}", row.valor))
                })?;
                Ok((date, value))
            })
            .collect()
    }
}
