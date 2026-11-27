//! O emissor na B3: o código CVM (para achar a companhia nos arquivos da CVM)
//! e o número de ações (para o lucro e o patrimônio por ação).
//!
//! A fonte é a página pública do emissor (`GetListedSupplementCompany`, sem
//! login), a mesma que os proventos automáticos já leem — só que aqui o que
//! interessa é `codeCVM` e `totalNumberShares` (`"4.197.317.998"`, em ações,
//! ON + PN). A consulta e o base64 do caminho são os de `crate::income_auto`.
//!
//! **O CNPJ de um FII** sai da página pública de fundos listados da B3, a que
//! o site dela usa (`fundsListedPage`, sem login nem chave; medido em
//! 27/11/2026): `fundsListedProxy/Search/GetListFunds` com
//! `{"typeFund":"FII","keyword":"<sigla>"}` acha o fundo (`id`, `acronym`), e
//! `GetDetailFund` com `{"idFNET":"<id>","idCEM":"<sigla>","typeFund":"FII"}`
//! dá o `cnpj` (14 dígitos) e o `fundName`. É a fonte do ticker: o informe da
//! CVM não traz o código de negociação (`domain::fund_issuer`).
//!
//! **Por que a B3, e não a CVM, para o número de ações.** O arquivo da CVM com
//! a composição do capital (`*_composicao_capital_*.csv`) não tem coluna de
//! escala, e as companhias preenchem de jeitos diferentes: medido em
//! 26/11/2026, uma traz o total em ações e outra em milhares de ações no mesmo
//! arquivo. A B3 publica em ações, para todas.

use std::time::Duration;

use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::domain::fund_issuer::format_cnpj;
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

/// Um FII na lista de fundos da B3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedFund {
    /// Como a CVM escreve (`00.000.000/0001-91`).
    pub cnpj: String,
    pub name: String,
}

/// O emissor da B3 (`WEGE`) → código CVM e número de ações; e o FII (`KNRI`)
/// → CNPJ.
#[async_trait]
pub trait IssuerDirectory: Send + Sync {
    async fn issuer(&self, issuer: &str) -> Result<IssuerInfo, String>;

    /// O fundo da sigla na lista de FIIs da B3. `Ok(None)`: a B3 não lista o
    /// fundo. Quem não consulta a B3 (os testes que não tratam disso) responde
    /// `Ok(None)`.
    async fn fund(&self, _issuer: &str) -> Result<Option<ListedFund>, String> {
        Ok(None)
    }
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

/// O `id` do fundo da sigla na lista de FIIs (a busca por palavra traz mais de
/// um; vale o de `acronym` igual à sigla).
pub fn parse_fund_list(value: &serde_json::Value, issuer: &str) -> Option<i64> {
    value
        .get("results")?
        .as_array()?
        .iter()
        .find(|item| {
            item.get("acronym")
                .and_then(|a| a.as_str())
                .is_some_and(|a| a.trim().eq_ignore_ascii_case(issuer))
        })?
        .get("id")?
        .as_i64()
}

/// O CNPJ e o nome do detalhe de um fundo; `None` sem CNPJ de 14 dígitos.
pub fn parse_fund_detail(value: &serde_json::Value) -> Option<ListedFund> {
    let text = |name: &str| value.get(name).and_then(|v| v.as_str()).map(str::trim);
    let cnpj = format_cnpj(text("cnpj")?)?;
    let name = text("fundName")
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();
    Some(ListedFund { cnpj, name })
}

impl B3Issuers {
    async fn get_json(&self, url: String) -> Result<serde_json::Value, String> {
        let response = self
            .http
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?
            .error_for_status()
            .map_err(|e| e.without_url().to_string())?;
        // O detalhe de um fundo que não existe volta 200 com o corpo vazio.
        let body = response
            .text()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if body.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl IssuerDirectory for B3Issuers {
    async fn fund(&self, issuer: &str) -> Result<Option<ListedFund>, String> {
        if !valid_issuer(issuer) {
            return Ok(None);
        }
        let list = format!(
            "{}/fundsListedProxy/Search/GetListFunds/{}",
            self.base,
            encoded(&format!(
                r#"{{"language":"pt-br","typeFund":"FII","pageNumber":1,"pageSize":20,"keyword":"{issuer}"}}"#
            ))
        );
        let Some(id) = parse_fund_list(&self.get_json(list).await?, issuer) else {
            return Ok(None);
        };
        let detail = format!(
            "{}/fundsListedProxy/Search/GetDetailFund/{}",
            self.base,
            encoded(&format!(
                r#"{{"language":"pt-br","idFNET":"{id}","idCEM":"{issuer}","typeFund":"FII"}}"#
            ))
        );
        Ok(parse_fund_detail(&self.get_json(detail).await?))
    }

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
    fn finds_the_fund_by_its_acronym_and_reads_the_cnpj() {
        // Como a B3 respondeu em 27/11/2026, com nomes e números fictícios.
        let list = json!({
            "page": { "pageNumber": 1, "pageSize": 20, "totalRecords": 2, "totalPages": 1 },
            "results": [
                { "id": 7, "acronym": "FICX", "fundName": "OUTRO FUNDO FICTICIO FII", "tradingName": "FII FICX" },
                { "id": 841, "acronym": "FICT", "fundName": "FUNDO FICTICIO FII", "tradingName": "FII FICT" }
            ]
        });
        assert_eq!(parse_fund_list(&list, "FICT"), Some(841));
        assert_eq!(parse_fund_list(&list, "NADA"), None);
        assert_eq!(parse_fund_list(&json!({ "results": [] }), "FICT"), None);
        assert_eq!(parse_fund_list(&serde_json::Value::Null, "FICT"), None);
        let detail = json!({ "idFNET": 841, "acronym": "FICT", "tradingCode": "FICT11", "cnpj": "11111111000111", "fundName": "FUNDO FICTICIO FII" });
        assert_eq!(
            parse_fund_detail(&detail),
            Some(ListedFund {
                cnpj: "11.111.111/0001-11".into(),
                name: "FUNDO FICTICIO FII".into()
            })
        );
        assert_eq!(parse_fund_detail(&json!({ "cnpj": "123" })), None);
        assert_eq!(parse_fund_detail(&serde_json::Value::Null), None);
    }

    #[test]
    fn only_four_letter_issuers() {
        assert!(valid_issuer("WEGE"));
        assert!(valid_issuer("B3SA"));
        assert!(!valid_issuer("wege"));
        assert!(!valid_issuer("WEGE3"));
    }
}
