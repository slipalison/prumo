//! Classificação Setorial da B3: a planilha pública que a própria página da B3
//! baixa (`Classificação setorial` em Empresas Listadas).
//!
//! A página é um aplicativo que chama a API pública
//! `listedCompaniesProxy/CompanyCall/GetDownloadIndustryClassification/<base64
//! do filtro>` e recebe um `.xlsx`. O endereço antigo (`bvmf.bmfbovespa.com.br/
//! InstDados/InformacoesEmpresas/ClassifSetorial.zip`) ainda responde, mas
//! entrega uma planilha de 2022; por isso não é usado.
//!
//! A planilha tem uma aba só, com colunas B a G:
//!
//! | B | C | D | E | F | G |
//! |---|---|---|---|---|---|
//! | Setor | Subsetor | Segmento | Nome de pregão | Código | Segmento de negociação |
//!
//! Setor, subsetor e segmento só aparecem na primeira linha de cada grupo (as
//! de baixo ficam em branco e valem o de cima), e a linha de cabeçalho se
//! repete a cada setor. O código é o do emissor, de quatro caracteres (`PETR`
//! para PETR3 e PETR4).

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{download, SectorEntry, SectorError, SectorFeed};
use crate::domain::sector::SectorSource;

const DEFAULT_BASE: &str =
    "https://sistemaswebb3-listados.b3.com.br/listedCompaniesProxy/CompanyCall";
/// O filtro que a página manda: só o idioma.
const FILTER: &str = r#"{"language":"pt-br"}"#;
/// Teto do XML descompactado de uma aba (a real tem ~100 KB).
const MAX_XML: u64 = 32 * 1024 * 1024;

pub struct B3Classification {
    http: reqwest::Client,
    base: String,
}

impl B3Classification {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, DEFAULT_BASE)
    }

    /// Para os testes apontarem para um servidor falso.
    pub fn with_base(http: reqwest::Client, base: &str) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    fn url(&self) -> String {
        format!(
            "{}/GetDownloadIndustryClassification/{}",
            self.base,
            STANDARD.encode(FILTER)
        )
    }
}

#[async_trait]
impl SectorFeed for B3Classification {
    fn source(&self) -> SectorSource {
        SectorSource::B3Stocks
    }

    async fn fetch(&self) -> Result<Vec<SectorEntry>, SectorError> {
        let bytes = download(&self.http, &self.url()).await?;
        parse_workbook(&bytes)
    }
}

fn format_error(what: impl std::fmt::Display) -> SectorError {
    SectorError::Format(format!("planilha da B3: {what}"))
}

fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<String, SectorError> {
    let file = archive
        .by_name(name)
        .map_err(|_| format_error(format!("falta {name}")))?;
    let mut text = String::new();
    file.take(MAX_XML)
        .read_to_string(&mut text)
        .map_err(format_error)?;
    // O arquivo da B3 começa com BOM; o leitor de XML não o espera.
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}

/// Adiciona ao texto o que um evento de referência (`&amp;`, `&#233;`) diz.
fn push_reference(
    text: &mut String,
    reference: &quick_xml::events::BytesRef<'_>,
) -> Result<(), SectorError> {
    if let Some(c) = reference.resolve_char_ref().map_err(format_error)? {
        text.push(c);
    } else if let Some(entity) = quick_xml::escape::resolve_predefined_entity(reference) {
        text.push_str(entity);
    }
    Ok(())
}

/// Os textos compartilhados (`<si><t>…</t></si>`), na ordem do índice.
fn shared_strings(xml: &str) -> Result<Vec<String>, SectorError> {
    let mut reader = Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(format_error)? {
            Event::Start(e) if e.name().as_ref() == "si" => current.clear(),
            Event::Start(e) if e.name().as_ref() == "t" => in_text = true,
            Event::End(e) if e.name().as_ref() == "t" => in_text = false,
            Event::End(e) if e.name().as_ref() == "si" => {
                strings.push(std::mem::take(&mut current))
            }
            Event::Text(t) if in_text => current.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_text => push_reference(&mut current, &r)?,
            Event::Eof => return Ok(strings),
            _ => {}
        }
    }
}

/// As linhas da aba: coluna (`B`, `C`...) → texto da célula.
fn sheet_rows(xml: &str, strings: &[String]) -> Result<Vec<HashMap<String, String>>, SectorError> {
    let mut reader = Reader::from_str(xml);
    let mut rows = Vec::new();
    let mut row: HashMap<String, String> = HashMap::new();
    let mut column: Option<String> = None;
    let mut shared = false;
    let mut value = String::new();
    let mut in_value = false;
    loop {
        match reader.read_event().map_err(format_error)? {
            Event::Start(e) if e.name().as_ref() == "row" => row = HashMap::new(),
            Event::End(e) if e.name().as_ref() == "row" => rows.push(std::mem::take(&mut row)),
            Event::Start(e) if e.name().as_ref() == "c" => {
                column = None;
                shared = false;
                value.clear();
                for attribute in e.attributes().flatten() {
                    let text = attribute.value.to_string();
                    match attribute.key.as_ref() {
                        "r" => {
                            column =
                                Some(text.chars().take_while(char::is_ascii_alphabetic).collect());
                        }
                        "t" => shared = text == "s",
                        _ => {}
                    }
                }
            }
            // Texto da célula: `<v>` (número ou índice do texto compartilhado)
            // ou `<is><t>` (texto na própria célula).
            Event::Start(e) if matches!(e.name().as_ref(), "v" | "t") => in_value = true,
            Event::End(e) if matches!(e.name().as_ref(), "v" | "t") => in_value = false,
            Event::Text(t) if in_value => value.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_value => push_reference(&mut value, &r)?,
            Event::End(e) if e.name().as_ref() == "c" => {
                if let Some(column) = column.take() {
                    let text = if shared {
                        value
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| strings.get(i))
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        value.clone()
                    };
                    row.insert(column, text.trim().to_owned());
                }
            }
            Event::Eof => return Ok(rows),
            _ => {}
        }
    }
}

/// O código do emissor: três a seis caracteres ASCII maiúsculos ou dígitos. O
/// "CÓDIGO" do cabeçalho tem acento e cai fora.
fn valid_code(code: &str) -> bool {
    (3..=6).contains(&code.len())
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Lê a planilha da B3 e devolve uma linha por código de emissor.
pub fn parse_workbook(bytes: &[u8]) -> Result<Vec<SectorEntry>, SectorError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(format_error)?;
    let strings = shared_strings(&read_entry(&mut archive, "xl/sharedStrings.xml")?)?;
    let rows = sheet_rows(
        &read_entry(&mut archive, "xl/worksheets/sheet1.xml")?,
        &strings,
    )?;

    let cell = |row: &HashMap<String, String>, column: &str| {
        row.get(column)
            .map(String::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mut entries: BTreeMap<String, SectorEntry> = BTreeMap::new();
    let mut group: Option<(String, String, String)> = None;
    for row in &rows {
        let (sector, subsector, segment) = (cell(row, "B"), cell(row, "C"), cell(row, "D"));
        let code = cell(row, "F");
        if !valid_code(&code) {
            // Cabeçalho (o do topo e o que abre cada setor): começa grupo novo.
            if !sector.is_empty() || !subsector.is_empty() || !segment.is_empty() {
                group = None;
            }
            continue;
        }
        // Em branco vale o de cima; o setor muda quando a célula vem preenchida.
        let (g_sector, g_subsector, g_segment) = group.get_or_insert_with(Default::default);
        if !sector.is_empty() {
            *g_sector = sector;
        }
        if !subsector.is_empty() {
            *g_subsector = subsector;
        }
        if !segment.is_empty() {
            *g_segment = segment;
        }
        if g_sector.is_empty() {
            continue;
        }
        entries.entry(code.clone()).or_insert_with(|| SectorEntry {
            code,
            sector: g_sector.clone(),
            subsector: Some(g_subsector.clone()).filter(|s| !s.is_empty()),
            segment: Some(g_segment.clone()).filter(|s| !s.is_empty()),
        });
    }
    if entries.is_empty() {
        return Err(format_error("nenhuma empresa encontrada"));
    }
    Ok(entries.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Amostra gravada em 25/11/2026 de `GetDownloadIndustryClassification`:
    /// três setores da planilha real, com as linhas e o cabeçalho repetido como
    /// a B3 publica (ver `tests/fixtures/README.md`).
    const SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/b3_classificacao_setorial.xlsx");

    fn by_code(entries: &[SectorEntry], code: &str) -> SectorEntry {
        entries.iter().find(|e| e.code == code).cloned().unwrap()
    }

    #[test]
    fn reads_the_sector_of_each_issuer() {
        let entries = parse_workbook(SAMPLE).unwrap();
        let petr = by_code(&entries, "PETR");
        assert_eq!(petr.sector, "Petróleo, Gás e Biocombustíveis");
        assert_eq!(
            petr.segment.as_deref(),
            Some("Exploração, Refino e Distribuição")
        );
        // Linha em branco herda o grupo de cima, inclusive mais de um segmento.
        let lupa = by_code(&entries, "LUPA");
        assert_eq!(lupa.sector, "Petróleo, Gás e Biocombustíveis");
        assert_eq!(lupa.segment.as_deref(), Some("Equipamentos e Serviços"));
        // O cabeçalho repetido abre outro setor sem herdar o anterior.
        let itub = by_code(&entries, "ITUB");
        assert_eq!(itub.sector, "Financeiro");
        assert_eq!(
            itub.subsector.as_deref(),
            Some("Intermediários Financeiros")
        );
        assert_eq!(itub.segment.as_deref(), Some("Bancos"));
        let wege = by_code(&entries, "WEGE");
        assert_eq!(wege.sector, "Bens Industriais");
        assert_eq!(wege.subsector.as_deref(), Some("Máquinas e Equipamentos"));
    }

    #[test]
    fn headers_and_blank_rows_are_not_issuers() {
        let entries = parse_workbook(SAMPLE).unwrap();
        assert!(entries.iter().all(|e| valid_code(&e.code)));
        assert!(entries.iter().all(|e| !e.code.contains("DIGO")));
        // Um código por emissor, sem repetição.
        let mut codes: Vec<&str> = entries.iter().map(|e| e.code.as_str()).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), before);
    }

    #[test]
    fn rejects_what_is_not_the_workbook() {
        assert!(matches!(
            parse_workbook(b"<html>erro</html>"),
            Err(SectorError::Format(_))
        ));
        assert!(matches!(parse_workbook(b""), Err(SectorError::Format(_))));
    }

    #[test]
    fn the_request_carries_the_language_filter_like_the_site() {
        let feed = B3Classification::with_base(reqwest::Client::new(), "https://exemplo.test/x/");
        assert_eq!(
            feed.url(),
            "https://exemplo.test/x/GetDownloadIndustryClassification/eyJsYW5ndWFnZSI6InB0LWJyIn0="
        );
    }

    #[test]
    fn entities_and_character_references_are_decoded() {
        let strings = shared_strings(
            "<sst><si><t>A &amp; B</t></si><si><r><t>Sa&#250;</t></r><r><t>de</t></r></si></sst>",
        )
        .unwrap();
        assert_eq!(strings, vec!["A & B".to_owned(), "Saúde".to_owned()]);
    }
}
