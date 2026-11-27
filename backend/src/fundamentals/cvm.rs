//! Leitura dos arquivos de dados abertos da CVM (`dados.cvm.gov.br`).
//!
//! Todos são CSV com `;`, em ISO-8859-1, sem aspas como delimitador (há
//! descrição com aspas no meio, como `"Escrow"`, e o leitor não pode tratá-las
//! como campo citado). Os de companhia vêm num `.zip` por ano, um CSV por
//! demonstração e por consolidação:
//!
//! - `CIA_ABERTA/CAD/DADOS/cad_cia_aberta.csv`: cadastro (sem zip);
//! - `CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_AAAA.zip`: DFP do ano;
//! - `CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_AAAA.zip`: ITR do ano;
//! - `FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_AAAA.zip`: informe mensal;
//! - `FII/DOC/INF_TRIMESTRAL/DADOS/inf_trimestral_fii_AAAA.zip`: trimestral.
//!
//! Cada leitor recebe o corpo como `Read` (em fluxo, ver [`super::download`])
//! e devolve só o que o Prumo guarda.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::str::FromStr;

use rust_decimal::Decimal;
use time::Date;

use super::download::{FetchError, Limited};
use crate::domain::fundamentals::{
    keeps_line, Column, Filing, FilingKind, FundReport, Line, Registration, RentalProperty, Scope,
    Statement,
};
use crate::sectors::cvm::{isin_root, latin1};

/// Teto de cada CSV aberto. O maior lido hoje (BPP individual do ITR) tem 86 MB.
pub(crate) const MAX_CSV: u64 = 1024 * 1024 * 1024;

/// O caminho de cada arquivo, a partir da raiz do site.
pub fn path(kind: FilingKind, year: i32) -> String {
    match kind {
        FilingKind::Annual => format!("/dados/CIA_ABERTA/DOC/DFP/DADOS/dfp_cia_aberta_{year}.zip"),
        FilingKind::Quarterly => {
            format!("/dados/CIA_ABERTA/DOC/ITR/DADOS/itr_cia_aberta_{year}.zip")
        }
    }
}

pub const REGISTRY_PATH: &str = "/dados/CIA_ABERTA/CAD/DADOS/cad_cia_aberta.csv";

pub fn fund_monthly_path(year: i32) -> String {
    format!("/dados/FII/DOC/INF_MENSAL/DADOS/inf_mensal_fii_{year}.zip")
}

pub fn fund_quarterly_path(year: i32) -> String {
    format!("/dados/FII/DOC/INF_TRIMESTRAL/DADOS/inf_trimestral_fii_{year}.zip")
}

pub(crate) fn format_error(what: impl std::fmt::Display) -> FetchError {
    FetchError::Format(format!("arquivo da CVM: {what}"))
}

pub(crate) fn csv_reader<R: Read>(reader: R) -> csv::Reader<R> {
    csv::ReaderBuilder::new()
        .delimiter(b';')
        .quoting(false)
        .flexible(true)
        .from_reader(reader)
}

/// Onde está cada coluna pelo nome do cabeçalho.
pub(crate) struct Columns(HashMap<String, usize>);

impl Columns {
    pub(crate) fn read<R: Read>(reader: &mut csv::Reader<R>) -> Result<Self, FetchError> {
        let headers = reader.byte_headers().map_err(format_error)?;
        Ok(Self(
            headers
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    (
                        latin1(h).trim().trim_start_matches('\u{feff}').to_owned(),
                        i,
                    )
                })
                .collect(),
        ))
    }

    pub(crate) fn index(&self, name: &str) -> Result<usize, FetchError> {
        self.0
            .get(name)
            .copied()
            .ok_or_else(|| format_error(format!("falta a coluna {name}")))
    }

    pub(crate) fn optional(&self, name: &str) -> Option<usize> {
        self.0.get(name).copied()
    }
}

pub(crate) fn text(record: &csv::ByteRecord, index: usize) -> String {
    record
        .get(index)
        .map(|v| latin1(v).trim().to_owned())
        .unwrap_or_default()
}

pub(crate) fn date(record: &csv::ByteRecord, index: usize) -> Option<Date> {
    let raw = record.get(index)?;
    let s = std::str::from_utf8(raw).ok()?.trim();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(s, &format).ok()
}

fn number(value: &str) -> Option<Decimal> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Decimal::from_str(value)
        .or_else(|_| Decimal::from_scientific(value))
        .ok()
        .map(|d| d.normalize())
}

pub(crate) fn integer(record: &csv::ByteRecord, index: usize) -> Option<i32> {
    std::str::from_utf8(record.get(index)?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

pub(crate) fn entry_name<R: Read>(entry: &zip::read::ZipFile<'_, R>) -> Result<String, FetchError> {
    entry.name().map(|n| n.into_owned()).map_err(format_error)
}

/// O que um CSV de demonstração é, pelo nome dentro do zip
/// (`dfp_cia_aberta_DRE_con_2025.csv`).
fn statement_entry(name: &str, kind: FilingKind, year: i32) -> Option<(Statement, Scope)> {
    let prefix = match kind {
        FilingKind::Annual => "dfp",
        FilingKind::Quarterly => "itr",
    };
    let rest = name.strip_prefix(&format!("{prefix}_cia_aberta_"))?;
    let rest = rest.strip_suffix(&format!("_{year}.csv"))?;
    let (statement, scope) = rest.rsplit_once('_')?;
    let scope = match scope {
        "con" => Scope::Consolidated,
        "ind" => Scope::Individual,
        _ => return None,
    };
    let statement = match statement {
        "BPA" => Statement::Assets,
        "BPP" => Statement::Liabilities,
        "DRE" => Statement::Income,
        "DFC_MD" | "DFC_MI" => Statement::CashFlow,
        "DVA" => Statement::ValueAdded,
        _ => return None,
    };
    Some((statement, scope))
}

type LineKey = (Scope, Statement, Column, String);

/// Um documento enquanto o arquivo é lido.
#[derive(Default)]
struct Draft {
    version: i32,
    name: String,
    lines: HashMap<LineKey, Line>,
}

/// Lê o `.zip` de DFP ou ITR de `year` e devolve os documentos das
/// companhias `codes`, na versão mais alta, com só as contas que o Prumo usa.
pub fn parse_company_archive<R: Read>(
    mut reader: R,
    kind: FilingKind,
    year: i32,
    codes: &HashSet<i32>,
) -> Result<Vec<Filing>, FetchError> {
    let mut drafts: HashMap<(i32, Date), Draft> = HashMap::new();
    let mut seen: HashSet<Statement> = HashSet::new();
    while let Some(entry) =
        zip::read::read_zipfile_from_stream(&mut reader).map_err(format_error)?
    {
        let Some((statement, scope)) = statement_entry(&entry_name(&entry)?, kind, year) else {
            continue;
        };
        seen.insert(statement);
        read_statement(
            Limited::new(entry, MAX_CSV),
            statement,
            scope,
            codes,
            &mut drafts,
        )?;
    }
    for required in [Statement::Assets, Statement::Liabilities, Statement::Income] {
        if !seen.contains(&required) {
            return Err(format_error(format!(
                "sem a demonstracao {} de {year}",
                required.as_str()
            )));
        }
    }
    let mut filings: Vec<Filing> = drafts
        .into_iter()
        .filter_map(|((cvm_code, reference_date), draft)| {
            finish(cvm_code, reference_date, kind, draft)
        })
        .collect();
    filings.sort_by_key(|f| (f.cvm_code, f.reference_date));
    Ok(filings)
}

/// Consolidada quando a companhia entregou; senão, individual.
fn finish(cvm_code: i32, reference_date: Date, kind: FilingKind, draft: Draft) -> Option<Filing> {
    let has = |scope: Scope| {
        draft.lines.keys().any(|(s, statement, _, _)| {
            *s == scope && matches!(statement, Statement::Income | Statement::Liabilities)
        })
    };
    let scope = if has(Scope::Consolidated) {
        Scope::Consolidated
    } else if has(Scope::Individual) {
        Scope::Individual
    } else {
        return None;
    };
    let mut lines: Vec<Line> = draft
        .lines
        .into_iter()
        .filter(|((s, _, _, _), _)| *s == scope)
        .map(|(_, line)| line)
        .collect();
    lines.sort_by(|a, b| {
        (a.statement, a.column.as_str(), &a.code).cmp(&(b.statement, b.column.as_str(), &b.code))
    });
    Some(Filing {
        cvm_code,
        kind,
        reference_date,
        version: draft.version,
        scope,
        company_name: draft.name,
        lines,
    })
}

fn read_statement<R: Read>(
    reader: R,
    statement: Statement,
    scope: Scope,
    codes: &HashSet<i32>,
    drafts: &mut HashMap<(i32, Date), Draft>,
) -> Result<(), FetchError> {
    let mut csv = csv_reader(reader);
    let columns = Columns::read(&mut csv)?;
    let cvm = columns.index("CD_CVM")?;
    let reference = columns.index("DT_REFER")?;
    let version = columns.index("VERSAO")?;
    let name = columns.index("DENOM_CIA")?;
    let currency = columns.index("MOEDA")?;
    let scale = columns.index("ESCALA_MOEDA")?;
    let order = columns.index("ORDEM_EXERC")?;
    let start = columns.optional("DT_INI_EXERC");
    let end = columns.index("DT_FIM_EXERC")?;
    let code = columns.index("CD_CONTA")?;
    let description = columns.index("DS_CONTA")?;
    let value = columns.index("VL_CONTA")?;
    let mut record = csv::ByteRecord::new();
    while csv.read_byte_record(&mut record).map_err(format_error)? {
        let Some(cvm_code) = integer(&record, cvm) else {
            continue;
        };
        if !codes.contains(&cvm_code) {
            continue;
        }
        let account = text(&record, code);
        let label = text(&record, description);
        if !keeps_line(statement, &account, &label) {
            continue;
        }
        let (Some(reference_date), Some(period_end), Some(row_version)) = (
            date(&record, reference),
            date(&record, end),
            integer(&record, version),
        ) else {
            continue;
        };
        // Só real, em reais ou em milhares; outra moeda ou escala não entra.
        let multiplier = match (
            text(&record, currency).as_str(),
            text(&record, scale).as_str(),
        ) {
            ("REAL", "MIL") => Decimal::from(1000),
            ("REAL", "UNIDADE") => Decimal::ONE,
            _ => continue,
        };
        let column = match fold_order(&text(&record, order)) {
            Some(column) => column,
            None => continue,
        };
        let Some(amount) = number(&text(&record, value)) else {
            continue;
        };
        let draft = drafts.entry((cvm_code, reference_date)).or_default();
        if row_version < draft.version {
            continue;
        }
        if row_version > draft.version {
            // Reapresentação: a versão nova troca tudo o que veio antes.
            draft.version = row_version;
            draft.lines.clear();
        }
        if draft.name.is_empty() {
            draft.name = text(&record, name).chars().take(200).collect();
        }
        let line = Line {
            statement,
            column,
            period_start: start.and_then(|i| date(&record, i)),
            period_end,
            code: account.clone(),
            description: label.chars().take(300).collect(),
            value: (amount * multiplier).normalize(),
        };
        // No ITR o resultado vem duas vezes: o trimestre e o acumulado do ano.
        // Fica o acumulado (o início mais antigo).
        let key = (scope, statement, column, account);
        match draft.lines.get(&key) {
            Some(existing) if existing.period_start <= line.period_start => {}
            _ => {
                draft.lines.insert(key, line);
            }
        }
    }
    Ok(())
}

fn fold_order(order: &str) -> Option<Column> {
    match order {
        "ÚLTIMO" => Some(Column::Current),
        "PENÚLTIMO" => Some(Column::Previous),
        _ => None,
    }
}

/// O cadastro das companhias (`cad_cia_aberta.csv`, sem zip). Um código com
/// mais de uma linha (o arquivo repete a companhia por endereço): vale a
/// ativa, e entre iguais a de situação mais recente.
pub fn parse_registry<R: Read>(reader: R) -> Result<Vec<Registration>, FetchError> {
    let mut csv = csv_reader(Limited::new(reader, MAX_CSV));
    let columns = Columns::read(&mut csv)?;
    let cvm = columns.index("CD_CVM")?;
    let cnpj = columns.index("CNPJ_CIA")?;
    let name = columns.index("DENOM_SOCIAL")?;
    let status = columns.index("SIT")?;
    let since = columns.index("DT_INI_SIT")?;
    let issuer_status = columns.index("SIT_EMISSOR")?;
    let sector = columns.index("SETOR_ATIV")?;
    let mut chosen: HashMap<i32, ((bool, String), Registration)> = HashMap::new();
    let mut record = csv::ByteRecord::new();
    while csv.read_byte_record(&mut record).map_err(format_error)? {
        let Some(cvm_code) = integer(&record, cvm).filter(|c| *c > 0) else {
            continue;
        };
        let registration = Registration {
            cvm_code,
            cnpj: text(&record, cnpj).chars().take(20).collect(),
            name: text(&record, name).chars().take(200).collect(),
            status: text(&record, status).chars().take(60).collect(),
            issuer_status: Some(
                text(&record, issuer_status)
                    .chars()
                    .take(80)
                    .collect::<String>(),
            )
            .filter(|s| !s.is_empty()),
            sector: Some(text(&record, sector).chars().take(120).collect::<String>())
                .filter(|s| !s.is_empty()),
        };
        if registration.name.is_empty() {
            continue;
        }
        let key = (registration.status == "ATIVO", text(&record, since));
        if chosen.get(&cvm_code).is_some_and(|(k, _)| *k >= key) {
            continue;
        }
        chosen.insert(cvm_code, (key, registration));
    }
    if chosen.is_empty() {
        return Err(format_error("cadastro sem nenhuma companhia"));
    }
    let mut list: Vec<Registration> = chosen.into_values().map(|(_, r)| r).collect();
    list.sort_by_key(|r| r.cvm_code);
    Ok(list)
}

/// A linha mais nova de cada fundo (`Data_Referencia`, e no empate a `Versao`).
struct Latest<T> {
    rows: HashMap<String, ((Date, i32), T)>,
}

impl<T> Latest<T> {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
        }
    }

    fn offer(&mut self, cnpj: String, key: (Date, i32), row: T) {
        if self.rows.get(&cnpj).is_some_and(|(k, _)| *k >= key) {
            return;
        }
        self.rows.insert(cnpj, (key, row));
    }
}

struct FundKeys {
    cnpj: usize,
    reference: usize,
    version: usize,
}

impl FundKeys {
    fn read(columns: &Columns) -> Result<Self, FetchError> {
        Ok(Self {
            cnpj: columns.index("CNPJ_Fundo_Classe")?,
            reference: columns.index("Data_Referencia")?,
            version: columns.index("Versao")?,
        })
    }

    fn of(&self, record: &csv::ByteRecord) -> Option<(String, (Date, i32))> {
        let cnpj = text(record, self.cnpj);
        if cnpj.is_empty() {
            return None;
        }
        Some((
            cnpj,
            (
                date(record, self.reference)?,
                integer(record, self.version)?,
            ),
        ))
    }
}

/// Lê o informe mensal de `year`: o cadastro (`geral`: ISIN e nome) e o
/// `complemento` (patrimônio líquido e cotas emitidas), o mais novo de cada
/// fundo com ISIN de cota.
pub fn parse_fund_monthly<R: Read>(
    mut reader: R,
    year: i32,
) -> Result<Vec<FundReport>, FetchError> {
    let general_name = format!("inf_mensal_fii_geral_{year}.csv");
    let complement_name = format!("inf_mensal_fii_complemento_{year}.csv");
    let mut general: Latest<(String, String)> = Latest::new();
    let mut complement: Latest<(Option<Decimal>, Option<Decimal>)> = Latest::new();
    let (mut saw_general, mut saw_complement) = (false, false);
    while let Some(entry) =
        zip::read::read_zipfile_from_stream(&mut reader).map_err(format_error)?
    {
        let name = entry_name(&entry)?;
        if name == general_name {
            saw_general = true;
            let mut csv = csv_reader(Limited::new(entry, MAX_CSV));
            let columns = Columns::read(&mut csv)?;
            let keys = FundKeys::read(&columns)?;
            let isin = columns.index("Codigo_ISIN")?;
            let fund = columns.index("Nome_Fundo_Classe")?;
            let mut record = csv::ByteRecord::new();
            while csv.read_byte_record(&mut record).map_err(format_error)? {
                if let Some((cnpj, key)) = keys.of(&record) {
                    general.offer(cnpj, key, (text(&record, isin), text(&record, fund)));
                }
            }
        } else if name == complement_name {
            saw_complement = true;
            let mut csv = csv_reader(Limited::new(entry, MAX_CSV));
            let columns = Columns::read(&mut csv)?;
            let keys = FundKeys::read(&columns)?;
            let equity = columns.index("Patrimonio_Liquido")?;
            let shares = columns.index("Cotas_Emitidas")?;
            let mut record = csv::ByteRecord::new();
            while csv.read_byte_record(&mut record).map_err(format_error)? {
                if let Some((cnpj, key)) = keys.of(&record) {
                    complement.offer(
                        cnpj,
                        key,
                        (
                            number(&text(&record, equity)),
                            number(&text(&record, shares)),
                        ),
                    );
                }
            }
        }
    }
    if !saw_general || !saw_complement {
        return Err(format_error(format!("informe mensal de {year} incompleto")));
    }
    let mut reports: Vec<FundReport> = complement
        .rows
        .into_iter()
        .filter_map(
            |(cnpj, ((reference_date, version), (net_assets, shares)))| {
                let (_, (isin, name)) = general.rows.get(&cnpj)?;
                let issuer = isin_root(isin)?;
                Some(FundReport {
                    cnpj: cnpj.chars().take(20).collect(),
                    issuer,
                    isin: isin.trim().to_uppercase(),
                    name: name.chars().take(200).collect(),
                    reference_date,
                    version,
                    net_assets,
                    shares,
                })
            },
        )
        .collect();
    reports.sort_by(|a, b| a.cnpj.cmp(&b.cnpj));
    Ok(reports)
}

/// A vacância declarada no informe trimestral mais novo de um fundo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundQuarter {
    pub cnpj: String,
    pub reference_date: Date,
    pub version: i32,
    pub properties: Vec<RentalProperty>,
}

/// Lê o informe trimestral de `year`: o `geral` diz qual é o informe mais
/// novo de cada fundo; o `imovel` traz os imóveis dele. Só os imóveis para
/// renda acabados entram (os em construção e os para venda não têm vacância).
pub fn parse_fund_quarterly<R: Read>(
    mut reader: R,
    year: i32,
) -> Result<Vec<FundQuarter>, FetchError> {
    let general_name = format!("inf_trimestral_fii_geral_{year}.csv");
    let property_name = format!("inf_trimestral_fii_imovel_{year}.csv");
    let mut latest: Latest<()> = Latest::new();
    let mut properties: HashMap<(String, Date, i32), Vec<RentalProperty>> = HashMap::new();
    let (mut saw_general, mut saw_property) = (false, false);
    while let Some(entry) =
        zip::read::read_zipfile_from_stream(&mut reader).map_err(format_error)?
    {
        let name = entry_name(&entry)?;
        if name == general_name {
            saw_general = true;
            let mut csv = csv_reader(Limited::new(entry, MAX_CSV));
            let columns = Columns::read(&mut csv)?;
            let keys = FundKeys::read(&columns)?;
            let mut record = csv::ByteRecord::new();
            while csv.read_byte_record(&mut record).map_err(format_error)? {
                if let Some((cnpj, key)) = keys.of(&record) {
                    latest.offer(cnpj, key, ());
                }
            }
        } else if name == property_name {
            saw_property = true;
            let mut csv = csv_reader(Limited::new(entry, MAX_CSV));
            let columns = Columns::read(&mut csv)?;
            let keys = FundKeys::read(&columns)?;
            let class = columns.index("Classe")?;
            let area = columns.index("Area")?;
            let vacancy = columns.index("Percentual_Vacancia")?;
            let mut record = csv::ByteRecord::new();
            while csv.read_byte_record(&mut record).map_err(format_error)? {
                if crate::domain::fundamentals::fold(&text(&record, class))
                    != "imoveis para renda acabados"
                {
                    continue;
                }
                if let Some((cnpj, (reference_date, version))) = keys.of(&record) {
                    properties
                        .entry((cnpj, reference_date, version))
                        .or_default()
                        .push(RentalProperty {
                            area: number(&text(&record, area)),
                            vacancy: number(&text(&record, vacancy)),
                        });
                }
            }
        }
    }
    if !saw_general || !saw_property {
        return Err(format_error(format!(
            "informe trimestral de {year} incompleto"
        )));
    }
    let mut quarters: Vec<FundQuarter> = latest
        .rows
        .into_iter()
        .map(|(cnpj, ((reference_date, version), ()))| FundQuarter {
            properties: properties
                .remove(&(cnpj.clone(), reference_date, version))
                .unwrap_or_default(),
            cnpj: cnpj.chars().take(20).collect(),
            reference_date,
            version,
        })
        .collect();
    quarters.sort_by(|a, b| a.cnpj.cmp(&b.cnpj));
    Ok(quarters)
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Recortes no formato real dos arquivos da CVM (cabeçalho, colunas,
    //! separador, latin1 e a ordem `ÚLTIMO`/`PENÚLTIMO`), com companhias e
    //! números inventados.

    use std::io::Write;

    pub const COMPANY_HEADER: &str = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;GRUPO_DFP;MOEDA;ESCALA_MOEDA;ORDEM_EXERC;DT_INI_EXERC;DT_FIM_EXERC;CD_CONTA;DS_CONTA;VL_CONTA;ST_CONTA_FIXA";
    pub const BALANCE_HEADER: &str = "CNPJ_CIA;DT_REFER;VERSAO;DENOM_CIA;CD_CVM;GRUPO_DFP;MOEDA;ESCALA_MOEDA;ORDEM_EXERC;DT_FIM_EXERC;CD_CONTA;DS_CONTA;VL_CONTA;ST_CONTA_FIXA";

    /// Texto em latin1, como a CVM grava.
    pub fn encode(text: &str) -> Vec<u8> {
        text.chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect()
    }

    /// Um `.zip` com estes arquivos (nome, conteúdo em texto).
    pub fn zip(files: &[(&str, String)]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, content) in files {
                writer.start_file(*name, options).unwrap();
                writer.write_all(&encode(content)).unwrap();
            }
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    /// Uma linha de resultado ou fluxo: o período é (início, fim) e a conta é
    /// (código, descrição).
    pub fn flow_row(
        cvm: &str,
        refer: &str,
        version: u32,
        order: &str,
        (start, end): (&str, &str),
        (code, desc): (&str, &str),
        value: &str,
    ) -> String {
        format!("00.000.000/0001-00;{refer};{version};CIA FICTICIA SA;{cvm};DF Consolidado - Demonstração do Resultado;REAL;MIL;{order};{start};{end};{code};{desc};{value};S")
    }

    /// Uma linha de balanço: a conta é (código, descrição).
    pub fn balance_row(
        cvm: &str,
        refer: &str,
        version: u32,
        order: &str,
        end: &str,
        (code, desc): (&str, &str),
        value: &str,
    ) -> String {
        format!("00.000.000/0001-00;{refer};{version};CIA FICTICIA SA;{cvm};DF Consolidado - Balanço Patrimonial;REAL;MIL;{order};{end};{code};{desc};{value};S")
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn codes(list: &[i32]) -> HashSet<i32> {
        list.iter().copied().collect()
    }

    /// Uma DFP de 2025 fictícia: a companhia 99901 (industrial, versão 1 e 2:
    /// a 2 vence), a 99902 (só individual), a 99903 (fora da carteira).
    fn dfp_2025() -> Vec<u8> {
        let dre_con = [
            COMPANY_HEADER.to_owned(),
            flow_row(
                "099901",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "111.0000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "1000.0000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "PENÚLTIMO",
                ("2024-01-01", "2024-12-31"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "900.0000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.04.01", "Despesas com Vendas"),
                "-10.0000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.11", "Lucro/Prejuízo Consolidado do Período"),
                "120.0000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.11.01", "Atribuído a Sócios da Empresa Controladora"),
                "100.5000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.99.01.01", "ON"),
                "1.5197100000",
            ),
            flow_row(
                "099903",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "5.0000000000",
            ),
        ]
        .join("\r\n");
        let dre_ind = [
            COMPANY_HEADER.to_owned(),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "1.0000000000",
            ),
            flow_row(
                "099902",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "70.0000000000",
            ),
            flow_row(
                "099902",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.11", "Lucro/Prejuízo do Período"),
                "7.0000000000",
            ),
        ]
        .join("\r\n");
        let bpp_con = [
            BALANCE_HEADER.to_owned(),
            balance_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                "2025-12-31",
                ("2.03", "Patrimônio Líquido Consolidado"),
                "5000.0000000000",
            ),
            balance_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                "2025-12-31",
                ("2.01.04", "Empréstimos e Financiamentos"),
                "300.0000000000",
            ),
            balance_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                "2025-12-31",
                ("2.01.01", "Obrigações Sociais e Trabalhistas"),
                "1.0000000000",
            ),
        ]
        .join("\r\n");
        let bpa_con = [
            BALANCE_HEADER.to_owned(),
            balance_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                "2025-12-31",
                ("1.01.01", "Caixa e Equivalentes de Caixa"),
                "200.0000000000",
            ),
        ]
        .join("\r\n");
        let dfc = [
            COMPANY_HEADER.to_owned(),
            // Descrição com aspas no meio, como no arquivo de verdade.
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("6.01.01.11", "Garantia \"Escrow\""),
                "0.0000000000",
            ),
            flow_row(
                "099901",
                "2025-12-31",
                2,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("6.01", "Caixa Líquido Atividades Operacionais"),
                "150.0000000000",
            ),
        ]
        .join("\r\n");
        zip(&[
            (
                "dfp_cia_aberta_2025.csv",
                "CNPJ_CIA;DT_REFER;VERSAO\r\n".to_owned(),
            ),
            ("dfp_cia_aberta_BPA_con_2025.csv", bpa_con),
            ("dfp_cia_aberta_BPA_ind_2025.csv", BALANCE_HEADER.to_owned()),
            ("dfp_cia_aberta_BPP_con_2025.csv", bpp_con),
            ("dfp_cia_aberta_BPP_ind_2025.csv", BALANCE_HEADER.to_owned()),
            ("dfp_cia_aberta_DFC_MI_con_2025.csv", dfc),
            (
                "dfp_cia_aberta_DMPL_con_2025.csv",
                "qualquer coisa".to_owned(),
            ),
            ("dfp_cia_aberta_DRE_con_2025.csv", dre_con),
            ("dfp_cia_aberta_DRE_ind_2025.csv", dre_ind),
        ])
    }

    fn line<'a>(
        filing: &'a Filing,
        statement: Statement,
        column: Column,
        code: &str,
    ) -> Option<&'a Line> {
        filing
            .lines
            .iter()
            .find(|l| l.statement == statement && l.column == column && l.code == code)
    }

    #[test]
    fn reads_only_the_portfolio_companies_in_the_highest_version() {
        let bytes = dfp_2025();
        let filings = parse_company_archive(
            bytes.as_slice(),
            FilingKind::Annual,
            2025,
            &codes(&[99901, 99902]),
        )
        .unwrap();
        assert_eq!(filings.len(), 2);
        let a = &filings[0];
        assert_eq!(a.cvm_code, 99901);
        assert_eq!(a.version, 2);
        assert_eq!(a.scope, Scope::Consolidated);
        assert_eq!(a.company_name, "CIA FICTICIA SA");
        assert_eq!(a.reference_date, date!(2025 - 12 - 31));
        // Em reais (escala MIL), a versão 1 descartada.
        assert_eq!(
            line(a, Statement::Income, Column::Current, "3.01")
                .unwrap()
                .value,
            dec!(1000000)
        );
        assert_eq!(
            line(a, Statement::Income, Column::Previous, "3.01")
                .unwrap()
                .value,
            dec!(900000)
        );
        assert_eq!(
            line(a, Statement::Income, Column::Current, "3.11.01")
                .unwrap()
                .value,
            dec!(100500)
        );
        assert_eq!(
            line(a, Statement::Liabilities, Column::Current, "2.03")
                .unwrap()
                .description,
            "Patrimônio Líquido Consolidado"
        );
        assert_eq!(
            line(a, Statement::CashFlow, Column::Current, "6.01")
                .unwrap()
                .value,
            dec!(150000)
        );
        // Contas que a conta não usa ficam fora.
        assert!(line(a, Statement::Income, Column::Current, "3.04.01").is_none());
        assert!(line(a, Statement::Income, Column::Current, "3.99.01.01").is_none());
        assert!(line(a, Statement::Liabilities, Column::Current, "2.01.01").is_none());
        // A consolidada vence a individual (o 1 da individual não entrou).
        assert!(a.lines.iter().all(|l| l.value != dec!(1000)));
        let b = &filings[1];
        assert_eq!(b.cvm_code, 99902);
        assert_eq!(b.scope, Scope::Individual);
        assert_eq!(
            line(b, Statement::Income, Column::Current, "3.11")
                .unwrap()
                .value,
            dec!(7000)
        );
    }

    #[test]
    fn an_empty_portfolio_reads_nothing() {
        let bytes = dfp_2025();
        assert!(
            parse_company_archive(bytes.as_slice(), FilingKind::Annual, 2025, &HashSet::new())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_quarterly_result_keeps_the_year_to_date() {
        let dre = [
            COMPANY_HEADER.to_owned(),
            flow_row(
                "099901",
                "2026-09-30",
                1,
                "ÚLTIMO",
                ("2026-07-01", "2026-09-30"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "10.0000000000",
            ),
            flow_row(
                "099901",
                "2026-09-30",
                1,
                "ÚLTIMO",
                ("2026-01-01", "2026-09-30"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "30.0000000000",
            ),
            flow_row(
                "099901",
                "2026-09-30",
                1,
                "PENÚLTIMO",
                ("2025-01-01", "2025-09-30"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "25.0000000000",
            ),
            flow_row(
                "099901",
                "2026-09-30",
                1,
                "PENÚLTIMO",
                ("2025-07-01", "2025-09-30"),
                ("3.01", "Receita de Venda de Bens e/ou Serviços"),
                "9.0000000000",
            ),
        ]
        .join("\n");
        let bytes = zip(&[
            ("itr_cia_aberta_BPA_con_2026.csv", BALANCE_HEADER.to_owned()),
            ("itr_cia_aberta_BPP_con_2026.csv", BALANCE_HEADER.to_owned()),
            ("itr_cia_aberta_DRE_con_2026.csv", dre),
        ]);
        let filings = parse_company_archive(
            bytes.as_slice(),
            FilingKind::Quarterly,
            2026,
            &codes(&[99901]),
        )
        .unwrap();
        let f = &filings[0];
        assert_eq!(f.kind, FilingKind::Quarterly);
        let current = line(f, Statement::Income, Column::Current, "3.01").unwrap();
        assert_eq!(current.value, dec!(30000));
        assert_eq!(current.period_start, Some(date!(2026 - 01 - 01)));
        assert_eq!(
            line(f, Statement::Income, Column::Previous, "3.01")
                .unwrap()
                .value,
            dec!(25000)
        );
    }

    #[test]
    fn other_currency_or_scale_is_skipped() {
        let dre = [
            COMPANY_HEADER.to_owned(),
            flow_row(
                "099901",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.01", "Receita"),
                "10",
            )
            .replace("REAL;MIL", "DOLAR;MIL"),
            flow_row(
                "099901",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.05", "Resultado"),
                "10",
            )
            .replace("REAL;MIL", "REAL;MILHAO"),
            flow_row(
                "099901",
                "2025-12-31",
                1,
                "ÚLTIMO",
                ("2025-01-01", "2025-12-31"),
                ("3.11", "Lucro/Prejuízo do Período"),
                "3",
            )
            .replace("REAL;MIL", "REAL;UNIDADE"),
        ]
        .join("\n");
        let bytes = zip(&[
            ("dfp_cia_aberta_BPA_ind_2025.csv", BALANCE_HEADER.to_owned()),
            ("dfp_cia_aberta_BPP_ind_2025.csv", BALANCE_HEADER.to_owned()),
            ("dfp_cia_aberta_DRE_ind_2025.csv", dre),
        ]);
        let filings =
            parse_company_archive(bytes.as_slice(), FilingKind::Annual, 2025, &codes(&[99901]))
                .unwrap();
        let f = &filings[0];
        assert_eq!(f.lines.len(), 1);
        assert_eq!(f.lines[0].value, dec!(3));
    }

    #[test]
    fn a_changed_format_is_an_error_not_an_empty_success() {
        // Sem DRE no zip.
        let bytes = zip(&[("dfp_cia_aberta_BPA_con_2025.csv", BALANCE_HEADER.to_owned())]);
        assert!(matches!(
            parse_company_archive(bytes.as_slice(), FilingKind::Annual, 2025, &codes(&[1])),
            Err(FetchError::Format(_))
        ));
        // Coluna renomeada.
        let bytes = zip(&[
            (
                "dfp_cia_aberta_BPA_con_2025.csv",
                BALANCE_HEADER.replace("CD_CVM", "CODIGO"),
            ),
            ("dfp_cia_aberta_BPP_con_2025.csv", BALANCE_HEADER.to_owned()),
            ("dfp_cia_aberta_DRE_con_2025.csv", COMPANY_HEADER.to_owned()),
        ]);
        assert!(
            parse_company_archive(bytes.as_slice(), FilingKind::Annual, 2025, &codes(&[1]))
                .is_err()
        );
        assert!(
            parse_company_archive(&b"nao e zip"[..], FilingKind::Annual, 2025, &codes(&[1]))
                .is_err()
        );
        // Zip do ano errado: os nomes não batem.
        let bytes = dfp_2025();
        assert!(parse_company_archive(
            bytes.as_slice(),
            FilingKind::Annual,
            2024,
            &codes(&[99901])
        )
        .is_err());
    }

    #[test]
    fn statement_names_inside_the_zip() {
        assert_eq!(
            statement_entry(
                "dfp_cia_aberta_DFC_MD_ind_2024.csv",
                FilingKind::Annual,
                2024
            ),
            Some((Statement::CashFlow, Scope::Individual))
        );
        assert_eq!(
            statement_entry(
                "itr_cia_aberta_DVA_con_2026.csv",
                FilingKind::Quarterly,
                2026
            ),
            Some((Statement::ValueAdded, Scope::Consolidated))
        );
        assert_eq!(
            statement_entry("dfp_cia_aberta_DMPL_con_2024.csv", FilingKind::Annual, 2024),
            None
        );
        assert_eq!(
            statement_entry("dfp_cia_aberta_2024.csv", FilingKind::Annual, 2024),
            None
        );
        assert_eq!(
            statement_entry("itr_cia_aberta_DRE_con_2026.csv", FilingKind::Annual, 2026),
            None
        );
    }

    const REGISTRY_HEADER: &str = "CNPJ_CIA;DENOM_SOCIAL;DENOM_COMERC;DT_REG;DT_CONST;DT_CANCEL;MOTIVO_CANCEL;SIT;DT_INI_SIT;CD_CVM;SETOR_ATIV;TP_MERC;CATEG_REG;DT_INI_CATEG;SIT_EMISSOR;DT_INI_SIT_EMISSOR;CONTROLE_ACIONARIO;TP_ENDER";

    #[test]
    fn the_registry_prefers_the_active_row() {
        let text = [
            REGISTRY_HEADER,
            "00.000.000/0001-00;CIA FICTICIA S.A.;FICTICIA;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99901;Máquinas;BOLSA;Categoria A;2010-01-01;FASE OPERACIONAL;2010-01-01;PRIVADO;SEDE",
            "00.000.000/0001-00;CIA FICTICIA S.A.;FICTICIA;2010-01-01;2000-01-01;2015-01-01;;CANCELADA;2015-01-01;99901;Máquinas;BOLSA;Categoria A;2010-01-01;FASE OPERACIONAL;2010-01-01;PRIVADO;SEDE",
            "11.111.111/0001-11;OUTRA FICTICIA S.A. - EM RECUPERAÇÃO JUDICIAL;OUTRA;2010-01-01;2000-01-01;;;ATIVO;2010-01-01;99902;Energia Elétrica;BOLSA;Categoria A;2010-01-01;EM RECUPERAÇÃO JUDICIAL OU EQUIVALENTE;2024-04-23;PRIVADO;SEDE \"B\"",
            "22.222.222/0001-22;SEM CODIGO;X;2010-01-01;;;;ATIVO;2010-01-01;;;;;;;;;",
        ]
        .join("\r\n");
        let list = parse_registry(encode(&text).as_slice()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].status, "ATIVO");
        assert_eq!(list[0].sector.as_deref(), Some("Máquinas"));
        assert!(!list[0].in_judicial_recovery());
        assert!(list[1].in_judicial_recovery());
        assert_eq!(
            list[1].name,
            "OUTRA FICTICIA S.A. - EM RECUPERAÇÃO JUDICIAL"
        );
        assert!(parse_registry(encode(REGISTRY_HEADER).as_slice()).is_err());
    }

    const GENERAL_HEADER: &str = "Tipo_Fundo_Classe;CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Entrega;Nome_Fundo_Classe;Data_Funcionamento;Publico_Alvo;Codigo_ISIN;Quantidade_Cotas_Emitidas";
    const COMPLEMENT_HEADER: &str = "CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Informacao_Numero_Cotistas;Total_Numero_Cotistas;Valor_Ativo;Patrimonio_Liquido;Cotas_Emitidas;Valor_Patrimonial_Cotas";

    #[test]
    fn the_monthly_report_gives_equity_and_shares_of_the_newest_month() {
        let general = [
            GENERAL_HEADER,
            "FII;00.000.000/0001-91;2026-09-01;1;2026-10-10;FUNDO FICTICIO FII;2010-01-01;Geral;BRFICTCTF000;100",
            "FII;00.000.000/0001-91;2026-10-01;1;2026-11-10;FUNDO FICTICIO FII;2010-01-01;Geral;BRFICTCTF000;100",
            "FII;11.111.111/0001-91;2026-10-01;1;2026-11-10;FUNDO SEM ISIN;2010-01-01;Geral;;100",
        ]
        .join("\r\n");
        let complement = [
            COMPLEMENT_HEADER,
            "00.000.000/0001-91;2026-09-01;1;2026-09-01;10;0;900000.00;10000;90",
            "00.000.000/0001-91;2026-10-01;1;2026-10-01;10;0;1000000.50;10000;100",
            "00.000.000/0001-91;2026-10-01;2;2026-10-01;10;0;1100000;10000;110",
            "11.111.111/0001-91;2026-10-01;1;2026-10-01;10;0;5;5;1",
        ]
        .join("\r\n");
        let bytes = zip(&[
            ("inf_mensal_fii_ativo_passivo_2026.csv", "x".to_owned()),
            ("inf_mensal_fii_complemento_2026.csv", complement),
            ("inf_mensal_fii_geral_2026.csv", general),
        ]);
        let reports = parse_fund_monthly(bytes.as_slice(), 2026).unwrap();
        assert_eq!(reports.len(), 1);
        let r = &reports[0];
        assert_eq!(r.issuer, "FICT");
        assert_eq!(r.reference_date, date!(2026 - 10 - 01));
        // A reapresentação (versão 2) vence.
        assert_eq!(r.version, 2);
        assert_eq!(r.net_assets, Some(dec!(1100000)));
        assert_eq!(r.shares, Some(dec!(10000)));
        assert!(parse_fund_monthly(
            zip(&[("inf_mensal_fii_geral_2026.csv", GENERAL_HEADER.to_owned())]).as_slice(),
            2026
        )
        .is_err());
    }

    const QUARTER_GENERAL_HEADER: &str =
        "Tipo_Fundo_Classe;CNPJ_Fundo_Classe;Data_Referencia;Versao;Data_Entrega;Nome_Fundo_Classe";
    const PROPERTY_HEADER: &str = "CNPJ_Fundo_Classe;Data_Referencia;Versao;Classe;Nome_Imovel;Endereco;Area;Numero_Unidades;Outras_Caracteristicas_Relevantes;Percentual_Vacancia;Percentual_Inadimplencia;Percentual_Receitas_FII";

    #[test]
    fn the_quarterly_report_lists_the_finished_rental_properties_of_the_newest_quarter() {
        let general = [
            QUARTER_GENERAL_HEADER,
            "FII;00.000.000/0001-91;2026-06-30;1;2026-08-01;FUNDO FICTICIO FII",
            "FII;00.000.000/0001-91;2026-09-30;1;2026-11-01;FUNDO FICTICIO FII",
            "FII;22.222.222/0001-91;2026-09-30;1;2026-11-01;FUNDO DE PAPEL FII",
        ]
        .join("\r\n");
        let properties = [
            PROPERTY_HEADER,
            "00.000.000/0001-91;2026-06-30;1;Imóveis para renda acabados;Galpão A;Rua X;1000;1;;0.5;0;0.5",
            "00.000.000/0001-91;2026-09-30;1;Imóveis para renda acabados;Galpão A;Rua X;1000;1;;0;0;0.5",
            "00.000.000/0001-91;2026-09-30;1;Imóveis para renda acabados;Galpão B;\"Bairro\", Cidade;3000;1;;0.2;0;0.5",
            "00.000.000/0001-91;2026-09-30;1;Imóveis para venda acabados;Lote;Rua Y;500;1;;1;0;0",
        ]
        .join("\r\n");
        let bytes = zip(&[
            ("inf_trimestral_fii_geral_2026.csv", general),
            ("inf_trimestral_fii_imovel_2026.csv", properties),
        ]);
        let quarters = parse_fund_quarterly(bytes.as_slice(), 2026).unwrap();
        assert_eq!(quarters.len(), 2);
        let brick = &quarters[0];
        assert_eq!(brick.reference_date, date!(2026 - 09 - 30));
        assert_eq!(brick.properties.len(), 2);
        let (vacancy, n) = crate::domain::fundamentals::physical_vacancy(&brick.properties);
        assert_eq!(vacancy.value(), Some(dec!(0.15)));
        assert_eq!(n, 2);
        assert!(quarters[1].properties.is_empty());
    }
}
