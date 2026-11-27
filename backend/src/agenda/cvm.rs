//! Os arquivos da CVM que a Agenda lê, no mesmo formato dos fundamentos
//! (`fundamentals::cvm`: CSV com `;`, ISO-8859-1, sem aspas como delimitador):
//!
//! - `CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_AAAA.zip`: os documentos
//!   periódicos e eventuais entregues no ano (um CSV, ~17 MB aberto, ~2 MB
//!   compactado). Colunas `CNPJ_Companhia;Nome_Companhia;Codigo_CVM;
//!   Data_Referencia;Categoria;Tipo;Especie;Assunto;Data_Entrega;
//!   Tipo_Apresentacao;Protocolo_Entrega;Versao;Link_Download` (conferido em
//!   27/11/2026).
//! - o índice de `dfp_cia_aberta_AAAA.zip` e de `itr_cia_aberta_AAAA.zip`
//!   (`dfp_cia_aberta_AAAA.csv`, a primeira entrada do zip): cada DFP ou ITR
//!   entregue, com `DT_RECEB` (a data de entrega) e `LINK_DOC`. Só a primeira
//!   entrada é lida: o leitor para nela e o resto do arquivo (30 MB do ITR) não
//!   é baixado.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use time::Date;

use crate::domain::agenda::{Document, DocumentCategory, DocumentSource};
use crate::domain::fundamentals::{fold, FilingKind};
use crate::fundamentals::cvm::{
    csv_reader, date, entry_name, format_error, integer, text, Columns, MAX_CSV,
};
use crate::fundamentals::download::{FetchError, Limited};

/// O endereço do arquivo IPE de um ano, a partir da raiz do site.
pub fn ipe_path(year: i32) -> String {
    format!("/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_{year}.zip")
}

/// O único endereço aceito para o documento original: o RAD da CVM.
const RAD: &str = "https://www.rad.cvm.gov.br/";

/// Um documento de uma companhia, com o número que o identifica na CVM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyDocument {
    pub cvm_code: i32,
    /// O `numSequencia` do link (único por documento entregue).
    pub document_id: String,
    pub document: Document,
}

/// A categoria do IPE que a Agenda guarda. As outras (ata de reunião da
/// administração, valores mobiliários negociados, políticas, estatuto...)
/// ficam de fora; o resultado vem do ITR e da DFP.
pub fn ipe_category(category: &str) -> Option<DocumentCategory> {
    match fold(category.trim()).as_str() {
        "fato relevante" => Some(DocumentCategory::MaterialFact),
        "comunicado ao mercado" => Some(DocumentCategory::MarketNotice),
        "aviso aos acionistas" => Some(DocumentCategory::ShareholderNotice),
        "assembleia" => Some(DocumentCategory::Meeting),
        _ => None,
    }
}

/// O link do documento, só no RAD da CVM e sempre em https (o índice do ITR
/// e da DFP traz `http://`).
pub fn rad_link(link: &str) -> Option<String> {
    let link = link.trim();
    let secure = match link.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => link.to_owned(),
    };
    let clean = secure.starts_with(RAD)
        && secure.len() <= 300
        && secure
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/:.?=&_-%".contains(c));
    clean.then_some(secure)
}

/// O valor de um parâmetro da consulta de um link (`numSequencia=1063774`).
fn query_param<'a>(link: &'a str, name: &str) -> Option<&'a str> {
    let query = link.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name && !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()))
            .then_some(value)
    })
}

fn optional(value: String, limit: usize) -> Option<String> {
    let value: String = value.chars().take(limit).collect();
    (!value.is_empty()).then_some(value)
}

/// Lê o IPE de `year` e devolve os documentos das companhias `codes` nas
/// categorias que a Agenda guarda. Linha repetida (o arquivo repete algumas)
/// conta uma vez.
pub fn parse_ipe<R: Read>(
    mut reader: R,
    year: i32,
    codes: &HashSet<i32>,
) -> Result<Vec<CompanyDocument>, FetchError> {
    let wanted_name = format!("ipe_cia_aberta_{year}.csv");
    let mut seen = false;
    let mut found: HashMap<String, CompanyDocument> = HashMap::new();
    while let Some(entry) =
        zip::read::read_zipfile_from_stream(&mut reader).map_err(format_error)?
    {
        if entry_name(&entry)? != wanted_name {
            continue;
        }
        seen = true;
        let mut csv = csv_reader(Limited::new(entry, MAX_CSV));
        let columns = Columns::read(&mut csv)?;
        let cvm = columns.index("Codigo_CVM")?;
        let category = columns.index("Categoria")?;
        let kind = columns.index("Tipo")?;
        let species = columns.index("Especie")?;
        let subject = columns.index("Assunto")?;
        let reference = columns.index("Data_Referencia")?;
        let delivered = columns.index("Data_Entrega")?;
        let version = columns.index("Versao")?;
        let link = columns.index("Link_Download")?;
        let mut record = csv::ByteRecord::new();
        while csv.read_byte_record(&mut record).map_err(format_error)? {
            let Some(cvm_code) = integer(&record, cvm).filter(|c| codes.contains(c)) else {
                continue;
            };
            let Some(category) = ipe_category(&text(&record, category)) else {
                continue;
            };
            let (Some(delivered_on), Some(url)) =
                (date(&record, delivered), rad_link(&text(&record, link)))
            else {
                continue;
            };
            let Some(document_id) = query_param(&url, "numSequencia").map(str::to_owned) else {
                continue;
            };
            let title = [text(&record, kind), text(&record, species)]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            found.insert(
                document_id.clone(),
                CompanyDocument {
                    cvm_code,
                    document_id,
                    document: Document {
                        source: DocumentSource::CvmIpe,
                        category,
                        title: optional(title, 200),
                        subject: optional(text(&record, subject), 400),
                        reference_date: date(&record, reference),
                        delivered_on,
                        version: integer(&record, version).unwrap_or(1),
                        url: Some(url),
                        filing: None,
                    },
                },
            );
        }
    }
    if !seen {
        return Err(format_error(format!("sem {wanted_name}")));
    }
    let mut list: Vec<CompanyDocument> = found.into_values().collect();
    list.sort_by(|a, b| {
        (a.cvm_code, a.document.delivered_on, &a.document_id).cmp(&(
            b.cvm_code,
            b.document.delivered_on,
            &b.document_id,
        ))
    });
    Ok(list)
}

/// A entrega de um ITR ou de uma DFP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilingReceipt {
    pub cvm_code: i32,
    pub kind: FilingKind,
    pub reference_date: Date,
    pub version: i32,
    pub received_on: Date,
    pub url: Option<String>,
}

/// O endereço do arquivo de DFP ou ITR (o mesmo dos fundamentos).
pub fn filing_path(kind: FilingKind, year: i32) -> String {
    crate::fundamentals::cvm::path(kind, year)
}

/// Lê o índice do `.zip` de DFP ou ITR de `year` (a primeira entrada) e para:
/// a entrega de cada documento das companhias `codes`, na versão mais alta.
pub fn parse_receipts<R: Read>(
    mut reader: R,
    kind: FilingKind,
    year: i32,
    codes: &HashSet<i32>,
) -> Result<Vec<FilingReceipt>, FetchError> {
    let prefix = match kind {
        FilingKind::Annual => "dfp",
        FilingKind::Quarterly => "itr",
    };
    let wanted_name = format!("{prefix}_cia_aberta_{year}.csv");
    while let Some(entry) =
        zip::read::read_zipfile_from_stream(&mut reader).map_err(format_error)?
    {
        if entry_name(&entry)? != wanted_name {
            continue;
        }
        let mut csv = csv_reader(Limited::new(entry, MAX_CSV));
        let columns = Columns::read(&mut csv)?;
        let cvm = columns.index("CD_CVM")?;
        let reference = columns.index("DT_REFER")?;
        let version = columns.index("VERSAO")?;
        let received = columns.index("DT_RECEB")?;
        let link = columns.optional("LINK_DOC");
        let mut best: HashMap<(i32, Date), FilingReceipt> = HashMap::new();
        let mut record = csv::ByteRecord::new();
        while csv.read_byte_record(&mut record).map_err(format_error)? {
            let Some(cvm_code) = integer(&record, cvm).filter(|c| codes.contains(c)) else {
                continue;
            };
            let (Some(reference_date), Some(received_on), Some(row_version)) = (
                date(&record, reference),
                date(&record, received),
                integer(&record, version),
            ) else {
                continue;
            };
            if best
                .get(&(cvm_code, reference_date))
                .is_some_and(|r| r.version >= row_version)
            {
                continue;
            }
            best.insert(
                (cvm_code, reference_date),
                FilingReceipt {
                    cvm_code,
                    kind,
                    reference_date,
                    version: row_version,
                    received_on,
                    url: link.and_then(|i| rad_link(&text(&record, i))),
                },
            );
        }
        let mut list: Vec<FilingReceipt> = best.into_values().collect();
        list.sort_by_key(|r| (r.cvm_code, r.reference_date));
        // O resto do arquivo (as demonstrações) não interessa: parar aqui
        // corta o download.
        return Ok(list);
    }
    Err(format_error(format!("sem {wanted_name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fundamentals::cvm::fixtures::zip;
    use time::macros::date;

    const IPE_HEADER: &str = "CNPJ_Companhia;Nome_Companhia;Codigo_CVM;Data_Referencia;Categoria;Tipo;Especie;Assunto;Data_Entrega;Tipo_Apresentacao;Protocolo_Entrega;Versao;Link_Download";

    fn ipe_row(
        cvm: i32,
        category: &str,
        kind: &str,
        subject: &str,
        delivered: &str,
        seq: u32,
    ) -> String {
        format!(
            "00.000.000/0001-00;CIA FICTICIA SA;{cvm};{delivered};{category};{kind};;{subject};{delivered};AP - Apresentação;0{cvm}IPE0101;1;https://www.rad.cvm.gov.br/ENET/frmDownloadDocumento.aspx?Tela=ext&descTipo=IPE&CodigoInstituicao=1&numProtocolo=9{seq}&numSequencia={seq}&numVersao=1"
        )
    }

    fn codes(list: &[i32]) -> HashSet<i32> {
        list.iter().copied().collect()
    }

    #[test]
    fn the_ipe_keeps_only_the_portfolio_and_the_relevant_categories() {
        let rows = [
            ipe_row(
                99901,
                "Fato Relevante",
                "",
                "Aquisição de controle ficticia",
                "2026-11-20",
                101,
            ),
            ipe_row(
                99901,
                "Comunicado ao Mercado",
                "Outros Comunicados Não Considerados Fatos Relevantes",
                "",
                "2026-11-21",
                102,
            ),
            // A mesma linha duas vezes (o arquivo repete).
            ipe_row(
                99901,
                "Comunicado ao Mercado",
                "Outros Comunicados Não Considerados Fatos Relevantes",
                "",
                "2026-11-21",
                102,
            ),
            ipe_row(
                99901,
                "Aviso aos Acionistas",
                "Outros avisos",
                "Pagamento de dividendos",
                "2026-11-22",
                103,
            ),
            ipe_row(
                99901,
                "Assembleia",
                "AGE",
                "Reforma do estatuto",
                "2026-11-23",
                104,
            ),
            ipe_row(
                99901,
                "Reunião da Administração",
                "",
                "Ata",
                "2026-11-23",
                105,
            ),
            ipe_row(
                99902,
                "Fato Relevante",
                "",
                "De outra companhia",
                "2026-11-20",
                106,
            ),
        ];
        let csv = std::iter::once(IPE_HEADER.to_owned())
            .chain(rows)
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = zip(&[("ipe_cia_aberta_2026.csv", csv)]);
        let found = parse_ipe(bytes.as_slice(), 2026, &codes(&[99901])).unwrap();
        let categories: Vec<DocumentCategory> = found.iter().map(|d| d.document.category).collect();
        assert_eq!(
            categories,
            vec![
                DocumentCategory::MaterialFact,
                DocumentCategory::MarketNotice,
                DocumentCategory::ShareholderNotice,
                DocumentCategory::Meeting,
            ]
        );
        let fact = &found[0];
        assert_eq!(fact.document_id, "101");
        assert_eq!(fact.document.delivered_on, date!(2026 - 11 - 20));
        assert_eq!(
            fact.document.subject.as_deref(),
            Some("Aquisição de controle ficticia")
        );
        assert_eq!(fact.document.title, None);
        assert!(fact
            .document
            .url
            .as_deref()
            .is_some_and(|u| u.starts_with(RAD)));
        assert_eq!(
            found[1].document.title.as_deref(),
            Some("Outros Comunicados Não Considerados Fatos Relevantes")
        );
        assert!(parse_ipe(bytes.as_slice(), 2026, &HashSet::new())
            .unwrap()
            .is_empty());
        // Sem o CSV do ano, ou com coluna faltando: erro, não "nada".
        assert!(parse_ipe(bytes.as_slice(), 2025, &codes(&[99901])).is_err());
        let broken = zip(&[(
            "ipe_cia_aberta_2026.csv",
            "Codigo_CVM;Categoria\n1;x".to_owned(),
        )]);
        assert!(parse_ipe(broken.as_slice(), 2026, &codes(&[1])).is_err());
    }

    #[test]
    fn only_rad_links_are_kept() {
        assert_eq!(
            rad_link("http://www.rad.cvm.gov.br/ENETCONSULTA/frmDownloadDocumento.aspx?CodigoInstituicao=1&NumeroSequencialDocumento=1"),
            Some("https://www.rad.cvm.gov.br/ENETCONSULTA/frmDownloadDocumento.aspx?CodigoInstituicao=1&NumeroSequencialDocumento=1".to_owned())
        );
        assert_eq!(rad_link("javascript:alert(1)"), None);
        assert_eq!(rad_link("https://exemplo.com/x"), None);
        assert_eq!(rad_link("https://www.rad.cvm.gov.br/x\"onclick=1"), None);
        assert_eq!(
            query_param("https://x/y?a=1&numSequencia=77", "numSequencia"),
            Some("77")
        );
        assert_eq!(
            query_param("https://x/y?numSequencia=7a", "numSequencia"),
            None
        );
    }

    #[test]
    fn the_receipts_come_from_the_first_entry_in_the_highest_version() {
        let index = [
            "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;CATEG_DOC;ID_DOC;DT_RECEB;LINK_DOC",
            "00.000.000/0001-00;2026-09-30;1;CIA FICTICIA SA;099901;ITR;1;2026-11-10;http://www.rad.cvm.gov.br/ENETCONSULTA/frmDownloadDocumento.aspx?CodigoInstituicao=1&NumeroSequencialDocumento=1",
            "00.000.000/0001-00;2026-09-30;2;CIA FICTICIA SA;099901;ITR;2;2026-11-14;http://www.rad.cvm.gov.br/ENETCONSULTA/frmDownloadDocumento.aspx?CodigoInstituicao=1&NumeroSequencialDocumento=2",
            "00.000.000/0001-00;2026-06-30;1;CIA FICTICIA SA;099901;ITR;3;2026-08-12;",
            "00.000.000/0001-00;2026-06-30;1;OUTRA FICTICIA SA;099902;ITR;4;2026-08-12;",
        ]
        .join("\n");
        let bytes = zip(&[
            ("itr_cia_aberta_2026.csv", index),
            ("itr_cia_aberta_DRE_con_2026.csv", "nao lido".to_owned()),
        ]);
        let found = parse_receipts(
            bytes.as_slice(),
            FilingKind::Quarterly,
            2026,
            &codes(&[99901]),
        )
        .unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].reference_date, date!(2026 - 06 - 30));
        assert_eq!(found[0].url, None);
        assert_eq!(found[1].version, 2);
        assert_eq!(found[1].received_on, date!(2026 - 11 - 14));
        assert!(found[1].url.as_deref().is_some_and(|u| u.starts_with(RAD)));
        assert!(parse_receipts(bytes.as_slice(), FilingKind::Annual, 2026, &codes(&[1])).is_err());
        assert_eq!(
            filing_path(FilingKind::Annual, 2025),
            "/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_2025.zip"
        );
        assert_eq!(
            ipe_path(2026),
            "/dados/CIA_ABERTA/DOC/IPE/DADOS/ipe_cia_aberta_2026.zip"
        );
    }
}
