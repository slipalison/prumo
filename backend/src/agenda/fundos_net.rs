//! Os documentos dos FIIs no Fundos.NET, o sistema da B3 em que os fundos os
//! entregam (a CVM não publica os eventuais de FII nos dados abertos: lá só há
//! os informes mensal, trimestral e anual). A página pública de consulta, sem
//! login nem chave, busca por CNPJ do fundo:
//!
//! `GET /fnet/publico/pesquisarGerenciadorDocumentosDados?d=0&s=0&l=100&o[0][dataEntrega]=desc&cnpjFundo=<só dígitos>`
//!
//! e devolve `{"data": [...], "recordsTotal": n}`, cada documento com `id`,
//! `categoriaDocumento`, `tipoDocumento`, `especieDocumento`, `dataEntrega`
//! (`26/11/2026 20:53`), `dataReferencia` (`26/11/2026`, `10/2026`...) e
//! `situacaoDocumento` (`A` ativo, `I` substituído). O documento original é
//! `/fnet/publico/exibirDocumento?id=<id>`. Conferido em 27/11/2026.
//!
//! Requisição pelo `http_client()` do app (identificação genérica, nenhum dado
//! da pessoa); nada de contornar bloqueio: falha é falha, e o gravado fica.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use time::{Date, Month};

use crate::domain::agenda::{Document, DocumentCategory, DocumentSource};
use crate::domain::fundamentals::fold;

pub const FUNDOS_NET_BASE: &str = "https://fnet.bmfbovespa.com.br";

/// Quantos documentos uma busca traz (os mais novos): cobre mais de um ano de
/// um FII ativo (informe mensal, relatório, aviso de rendimento por mês).
pub const PAGE_SIZE: u32 = 100;

const TIMEOUT: Duration = Duration::from_secs(20);
/// O maior corpo aceito: 100 documentos são ~100 KB.
const MAX_BODY: usize = 4 * 1024 * 1024;

/// Um documento de um fundo, com o número que o identifica no Fundos.NET.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundDocument {
    pub document_id: String,
    pub document: Document,
}

/// Quem lista os documentos de um fundo (o Fundos.NET, ou um falso nos testes).
#[async_trait]
pub trait FundDocuments: Send + Sync {
    /// Os documentos do fundo de CNPJ `cnpj` (com ou sem máscara).
    async fn documents(&self, cnpj: &str) -> Result<Vec<FundDocument>, String>;
}

pub struct FundosNet {
    http: reqwest::Client,
    base: String,
}

impl FundosNet {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, FUNDOS_NET_BASE)
    }

    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }
}

/// Só os dígitos do CNPJ (`12.345.678/0001-90` → `12345678000190`).
pub fn cnpj_digits(cnpj: &str) -> Option<String> {
    let digits: String = cnpj.chars().filter(char::is_ascii_digit).collect();
    (digits.len() == 14).then_some(digits)
}

#[async_trait]
impl FundDocuments for FundosNet {
    async fn documents(&self, cnpj: &str) -> Result<Vec<FundDocument>, String> {
        let digits = cnpj_digits(cnpj).ok_or_else(|| "cnpj invalido".to_owned())?;
        let url = format!(
            "{}/fnet/publico/pesquisarGerenciadorDocumentosDados",
            self.base
        );
        let page = PAGE_SIZE.to_string();
        let response = self
            .http
            .get(&url)
            .timeout(TIMEOUT)
            .query(&[
                ("d", "0"),
                ("s", "0"),
                ("l", page.as_str()),
                ("o[0][dataEntrega]", "desc"),
                ("cnpjFundo", digits.as_str()),
            ])
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if !response.status().is_success() {
            return Err(format!("status {}", response.status()));
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if body.len() > MAX_BODY {
            return Err("resposta grande demais".to_owned());
        }
        parse_listing(&body)
    }
}

#[derive(Deserialize)]
struct Listing {
    data: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: u64,
    #[serde(default)]
    categoria_documento: Option<String>,
    #[serde(default)]
    tipo_documento: Option<String>,
    #[serde(default)]
    especie_documento: Option<String>,
    #[serde(default)]
    data_entrega: Option<String>,
    #[serde(default)]
    data_referencia: Option<String>,
    #[serde(default)]
    situacao_documento: Option<String>,
    #[serde(default)]
    versao: Option<i32>,
}

/// A categoria que a Agenda guarda, pela categoria e pelo tipo do Fundos.NET.
/// Os informes mensal e anual estruturados e o aviso estruturado de rendimento
/// (o provento já vem da B3) ficam de fora.
pub fn fund_category(category: &str, kind: &str) -> Option<DocumentCategory> {
    let category = fold(category.trim());
    let kind = fold(kind.trim());
    match category.as_str() {
        "fato relevante" => Some(DocumentCategory::MaterialFact),
        "comunicado ao mercado" => Some(DocumentCategory::MarketNotice),
        "aviso aos cotistas" => Some(DocumentCategory::ShareholderNotice),
        "assembleia" => Some(DocumentCategory::Meeting),
        "relatorios" if kind == "relatorio gerencial" => Some(DocumentCategory::ManagementReport),
        "informes periodicos"
            if kind == "demonstracoes financeiras" || kind == "informe trimestral estruturado" =>
        {
            Some(DocumentCategory::Results)
        }
        _ => None,
    }
}

/// `26/11/2026` ou `26/11/2026 20:53` → data; `10/2026` → o primeiro dia do
/// mês; o resto → `None`.
pub fn fund_date(text: &str) -> Option<Date> {
    let day_part = text.trim().split(' ').next()?;
    let parts: Vec<&str> = day_part.split('/').collect();
    let (day, month, year) = match parts.as_slice() {
        [d, m, y] => (d.parse().ok()?, m.parse::<u8>().ok()?, y.parse().ok()?),
        [m, y] => (1, m.parse::<u8>().ok()?, y.parse().ok()?),
        _ => return None,
    };
    if !(1990..=2200).contains(&year) {
        return None;
    }
    Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()
}

fn optional(value: Option<String>, limit: usize) -> Option<String> {
    let value: String = value?.trim().chars().take(limit).collect();
    (!value.is_empty()).then_some(value)
}

/// Lê a resposta da consulta: os documentos ativos nas categorias da Agenda. O
/// link do original é sempre o do Fundos.NET público, que é o que a pessoa
/// abre, venha a consulta de onde vier.
pub fn parse_listing(body: &[u8]) -> Result<Vec<FundDocument>, String> {
    let listing: Listing =
        serde_json::from_slice(body).map_err(|_| "resposta do Fundos.NET ilegivel".to_owned())?;
    let mut list: Vec<FundDocument> = listing
        .data
        .into_iter()
        .filter(|row| row.situacao_documento.as_deref().is_none_or(|s| s == "A"))
        .filter_map(|row| {
            let category = fund_category(
                row.categoria_documento.as_deref().unwrap_or_default(),
                row.tipo_documento.as_deref().unwrap_or_default(),
            )?;
            let delivered_on = fund_date(row.data_entrega.as_deref()?)?;
            let title = [row.tipo_documento, row.especie_documento]
                .into_iter()
                .filter_map(|t| optional(t, 200))
                .collect::<Vec<_>>()
                .join(" · ");
            Some(FundDocument {
                document_id: row.id.to_string(),
                document: Document {
                    source: DocumentSource::FundosNet,
                    category,
                    title: optional(Some(title), 200),
                    subject: None,
                    reference_date: row.data_referencia.as_deref().and_then(fund_date),
                    delivered_on,
                    version: row.versao.unwrap_or(1),
                    url: Some(format!(
                        "{FUNDOS_NET_BASE}/fnet/publico/exibirDocumento?id={}",
                        row.id
                    )),
                    filing: None,
                },
            })
        })
        .collect();
    list.sort_by(|a, b| {
        (a.document.delivered_on, &a.document_id).cmp(&(b.document.delivered_on, &b.document_id))
    });
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    /// A resposta no formato real, com fundo, ids e textos fictícios.
    const LISTING: &str = r#"{"draw":1,"recordsFiltered":6,"recordsTotal":6,"data":[
        {"id":9000006,"categoriaDocumento":"Fato Relevante","tipoDocumento":"","especieDocumento":"","dataEntrega":"26/11/2026 20:53","dataReferencia":"26/11/2026","situacaoDocumento":"A","versao":1,"descricaoFundo":"FUNDO FICTICIO FII"},
        {"id":9000005,"categoriaDocumento":"Relatórios","tipoDocumento":"Relatório Gerencial","especieDocumento":"","dataEntrega":"13/11/2026 14:03","dataReferencia":"10/2026","situacaoDocumento":"A","versao":1},
        {"id":9000004,"categoriaDocumento":"Aviso aos Cotistas - Estruturado","tipoDocumento":"Rendimentos e Amortizações","especieDocumento":"","dataEntrega":"30/10/2026 18:00","dataReferencia":"30/10/2026","situacaoDocumento":"A","versao":1},
        {"id":9000003,"categoriaDocumento":"Assembleia","tipoDocumento":"AGO","especieDocumento":"Edital de Convocação","dataEntrega":"11/11/2026 17:36","dataReferencia":"14/12/2026 18:00","situacaoDocumento":"A","versao":1},
        {"id":9000002,"categoriaDocumento":"Informes Periódicos","tipoDocumento":"Informe Trimestral Estruturado","especieDocumento":"","dataEntrega":"13/11/2026 11:13","dataReferencia":"30/09/2026","situacaoDocumento":"A","versao":1},
        {"id":9000001,"categoriaDocumento":"Fato Relevante","tipoDocumento":"","especieDocumento":"","dataEntrega":"01/11/2026 10:00","dataReferencia":"01/11/2026","situacaoDocumento":"I","versao":1}
    ]}"#;

    #[test]
    fn the_listing_keeps_the_active_documents_of_the_agenda_categories() {
        let found = parse_listing(LISTING.as_bytes()).unwrap();
        let ids: Vec<&str> = found.iter().map(|d| d.document_id.as_str()).collect();
        assert_eq!(ids, vec!["9000003", "9000002", "9000005", "9000006"]);
        let meeting = &found[0].document;
        assert_eq!(meeting.category, DocumentCategory::Meeting);
        assert_eq!(meeting.title.as_deref(), Some("AGO · Edital de Convocação"));
        assert_eq!(meeting.reference_date, Some(date!(2026 - 12 - 14)));
        assert_eq!(found[1].document.category, DocumentCategory::Results);
        assert_eq!(
            found[2].document.category,
            DocumentCategory::ManagementReport
        );
        assert_eq!(
            found[2].document.reference_date,
            Some(date!(2026 - 10 - 01))
        );
        let fact = &found[3].document;
        assert_eq!(fact.delivered_on, date!(2026 - 11 - 26));
        assert_eq!(
            fact.url.as_deref(),
            Some("https://fnet.bmfbovespa.com.br/fnet/publico/exibirDocumento?id=9000006")
        );
        assert_eq!(fact.title, None);
        assert!(parse_listing(b"<html>manutencao</html>").is_err());
    }

    #[test]
    fn dates_and_cnpj() {
        assert_eq!(fund_date("26/11/2026 20:53"), Some(date!(2026 - 11 - 26)));
        assert_eq!(fund_date("10/2026"), Some(date!(2026 - 10 - 01)));
        assert_eq!(fund_date("2026"), None);
        assert_eq!(fund_date("31/02/2026"), None);
        assert_eq!(
            cnpj_digits("12.345.678/0001-90").as_deref(),
            Some("12345678000190")
        );
        assert_eq!(cnpj_digits("123"), None);
        assert_eq!(
            fund_category("Informes Periódicos", "Informe Mensal Estruturado "),
            None
        );
        assert_eq!(
            fund_category("Informes Periódicos", "Demonstrações Financeiras"),
            Some(DocumentCategory::Results)
        );
    }
}
