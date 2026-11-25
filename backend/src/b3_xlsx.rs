//! Abre uma planilha `.xlsx` que já está na memória e entrega as células.
//!
//! O arquivo é dado sensível (a carteira inteira da pessoa): vem do corpo da
//! requisição, é lido a partir de `&[u8]` e nunca toca o disco. Nenhum erro
//! daqui carrega conteúdo do arquivo, e nada é escrito em log.
//!
//! A leitura é em fluxo, célula a célula (`worksheet_cells_reader` do
//! `calamine`), em vez de montar a grade densa da aba: uma planilha com uma
//! célula perdida em `XFD1048576` custaria dezenas de gigabytes na grade, e
//! aqui custa uma célula. Os tetos abaixo existem para que arquivo malicioso ou
//! absurdo vire erro e não esgote a memória (o binário de produção aborta em
//! pânico).

use std::io::Cursor;

use calamine::{DataRef, Reader, Xlsx};
use rust_decimal::Decimal;

use crate::domain::b3_import::{RawCell, RawRow, RawSheet};

/// Entradas do zip (um `.xlsx` real tem uma dezena).
pub const MAX_ENTRIES: usize = 1_000;
/// Soma dos tamanhos descompactados declarados no zip.
pub const MAX_UNCOMPRESSED: u64 = 64 * 1024 * 1024;
/// Abas lidas por arquivo.
pub const MAX_SHEETS: usize = 20;
/// Última linha aceita. A B3 limita a consulta a poucas dezenas de milhares.
pub const MAX_ROWS: u32 = 200_000;
/// Colunas lidas; o que passa disso é ignorado (o extrato tem no máximo dez).
pub const MAX_COLUMNS: u32 = 64;
/// Células lidas por arquivo.
pub const MAX_CELLS: usize = 2_000_000;
/// Texto de célula acima disso é cortado.
const MAX_CELL_CHARS: usize = 500;

/// Por que o arquivo não abriu. As mensagens vão para a pessoa, sem acento
/// (texto gerado no servidor) e sem ecoar nada do arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// Não é um `.xlsx` (outro formato, texto, imagem).
    NotXlsx,
    /// `.xls` antigo (Excel 97-2003).
    LegacyXls,
    /// Zip ou XML quebrado: arquivo corrompido ou cortado.
    Corrupt,
    /// Passa dos tetos de tamanho ou de linhas.
    TooLarge,
}

impl ReadError {
    pub fn message(self) -> &'static str {
        match self {
            ReadError::NotXlsx => "Este arquivo nao e uma planilha .xlsx. Envie o arquivo que a Area do Investidor exporta, sem converter.",
            ReadError::LegacyXls => "Este e o formato antigo do Excel (.xls). Na Area do Investidor, baixe a planilha em Excel (.xlsx).",
            ReadError::Corrupt => "Nao consegui abrir a planilha: ela parece corrompida ou cortada. Baixe de novo na Area do Investidor.",
            ReadError::TooLarge => "A planilha e grande demais para ler. Baixe um periodo menor na Area do Investidor.",
        }
    }
}

/// Lê todas as abas visíveis de um `.xlsx`.
pub fn read_workbook(bytes: &[u8]) -> Result<Vec<RawSheet>, ReadError> {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(ReadError::LegacyXls);
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(ReadError::NotXlsx);
    }
    check_zip(bytes)?;
    let mut workbook: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).map_err(|_| ReadError::Corrupt)?;
    let names = workbook.sheet_names();
    if names.len() > MAX_SHEETS {
        return Err(ReadError::TooLarge);
    }
    let mut cells_left = MAX_CELLS;
    let mut sheets = Vec::with_capacity(names.len());
    for name in names {
        let rows = read_sheet(&mut workbook, &name, &mut cells_left)?;
        sheets.push(RawSheet { name, rows });
    }
    Ok(sheets)
}

/// Confere o zip antes de descompactar: entradas demais ou tamanho declarado
/// grande demais é bomba de descompressão.
fn check_zip(bytes: &[u8]) -> Result<(), ReadError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| ReadError::Corrupt)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ReadError::TooLarge);
    }
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| ReadError::Corrupt)?;
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED {
            return Err(ReadError::TooLarge);
        }
    }
    Ok(())
}

fn read_sheet(
    workbook: &mut Xlsx<Cursor<&[u8]>>,
    name: &str,
    cells_left: &mut usize,
) -> Result<Vec<RawRow>, ReadError> {
    let mut reader = workbook
        .worksheet_cells_reader(name)
        .map_err(|_| ReadError::Corrupt)?;
    let mut rows: std::collections::BTreeMap<u32, Vec<RawCell>> = std::collections::BTreeMap::new();
    while let Some(cell) = reader.next_cell().map_err(|_| ReadError::Corrupt)? {
        let (row, column) = cell.get_position();
        let value = convert(cell.get_value());
        if value == RawCell::Empty || column >= MAX_COLUMNS {
            continue;
        }
        if row >= MAX_ROWS {
            return Err(ReadError::TooLarge);
        }
        *cells_left = cells_left.checked_sub(1).ok_or(ReadError::TooLarge)?;
        let cells = rows.entry(row).or_default();
        let at = column as usize;
        if cells.len() <= at {
            cells.resize(at + 1, RawCell::Empty);
        }
        cells[at] = value;
    }
    Ok(rows
        .into_iter()
        .map(|(row, cells)| RawRow {
            line: row + 1,
            cells,
        })
        .collect())
}

fn convert(value: &DataRef<'_>) -> RawCell {
    match value {
        DataRef::Int(number) => RawCell::Number(Decimal::from(*number)),
        DataRef::Float(number) => float_cell(*number),
        DataRef::DateTime(date) => float_cell(date.as_f64()),
        DataRef::String(text) => text_cell(text),
        DataRef::SharedString(text) => text_cell(text),
        DataRef::DateTimeIso(text) | DataRef::DurationIso(text) => text_cell(text),
        DataRef::Bool(_) | DataRef::Error(_) | DataRef::Empty => RawCell::Empty,
    }
}

/// O Excel guarda número em ponto flutuante (`10.120000000000001`): arredonda
/// em 8 casas, que cobre preço e quantidade fracionária de cripto.
fn float_cell(number: f64) -> RawCell {
    let text = number.to_string();
    match text
        .parse::<Decimal>()
        .or_else(|_| Decimal::from_scientific(&text))
    {
        Ok(decimal) => RawCell::Number(decimal.round_dp(8).normalize()),
        Err(_) => RawCell::Empty,
    }
}

fn text_cell(text: &str) -> RawCell {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return RawCell::Empty;
    }
    RawCell::Text(trimmed.chars().take(MAX_CELL_CHARS).collect())
}
