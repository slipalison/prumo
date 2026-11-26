//! O emissor na B3: o código CVM (para achar a companhia nos arquivos da CVM)
//! e o número de ações (para o lucro e o patrimônio por ação).
//!
//! A fonte é a página pública do emissor (`GetListedSupplementCompany`, sem
//! login), a mesma que os proventos automáticos já leem — só que aqui o que
//! interessa é `codeCVM` e `totalNumberShares` (`"4.197.317.998"`, em ações,
//! ON + PN). A consulta e o base64 do caminho são os de `crate::income_auto`.
//!
//! **Por que a B3, e não a CVM, para o número de ações.** O arquivo da CVM com
//! a composição do capital (`*_composicao_capital_*.csv`) não tem coluna de
//! escala, e as companhias preenchem de jeitos diferentes: medido em
//! 26/11/2026, uma traz o total em ações e outra em milhares de ações no mesmo
//! arquivo. A B3 publica em ações, para todas.

use std::time::Duration;

use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::domain::income_auto::{parse_br_decimal, Venue};
use crate::income_auto::{encoded, supplement_query};

const B3_BASE: &str = "https://sistemaswebb3-listados.b3.com.br";

/// O que a B3 diz de um emissor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IssuerInfo {
    /// `None`: a B3 não informa código CVM (ETF cadastrado em Ações Nacionais).
    pub cvm_code: Option<i32>,
    /// Total de ações emitidas (ON + PN), com as em tesouraria.
    pub total_shares: Option<Decimal>,
}

/// O emissor da B3 (`WEGE`) → código CVM e número de ações.
#[async_trait]
pub trait IssuerDirectory: Send + Sync {
    async fn issuer(&self, issuer: &str) -> Result<IssuerInfo, String>;
}

/// As páginas públicas da B3.
pub struct B3Issuers {
    http: reqwest::Client,
    base: String,
}

impl B3Issuers {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, B3_BASE)
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }
}

fn valid_issuer(issuer: &str) -> bool {
    issuer.len() == 4
        && issuer
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Lê a página do emissor: um array com um objeto (ou o objeto direto).
pub fn parse_issuer(value: &serde_json::Value) -> IssuerInfo {
    let item = match value {
        serde_json::Value::Array(items) => items.first(),
        serde_json::Value::Object(_) => Some(value),
        _ => None,
    };
    let text = |name: &str| item.and_then(|i| i.get(name)).and_then(|v| v.as_str());
    IssuerInfo {
        cvm_code: text("codeCVM")
            .and_then(|c| c.trim().parse::<i32>().ok())
            .filter(|c| *c > 0),
        total_shares: text("totalNumberShares")
            .and_then(parse_br_decimal)
            .filter(|s| *s > Decimal::ZERO),
    }
}

#[async_trait]
impl IssuerDirectory for B3Issuers {
    async fn issuer(&self, issuer: &str) -> Result<IssuerInfo, String> {
        if !valid_issuer(issuer) {
            return Ok(IssuerInfo::default());
        }
        let url = format!(
            "{}/listedCompaniesProxy/CompanyCall/GetListedSupplementCompany/{}",
            self.base,
            encoded(&supplement_query(issuer, Venue::Company))
        );
        let value: serde_json::Value = self
            .http
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?
            .error_for_status()
            .map_err(|e| e.without_url().to_string())?
            .json()
            .await
            .map_err(|e| e.without_url().to_string())?;
        Ok(parse_issuer(&value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use serde_json::json;

    #[test]
    fn reads_the_code_and_the_shares() {
        let page = json!([{ "code": "FICT", "codeCVM": "99901", "totalNumberShares": "4.197.317.998", "cashDividends": [] }]);
        assert_eq!(
            parse_issuer(&page),
            IssuerInfo {
                cvm_code: Some(99901),
                total_shares: Some(dec!(4197317998))
            }
        );
        // ETF: a B3 diz código 0; sem ações, sem número.
        let etf = json!([{ "codeCVM": "0", "totalNumberShares": "0" }]);
        assert_eq!(parse_issuer(&etf), IssuerInfo::default());
        assert_eq!(parse_issuer(&json!([])), IssuerInfo::default());
        assert_eq!(parse_issuer(&json!({ "codeCVM": "7" })).cvm_code, Some(7));
        assert_eq!(parse_issuer(&json!("x")), IssuerInfo::default());
    }

    #[test]
    fn only_four_letter_issuers() {
        assert!(valid_issuer("WEGE"));
        assert!(valid_issuer("B3SA"));
        assert!(!valid_issuer("wege"));
        assert!(!valid_issuer("WEGE3"));
    }
}
