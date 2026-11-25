//! Importação do extrato da B3 (Área do Investidor): o que as planilhas dizem,
//! já normalizado, e a prévia do que entraria na carteira.
//!
//! Este módulo é puro: recebe células já lidas ([`RawSheet`]) e devolve linhas
//! normalizadas. Abrir o `.xlsx` é com `crate::b3_xlsx`; a rota HTTP é com
//! `http::b3_import`. Nada daqui grava em banco, em disco ou em log.
//!
//! # Fontes dos formatos
//!
//! A B3 não publica o esquema das planilhas que a Área do Investidor
//! (investidor.b3.com.br, menu Extratos) exporta. Os cabeçalhos e os rótulos
//! abaixo vêm de leitores abertos que processam esses arquivos e de planilhas
//! de exemplo deles; o contrato foi confrontado entre as fontes:
//!
//! - `github.com/MarceloPCF/COIR`, `coir/importers/b3.py`: cabeçalhos de
//!   Negociação e Movimentação, datas em texto `dd/mm/aaaa`, `-` como vazio,
//!   número como float ou texto com ponto de milhar, `Credito`/`Debito`.
//! - `github.com/4nier-app/patrimonio`, `crates/providers/src/b3_extrato.rs`:
//!   mercado "à Vista"/"Fracionário" (código com `F`), rótulos da Movimentação
//!   (Dividendo, Juros Sobre Capital Próprio, Rendimento, Desdobro,
//!   Bonificação em Ativos, "- Transferido").
//! - `github.com/tbc174/tickerlab`, `scripts/gerar_planilhas_de_exemplo.py`:
//!   planilhas fictícias nos três formatos, incluindo Proventos a Receber
//!   (`Previsão de pagamento`, `-` sem data, linha `Total líquido` no fim),
//!   Tesouro Direto (`Tesouro Selic 2029`, `COMPRA / VENDA`) e Empréstimo.
//! - `github.com/MuriloScarpaSitonio/multi-sources-financial-control`,
//!   `integrations/b3/proventos.py`: aba `Proventos Recebidos` com `Produto`,
//!   `Pagamento`, `Tipo de Evento`, `Valor líquido`.
//! - `github.com/larahfelipe/ex3`, `docs/transaction-import.md`: colunas dos três
//!   extratos reunidas e a regra de que a liquidação da Movimentação repete a
//!   Negociação e o provento aparece nas duas planilhas.
//! - `github.com/joao-sallaberry/portfolio-tracker`, `src/lib/b3-movimentacao.ts`:
//!   vocabulário de movimentações (Fração em Ativos, Leilão de Fração,
//!   Atualização, Direitos de Subscrição, Cessão de Direitos, Resgate).
//!
//! Nenhum arquivo real foi usado: os testes montam planilhas fictícias.
//!
//! # Identidade estável (`external_ref`)
//!
//! Toda linha normalizada leva um `external_ref`: `b3:v1:<tipo>:<hash>`, com o
//! hash (SHA-256, 128 bits) do conteúdo da linha. A onda de gravação usa esse
//! texto como chave única para a importação ser idempotente:
//!
//! - não entra o nome do arquivo: períodos que se sobrepõem em dois arquivos
//!   dão o mesmo `external_ref` para o mesmo negócio;
//! - linhas idênticas dentro de um arquivo (duas compras iguais no mesmo dia)
//!   recebem um contador de ocorrência e continuam distintas;
//! - o provento não leva a origem (Movimentação, Proventos recebidos ou
//!   provisionados) nem a corretora: o mesmo pagamento nas duas planilhas é
//!   uma linha só.

use std::collections::{BTreeMap, HashMap, HashSet};

use aws_lc_rs::digest;
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, Duration, Month};

use crate::domain::asset_class::AssetClass;
use crate::quotes::classify_b3;

/// Versão do formato de `external_ref`. Muda só se o conteúdo do hash mudar.
pub const REF_VERSION: &str = "v1";

/// Teto de módulo para número lido (10^15): lixo não vira overflow.
fn max_magnitude() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

// ---------------------------------------------------------------------------
// Células cruas
// ---------------------------------------------------------------------------

/// Uma célula como a planilha a guarda. Data do Excel chega como número de
/// série (dias desde 30/12/1899).
#[derive(Debug, Clone, PartialEq)]
pub enum RawCell {
    Empty,
    Text(String),
    Number(Decimal),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawRow {
    /// Número da linha na planilha (a primeira é 1).
    pub line: u32,
    pub cells: Vec<RawCell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawSheet {
    pub name: String,
    pub rows: Vec<RawRow>,
}

// ---------------------------------------------------------------------------
// Texto, número e data em pt-BR
// ---------------------------------------------------------------------------

/// Texto comparável: minúsculo, sem acento, espaços colapsados. É assim que os
/// cabeçalhos e os rótulos da B3 são comparados, para tolerar `Instituição` e
/// `INSTITUICAO`, espaço a mais e espaço sem quebra.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch == '\u{feff}' || ch == '\u{200b}' {
            continue;
        }
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        let plain = match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other,
        };
        out.extend(plain.to_lowercase());
    }
    out
}

/// Célula em branco: vazia, só espaço ou o `-` com que a B3 marca "sem valor".
fn is_blank(cell: &RawCell) -> bool {
    match cell {
        RawCell::Empty => true,
        RawCell::Text(text) => matches!(text.trim(), "" | "-" | "--" | "—"),
        RawCell::Number(_) => false,
    }
}

fn cell_text(cell: &RawCell) -> String {
    match cell {
        RawCell::Empty => String::new(),
        RawCell::Text(text) => text.split_whitespace().collect::<Vec<_>>().join(" "),
        RawCell::Number(number) => number.normalize().to_string(),
    }
}

/// Número em pt-BR (`1.234,56`, `R$ 12,5`, `-3,4`) ou em ponto decimal que o
/// Excel às vezes grava (`12.48`). Ponto seguido de exatamente três dígitos,
/// em grupos, é separador de milhar (`1.000` é mil).
pub fn parse_decimal_pt_br(text: &str) -> Option<Decimal> {
    let cleaned: String = text
        .trim()
        .trim_start_matches("R$")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let (negative, body) = match cleaned.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, cleaned.as_str()),
    };
    if body.is_empty()
        || !body
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    {
        // Notação científica que o Excel grava em texto (`1E-3`).
        let value = Decimal::from_scientific(&cleaned).ok()?;
        return (value.abs() <= max_magnitude()).then_some(value);
    }
    let canonical = if body.contains(',') {
        if body.matches(',').count() > 1 {
            return None;
        }
        body.replace('.', "").replace(',', ".")
    } else if body.contains('.') {
        let groups: Vec<&str> = body.split('.').collect();
        let thousands = groups.len() > 1
            && !groups[0].is_empty()
            && groups[0].len() <= 3
            && groups[1..].iter().all(|g| g.len() == 3);
        if thousands {
            body.replace('.', "")
        } else if groups.len() == 2 && !groups[0].is_empty() && !groups[1].is_empty() {
            body.to_owned()
        } else {
            return None;
        }
    } else {
        body.to_owned()
    };
    let value: Decimal = canonical.parse().ok()?;
    let value = if negative { -value } else { value };
    (value.abs() <= max_magnitude()).then_some(value)
}

/// Data em `dd/mm/aaaa` (com hora opcional) ou ISO, ou número de série do Excel.
pub fn parse_date_cell(cell: &RawCell) -> Option<Date> {
    let date = match cell {
        RawCell::Empty => return None,
        RawCell::Number(serial) => {
            let days = serial.floor().to_string().parse::<i64>().ok()?;
            let epoch = Date::from_calendar_date(1899, Month::December, 30).ok()?;
            epoch.checked_add(Duration::days(days))?
        }
        RawCell::Text(text) => parse_date_text(text)?,
    };
    (1990..=2100).contains(&date.year()).then_some(date)
}

fn parse_date_text(text: &str) -> Option<Date> {
    let token = text.split_whitespace().next()?;
    let token = token.split('T').next().unwrap_or(token);
    if let Some((year, rest)) = token.split_once('-') {
        let (month, day) = rest.split_once('-')?;
        return date_from(year, month, day);
    }
    let mut parts = token.split('/');
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    date_from(year, month, day)
}

fn date_from(year: &str, month: &str, day: &str) -> Option<Date> {
    let year: i32 = year.parse().ok().filter(|_| year.len() == 4)?;
    let month = Month::try_from(month.parse::<u8>().ok()?).ok()?;
    Date::from_calendar_date(year, month, day.parse().ok()?).ok()
}

fn iso(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

mod iso_date {
    use serde::Serializer;
    use time::Date;

    pub fn serialize<S: Serializer>(date: &Date, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::iso(*date))
    }

    pub mod option {
        use super::*;

        pub fn serialize<S: Serializer>(
            date: &Option<Date>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match date {
                Some(date) => super::serialize(date, serializer),
                None => serializer.serialize_none(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Produto e código
// ---------------------------------------------------------------------------

/// `ABCD3`, `KNRI11`, `B3SA3`: quatro caracteres e um ou dois dígitos.
pub fn is_ticker(code: &str) -> bool {
    let bytes = code.as_bytes();
    if !(5..=6).contains(&bytes.len()) {
        return false;
    }
    let (head, tail) = bytes.split_at(4);
    head.iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && tail.iter().all(u8::is_ascii_digit)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProductKind {
    Ticker,
    Treasury,
    Derivative,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Product {
    kind: ProductKind,
    /// Código negociado (`PETR4`) ou o nome do título do Tesouro.
    key: String,
    /// Nome depois do código; vazio quando a planilha só traz o código.
    name: String,
}

/// O "Produto" da B3 vem como `PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS`.
/// Tesouro Direto vem sem código (`Tesouro Selic 2029`).
fn parse_product(raw: &str) -> Product {
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let head_folded = fold(text.split(" - ").next().unwrap_or(&text));
    if head_folded.starts_with("tesouro") {
        return Product {
            kind: ProductKind::Treasury,
            key: text.to_uppercase(),
            name: text,
        };
    }
    if ["opcao", "futuro", "termo"]
        .iter()
        .any(|word| head_folded.starts_with(word))
    {
        return Product {
            kind: ProductKind::Derivative,
            key: text.to_uppercase(),
            name: text,
        };
    }
    let first = text.split_whitespace().next().unwrap_or("").to_uppercase();
    if is_ticker(&first) {
        let name = match text.split_once(" - ") {
            Some((_, rest)) => rest.trim().to_owned(),
            None => text[first.len().min(text.len())..].trim().to_owned(),
        };
        return Product {
            kind: ProductKind::Ticker,
            key: first,
            name,
        };
    }
    Product {
        kind: ProductKind::Other,
        key: text.to_uppercase(),
        name: text,
    }
}

// ---------------------------------------------------------------------------
// Modelo normalizado
// ---------------------------------------------------------------------------

/// Qual planilha a B3 exportou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// Negociação: compras e vendas.
    Trades,
    /// Proventos recebidos (pagos).
    IncomeReceived,
    /// Movimentação: todo débito e crédito da custódia.
    Movements,
    /// Proventos a receber (provisionados), com a data prevista.
    IncomeProvisioned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeMarket {
    /// Mercado à vista (lote padrão).
    Spot,
    /// Mercado fracionário (código com `F`, que sai do `code`).
    Fractional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// `Credito`: entrou na custódia ou na conta.
    Credit,
    /// `Debito`: saiu.
    Debit,
}

/// Compra ou venda da planilha de Negociação. Não traz corretagem nem taxas.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradeLine {
    pub external_ref: String,
    #[serde(with = "iso_date")]
    pub date: Date,
    pub side: TradeSide,
    pub market: TradeMarket,
    /// Código sem o `F` do fracionário.
    pub code: String,
    pub institution: Option<String>,
    #[serde(with = "rust_decimal::serde::float")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub unit_price: Decimal,
    /// Valor da planilha (quantidade × preço, arredondado pela B3).
    #[serde(with = "rust_decimal::serde::float")]
    pub total: Decimal,
    /// Linha na planilha, para a pessoa achar.
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Desdobro: a planilha traz as cotas creditadas, não o fator.
    Split,
    /// Grupamento.
    ReverseSplit,
    /// Bonificação em ativos (cotas ou ações recebidas, com custo informado).
    Bonus,
    /// Fração em Ativos: sobras de um evento, antes do leilão.
    FractionInKind,
    /// Amortização ou restituição de capital: devolve dinheiro, reduz o custo.
    Amortization,
    /// Resgate (fundo ou título).
    Redemption,
    /// Vencimento de título.
    Maturity,
    /// Qualquer movimento do Tesouro Direto (compra, venda, juros, resgate). A
    /// frente do Tesouro trata o preço; aqui só se marca.
    TreasuryMovement,
}

/// Evento que muda a quantidade ou o custo sem ser compra nem venda.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventLine {
    pub external_ref: String,
    #[serde(with = "iso_date")]
    pub date: Date,
    pub kind: EventKind,
    pub direction: Direction,
    /// `None` no Tesouro Direto (não tem código de negociação).
    pub code: Option<String>,
    /// Nome do produto como a B3 escreve.
    pub product: String,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub quantity: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub unit_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub value: Option<Decimal>,
    pub institution: Option<String>,
    /// A movimentação como a B3 escreve ("Bonificação em Ativos").
    pub description: String,
    /// Título do Tesouro Direto: renda fixa, preço pela frente do Tesouro.
    pub treasury: bool,
    pub source: FileKind,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeKind {
    Dividend,
    /// Juros sobre capital próprio. O valor da planilha já é o líquido (IR de
    /// 15% retido na fonte).
    Jcp,
    /// Rendimento de fundo imobiliário (e de outros fundos).
    Yield,
    /// Reembolso de provento de ação emprestada.
    Reimbursement,
    /// Leilão de frações: dinheiro da venda das sobras de um evento.
    FractionAuction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeStatus {
    Received,
    Provisioned,
}

/// Provento recebido ou a receber. O valor é sempre o líquido.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IncomeLine {
    pub external_ref: String,
    pub kind: IncomeKind,
    pub status: IncomeStatus,
    /// Data de pagamento; na planilha de provisionados pode ser `-`.
    #[serde(with = "iso_date::option")]
    pub date: Option<Date>,
    pub code: String,
    pub product: String,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub quantity: Option<Decimal>,
    /// Valor por cota da planilha (bruto na maioria dos eventos).
    #[serde(with = "rust_decimal::serde::float_option")]
    pub unit_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float")]
    pub net_amount: Decimal,
    pub institution: Option<String>,
    pub source: FileKind,
    pub line: u32,
}

// ---------------------------------------------------------------------------
// Motivos
// ---------------------------------------------------------------------------

/// Por que uma linha ficou de fora. `code()` é o contrato; `text()` é o
/// texto do servidor (sem acento), que a tela troca pelo dela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reason {
    // Não entendida.
    InvalidDate,
    InvalidNumber,
    UnknownSide,
    UnsupportedMarket,
    InvalidCode,
    UnknownDirection,
    UnknownMovement,
    Update,
    Subscription,
    NoCode,
    MissingValue,
    MissingQuantity,
    UnknownIncomeType,
    IncomeDebit,
    // Entendida, mas fora da importação.
    Settlement,
    CustodyTransfer,
    Transferred,
    Lending,
    DuplicateInRequest,
}

impl Reason {
    pub fn code(self) -> &'static str {
        match self {
            Reason::InvalidDate => "invalid_date",
            Reason::InvalidNumber => "invalid_number",
            Reason::UnknownSide => "unknown_side",
            Reason::UnsupportedMarket => "unsupported_market",
            Reason::InvalidCode => "invalid_code",
            Reason::UnknownDirection => "unknown_direction",
            Reason::UnknownMovement => "unknown_movement",
            Reason::Update => "update",
            Reason::Subscription => "subscription",
            Reason::NoCode => "no_code",
            Reason::MissingValue => "missing_value",
            Reason::MissingQuantity => "missing_quantity",
            Reason::UnknownIncomeType => "unknown_income_type",
            Reason::IncomeDebit => "income_debit",
            Reason::Settlement => "settlement",
            Reason::CustodyTransfer => "custody_transfer",
            Reason::Transferred => "transferred",
            Reason::Lending => "lending",
            Reason::DuplicateInRequest => "duplicate_in_request",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Reason::InvalidDate => "data ilegivel",
            Reason::InvalidNumber => "quantidade, preco ou valor ilegivel ou zerado",
            Reason::UnknownSide => "tipo de movimentacao diferente de Compra e Venda",
            Reason::UnsupportedMarket => "mercado ainda nao suportado (opcoes, termo, futuro)",
            Reason::InvalidCode => "codigo de negociacao ilegivel",
            Reason::UnknownDirection => "entrada/saida diferente de Credito e Debito",
            Reason::UnknownMovement => "movimentacao que o Prumo ainda nao entende",
            Reason::Update => "atualizacao: pode ter mudado a quantidade, confira e lance a mao",
            Reason::Subscription => "subscricao ou cessao de direitos: ainda nao suportado",
            Reason::NoCode => {
                "produto sem codigo de negociacao (renda fixa privada, opcao ou futuro)"
            }
            Reason::MissingValue => "linha sem o valor",
            Reason::MissingQuantity => "linha sem a quantidade",
            Reason::UnknownIncomeType => "tipo de evento que o Prumo ainda nao entende",
            Reason::IncomeDebit => "provento debitado, nao recebido",
            Reason::Settlement => "compra ou venda liquidada: vem da planilha de Negociacao",
            Reason::CustodyTransfer => "transferencia de custodia entre corretoras",
            Reason::Transferred => "repete outro lancamento da planilha",
            Reason::Lending => "emprestimo de ativos (aluguel)",
            Reason::DuplicateInRequest => "ja veio em outra planilha enviada",
        }
    }
}

/// Linha que o Prumo não entendeu: vai para a pessoa decidir.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnrecognizedRow {
    /// Índice do arquivo no envio.
    pub file: usize,
    pub sheet: String,
    pub line: u32,
    pub reason_code: &'static str,
    pub reason: &'static str,
    /// Células preenchidas, separadas por " | ", cortadas em 200 caracteres.
    pub text: String,
}

/// Linhas que o Prumo entendeu e deixou de fora, contadas por motivo.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IgnoredSummary {
    pub reason_code: &'static str,
    pub reason: &'static str,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// Detecção da planilha pelos cabeçalhos
// ---------------------------------------------------------------------------

struct ColumnSpec {
    /// Nome que a mensagem de erro mostra (sem acento).
    label: &'static str,
    /// Cabeçalhos aceitos, já dobrados por [`fold`].
    aliases: &'static [&'static str],
    required: bool,
}

const fn column(
    label: &'static str,
    aliases: &'static [&'static str],
    required: bool,
) -> ColumnSpec {
    ColumnSpec {
        label,
        aliases,
        required,
    }
}

// A ordem das colunas de cada tabela é a ordem em que os leitores abaixo as
// pegam (`Columns::at`).
const TRADE_COLUMNS: [ColumnSpec; 9] = [
    column("Data do Negocio", &["data do negocio"], true),
    column(
        "Tipo de Movimentacao",
        &["tipo de movimentacao", "tipo de operacao"],
        true,
    ),
    column("Mercado", &["mercado"], true),
    column(
        "Prazo/Vencimento",
        &["prazo/vencimento", "prazo / vencimento", "vencimento"],
        false,
    ),
    column("Instituicao", &["instituicao"], false),
    column(
        "Codigo de Negociacao",
        &["codigo de negociacao", "codigo de negociacao do ativo"],
        true,
    ),
    column("Quantidade", &["quantidade"], true),
    column("Preco", &["preco", "preco unitario"], true),
    column("Valor", &["valor", "valor da operacao"], false),
];

const MOVEMENT_COLUMNS: [ColumnSpec; 8] = [
    column("Entrada/Saida", &["entrada/saida", "entrada / saida"], true),
    column("Data", &["data"], true),
    column("Movimentacao", &["movimentacao"], true),
    column("Produto", &["produto"], true),
    column("Instituicao", &["instituicao"], false),
    column("Quantidade", &["quantidade"], true),
    column("Preco unitario", &["preco unitario", "preco"], false),
    column("Valor da Operacao", &["valor da operacao", "valor"], false),
];

const INCOME_COLUMNS: [ColumnSpec; 7] = [
    column("Produto", &["produto"], true),
    column("Tipo de Evento", &["tipo de evento", "evento"], true),
    column(
        "Pagamento",
        &["pagamento", "data de pagamento", "data do pagamento"],
        false,
    ),
    column(
        "Previsao de pagamento",
        &[
            "previsao de pagamento",
            "previsao de recebimento",
            "data prevista de pagamento",
        ],
        false,
    ),
    column("Instituicao", &["instituicao"], false),
    column("Quantidade", &["quantidade"], false),
    column("Preco unitario", &["preco unitario"], false),
];
const INCOME_NET: ColumnSpec = column("Valor liquido", &["valor liquido"], true);

/// Colunas achadas na linha de cabeçalho, na ordem da tabela de especificação.
struct Columns {
    index: Vec<Option<usize>>,
}

impl Columns {
    fn resolve(header: &[String], specs: &[ColumnSpec]) -> Self {
        let index = specs
            .iter()
            .map(|spec| {
                header
                    .iter()
                    .position(|cell| spec.aliases.contains(&cell.as_str()))
            })
            .collect();
        Self { index }
    }

    fn at(&self, position: usize) -> Option<usize> {
        self.index.get(position).copied().flatten()
    }

    fn missing(&self, specs: &[ColumnSpec]) -> Vec<&'static str> {
        specs
            .iter()
            .zip(&self.index)
            .filter(|(spec, found)| spec.required && found.is_none())
            .map(|(spec, _)| spec.label)
            .collect()
    }
}

fn has(header: &[String], name: &str) -> bool {
    header.iter().any(|cell| cell == name)
}

enum Detected {
    Trades(Columns),
    Movements(Columns),
    Income(Columns, Columns, IncomeStatus),
    /// Reconheci o tipo, mas faltam colunas.
    Incomplete(FileKind, Vec<&'static str>),
    /// Planilha de Posição ou consolidado: existe, mas não é lida nesta versão.
    Position,
}

fn detect_header(header: &[String]) -> Option<Detected> {
    if has(header, "data do negocio") && has(header, "codigo de negociacao") {
        let columns = Columns::resolve(header, &TRADE_COLUMNS);
        let missing = columns.missing(&TRADE_COLUMNS);
        return Some(if missing.is_empty() {
            Detected::Trades(columns)
        } else {
            Detected::Incomplete(FileKind::Trades, missing)
        });
    }
    if has(header, "entrada/saida") || has(header, "entrada / saida") {
        let columns = Columns::resolve(header, &MOVEMENT_COLUMNS);
        let missing = columns.missing(&MOVEMENT_COLUMNS);
        return Some(if missing.is_empty() {
            Detected::Movements(columns)
        } else {
            Detected::Incomplete(FileKind::Movements, missing)
        });
    }
    if has(header, "tipo de evento") && has(header, "produto") {
        let columns = Columns::resolve(header, &INCOME_COLUMNS);
        let net = Columns::resolve(header, std::slice::from_ref(&INCOME_NET));
        let (status, kind) = if columns.at(2).is_some() {
            (IncomeStatus::Received, FileKind::IncomeReceived)
        } else {
            (IncomeStatus::Provisioned, FileKind::IncomeProvisioned)
        };
        let mut missing = columns.missing(&INCOME_COLUMNS);
        missing.extend(net.missing(std::slice::from_ref(&INCOME_NET)));
        if columns.at(2).is_none() && columns.at(3).is_none() {
            missing.push("Pagamento ou Previsao de pagamento");
        }
        return Some(if missing.is_empty() {
            Detected::Income(columns, net, status)
        } else {
            Detected::Incomplete(kind, missing)
        });
    }
    if has(header, "codigo isin / distribuicao") || has(header, "codigo isin") {
        return Some(Detected::Position);
    }
    None
}

/// Quantas linhas do topo valem como candidatas a cabeçalho (a B3 às vezes põe
/// título antes).
const HEADER_SCAN_ROWS: usize = 15;

fn folded_row(row: &RawRow) -> Vec<String> {
    row.cells
        .iter()
        .map(|cell| fold(&cell_text(cell)))
        .collect()
}

// ---------------------------------------------------------------------------
// Leitura de um arquivo
// ---------------------------------------------------------------------------

/// O que a leitura de um arquivo achou.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileReading {
    /// Tipos de planilha reconhecidos (quase sempre um só).
    pub kinds: Vec<FileKind>,
    /// Linhas de dados vistas (sem as vazias nem o rodapé de total).
    pub rows: usize,
    pub trades: Vec<TradeLine>,
    pub events: Vec<EventLine>,
    pub income: Vec<IncomeLine>,
    pub unrecognized: Vec<UnrecognizedRow>,
    pub ignored: Vec<Reason>,
}

/// Por que um arquivo não rendeu nenhuma planilha lida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatProblem {
    /// Nenhum cabeçalho conhecido.
    Unknown,
    /// Tipo reconhecido, mas faltam colunas.
    MissingColumns(FileKind, Vec<&'static str>),
    /// Posição ou consolidado.
    Position,
}

impl FormatProblem {
    /// Texto do servidor (sem acento).
    pub fn message(&self) -> String {
        match self {
            FormatProblem::Unknown => "Formato nao reconhecido: nao achei os cabecalhos de Negociacao, Movimentacao nem Proventos da Area do Investidor da B3. Envie a planilha como a B3 exporta, sem editar.".to_owned(),
            FormatProblem::MissingColumns(kind, missing) => format!(
                "Parece a planilha de {}, mas faltam colunas: {}. A B3 pode ter mudado o layout.",
                kind_label(*kind),
                missing.join(", ")
            ),
            FormatProblem::Position => "Esta e a planilha de Posicao, que o Prumo ainda nao le. Envie Negociacao, Movimentacao ou Proventos.".to_owned(),
        }
    }
}

fn kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Trades => "Negociacao",
        FileKind::Movements => "Movimentacao",
        FileKind::IncomeReceived => "Proventos recebidos",
        FileKind::IncomeProvisioned => "Proventos a receber",
    }
}

/// Lê todas as abas de um arquivo. `Err` quando nenhuma aba é uma planilha
/// conhecida; `Ok` pode trazer linhas não entendidas.
pub fn read_file(sheets: &[RawSheet]) -> Result<FileReading, FormatProblem> {
    let mut reading = FileReading::default();
    let mut problem: Option<FormatProblem> = None;
    let mut refs = RefCounter::default();
    let mut found = false;
    for sheet in sheets {
        let Some((header_at, detected)) = sheet
            .rows
            .iter()
            .take(HEADER_SCAN_ROWS)
            .enumerate()
            .find_map(|(at, row)| detect_header(&folded_row(row)).map(|d| (at, d)))
        else {
            continue;
        };
        let body = &sheet.rows[header_at + 1..];
        match detected {
            Detected::Trades(columns) => {
                found = true;
                reading.kinds.push(FileKind::Trades);
                read_trades(sheet, body, &columns, &mut refs, &mut reading);
            }
            Detected::Movements(columns) => {
                found = true;
                reading.kinds.push(FileKind::Movements);
                read_movements(sheet, body, &columns, &mut refs, &mut reading);
            }
            Detected::Income(columns, net, status) => {
                found = true;
                reading.kinds.push(match status {
                    IncomeStatus::Received => FileKind::IncomeReceived,
                    IncomeStatus::Provisioned => FileKind::IncomeProvisioned,
                });
                read_income(sheet, body, &columns, &net, status, &mut refs, &mut reading);
            }
            Detected::Incomplete(kind, missing) => {
                problem.get_or_insert(FormatProblem::MissingColumns(kind, missing));
            }
            Detected::Position => {
                problem.get_or_insert(FormatProblem::Position);
            }
        }
    }
    if found {
        Ok(reading)
    } else {
        Err(problem.unwrap_or(FormatProblem::Unknown))
    }
}

/// Acesso a uma linha com as colunas já achadas.
struct RowView<'a> {
    row: &'a RawRow,
    columns: &'a Columns,
}

impl<'a> RowView<'a> {
    fn cell(&self, position: usize) -> &'a RawCell {
        static EMPTY: RawCell = RawCell::Empty;
        self.columns
            .at(position)
            .and_then(|at| self.row.cells.get(at))
            .unwrap_or(&EMPTY)
    }

    fn text(&self, position: usize) -> String {
        cell_text(self.cell(position))
    }

    fn optional_text(&self, position: usize) -> Option<String> {
        let cell = self.cell(position);
        (!is_blank(cell)).then(|| cell_text(cell))
    }

    /// `Ok(None)` quando em branco; `Err` quando preenchida e ilegível.
    fn decimal(&self, position: usize) -> Result<Option<Decimal>, ()> {
        let cell = self.cell(position);
        match cell {
            _ if is_blank(cell) => Ok(None),
            RawCell::Number(number) => Ok(Some(*number)),
            RawCell::Text(text) => parse_decimal_pt_br(text).map(Some).ok_or(()),
            RawCell::Empty => Ok(None),
        }
    }
}

/// Linha sem nada, ou o rodapé `Total`/`Total líquido`.
fn is_skippable(row: &RawRow) -> bool {
    if row.cells.iter().all(is_blank) {
        return true;
    }
    row.cells.iter().any(|cell| match cell {
        RawCell::Text(text) => fold(text).starts_with("total"),
        _ => false,
    }) && row.cells.iter().filter(|c| !is_blank(c)).count() <= 2
}

fn row_text(row: &RawRow) -> String {
    let joined = row
        .cells
        .iter()
        .filter(|cell| !is_blank(cell))
        .map(cell_text)
        .collect::<Vec<_>>()
        .join(" | ");
    joined.chars().take(200).collect()
}

fn unrecognized(sheet: &RawSheet, row: &RawRow, reason: Reason) -> UnrecognizedRow {
    UnrecognizedRow {
        file: 0,
        sheet: sheet.name.clone(),
        line: row.line,
        reason_code: reason.code(),
        reason: reason.text(),
        text: row_text(row),
    }
}

/// Contador de ocorrências de conteúdo idêntico no arquivo, para duas linhas
/// iguais (comuns no fracionário) receberem `external_ref` diferentes.
#[derive(Default)]
struct RefCounter {
    seen: HashMap<String, u32>,
}

impl RefCounter {
    fn make(&mut self, family: &str, content: &str) -> String {
        let key = format!("{family}|{content}");
        let occurrence = self.seen.entry(key.clone()).or_insert(0);
        let current = *occurrence;
        *occurrence += 1;
        external_ref(family, &format!("{key}|#{current}"))
    }
}

fn external_ref(family: &str, content: &str) -> String {
    let hash = digest::digest(&digest::SHA256, content.as_bytes());
    let hex: String = hash
        .as_ref()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("b3:{REF_VERSION}:{family}:{hex}")
}

fn num(value: Decimal) -> String {
    value.normalize().to_string()
}

fn opt_num(value: Option<Decimal>) -> String {
    value.map_or_else(|| "-".to_owned(), num)
}

fn opt_date(value: Option<Date>) -> String {
    value.map_or_else(|| "-".to_owned(), iso)
}

fn positive(value: Option<Decimal>) -> Option<Decimal> {
    value.filter(|v| *v > Decimal::ZERO)
}

// --- Negociação -------------------------------------------------------------

fn read_trades(
    sheet: &RawSheet,
    body: &[RawRow],
    columns: &Columns,
    refs: &mut RefCounter,
    out: &mut FileReading,
) {
    for row in body {
        if is_skippable(row) {
            continue;
        }
        out.rows += 1;
        match trade_row(row, columns, refs) {
            Ok(line) => out.trades.push(line),
            Err(reason) => out.unrecognized.push(unrecognized(sheet, row, reason)),
        }
    }
}

fn trade_row(row: &RawRow, columns: &Columns, refs: &mut RefCounter) -> Result<TradeLine, Reason> {
    let view = RowView { row, columns };
    let market_text = fold(&view.text(2));
    let market = if market_text.contains("fracion") {
        TradeMarket::Fractional
    } else if market_text.contains("vista") {
        TradeMarket::Spot
    } else {
        return Err(Reason::UnsupportedMarket);
    };
    let side = match fold(&view.text(1)).as_str() {
        "compra" | "c" => TradeSide::Buy,
        "venda" | "v" => TradeSide::Sell,
        _ => return Err(Reason::UnknownSide),
    };
    let date = parse_date_cell(view.cell(0)).ok_or(Reason::InvalidDate)?;
    let raw_code = view.text(5).to_uppercase();
    let code = match market {
        TradeMarket::Fractional => raw_code
            .strip_suffix('F')
            .filter(|rest| is_ticker(rest))
            .unwrap_or(&raw_code)
            .to_owned(),
        TradeMarket::Spot => raw_code,
    };
    if !is_ticker(&code) {
        return Err(Reason::InvalidCode);
    }
    let quantity = positive(view.decimal(6).map_err(|()| Reason::InvalidNumber)?)
        .ok_or(Reason::InvalidNumber)?;
    let unit_price = positive(view.decimal(7).map_err(|()| Reason::InvalidNumber)?)
        .ok_or(Reason::InvalidNumber)?;
    let total = match view.decimal(8).map_err(|()| Reason::InvalidNumber)? {
        Some(value) if value > Decimal::ZERO => value,
        Some(_) => return Err(Reason::InvalidNumber),
        None => (quantity * unit_price).round_dp(2),
    };
    let institution = view.optional_text(4);
    let side_key = match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    };
    let market_key = match market {
        TradeMarket::Spot => "spot",
        TradeMarket::Fractional => "fractional",
    };
    let content = format!(
        "{}|{side_key}|{market_key}|{code}|{}|{}|{}|{}",
        iso(date),
        num(quantity),
        num(unit_price),
        num(total),
        institution.as_deref().map(fold).unwrap_or_default(),
    );
    Ok(TradeLine {
        external_ref: refs.make("trade", &content),
        date,
        side,
        market,
        code,
        institution,
        quantity,
        unit_price,
        total,
        line: row.line,
    })
}

// --- Movimentação -----------------------------------------------------------

/// O que a movimentação significa para a importação.
enum MovementClass {
    Ignore(Reason),
    Reject(Reason),
    Income(IncomeKind),
    Event(EventKind),
}

fn classify_movement(movement: &str, product: &Product) -> MovementClass {
    let key = fold(movement);
    if key == "transferencia" {
        return MovementClass::Ignore(Reason::CustodyTransfer);
    }
    if key.ends_with(" - transferido") {
        return MovementClass::Ignore(Reason::Transferred);
    }
    if key.starts_with("emprestimo") {
        return MovementClass::Ignore(Reason::Lending);
    }
    if product.kind == ProductKind::Treasury {
        return MovementClass::Event(EventKind::TreasuryMovement);
    }
    if key.starts_with("transferencia - liquidacao")
        || matches!(
            key.as_str(),
            "compra" | "venda" | "compra / venda" | "compra/venda"
        )
    {
        return MovementClass::Ignore(Reason::Settlement);
    }
    match key.as_str() {
        "dividendo" => MovementClass::Income(IncomeKind::Dividend),
        "rendimento" => MovementClass::Income(IncomeKind::Yield),
        "leilao de fracao" | "leilao de fracoes" => {
            MovementClass::Income(IncomeKind::FractionAuction)
        }
        "desdobro" | "desdobramento" => MovementClass::Event(EventKind::Split),
        "grupamento" => MovementClass::Event(EventKind::ReverseSplit),
        "fracao em ativos" => MovementClass::Event(EventKind::FractionInKind),
        "amortizacao" | "restituicao de capital" => MovementClass::Event(EventKind::Amortization),
        "resgate" => MovementClass::Event(EventKind::Redemption),
        "vencimento" => MovementClass::Event(EventKind::Maturity),
        "atualizacao" => MovementClass::Reject(Reason::Update),
        _ if key.starts_with("juros sobre capital proprio") => {
            MovementClass::Income(IncomeKind::Jcp)
        }
        _ if key.starts_with("reembolso") => MovementClass::Income(IncomeKind::Reimbursement),
        _ if key.starts_with("bonificacao") => MovementClass::Event(EventKind::Bonus),
        _ if key.starts_with("direito") || key.starts_with("cessao") => {
            MovementClass::Reject(Reason::Subscription)
        }
        _ => MovementClass::Reject(Reason::UnknownMovement),
    }
}

enum MovementRow {
    Event(EventLine),
    Income(IncomeLine),
    Ignored(Reason),
}

fn read_movements(
    sheet: &RawSheet,
    body: &[RawRow],
    columns: &Columns,
    refs: &mut RefCounter,
    out: &mut FileReading,
) {
    for row in body {
        if is_skippable(row) {
            continue;
        }
        out.rows += 1;
        match movement_row(row, columns, refs) {
            Ok(MovementRow::Event(line)) => out.events.push(line),
            Ok(MovementRow::Income(line)) => out.income.push(line),
            Ok(MovementRow::Ignored(reason)) => out.ignored.push(reason),
            Err(reason) => out.unrecognized.push(unrecognized(sheet, row, reason)),
        }
    }
}

fn movement_row(
    row: &RawRow,
    columns: &Columns,
    refs: &mut RefCounter,
) -> Result<MovementRow, Reason> {
    let view = RowView { row, columns };
    let direction = match fold(&view.text(0)).as_str() {
        "credito" | "entrada" => Direction::Credit,
        "debito" | "saida" => Direction::Debit,
        _ => return Err(Reason::UnknownDirection),
    };
    let movement = view.text(2);
    let product = parse_product(&view.text(3));
    let class = classify_movement(&movement, &product);
    match class {
        MovementClass::Ignore(reason) => return Ok(MovementRow::Ignored(reason)),
        MovementClass::Reject(reason) => return Err(reason),
        _ => {}
    }
    let date = parse_date_cell(view.cell(1)).ok_or(Reason::InvalidDate)?;
    let quantity = view.decimal(5).map_err(|()| Reason::InvalidNumber)?;
    let unit_price = view.decimal(6).map_err(|()| Reason::InvalidNumber)?;
    let value = view.decimal(7).map_err(|()| Reason::InvalidNumber)?;
    let institution = view.optional_text(4);
    match class {
        MovementClass::Income(kind) => {
            if product.kind != ProductKind::Ticker {
                return Err(Reason::NoCode);
            }
            if direction == Direction::Debit {
                return Err(Reason::IncomeDebit);
            }
            let net_amount = positive(value).ok_or(Reason::MissingValue)?;
            Ok(MovementRow::Income(income_line(
                refs,
                IncomeParts {
                    kind,
                    status: IncomeStatus::Received,
                    date: Some(date),
                    product: &product,
                    quantity,
                    unit_price,
                    net_amount,
                    institution,
                    source: FileKind::Movements,
                    line: row.line,
                },
            )))
        }
        MovementClass::Event(kind) => {
            let treasury = product.kind == ProductKind::Treasury;
            if !treasury && product.kind != ProductKind::Ticker {
                return Err(Reason::NoCode);
            }
            let quantity = positive(quantity);
            let value = positive(value);
            match kind {
                EventKind::Split
                | EventKind::ReverseSplit
                | EventKind::Bonus
                | EventKind::FractionInKind
                    if quantity.is_none() =>
                {
                    return Err(Reason::MissingQuantity)
                }
                EventKind::Amortization if value.is_none() => return Err(Reason::MissingValue),
                EventKind::Redemption | EventKind::Maturity | EventKind::TreasuryMovement
                    if quantity.is_none() && value.is_none() =>
                {
                    return Err(Reason::MissingValue)
                }
                _ => {}
            }
            let code = (!treasury).then(|| product.key.clone());
            let direction_key = match direction {
                Direction::Credit => "credit",
                Direction::Debit => "debit",
            };
            let content = format!(
                "{}|{kind:?}|{direction_key}|{}|{}|{}|{}|{}",
                iso(date),
                product.key,
                opt_num(quantity),
                opt_num(value),
                opt_num(unit_price),
                institution.as_deref().map(fold).unwrap_or_default(),
            );
            Ok(MovementRow::Event(EventLine {
                external_ref: refs.make("event", &content),
                date,
                kind,
                direction,
                code,
                product: product_display(&product),
                quantity,
                unit_price,
                value,
                institution,
                description: movement,
                treasury,
                source: FileKind::Movements,
                line: row.line,
            }))
        }
        MovementClass::Ignore(_) | MovementClass::Reject(_) => Err(Reason::UnknownMovement),
    }
}

fn product_display(product: &Product) -> String {
    if product.name.is_empty() {
        product.key.clone()
    } else if product.kind == ProductKind::Ticker {
        format!("{} - {}", product.key, product.name)
    } else {
        product.name.clone()
    }
}

// --- Proventos --------------------------------------------------------------

struct IncomeParts<'a> {
    kind: IncomeKind,
    status: IncomeStatus,
    date: Option<Date>,
    product: &'a Product,
    quantity: Option<Decimal>,
    unit_price: Option<Decimal>,
    net_amount: Decimal,
    institution: Option<String>,
    source: FileKind,
    line: u32,
}

fn income_line(refs: &mut RefCounter, parts: IncomeParts<'_>) -> IncomeLine {
    // Sem origem, sem corretora e sem quantidade: o mesmo pagamento nas duas
    // planilhas tem o mesmo `external_ref`.
    let content = format!(
        "{:?}|{}|{}|{}",
        parts.kind,
        parts.product.key,
        opt_date(parts.date),
        num(parts.net_amount),
    );
    IncomeLine {
        external_ref: refs.make("income", &content),
        kind: parts.kind,
        status: parts.status,
        date: parts.date,
        code: parts.product.key.clone(),
        product: product_display(parts.product),
        quantity: positive(parts.quantity),
        unit_price: positive(parts.unit_price),
        net_amount: parts.net_amount,
        institution: parts.institution,
        source: parts.source,
        line: parts.line,
    }
}

fn income_kind(label: &str) -> Option<IncomeKind> {
    let key = fold(label);
    match key.as_str() {
        "dividendo" => Some(IncomeKind::Dividend),
        "rendimento" => Some(IncomeKind::Yield),
        "leilao de fracao" | "leilao de fracoes" => Some(IncomeKind::FractionAuction),
        _ if key.starts_with("reembolso") => Some(IncomeKind::Reimbursement),
        _ if key.starts_with("juros sobre capital proprio") => Some(IncomeKind::Jcp),
        _ => None,
    }
}

fn read_income(
    sheet: &RawSheet,
    body: &[RawRow],
    columns: &Columns,
    net: &Columns,
    status: IncomeStatus,
    refs: &mut RefCounter,
    out: &mut FileReading,
) {
    let source = match status {
        IncomeStatus::Received => FileKind::IncomeReceived,
        IncomeStatus::Provisioned => FileKind::IncomeProvisioned,
    };
    for row in body {
        if is_skippable(row) {
            continue;
        }
        out.rows += 1;
        match income_row(row, columns, net, status, source, refs) {
            Ok(line) => out.income.push(line),
            Err(reason) => out.unrecognized.push(unrecognized(sheet, row, reason)),
        }
    }
}

fn income_row(
    row: &RawRow,
    columns: &Columns,
    net: &Columns,
    status: IncomeStatus,
    source: FileKind,
    refs: &mut RefCounter,
) -> Result<IncomeLine, Reason> {
    let view = RowView { row, columns };
    let product = parse_product(&view.text(0));
    let kind = income_kind(&view.text(1)).ok_or(Reason::UnknownIncomeType)?;
    if product.kind != ProductKind::Ticker {
        return Err(Reason::NoCode);
    }
    // Recebido tem data; provisionado pode estar sem ("-").
    let date_position = if status == IncomeStatus::Received {
        2
    } else {
        3
    };
    let date_cell = view.cell(date_position);
    let date = if is_blank(date_cell) {
        None
    } else {
        Some(parse_date_cell(date_cell).ok_or(Reason::InvalidDate)?)
    };
    if status == IncomeStatus::Received && date.is_none() {
        return Err(Reason::InvalidDate);
    }
    let net_view = RowView { row, columns: net };
    let net_amount = positive(net_view.decimal(0).map_err(|()| Reason::InvalidNumber)?)
        .ok_or(Reason::MissingValue)?;
    let quantity = view.decimal(5).map_err(|()| Reason::InvalidNumber)?;
    let unit_price = view.decimal(6).map_err(|()| Reason::InvalidNumber)?;
    Ok(income_line(
        refs,
        IncomeParts {
            kind,
            status,
            date,
            product: &product,
            quantity,
            unit_price,
            net_amount,
            institution: view.optional_text(4),
            source,
            line: row.line,
        },
    ))
}

// ---------------------------------------------------------------------------
// A prévia do envio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    /// Lido (pode ter linhas não entendidas).
    Read,
    /// Abriu como planilha, mas não é nenhuma das conhecidas.
    UnknownFormat,
    /// Não abriu como `.xlsx` (corrompido, outro formato, grande demais).
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileReport {
    pub index: usize,
    pub name: String,
    pub status: FileStatus,
    pub kinds: Vec<FileKind>,
    /// Texto do servidor (sem acento) quando o arquivo não foi lido.
    pub message: Option<String>,
    pub rows: usize,
    /// Linhas que viraram negócio, evento ou provento.
    pub accepted: usize,
    pub unrecognized: usize,
    pub ignored: usize,
}

/// De onde veio o palpite de classe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassBasis {
    /// Pelo nome do produto (planilha de Movimentação ou Proventos).
    Name,
    /// Só pelo código: a Negociação não traz o nome. A pessoa confirma.
    Code,
    /// Título do Tesouro Direto.
    Treasury,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AssetTotals {
    #[serde(with = "rust_decimal::serde::float")]
    pub bought_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub sold_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub bought_value: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub sold_value: Decimal,
    /// Compras menos vendas dos negócios lidos. Não conta desdobro nem
    /// bonificação: é o saldo do que a planilha de Negociação mostra.
    #[serde(with = "rust_decimal::serde::float")]
    pub net_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub income_received: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub income_provisioned: Decimal,
}

/// Tudo que entraria para um ativo.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssetGroup {
    /// Código negociado, ou o nome do título do Tesouro.
    pub key: String,
    pub name: Option<String>,
    pub suggested_class: AssetClass,
    pub class_basis: ClassBasis,
    pub listed_on_b3: bool,
    pub treasury: bool,
    pub totals: AssetTotals,
    pub trades: Vec<TradeLine>,
    pub events: Vec<EventLine>,
    pub income: Vec<IncomeLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PreviewSummary {
    pub assets: usize,
    pub trades: usize,
    pub events: usize,
    pub income: usize,
    pub unrecognized: usize,
    pub ignored: usize,
    #[serde(with = "iso_date::option")]
    pub first_date: Option<Date>,
    #[serde(with = "iso_date::option")]
    pub last_date: Option<Date>,
}

/// A resposta de `POST /imports/b3/preview`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preview {
    pub files: Vec<FileReport>,
    pub assets: Vec<AssetGroup>,
    pub unrecognized: Vec<UnrecognizedRow>,
    pub ignored: Vec<IgnoredSummary>,
    pub summary: PreviewSummary,
}

/// O resultado da leitura de um arquivo do envio.
pub enum FileOutcome {
    Read(FileReading),
    UnknownFormat(FormatProblem),
    Unreadable(String),
}

fn kind_priority(kinds: &[FileKind]) -> FileKind {
    // Quem manda no desempate de linhas repetidas entre planilhas: a Negociação
    // para negócios, o provento recebido, depois a Movimentação, e por fim o
    // provisionado (que vira recebido quando é pago).
    kinds.iter().copied().min().unwrap_or(FileKind::Movements)
}

/// Junta os arquivos lidos: tira a linha repetida entre planilhas, agrupa por
/// ativo e sugere a classe. `names` são os nomes dos arquivos, na ordem do envio.
pub fn build_preview(names: Vec<String>, outcomes: Vec<FileOutcome>) -> Preview {
    let mut reports: Vec<FileReport> = Vec::with_capacity(outcomes.len());
    let mut readings: Vec<(usize, FileReading)> = Vec::new();
    for (index, (name, outcome)) in names.into_iter().zip(outcomes).enumerate() {
        let (status, message, reading) = match outcome {
            FileOutcome::Read(reading) => (FileStatus::Read, None, Some(reading)),
            FileOutcome::UnknownFormat(problem) => {
                (FileStatus::UnknownFormat, Some(problem.message()), None)
            }
            FileOutcome::Unreadable(message) => (FileStatus::Unreadable, Some(message), None),
        };
        reports.push(FileReport {
            index,
            name,
            status,
            kinds: reading
                .as_ref()
                .map(|r| r.kinds.clone())
                .unwrap_or_default(),
            message,
            rows: reading.as_ref().map_or(0, |r| r.rows),
            accepted: 0,
            unrecognized: 0,
            ignored: 0,
        });
        if let Some(reading) = reading {
            readings.push((index, reading));
        }
    }
    // Quem vem antes na prioridade fica com a linha repetida.
    readings.sort_by_key(|(index, reading)| (kind_priority(&reading.kinds), *index));

    let mut seen: HashSet<String> = HashSet::new();
    let mut ignored: BTreeMap<Reason, usize> = BTreeMap::new();
    let mut unrecognized: Vec<UnrecognizedRow> = Vec::new();
    let mut unrecognized_seen: HashMap<(&'static str, String), usize> = HashMap::new();
    let mut trades: Vec<TradeLine> = Vec::new();
    let mut events: Vec<EventLine> = Vec::new();
    let mut income: Vec<IncomeLine> = Vec::new();
    for (index, reading) in readings {
        let report = &mut reports[index];
        for reason in &reading.ignored {
            *ignored.entry(*reason).or_default() += 1;
            report.ignored += 1;
        }
        let mut keep = |external_ref: &str, report: &mut FileReport| {
            if seen.insert(external_ref.to_owned()) {
                report.accepted += 1;
                true
            } else {
                *ignored.entry(Reason::DuplicateInRequest).or_default() += 1;
                report.ignored += 1;
                false
            }
        };
        for line in reading.trades {
            if keep(&line.external_ref, report) {
                trades.push(line);
            }
        }
        for line in reading.events {
            if keep(&line.external_ref, report) {
                events.push(line);
            }
        }
        for line in reading.income {
            if keep(&line.external_ref, report) {
                income.push(line);
            }
        }
        report.unrecognized = reading.unrecognized.len();
        for mut row in reading.unrecognized {
            // A mesma linha em outro arquivo (períodos que se sobrepõem) é
            // listada uma vez só.
            let key = (row.reason_code, row.text.clone());
            if *unrecognized_seen.entry(key).or_insert(index) != index {
                continue;
            }
            row.file = index;
            unrecognized.push(row);
        }
    }
    unrecognized.sort_by(|a, b| (a.file, &a.sheet, a.line).cmp(&(b.file, &b.sheet, b.line)));

    let assets = group_assets(trades, events, income);
    let dates: Vec<Date> = assets
        .iter()
        .flat_map(|asset| {
            asset
                .trades
                .iter()
                .map(|t| Some(t.date))
                .chain(asset.events.iter().map(|e| Some(e.date)))
                .chain(asset.income.iter().map(|i| i.date))
        })
        .flatten()
        .collect();
    let summary = PreviewSummary {
        assets: assets.len(),
        trades: assets.iter().map(|a| a.trades.len()).sum(),
        events: assets.iter().map(|a| a.events.len()).sum(),
        income: assets.iter().map(|a| a.income.len()).sum(),
        unrecognized: unrecognized.len(),
        ignored: ignored.values().sum(),
        first_date: dates.iter().min().copied(),
        last_date: dates.iter().max().copied(),
    };
    Preview {
        files: reports,
        assets,
        unrecognized,
        ignored: ignored
            .into_iter()
            .map(|(reason, count)| IgnoredSummary {
                reason_code: reason.code(),
                reason: reason.text(),
                count,
            })
            .collect(),
        summary,
    }
}

#[derive(Default)]
struct GroupBuilder {
    name: Option<String>,
    treasury: bool,
    trades: Vec<TradeLine>,
    events: Vec<EventLine>,
    income: Vec<IncomeLine>,
}

fn group_assets(
    trades: Vec<TradeLine>,
    events: Vec<EventLine>,
    income: Vec<IncomeLine>,
) -> Vec<AssetGroup> {
    let mut groups: BTreeMap<String, GroupBuilder> = BTreeMap::new();
    for line in trades {
        groups
            .entry(line.code.clone())
            .or_default()
            .trades
            .push(line);
    }
    for line in events {
        let key = line
            .code
            .clone()
            .unwrap_or_else(|| line.product.to_uppercase());
        let group = groups.entry(key).or_default();
        group.treasury |= line.treasury;
        if group.name.is_none() {
            group.name = name_from_product(&line.product);
        }
        group.events.push(line);
    }
    for line in income {
        let group = groups.entry(line.code.clone()).or_default();
        if group.name.is_none() {
            group.name = name_from_product(&line.product);
        }
        group.income.push(line);
    }
    groups
        .into_iter()
        .map(|(key, mut group)| {
            group.trades.sort_by_key(|t| (t.date, t.line));
            group.events.sort_by_key(|e| (e.date, e.line));
            group.income.sort_by_key(|i| (i.date, i.line));
            let totals = totals_of(&group);
            let (suggested_class, class_basis, listed_on_b3) = if group.treasury {
                (AssetClass::FixedIncome, ClassBasis::Treasury, false)
            } else if let Some(name) = &group.name {
                let upper = name.to_uppercase();
                (
                    classify_b3(&key, "EQUITY", &upper, &upper),
                    ClassBasis::Name,
                    true,
                )
            } else {
                (classify_b3(&key, "EQUITY", "", ""), ClassBasis::Code, true)
            };
            AssetGroup {
                key,
                name: group.name,
                suggested_class,
                class_basis,
                listed_on_b3,
                treasury: group.treasury,
                totals,
                trades: group.trades,
                events: group.events,
                income: group.income,
            }
        })
        .collect()
}

/// `PETR4 - PETROLEO BRASILEIRO S.A.` → `PETROLEO BRASILEIRO S.A.`.
fn name_from_product(product: &str) -> Option<String> {
    let name = match product.split_once(" - ") {
        Some((head, rest)) if is_ticker(head.trim()) => rest.trim(),
        _ => product.trim(),
    };
    (!name.is_empty()).then(|| name.to_owned())
}

fn totals_of(group: &GroupBuilder) -> AssetTotals {
    let mut totals = AssetTotals::default();
    for trade in &group.trades {
        match trade.side {
            TradeSide::Buy => {
                totals.bought_quantity += trade.quantity;
                totals.bought_value += trade.total;
            }
            TradeSide::Sell => {
                totals.sold_quantity += trade.quantity;
                totals.sold_value += trade.total;
            }
        }
    }
    totals.net_quantity = totals.bought_quantity - totals.sold_quantity;
    for line in &group.income {
        match line.status {
            IncomeStatus::Received => totals.income_received += line.net_amount,
            IncomeStatus::Provisioned => totals.income_provisioned += line.net_amount,
        }
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn text(value: &str) -> RawCell {
        RawCell::Text(value.to_owned())
    }

    fn number(value: Decimal) -> RawCell {
        RawCell::Number(value)
    }

    fn sheet(name: &str, rows: Vec<Vec<RawCell>>) -> RawSheet {
        RawSheet {
            name: name.to_owned(),
            rows: rows
                .into_iter()
                .enumerate()
                .map(|(at, cells)| RawRow {
                    line: u32::try_from(at + 1).unwrap(),
                    cells,
                })
                .collect(),
        }
    }

    fn header(names: &[&str]) -> Vec<RawCell> {
        names.iter().map(|n| text(n)).collect()
    }

    fn row(values: &[&str]) -> Vec<RawCell> {
        values
            .iter()
            .map(|v| {
                if v.is_empty() {
                    RawCell::Empty
                } else {
                    text(v)
                }
            })
            .collect()
    }

    const TRADE_HEADER: [&str; 9] = [
        "Data do Negócio",
        "Tipo de Movimentação",
        "Mercado",
        "Prazo/Vencimento",
        "Instituição",
        "Código de Negociação",
        "Quantidade",
        "Preço",
        "Valor",
    ];
    const MOVEMENT_HEADER: [&str; 8] = [
        "Entrada/Saída",
        "Data",
        "Movimentação",
        "Produto",
        "Instituição",
        "Quantidade",
        "Preço unitário",
        "Valor da Operação",
    ];
    const RECEIVED_HEADER: [&str; 7] = [
        "Produto",
        "Pagamento",
        "Tipo de Evento",
        "Instituição",
        "Quantidade",
        "Preço unitário",
        "Valor líquido",
    ];
    const PROVISIONED_HEADER: [&str; 9] = [
        "Produto",
        "Tipo",
        "Tipo de Evento",
        "Previsão de pagamento",
        "Instituição",
        "Conta",
        "Quantidade",
        "Preço unitário",
        "Valor líquido",
    ];
    const BROKER: &str = "CORRETORA EXEMPLO S/A";
    const PETR: &str = "PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS";

    fn trade(
        date: &str,
        side: &str,
        market: &str,
        code: &str,
        qty: &str,
        price: &str,
        total: &str,
    ) -> Vec<RawCell> {
        row(&[date, side, market, "-", BROKER, code, qty, price, total])
    }

    fn movement(
        direction: &str,
        date: &str,
        what: &str,
        product: &str,
        qty: &str,
        price: &str,
        value: &str,
    ) -> Vec<RawCell> {
        row(&[direction, date, what, product, BROKER, qty, price, value])
    }

    #[test]
    fn fold_ignores_accent_case_and_spacing() {
        assert_eq!(fold("  Instituição\u{a0} "), "instituicao");
        assert_eq!(fold("ENTRADA/SAÍDA"), "entrada/saida");
        assert_eq!(fold("Preço   unitário"), "preco unitario");
        assert_eq!(fold("\u{feff}Código de Negociação"), "codigo de negociacao");
        assert_eq!(fold("   "), "");
    }

    #[test]
    fn parses_pt_br_numbers() {
        assert_eq!(parse_decimal_pt_br("1.234,56"), Some(dec!(1234.56)));
        assert_eq!(parse_decimal_pt_br("R$ 12,5"), Some(dec!(12.5)));
        assert_eq!(parse_decimal_pt_br("-3,40"), Some(dec!(-3.40)));
        assert_eq!(parse_decimal_pt_br("1.000"), Some(dec!(1000)));
        assert_eq!(parse_decimal_pt_br("1.234.567"), Some(dec!(1234567)));
        assert_eq!(parse_decimal_pt_br("12.48"), Some(dec!(12.48)));
        assert_eq!(parse_decimal_pt_br("0.5"), Some(dec!(0.5)));
        assert_eq!(parse_decimal_pt_br("100"), Some(dec!(100)));
        assert_eq!(parse_decimal_pt_br("1E-3"), Some(dec!(0.001)));
        for bad in [
            "",
            "abc",
            "1,2,3",
            "1..2",
            "12.48.5",
            "R$",
            "--5",
            "9999999999999999999,00",
        ] {
            assert_eq!(parse_decimal_pt_br(bad), None, "{bad}");
        }
    }

    #[test]
    fn parses_dates_as_text_serial_and_iso() {
        let expected = Date::from_calendar_date(2026, Month::March, 5).unwrap();
        assert_eq!(parse_date_cell(&text("05/03/2026")), Some(expected));
        assert_eq!(parse_date_cell(&text("5/3/2026 10:30")), Some(expected));
        assert_eq!(parse_date_cell(&text("2026-03-05")), Some(expected));
        assert_eq!(parse_date_cell(&number(dec!(46086))), Some(expected));
        assert_eq!(parse_date_cell(&number(dec!(46086.75))), Some(expected));
        for bad in [
            "",
            "-",
            "31/02/2026",
            "05/03/26",
            "ontem",
            "05/13/2026",
            "1/2/3/2026",
        ] {
            assert_eq!(parse_date_cell(&text(bad)), None, "{bad}");
        }
        assert_eq!(parse_date_cell(&number(dec!(5))), None, "ano fora da faixa");
        assert_eq!(parse_date_cell(&RawCell::Empty), None);
    }

    #[test]
    fn tickers_and_products() {
        for ok in ["PETR4", "KNRI11", "B3SA3", "BOVA11"] {
            assert!(is_ticker(ok), "{ok}");
        }
        for bad in ["PETR", "petr4", "PETR444", "PETRO4", "ABC-4", ""] {
            assert!(!is_ticker(bad), "{bad}");
        }
        let stock = parse_product(PETR);
        assert_eq!(stock.kind, ProductKind::Ticker);
        assert_eq!(stock.key, "PETR4");
        assert_eq!(stock.name, "PETROLEO BRASILEIRO S.A. PETROBRAS");
        let fii = parse_product("HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII");
        assert_eq!(fii.key, "HGLG11");
        assert_eq!(fii.name, "CGHG LOGISTICA FDO INV IMOB - FII");
        let bare = parse_product("WEGE3");
        assert_eq!(
            (bare.kind, bare.key.as_str(), bare.name.as_str()),
            (ProductKind::Ticker, "WEGE3", "")
        );
        let bond = parse_product("Tesouro  Selic 2029");
        assert_eq!(bond.kind, ProductKind::Treasury);
        assert_eq!(bond.key, "TESOURO SELIC 2029");
        assert_eq!(
            parse_product("Opção de Compra - PETRK300").kind,
            ProductKind::Derivative
        );
        assert_eq!(
            parse_product("Futuro - WINZ26").kind,
            ProductKind::Derivative
        );
        assert_eq!(
            parse_product("CDB - BANCO EXEMPLO - 2030").kind,
            ProductKind::Other
        );
    }

    #[test]
    fn detects_each_kind_by_header_tolerating_variation() {
        let variants: Vec<(Vec<RawCell>, FileKind)> = vec![
            (header(&TRADE_HEADER), FileKind::Trades),
            (
                header(&[
                    "DATA DO NEGOCIO",
                    "tipo de movimentacao",
                    "Mercado",
                    "instituicao",
                    "CODIGO DE NEGOCIACAO",
                    "Quantidade",
                    "Preco",
                    "Valor",
                    "Coluna Extra",
                ]),
                FileKind::Trades,
            ),
            (header(&MOVEMENT_HEADER), FileKind::Movements),
            (
                header(&[
                    "entrada/saida",
                    "DATA",
                    " Movimentação ",
                    "Produto",
                    "Instituição",
                    "Quantidade",
                    "Preço Unitário",
                    "Valor da Operação",
                    "Extra",
                ]),
                FileKind::Movements,
            ),
            (header(&RECEIVED_HEADER), FileKind::IncomeReceived),
            (header(&PROVISIONED_HEADER), FileKind::IncomeProvisioned),
        ];
        for (cells, expected) in variants {
            let reading = read_file(&[sheet("Aba", vec![cells])]).unwrap();
            assert_eq!(reading.kinds, vec![expected]);
            assert_eq!(reading.rows, 0);
        }
    }

    #[test]
    fn header_may_sit_below_a_title() {
        let rows = vec![
            row(&["Extrato de negociação"]),
            row(&[]),
            header(&TRADE_HEADER),
            trade(
                "05/03/2026",
                "Compra",
                "Mercado à Vista",
                "ABCD3",
                "100",
                "10,5",
                "1.050,00",
            ),
        ];
        let reading = read_file(&[sheet("Negociação", rows)]).unwrap();
        assert_eq!(reading.trades.len(), 1);
        assert_eq!(reading.trades[0].line, 4);
    }

    #[test]
    fn unknown_and_incomplete_formats_say_why() {
        let unknown = read_file(&[sheet(
            "Plan1",
            vec![header(&["Nome", "Idade"]), row(&["Ana", "30"])],
        )]);
        assert_eq!(unknown, Err(FormatProblem::Unknown));
        assert!(FormatProblem::Unknown
            .message()
            .contains("Formato nao reconhecido"));
        assert_eq!(read_file(&[]), Err(FormatProblem::Unknown));

        let incomplete = read_file(&[sheet(
            "X",
            vec![header(&["Entrada/Saída", "Data", "Produto", "Quantidade"])],
        )])
        .unwrap_err();
        let FormatProblem::MissingColumns(kind, missing) = &incomplete else {
            panic!("esperava colunas faltando: {incomplete:?}");
        };
        assert_eq!(*kind, FileKind::Movements);
        assert_eq!(missing, &["Movimentacao"]);
        assert!(incomplete.message().contains("Movimentacao"));
        assert!(incomplete.message().contains("faltam colunas"));

        let no_date = read_file(&[sheet(
            "X",
            vec![header(&["Produto", "Tipo de Evento", "Valor líquido"])],
        )])
        .unwrap_err();
        assert!(no_date.message().contains("Previsao de pagamento"));

        let position = read_file(&[sheet(
            "Posição - Ações",
            vec![header(&[
                "Produto",
                "Instituição",
                "Conta",
                "Código de Negociação",
                "Código ISIN / Distribuição",
                "Quantidade",
            ])],
        )])
        .unwrap_err();
        assert_eq!(position, FormatProblem::Position);
        assert!(position.message().contains("Posicao"));
    }

    #[test]
    fn reads_trades_including_fractional_and_flags_the_rest() {
        let rows = vec![
            header(&TRADE_HEADER),
            trade(
                "05/03/2026",
                "Compra",
                "Mercado à Vista",
                "ABCD3",
                "100",
                "10,12",
                "1.012,00",
            ),
            trade(
                "05/03/2026",
                "Compra",
                "Mercado Fracionário",
                "ABCD3F",
                "7",
                "10,13",
                "70,91",
            ),
            vec![
                text("06/03/2026"),
                text("Venda"),
                text("Mercado à Vista"),
                text("-"),
                text(BROKER),
                text("WXYZ11"),
                number(dec!(10)),
                number(dec!(98.5)),
                number(dec!(985)),
            ],
            trade(
                "06/03/2026",
                "Compra",
                "Opção de Compra",
                "ABCDC120",
                "100",
                "0,35",
                "35,00",
            ),
            trade(
                "06/03/2026",
                "Compra",
                "Mercado à Vista",
                "ABCD3",
                "0",
                "10",
                "0",
            ),
            trade(
                "06/03/2026",
                "Transferência",
                "Mercado à Vista",
                "ABCD3",
                "1",
                "10",
                "10",
            ),
            trade(
                "32/13/2026",
                "Compra",
                "Mercado à Vista",
                "ABCD3",
                "1",
                "10",
                "10",
            ),
            trade(
                "06/03/2026",
                "Compra",
                "Mercado à Vista",
                "ABC",
                "1",
                "10",
                "10",
            ),
            trade(
                "06/03/2026",
                "Compra",
                "Mercado à Vista",
                "ABCD3",
                "x",
                "10",
                "10",
            ),
            row(&[]),
            row(&["", "", "", "", "", "", "", "", "Total"]),
        ];
        let reading = read_file(&[sheet("Negociação", rows)]).unwrap();
        assert_eq!(reading.rows, 9);
        assert_eq!(reading.trades.len(), 3);
        let first = &reading.trades[0];
        assert_eq!(first.code, "ABCD3");
        assert_eq!(first.market, TradeMarket::Spot);
        assert_eq!(first.side, TradeSide::Buy);
        assert_eq!(first.quantity, dec!(100));
        assert_eq!(first.unit_price, dec!(10.12));
        assert_eq!(first.total, dec!(1012));
        assert_eq!(first.institution.as_deref(), Some(BROKER));
        let fractional = &reading.trades[1];
        assert_eq!(fractional.code, "ABCD3");
        assert_eq!(fractional.market, TradeMarket::Fractional);
        assert_eq!(reading.trades[2].side, TradeSide::Sell);
        assert_eq!(reading.trades[2].unit_price, dec!(98.5));

        let reasons: Vec<&str> = reading.unrecognized.iter().map(|r| r.reason_code).collect();
        assert_eq!(
            reasons,
            [
                "unsupported_market",
                "invalid_number",
                "unknown_side",
                "invalid_date",
                "invalid_code",
                "invalid_number"
            ]
        );
        assert_eq!(reading.unrecognized[0].line, 5);
        assert!(reading.unrecognized[0].text.contains("ABCDC120"));
    }

    #[test]
    fn a_missing_total_is_computed() {
        let rows = vec![
            header(&TRADE_HEADER),
            row(&[
                "05/03/2026",
                "Compra",
                "Mercado à Vista",
                "-",
                BROKER,
                "ABCD3",
                "3",
                "10,333",
                "-",
            ]),
        ];
        let reading = read_file(&[sheet("N", rows)]).unwrap();
        assert_eq!(reading.trades[0].total, dec!(31.00));
    }

    #[test]
    fn identical_rows_get_distinct_refs_and_the_same_content_the_same_ref() {
        let one = |rows: Vec<Vec<RawCell>>| read_file(&[sheet("N", rows)]).unwrap();
        let line = || {
            trade(
                "05/03/2026",
                "Compra",
                "Mercado Fracionário",
                "ABCD3F",
                "2",
                "10",
                "20",
            )
        };
        let twice = one(vec![header(&TRADE_HEADER), line(), line()]);
        assert_ne!(twice.trades[0].external_ref, twice.trades[1].external_ref);
        let again = one(vec![header(&TRADE_HEADER), line(), line()]);
        assert_eq!(twice.trades[0].external_ref, again.trades[0].external_ref);
        assert_eq!(twice.trades[1].external_ref, again.trades[1].external_ref);
        // Outro arquivo com só a primeira ocorrência do mesmo negócio.
        let overlap = one(vec![header(&TRADE_HEADER), line()]);
        assert_eq!(twice.trades[0].external_ref, overlap.trades[0].external_ref);
        assert!(twice.trades[0].external_ref.starts_with("b3:v1:trade:"));
        assert_eq!(
            twice.trades[0].external_ref.len(),
            "b3:v1:trade:".len() + 32
        );
        // Qualquer campo diferente muda o ref.
        let other = one(vec![
            header(&TRADE_HEADER),
            trade(
                "05/03/2026",
                "Venda",
                "Mercado Fracionário",
                "ABCD3F",
                "2",
                "10",
                "20",
            ),
        ]);
        assert_ne!(twice.trades[0].external_ref, other.trades[0].external_ref);
    }

    #[test]
    fn movements_are_classified() {
        let rows = vec![
            header(&MOVEMENT_HEADER),
            movement(
                "Credito",
                "20/12/2024",
                "Dividendo",
                PETR,
                "100",
                "0,9",
                "90,00",
            ),
            movement(
                "Credito",
                "18/12/2024",
                "Juros Sobre Capital Próprio",
                PETR,
                "200",
                "0,5",
                "85",
            ),
            movement(
                "Credito",
                "15/03/2024",
                "Rendimento",
                "HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII",
                "10",
                "1,1",
                "11",
            ),
            movement(
                "Credito",
                "15/04/2024",
                "Reembolso",
                "HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII",
                "10",
                "-",
                "11",
            ),
            movement(
                "Credito",
                "10/05/2024",
                "Leilão de Fração",
                "ITUB4 - ITAU UNIBANCO",
                "-",
                "-",
                "2,5",
            ),
            movement(
                "Credito",
                "20/03/2025",
                "Bonificação em Ativos",
                "ITUB4 - ITAU UNIBANCO",
                "10",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "21/03/2025",
                "Desdobro",
                "WEGE3 - WEG S.A.",
                "40",
                "-",
                "-",
            ),
            movement(
                "Debito",
                "22/03/2025",
                "Grupamento",
                "WEGE3 - WEG S.A.",
                "40",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "23/03/2025",
                "Fração em Ativos",
                "WEGE3 - WEG S.A.",
                "0,5",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "24/03/2025",
                "Amortização",
                "HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII",
                "10",
                "-",
                "5",
            ),
            movement(
                "Credito",
                "05/03/2024",
                "Transferência - Liquidação",
                PETR,
                "100",
                "30",
                "3000",
            ),
            movement(
                "Debito",
                "05/11/2024",
                "Transferência",
                PETR,
                "50",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "05/11/2024",
                "Dividendo - Transferido",
                PETR,
                "50",
                "1",
                "50",
            ),
            movement(
                "Credito",
                "20/01/2025",
                "Empréstimo",
                PETR,
                "50",
                "-",
                "3,5",
            ),
            movement(
                "Debito",
                "02/04/2025",
                "COMPRA / VENDA",
                "Tesouro Selic 2029",
                "1",
                "15000",
                "15000",
            ),
            movement(
                "Credito",
                "02/05/2025",
                "Resgate",
                "Tesouro Selic 2029",
                "1",
                "15500",
                "15500",
            ),
            movement(
                "Credito",
                "02/06/2025",
                "Atualização",
                "KNRI11 - KINEA",
                "10",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "02/06/2025",
                "Direitos de Subscrição - Exercido",
                "KNRI11 - KINEA",
                "10",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "02/06/2025",
                "Algo Novo da B3",
                "KNRI11 - KINEA",
                "10",
                "-",
                "-",
            ),
            movement(
                "Credito",
                "02/06/2025",
                "Dividendo",
                "CDB - BANCO EXEMPLO",
                "1",
                "-",
                "10",
            ),
            movement("Debito", "02/06/2025", "Dividendo", PETR, "1", "-", "10"),
            movement("Credito", "02/06/2025", "Dividendo", PETR, "1", "-", "-"),
            movement("Credito", "02/06/2025", "Desdobro", PETR, "-", "-", "-"),
            movement("Credito", "02/06/2025", "Amortização", PETR, "1", "-", "-"),
            movement("Entrada", "02/06/2025", "Dividendo", PETR, "1", "-", "5"),
            movement("Talvez", "02/06/2025", "Dividendo", PETR, "1", "-", "5"),
            movement("Credito", "data ruim", "Dividendo", PETR, "1", "-", "5"),
            movement("Credito", "02/06/2025", "Dividendo", PETR, "x", "-", "5"),
        ];
        let reading = read_file(&[sheet("Movimentação", rows)]).unwrap();
        assert_eq!(reading.income.len(), 6, "{:?}", reading.income);
        let kinds: Vec<IncomeKind> = reading.income.iter().map(|i| i.kind).collect();
        assert_eq!(
            kinds,
            [
                IncomeKind::Dividend,
                IncomeKind::Jcp,
                IncomeKind::Yield,
                IncomeKind::Reimbursement,
                IncomeKind::FractionAuction,
                IncomeKind::Dividend
            ]
        );
        assert_eq!(reading.income[1].net_amount, dec!(85));
        assert_eq!(reading.income[0].code, "PETR4");
        assert_eq!(reading.income[0].quantity, Some(dec!(100)));
        assert_eq!(reading.income[0].status, IncomeStatus::Received);
        assert_eq!(reading.income[0].source, FileKind::Movements);

        let events: Vec<(EventKind, Direction, bool)> = reading
            .events
            .iter()
            .map(|e| (e.kind, e.direction, e.treasury))
            .collect();
        assert_eq!(
            events,
            [
                (EventKind::Bonus, Direction::Credit, false),
                (EventKind::Split, Direction::Credit, false),
                (EventKind::ReverseSplit, Direction::Debit, false),
                (EventKind::FractionInKind, Direction::Credit, false),
                (EventKind::Amortization, Direction::Credit, false),
                (EventKind::TreasuryMovement, Direction::Debit, true),
                (EventKind::TreasuryMovement, Direction::Credit, true),
            ]
        );
        let treasury = &reading.events[5];
        assert_eq!(treasury.code, None);
        assert_eq!(treasury.product, "Tesouro Selic 2029");
        assert_eq!(treasury.value, Some(dec!(15000)));
        assert_eq!(treasury.description, "COMPRA / VENDA");
        assert_eq!(reading.events[0].description, "Bonificação em Ativos");

        assert_eq!(
            reading.ignored,
            [
                Reason::Settlement,
                Reason::CustodyTransfer,
                Reason::Transferred,
                Reason::Lending
            ]
        );
        let rejected: Vec<&str> = reading.unrecognized.iter().map(|r| r.reason_code).collect();
        assert_eq!(
            rejected,
            [
                "update",
                "subscription",
                "unknown_movement",
                "no_code",
                "income_debit",
                "missing_value",
                "missing_quantity",
                "missing_value",
                "unknown_direction",
                "invalid_date",
                "invalid_number",
            ]
        );
    }

    #[test]
    fn income_sheets_read_received_and_provisioned() {
        let received = vec![
            header(&RECEIVED_HEADER),
            row(&[
                PETR,
                "20/06/2024",
                "Dividendo",
                BROKER,
                "100",
                "0,9",
                "90,00",
            ]),
            row(&[
                PETR,
                "18/12/2024",
                "JUROS SOBRE CAPITAL PRÓPRIO",
                BROKER,
                "200",
                "0,5",
                "85",
            ]),
            row(&[
                "ITUB4 - ITAU UNIBANCO",
                "01/12/2024",
                "Reembolso - Juros Sobre Capital Próprio",
                BROKER,
                "20",
                "0,2",
                "3,4",
            ]),
            row(&[
                "PAGAMENTO DE JUROS - CDB",
                "01/12/2024",
                "PAGAMENTO DE JUROS",
                BROKER,
                "1",
                "-",
                "10",
            ]),
            row(&[PETR, "-", "Dividendo", BROKER, "1", "-", "10"]),
            row(&["", "", "", "", "", "", "Total"]),
        ];
        let reading = read_file(&[sheet("Proventos Recebidos", received)]).unwrap();
        assert_eq!(reading.kinds, vec![FileKind::IncomeReceived]);
        assert_eq!(reading.income.len(), 3);
        assert_eq!(reading.income[1].kind, IncomeKind::Jcp);
        assert_eq!(reading.income[2].kind, IncomeKind::Reimbursement);
        assert!(reading
            .income
            .iter()
            .all(|i| i.status == IncomeStatus::Received && i.date.is_some()));
        let rejected: Vec<&str> = reading.unrecognized.iter().map(|r| r.reason_code).collect();
        assert_eq!(rejected, ["unknown_income_type", "invalid_date"]);

        let provisioned = vec![
            header(&PROVISIONED_HEADER),
            row(&[
                PETR,
                "PN",
                "DIVIDENDO",
                "20/11/2026",
                BROKER,
                "1000001",
                "150",
                "1,1",
                "165,0",
            ]),
            row(&[
                PETR,
                "PN",
                "JUROS SOBRE CAPITAL PRÓPRIO",
                "-",
                BROKER,
                "1000001",
                "150",
                "0,4",
                "51",
            ]),
            row(&[
                "WEGE3 - WEG S.A.",
                "ON",
                "LEILÃO DE FRAÇÕES",
                "10/11/2026",
                BROKER,
                "1000001",
                "-",
                "18,5",
                "18,5",
            ]),
            row(&["", "", "", "", "", "", "", "Total líquido", "234,5"]),
        ];
        let reading = read_file(&[sheet("Proventos a Receber", provisioned)]).unwrap();
        assert_eq!(reading.kinds, vec![FileKind::IncomeProvisioned]);
        assert_eq!(reading.rows, 3);
        assert_eq!(reading.income.len(), 3);
        assert_eq!(reading.income[1].date, None);
        assert_eq!(reading.income[2].kind, IncomeKind::FractionAuction);
        assert_eq!(reading.income[2].quantity, None);
        assert!(reading
            .income
            .iter()
            .all(|i| i.status == IncomeStatus::Provisioned));
    }

    #[test]
    fn the_same_payment_in_two_sheets_has_the_same_ref() {
        let movements = read_file(&[sheet(
            "M",
            vec![
                header(&MOVEMENT_HEADER),
                movement(
                    "Credito",
                    "20/06/2024",
                    "Dividendo",
                    PETR,
                    "100",
                    "0,9",
                    "90",
                ),
            ],
        )])
        .unwrap();
        let received = read_file(&[sheet(
            "P",
            vec![
                header(&RECEIVED_HEADER),
                row(&[
                    PETR,
                    "20/06/2024",
                    "Dividendo",
                    "OUTRA CORRETORA",
                    "100",
                    "0,9",
                    "90,00",
                ]),
            ],
        )])
        .unwrap();
        assert_eq!(
            movements.income[0].external_ref,
            received.income[0].external_ref
        );
        assert!(received.income[0].external_ref.starts_with("b3:v1:income:"));
    }

    #[test]
    fn preview_groups_by_asset_and_drops_what_came_twice() {
        let trades = read_file(&[sheet(
            "Negociação",
            vec![
                header(&TRADE_HEADER),
                trade(
                    "05/03/2024",
                    "Compra",
                    "Mercado à Vista",
                    "PETR4",
                    "100",
                    "30",
                    "3000",
                ),
                trade(
                    "10/09/2024",
                    "Compra",
                    "Mercado à Vista",
                    "PETR4",
                    "100",
                    "36",
                    "3600",
                ),
                trade(
                    "12/02/2025",
                    "Venda",
                    "Mercado à Vista",
                    "PETR4",
                    "50",
                    "40",
                    "2000",
                ),
                trade(
                    "01/02/2024",
                    "Compra",
                    "Mercado à Vista",
                    "HGLG11",
                    "10",
                    "160",
                    "1600",
                ),
                trade(
                    "01/02/2024",
                    "Compra",
                    "Mercado à Vista",
                    "BOVV11",
                    "5",
                    "100",
                    "500",
                ),
                trade(
                    "01/02/2024",
                    "Compra",
                    "Mercado à Vista",
                    "AAPL34",
                    "5",
                    "50",
                    "250",
                ),
            ],
        )])
        .unwrap();
        let movements = read_file(&[sheet(
            "Movimentação",
            vec![
                header(&MOVEMENT_HEADER),
                movement(
                    "Credito",
                    "05/03/2024",
                    "Transferência - Liquidação",
                    PETR,
                    "100",
                    "30",
                    "3000",
                ),
                movement(
                    "Credito",
                    "20/06/2024",
                    "Dividendo",
                    PETR,
                    "100",
                    "0,9",
                    "90",
                ),
                movement(
                    "Credito",
                    "15/03/2024",
                    "Rendimento",
                    "HGLG11 - CGHG LOGISTICA FDO INV IMOB - FII",
                    "10",
                    "1,1",
                    "11",
                ),
                movement(
                    "Credito",
                    "10/03/2025",
                    "Bonificação em Ativos",
                    "AAPL34 - APPLE INC DRN",
                    "1",
                    "-",
                    "-",
                ),
                movement(
                    "Debito",
                    "02/04/2025",
                    "COMPRA / VENDA",
                    "Tesouro Selic 2029",
                    "1",
                    "15000",
                    "15000",
                ),
                movement("Credito", "02/06/2025", "Algo Novo", PETR, "1", "-", "-"),
            ],
        )])
        .unwrap();
        let received = read_file(&[sheet(
            "Proventos Recebidos",
            vec![
                header(&RECEIVED_HEADER),
                row(&[PETR, "20/06/2024", "Dividendo", BROKER, "100", "0,9", "90"]),
            ],
        )])
        .unwrap();
        let provisioned = read_file(&[sheet(
            "A receber",
            vec![
                header(&PROVISIONED_HEADER),
                row(&[
                    PETR,
                    "PN",
                    "DIVIDENDO",
                    "20/11/2026",
                    BROKER,
                    "1",
                    "150",
                    "1,1",
                    "165",
                ]),
            ],
        )])
        .unwrap();
        // O mesmo arquivo de negociação enviado duas vezes.
        let preview = build_preview(
            vec![
                "mov.xlsx".into(),
                "neg.xlsx".into(),
                "rec.xlsx".into(),
                "neg2.xlsx".into(),
                "a-receber.xlsx".into(),
                "lixo.xlsx".into(),
                "ruim.xlsx".into(),
            ],
            vec![
                FileOutcome::Read(movements),
                FileOutcome::Read(trades.clone()),
                FileOutcome::Read(received),
                FileOutcome::Read(trades),
                FileOutcome::Read(provisioned),
                FileOutcome::UnknownFormat(FormatProblem::Unknown),
                FileOutcome::Unreadable("nao abriu".into()),
            ],
        );

        let keys: Vec<&str> = preview.assets.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(
            keys,
            ["AAPL34", "BOVV11", "HGLG11", "PETR4", "TESOURO SELIC 2029"]
        );
        let petr = &preview.assets[3];
        assert_eq!(
            petr.name.as_deref(),
            Some("PETROLEO BRASILEIRO S.A. PETROBRAS")
        );
        assert_eq!(petr.suggested_class, AssetClass::BrStocks);
        assert_eq!(petr.class_basis, ClassBasis::Name);
        assert!(petr.listed_on_b3);
        assert_eq!(petr.trades.len(), 3);
        assert_eq!(petr.totals.bought_quantity, dec!(200));
        assert_eq!(petr.totals.sold_quantity, dec!(50));
        assert_eq!(petr.totals.net_quantity, dec!(150));
        assert_eq!(petr.totals.bought_value, dec!(6600));
        assert_eq!(petr.totals.sold_value, dec!(2000));
        // O dividendo veio na Movimentação e nos Proventos: fica o dos Proventos.
        assert_eq!(petr.income.len(), 2);
        assert_eq!(petr.income[0].source, FileKind::IncomeReceived);
        assert_eq!(petr.income[1].status, IncomeStatus::Provisioned);
        assert_eq!(petr.totals.income_received, dec!(90));
        assert_eq!(petr.totals.income_provisioned, dec!(165));
        assert!(petr.trades.windows(2).all(|w| w[0].date <= w[1].date));

        let hglg = &preview.assets[2];
        assert_eq!(hglg.suggested_class, AssetClass::BrReits);
        assert_eq!(hglg.class_basis, ClassBasis::Name);
        let aapl = &preview.assets[0];
        assert_eq!(aapl.suggested_class, AssetClass::IntlStocks);
        assert_eq!(aapl.events.len(), 1);
        // Só o código, e terminando em 11: o palpite é de FII e a tela pede confirmação.
        let bovv = &preview.assets[1];
        assert_eq!(bovv.class_basis, ClassBasis::Code);
        assert_eq!(bovv.suggested_class, AssetClass::BrReits);
        let bond = &preview.assets[4];
        assert!(bond.treasury);
        assert_eq!(bond.suggested_class, AssetClass::FixedIncome);
        assert_eq!(bond.class_basis, ClassBasis::Treasury);
        assert!(!bond.listed_on_b3);
        assert_eq!(bond.events.len(), 1);

        let ignored: Vec<(&str, usize)> = preview
            .ignored
            .iter()
            .map(|i| (i.reason_code, i.count))
            .collect();
        assert_eq!(ignored, [("settlement", 1), ("duplicate_in_request", 7)]);
        assert_eq!(preview.unrecognized.len(), 1);
        assert_eq!(preview.unrecognized[0].file, 0);
        assert_eq!(preview.unrecognized[0].reason_code, "unknown_movement");
        assert_eq!(preview.summary.assets, 5);
        assert_eq!(preview.summary.trades, 6);
        assert_eq!(preview.summary.income, 3);
        assert_eq!(preview.summary.events, 2);
        assert_eq!(preview.summary.unrecognized, 1);
        assert_eq!(
            preview.summary.first_date,
            Date::from_calendar_date(2024, Month::February, 1).ok()
        );
        assert_eq!(
            preview.summary.last_date,
            Date::from_calendar_date(2026, Month::November, 20).ok()
        );

        let files: Vec<(FileStatus, usize, usize)> = preview
            .files
            .iter()
            .map(|f| (f.status, f.accepted, f.ignored))
            .collect();
        assert_eq!(files[0], (FileStatus::Read, 3, 2));
        assert_eq!(files[1], (FileStatus::Read, 6, 0));
        assert_eq!(files[2], (FileStatus::Read, 1, 0));
        assert_eq!(files[3], (FileStatus::Read, 0, 6));
        assert_eq!(files[5].0, FileStatus::UnknownFormat);
        assert!(files[5].0 != FileStatus::Read && preview.files[5].message.is_some());
        assert_eq!(files[6].0, FileStatus::Unreadable);
        assert_eq!(preview.files[6].message.as_deref(), Some("nao abriu"));
    }

    #[test]
    fn preview_serializes_dates_and_money_for_the_api() {
        let reading = read_file(&[sheet(
            "N",
            vec![
                header(&TRADE_HEADER),
                trade(
                    "05/03/2026",
                    "Compra",
                    "Mercado à Vista",
                    "ABCD3",
                    "100",
                    "10,12",
                    "1012",
                ),
            ],
        )])
        .unwrap();
        let preview = build_preview(vec!["n.xlsx".into()], vec![FileOutcome::Read(reading)]);
        let json = serde_json::to_value(&preview).unwrap();
        let asset = &json["assets"][0];
        assert_eq!(asset["key"], "ABCD3");
        assert_eq!(asset["suggested_class"], "br_stocks");
        assert_eq!(asset["class_basis"], "code");
        assert_eq!(asset["trades"][0]["date"], "2026-03-05");
        assert_eq!(asset["trades"][0]["side"], "buy");
        assert_eq!(asset["trades"][0]["unit_price"], 10.12);
        assert_eq!(asset["totals"]["net_quantity"], 100.0);
        assert_eq!(json["files"][0]["status"], "read");
        assert_eq!(json["files"][0]["kinds"][0], "trades");
        assert_eq!(json["summary"]["first_date"], "2026-03-05");
        assert_eq!(json["unrecognized"], serde_json::json!([]));
    }

    #[test]
    fn empty_preview_is_valid() {
        let preview = build_preview(vec![], vec![]);
        assert!(preview.assets.is_empty());
        assert_eq!(preview.summary.first_date, None);
        let json = serde_json::to_value(&preview).unwrap();
        assert_eq!(json["summary"]["last_date"], serde_json::Value::Null);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        /// Formata como o Excel em pt-BR: milhar com ponto e decimal com vírgula.
        fn pt_br(cents: i64) -> String {
            let sign = if cents < 0 { "-" } else { "" };
            let cents = cents.unsigned_abs();
            let (whole, frac) = (cents / 100, cents % 100);
            let digits = whole.to_string();
            let mut grouped = String::new();
            for (at, ch) in digits.chars().enumerate() {
                if at > 0 && (digits.len() - at) % 3 == 0 {
                    grouped.push('.');
                }
                grouped.push(ch);
            }
            format!("{sign}{grouped},{frac:02}")
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(300))]

            #[test]
            fn pt_br_numbers_round_trip(cents in -100_000_000_000i64..100_000_000_000) {
                prop_assert_eq!(parse_decimal_pt_br(&pt_br(cents)), Some(Decimal::new(cents, 2)));
                prop_assert_eq!(parse_decimal_pt_br(&format!("R$ {}", pt_br(cents.abs()))), Some(Decimal::new(cents.abs(), 2)));
            }

            #[test]
            fn reading_never_panics_and_refs_are_stable(
                cells in prop::collection::vec(".{0,12}", 9),
                header_noise in "[ a-zA-ZçãéÇ/]{0,20}",
            ) {
                let build = || {
                    vec![
                        header(&TRADE_HEADER),
                        cells.iter().map(|c| text(c)).collect::<Vec<_>>(),
                        vec![text(&header_noise)],
                    ]
                };
                let first = read_file(&[sheet("N", build())]).unwrap();
                let second = read_file(&[sheet("N", build())]).unwrap();
                prop_assert_eq!(first, second);
            }
        }
    }
}
