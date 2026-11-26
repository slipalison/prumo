//! Informe mensal de FII da CVM (dados abertos).
//!
//! `https://dados.cvm.gov.br/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_AAAA.zip`
//! traz, para cada fundo e cada mês do ano, três CSV (`;` como separador,
//! ISO-8859-1):
//!
//! - `inf_mensal_fii_geral_AAAA.csv`: cadastro. Interessam o CNPJ, o código ISIN
//!   da cota e o `Segmento_Atuacao` que o administrador declara;
//! - `inf_mensal_fii_ativo_passivo_AAAA.csv`: o balanço, com o que o fundo tem
//!   em imóveis, em recebíveis (CRI, LCI...) e em cotas de outros FIIs;
//! - `inf_mensal_fii_complemento_AAAA.csv`: cotistas e rentabilidade (não usado).
//!
//! **Ticker.** O informe não traz o ticker, mas o ISIN da cota é `BR` + as
//! quatro letras do emissor + `CTF` + série (`BRKNRICTF007` → `KNRI`), e o
//! ticker é essas quatro letras mais a série (`KNRI11`). A chave da referência
//! é, portanto, as quatro letras.
//!
//! **Tipo.** A CVM tem o campo `Mandato`, mas ele vem vazio, e o segmento
//! declarado é largamente `Multicategoria` ou `Outros`. Para dizer se um fundo
//! é de tijolo, papel ou fundo de fundos, o Prumo olha o balanço do último
//! informe: soma o que há em imóveis (e em sociedades de atividades
//! imobiliárias), em recebíveis (CRI, LCI, LIG e letras hipotecárias) e em
//! cotas de FII, e chama de tipo o que tem dois terços ou mais dessa soma —
//! o mesmo corte de dois terços que a B3 usa para holdings. Sem nenhum tipo
//! com dois terços, é híbrido. Caixa e títulos públicos ficam fora da conta.
//! A regra é do Prumo, não da CVM, e está em `docs/saude.md`.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::str::FromStr;

use async_trait::async_trait;
use rust_decimal::Decimal;
use time::OffsetDateTime;

use super::{download, SectorEntry, SectorError, SectorFeed};
use crate::domain::sector::SectorSource;

const DEFAULT_BASE: &str = "https://dados.cvm.gov.br";
/// Teto de cada CSV descompactado (o de 2026 tem ~5 MB).
const MAX_CSV: u64 = 128 * 1024 * 1024;

pub const BRICK: &str = "Tijolo";
pub const PAPER: &str = "Papel";
pub const FUND_OF_FUNDS: &str = "Fundo de fundos";
pub const HYBRID: &str = "Híbrido";

pub struct CvmFunds {
    http: reqwest::Client,
    base: String,
    /// Anos a tentar, do mais novo para o mais velho. `None` = o ano de agora
    /// e o anterior (em janeiro o arquivo novo ainda pode não existir).
    years: Option<Vec<i32>>,
}

impl CvmFunds {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_base(http, DEFAULT_BASE, None)
    }

    /// Para os testes apontarem para um servidor falso e fixarem os anos.
    pub fn with_base(http: reqwest::Client, base: &str, years: Option<Vec<i32>>) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            years,
        }
    }

    fn years(&self) -> Vec<i32> {
        self.years.clone().unwrap_or_else(|| {
            let now = OffsetDateTime::now_utc().year();
            vec![now, now - 1]
        })
    }

    fn url(&self, year: i32) -> String {
        format!(
            "{}/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_{year}.zip",
            self.base
        )
    }
}

#[async_trait]
impl SectorFeed for CvmFunds {
    fn source(&self) -> SectorSource {
        SectorSource::CvmReits
    }

    async fn fetch(&self) -> Result<Vec<SectorEntry>, SectorError> {
        let mut last_error = SectorError::Unavailable("nenhum ano tentado".to_owned());
        for year in self.years() {
            let outcome = match download(&self.http, &self.url(year)).await {
                Ok(bytes) => parse_archive(&bytes, year),
                Err(error) => Err(error),
            };
            match outcome {
                Ok(entries) if !entries.is_empty() => return Ok(entries),
                Ok(_) => {
                    last_error = SectorError::Format(format!("informe de {year} sem fundos"));
                }
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
}

fn format_error(what: impl std::fmt::Display) -> SectorError {
    SectorError::Format(format!("informe da CVM: {what}"))
}

/// Os bytes ISO-8859-1 como texto: cada byte é o ponto de código de mesmo valor.
pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| char::from(*b)).collect()
}

/// Um CSV do informe: nome da coluna → texto, uma linha por vez.
fn read_csv(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<HashMap<String, String>>, SectorError> {
    let file = archive
        .by_name(name)
        .map_err(|_| format_error(format!("falta {name}")))?;
    let mut raw = Vec::new();
    file.take(MAX_CSV)
        .read_to_end(&mut raw)
        .map_err(format_error)?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .flexible(true)
        .from_reader(raw.as_slice());
    let headers: Vec<String> = reader
        .byte_headers()
        .map_err(format_error)?
        .iter()
        .map(|h| latin1(h).trim().to_owned())
        .collect();
    let mut rows = Vec::new();
    for record in reader.byte_records() {
        let record = record.map_err(format_error)?;
        rows.push(
            headers
                .iter()
                .zip(record.iter())
                .map(|(name, value)| (name.clone(), latin1(value).trim().to_owned()))
                .collect(),
        );
    }
    Ok(rows)
}

fn field<'a>(row: &'a HashMap<String, String>, name: &str) -> &'a str {
    row.get(name).map(String::as_str).unwrap_or_default()
}

fn amount(row: &HashMap<String, String>, name: &str) -> Decimal {
    Decimal::from_str(field(row, name)).unwrap_or(Decimal::ZERO)
}

/// A referência mais nova de cada fundo: `Data_Referencia` (ISO, ordena como
/// texto) e, no empate, a versão mais alta (reapresentação do informe).
fn latest_by_fund(rows: Vec<HashMap<String, String>>) -> HashMap<String, HashMap<String, String>> {
    let key = |row: &HashMap<String, String>| {
        (
            field(row, "Data_Referencia").to_owned(),
            field(row, "Versao").parse::<u32>().unwrap_or(0),
        )
    };
    let mut latest: HashMap<String, HashMap<String, String>> = HashMap::new();
    for row in rows {
        let cnpj = field(&row, "CNPJ_Fundo_Classe").to_owned();
        if cnpj.is_empty() {
            continue;
        }
        match latest.get(&cnpj) {
            Some(current) if key(current) >= key(&row) => {}
            _ => {
                latest.insert(cnpj, row);
            }
        }
    }
    latest
}

/// Tijolo, papel, fundo de fundos ou híbrido, pelo balanço. `None` quando o
/// fundo não tem nada nessas três contas (fundo novo, sem carteira ainda).
pub fn fund_kind(balance: &HashMap<String, String>) -> Option<&'static str> {
    let sum = |names: &[&str]| -> Decimal { names.iter().map(|n| amount(balance, n)).sum() };
    let brick = sum(&[
        "Direitos_Bens_Imoveis",
        "Acoes_Sociedades_Atividades_FII",
        "Cotas_Sociedades_Atividades_FII",
    ]);
    let paper = sum(&[
        "CRI",
        "CRI_CRA",
        "LCI",
        "LCI_LCA",
        "Letras_Hipotecarias",
        "LIG",
    ]);
    let funds = sum(&["FII"]);
    let base = brick + paper + funds;
    if base <= Decimal::ZERO {
        return None;
    }
    let two_thirds = |part: Decimal| part * Decimal::from(3) >= base * Decimal::from(2);
    Some(if two_thirds(brick) {
        BRICK
    } else if two_thirds(paper) {
        PAPER
    } else if two_thirds(funds) {
        FUND_OF_FUNDS
    } else {
        HYBRID
    })
}

/// As quatro letras do emissor a partir do ISIN da cota (`BRKNRICTF007` → `KNRI`).
pub(crate) fn isin_root(isin: &str) -> Option<String> {
    let isin = isin.trim().to_uppercase();
    if isin.len() != 12 || !isin.starts_with("BR") || !isin.is_ascii() {
        return None;
    }
    let root = &isin[2..6];
    let valid = root.chars().all(|c| c.is_ascii_alphanumeric())
        && root.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    valid.then(|| root.to_owned())
}

/// Lê o `.zip` do informe de `year` e devolve uma linha por emissor.
pub fn parse_archive(bytes: &[u8], year: i32) -> Result<Vec<SectorEntry>, SectorError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(format_error)?;
    let general = latest_by_fund(read_csv(
        &mut archive,
        &format!("inf_mensal_fii_geral_{year}.csv"),
    )?);
    let balances = latest_by_fund(read_csv(
        &mut archive,
        &format!("inf_mensal_fii_ativo_passivo_{year}.csv"),
    )?);

    // Quando dois fundos têm o mesmo emissor (fundo que mudou de CNPJ ao virar
    // classe), vale o informe mais novo.
    let mut chosen: HashMap<String, (String, SectorEntry)> = HashMap::new();
    for (cnpj, row) in &general {
        let Some(code) = isin_root(field(row, "Codigo_ISIN")) else {
            continue;
        };
        let Some(kind) = balances.get(cnpj).and_then(fund_kind) else {
            continue;
        };
        let segment = Some(field(row, "Segmento_Atuacao").to_owned()).filter(|s| !s.is_empty());
        let reference = field(row, "Data_Referencia").to_owned();
        if chosen.get(&code).is_some_and(|(at, _)| *at >= reference) {
            continue;
        }
        chosen.insert(
            code.clone(),
            (
                reference,
                SectorEntry {
                    code,
                    sector: kind.to_owned(),
                    subsector: None,
                    segment,
                },
            ),
        );
    }
    let mut entries: Vec<SectorEntry> = chosen.into_values().map(|(_, entry)| entry).collect();
    entries.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Amostra gravada em 25/11/2026 de `inf_mensal_fii_2026.zip`: um punhado de
    /// fundos reais, com as colunas do arquivo e os contatos do administrador
    /// em branco (ver `tests/fixtures/README.md`).
    const SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/cvm_inf_mensal_fii_2026.zip");

    fn balance(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn by_code(entries: &[SectorEntry], code: &str) -> SectorEntry {
        entries.iter().find(|e| e.code == code).cloned().unwrap()
    }

    #[test]
    fn classifies_each_fund_by_its_balance_sheet() {
        let entries = parse_archive(SAMPLE, 2026).unwrap();
        let knri = by_code(&entries, "KNRI");
        assert_eq!(knri.sector, BRICK);
        assert_eq!(knri.segment.as_deref(), Some("Multicategoria"));
        assert_eq!(knri.subsector, None);
        assert_eq!(by_code(&entries, "HGLG").sector, BRICK);
        assert_eq!(by_code(&entries, "KNCR").sector, PAPER);
        assert_eq!(by_code(&entries, "TAGI").sector, FUND_OF_FUNDS);
        assert_eq!(by_code(&entries, "KNRE").sector, HYBRID);
        // O segmento é o que o administrador declara, mesmo quando destoa do tipo.
        let mxrf = by_code(&entries, "MXRF");
        assert_eq!(mxrf.sector, PAPER);
        assert_eq!(mxrf.segment.as_deref(), Some("Logística"));
        assert_eq!(
            by_code(&entries, "VISC").segment.as_deref(),
            Some("Shoppings")
        );
    }

    #[test]
    fn one_row_per_issuer_and_the_newest_report_wins() {
        let entries = parse_archive(SAMPLE, 2026).unwrap();
        // A amostra tem 11 fundos: um sem ISIN (fora) e dois de CNPJs diferentes
        // com o emissor TRXF, que viram uma linha só.
        assert_eq!(entries.len(), 9);
        let mut codes: Vec<&str> = entries.iter().map(|e| e.code.as_str()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 9);
        // O informe de outubro (híbrido) vale mais que o de setembro (tijolo),
        // do outro CNPJ do mesmo emissor.
        assert_eq!(by_code(&entries, "TRXF").sector, HYBRID);
    }

    #[test]
    fn two_thirds_decide_the_type() {
        assert_eq!(
            fund_kind(&balance(&[
                ("Direitos_Bens_Imoveis", "700"),
                ("CRI", "300")
            ])),
            Some(BRICK)
        );
        assert_eq!(
            fund_kind(&balance(&[
                ("Direitos_Bens_Imoveis", "100"),
                ("CRI", "500"),
                ("LCI", "100")
            ])),
            Some(PAPER)
        );
        assert_eq!(
            fund_kind(&balance(&[("FII", "900"), ("CRI", "100")])),
            Some(FUND_OF_FUNDS)
        );
        assert_eq!(
            fund_kind(&balance(&[
                ("Direitos_Bens_Imoveis", "500"),
                ("CRI", "500")
            ])),
            Some(HYBRID)
        );
        // Exatamente dois terços já é o tipo.
        assert_eq!(
            fund_kind(&balance(&[
                ("Direitos_Bens_Imoveis", "200"),
                ("CRI", "100")
            ])),
            Some(BRICK)
        );
        // Caixa e títulos públicos não entram na conta.
        assert_eq!(
            fund_kind(&balance(&[
                ("Direitos_Bens_Imoveis", "100"),
                ("Titulos_Publicos", "9000"),
                ("Disponibilidades", "9000")
            ])),
            Some(BRICK)
        );
        assert_eq!(fund_kind(&balance(&[("Disponibilidades", "10")])), None);
        assert_eq!(fund_kind(&HashMap::new()), None);
    }

    #[test]
    fn the_ticker_root_comes_from_the_isin() {
        assert_eq!(isin_root("BRKNRICTF007").as_deref(), Some("KNRI"));
        assert_eq!(isin_root("brmxrfctf008").as_deref(), Some("MXRF"));
        assert_eq!(isin_root(""), None);
        assert_eq!(isin_root("BR0000CTF000"), None);
        assert_eq!(isin_root("US0378331005"), None);
        assert_eq!(isin_root("BRKNRI"), None);
    }

    #[test]
    fn rejects_what_is_not_the_archive() {
        assert!(matches!(
            parse_archive(b"nao e zip", 2026),
            Err(SectorError::Format(_))
        ));
        // Zip de outro ano: falta o CSV daquele ano.
        assert!(matches!(
            parse_archive(SAMPLE, 2019),
            Err(SectorError::Format(_))
        ));
    }

    #[test]
    fn latin1_keeps_the_accents_of_the_file() {
        assert_eq!(
            latin1(&[0x4c, 0x6f, 0x67, 0xed, 0x73, 0x74, 0x69, 0x63, 0x61]),
            "Logística"
        );
    }
}
