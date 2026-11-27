//! Fechamento diário de um índice da B3 (o IFIX) pelas estatísticas
//! históricas públicas do site da B3, sem login nem chave:
//! `GET /indexStatisticsProxy/IndexCall/GetPortfolioDay/{base64}`, com o JSON
//! `{"index":"IFIX","language":"pt-br","year":"2025"}` em base64 no caminho.
//! É o que a página "Estatísticas históricas" de cada índice
//! (`b3.com.br`, Índices › IFIX) mostra e baixa.
//!
//! Medido com `curl` em 27/11/2026, com o cliente do app (uns 9 KB por ano):
//!
//! - um pedido por **ano**: `results` traz uma linha por dia do mês (`day` de
//!   1 a 31) com `rateValue1` a `rateValue12`, o fechamento de cada mês naquele
//!   dia em texto brasileiro (`"3.117,34"`), ou `null` sem pregão;
//! - `min` e `max` são o menor e o maior fechamento de cada mês (a Prumo não
//!   usa);
//! - ano sem dado (antes de o índice existir) responde
//!   `{"min":null,"max":null,"results":null}`; o IFIX começa em 30/12/2010,
//!   com 1.000 pontos;
//! - o pregão em andamento não aparece: o dia entra depois do fechamento (às
//!   14h de 27/11/2026 o último dia era 26/11; o Ibovespa da mesma tabela bate
//!   com o fechamento do Yahoo).
//!
//! A busca é gentil: um ano por vez, com uma pausa entre eles, e só os anos
//! da janela pedida (a tarefa diária pede do dia seguinte ao último guardado,
//! então no dia a dia é um pedido só).

use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use time::{Date, Month};

use super::{HistoryError, Series};
use crate::domain::income_auto::parse_br_decimal;

const B3_BASE: &str = "https://sistemaswebb3-listados.b3.com.br";

/// A pausa entre dois anos pedidos.
pub const PAUSE: Duration = Duration::from_millis(500);

/// Um índice da B3 e o primeiro ano com dado (antes dele a fonte responde
/// vazio, e não adianta pedir).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct B3Index {
    pub code: &'static str,
    pub first_year: i32,
}

/// O IFIX: base de 1.000 pontos em 30/12/2010.
pub const IFIX: B3Index = B3Index {
    code: "IFIX",
    first_year: 2010,
};

pub struct B3Indexes {
    http: reqwest::Client,
    base: String,
    pause: Duration,
}

#[derive(Deserialize)]
struct PortfolioDay {
    #[serde(default)]
    results: Option<Vec<serde_json::Map<String, serde_json::Value>>>,
}

/// O parâmetro do caminho: o JSON da consulta em base64. Com o código em
/// letras e o ano em dígitos, o base64 nunca tem `/` nem `+`.
pub fn query(index: B3Index, year: i32) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!(
        r#"{{"index":"{}","language":"pt-br","year":"{year}"}}"#,
        index.code
    ))
}

/// Os fechamentos de um ano da tabela da B3, em ordem. Dia que não existe
/// (31/02), valor vazio ou ilegível e valor não positivo ficam de fora.
pub fn parse_year(year: i32, body: &str) -> Result<Series, HistoryError> {
    let parsed: PortfolioDay = serde_json::from_str(body)
        .map_err(|e| HistoryError::Unavailable(format!("resposta da b3 ilegivel: {e}")))?;
    let mut out = Series::new();
    for row in parsed.results.unwrap_or_default() {
        let Some(day) = row
            .get("day")
            .and_then(serde_json::Value::as_u64)
            .and_then(|d| u8::try_from(d).ok())
        else {
            continue;
        };
        for number in 1..=12u8 {
            let Some(text) = row
                .get(&format!("rateValue{number}"))
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let (Some(value), Ok(month)) = (parse_br_decimal(text), Month::try_from(number)) else {
                continue;
            };
            if let Ok(date) = Date::from_calendar_date(year, month, day) {
                if value > rust_decimal::Decimal::ZERO {
                    out.push((date, value));
                }
            }
        }
    }
    out.sort_by_key(|(date, _)| *date);
    out.dedup_by_key(|(date, _)| *date);
    Ok(out)
}

impl B3Indexes {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, B3_BASE)
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            pause: PAUSE,
        }
    }

    /// Troca a pausa entre os anos (o teste não espera).
    pub fn with_pause(mut self, pause: Duration) -> Self {
        self.pause = pause;
        self
    }

    /// Os fechamentos do índice de `from` a `to`, inclusive: um pedido por ano
    /// da janela, a partir do primeiro ano do índice.
    pub async fn levels(
        &self,
        index: B3Index,
        from: Date,
        to: Date,
    ) -> Result<Series, HistoryError> {
        let mut out = Series::new();
        let first = from.year().max(index.first_year);
        for (n, year) in (first..=to.year()).enumerate() {
            if n > 0 && !self.pause.is_zero() {
                tokio::time::sleep(self.pause).await;
            }
            let series = self.year(index, year).await?;
            out.extend(
                series
                    .into_iter()
                    .filter(|(date, _)| *date >= from && *date <= to),
            );
        }
        Ok(out)
    }

    async fn year(&self, index: B3Index, year: i32) -> Result<Series, HistoryError> {
        let unavailable = |e: &dyn std::fmt::Display| HistoryError::Unavailable(e.to_string());
        let url = format!(
            "{}/indexStatisticsProxy/IndexCall/GetPortfolioDay/{}",
            self.base,
            query(index, year)
        );
        let body = self
            .http
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| unavailable(&e))?
            .error_for_status()
            .map_err(|e| unavailable(&e))?
            .text()
            .await
            .map_err(|e| unavailable(&e))?;
        parse_year(year, &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    #[test]
    fn query_is_the_json_the_page_sends() {
        assert_eq!(
            query(IFIX, 2025),
            "eyJpbmRleCI6IklGSVgiLCJsYW5ndWFnZSI6InB0LWJyIiwieWVhciI6IjIwMjUifQ=="
        );
        // Nenhum ano tem `/` ou `+` no caminho.
        for year in 2000..2200 {
            let q = query(IFIX, year);
            assert!(!q.contains('/') && !q.contains('+'), "{year}: {q}");
        }
    }

    #[test]
    fn reads_each_month_column_and_skips_what_is_not_a_close() {
        let body = r#"{"min":{"day":0,"rateValue1":"1,00"},"results":[
            {"day":1,"rateValue1":null,"rateValue2":"3.001,50","rateValue12":"3.660,23"},
            {"day":2,"rateValue1":"3.117,34","rateValue2":"","rateValue3":"abc"},
            {"day":30,"rateValue2":"9.999,99","rateValue3":"0,00"},
            {"rateValue4":"1,00"}
        ]}"#;
        let series = parse_year(2025, body).unwrap();
        assert_eq!(
            series,
            vec![
                (date!(2025 - 01 - 02), dec!(3117.34)),
                (date!(2025 - 02 - 01), dec!(3001.50)),
                (date!(2025 - 12 - 01), dec!(3660.23)),
            ]
        );
    }

    #[test]
    fn year_without_data_is_empty_and_garbage_is_an_error() {
        let empty = r#"{"min":null,"max":null,"results":null}"#;
        assert!(parse_year(1999, empty).unwrap().is_empty());
        assert!(matches!(
            parse_year(2025, "<html>"),
            Err(HistoryError::Unavailable(_))
        ));
    }
}
