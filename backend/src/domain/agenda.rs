//! Agenda: a linha do tempo dos eventos dos ativos da carteira. Regra pura,
//! sem banco e sem rede; quem lê o banco é `crate::agenda`. O porquê de cada
//! escolha está em `docs/agenda.md`.
//!
//! - **Proventos.** Cada provento da Renda (`income_events`: lançado, importado
//!   ou automático da B3) dá dois eventos, a data com e o pagamento, com o
//!   líquido que a Renda mostra. O anúncio da B3 ainda sem data de pagamento
//!   ("a definir"), que o automático não cria, entra só com a data com e o
//!   líquido estimado pela quantidade do livro na data com.
//! - **Fora do padrão.** O anúncio da B3 cujo valor por cota se afasta do
//!   habitual ([`unusual`]) vira um evento no dia do anúncio e marca a data com
//!   e o pagamento.
//! - **Documentos.** Resultado entregue à CVM (ITR e DFP; demonstrações e
//!   informe trimestral do FII), fato relevante, comunicado, aviso e
//!   assembleia, com o link do documento original.
//! - **Sem pregão.** Os dias de semana sem pregão da B3 (`tax::calendar`).

pub mod unusual;

use rust_decimal::Decimal;
use time::{Date, Duration, Weekday};
use uuid::Uuid;

use super::asset_class::{AssetClass, Market};
use super::entitlement::quantity_on;
use super::fundamentals::FilingKind;
use super::income::{amounts, IncomeKind, IncomeSource};
use super::income_auto::{cash_kind, VALUE_TOLERANCE};
use super::ledger::{Entry, LedgerError};
use super::tax::calendar::{is_trading_day, Closure};
use unusual::{Direction, Reference, Unusual};

/// O período mais longo que uma consulta da Agenda pode pedir.
pub const MAX_RANGE_DAYS: i64 = 400;

/// O tipo do evento. O nome vai para o JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Dia de semana sem pregão na B3.
    MarketClosed,
    /// Data com de um provento (o último dia para ter a cota e receber).
    IncomeRecord,
    /// Pagamento de um provento.
    IncomePayment,
    /// Provento anunciado fora do padrão do ativo.
    UnusualIncome,
    /// Resultado entregue à CVM.
    Results,
    MaterialFact,
    /// Comunicado ao mercado, aviso aos acionistas ou cotistas, assembleia,
    /// relatório gerencial do FII.
    Notice,
}

/// O que um documento é. O nome vai para o banco e para o JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCategory {
    MaterialFact,
    MarketNotice,
    ShareholderNotice,
    Meeting,
    Results,
    ManagementReport,
}

impl DocumentCategory {
    pub const ALL: [DocumentCategory; 6] = [
        DocumentCategory::MaterialFact,
        DocumentCategory::MarketNotice,
        DocumentCategory::ShareholderNotice,
        DocumentCategory::Meeting,
        DocumentCategory::Results,
        DocumentCategory::ManagementReport,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DocumentCategory::MaterialFact => "material_fact",
            DocumentCategory::MarketNotice => "market_notice",
            DocumentCategory::ShareholderNotice => "shareholder_notice",
            DocumentCategory::Meeting => "meeting",
            DocumentCategory::Results => "results",
            DocumentCategory::ManagementReport => "management_report",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        DocumentCategory::ALL
            .into_iter()
            .find(|c| c.as_str() == value)
    }

    pub fn event_kind(self) -> EventKind {
        match self {
            DocumentCategory::MaterialFact => EventKind::MaterialFact,
            DocumentCategory::Results => EventKind::Results,
            DocumentCategory::MarketNotice
            | DocumentCategory::ShareholderNotice
            | DocumentCategory::Meeting
            | DocumentCategory::ManagementReport => EventKind::Notice,
        }
    }
}

/// De onde veio o documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentSource {
    /// Documentos periódicos e eventuais das companhias (CVM, dados abertos).
    CvmIpe,
    /// ITR ou DFP entregue à CVM (o índice do arquivo de dados abertos).
    CvmFiling,
    /// Fundos.NET, onde os FIIs entregam os documentos.
    FundosNet,
}

impl DocumentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentSource::CvmIpe => "cvm_ipe",
            DocumentSource::CvmFiling => "cvm_filing",
            DocumentSource::FundosNet => "fundos_net",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            DocumentSource::CvmIpe,
            DocumentSource::CvmFiling,
            DocumentSource::FundosNet,
        ]
        .into_iter()
        .find(|s| s.as_str() == value)
    }
}

/// Um documento entregue, já ligado aos ativos da carteira que ele cita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub source: DocumentSource,
    pub category: DocumentCategory,
    /// O tipo e a espécie como a fonte escreve ("AGO", "Edital de Convocação").
    pub title: Option<String>,
    /// O assunto, quando a fonte dá.
    pub subject: Option<String>,
    pub reference_date: Option<Date>,
    pub delivered_on: Date,
    pub version: i32,
    /// O documento original; o ITR e a DFP às vezes vêm sem link.
    pub url: Option<String>,
    /// ITR ou DFP, no resultado da companhia.
    pub filing: Option<FilingKind>,
}

/// Recebido, a receber ou com o pagamento ainda a definir pela empresa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomeState {
    Received,
    Upcoming,
    Undated,
}

/// A marca de "fora do padrão" num provento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnusualMark {
    pub direction: Direction,
    pub recent: Reference,
    pub long: Option<Reference>,
    pub years: u32,
}

impl From<&Unusual> for UnusualMark {
    fn from(u: &Unusual) -> Self {
        Self {
            direction: u.direction,
            recent: u.recent,
            long: u.long,
            years: u.years,
        }
    }
}

/// Um provento da pessoa (a data com ou o pagamento).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomeDetail {
    /// O provento da Renda; `None` no anúncio sem data de pagamento.
    pub income_id: Option<Uuid>,
    pub kind: IncomeKind,
    pub record_date: Option<Date>,
    pub ex_date: Option<Date>,
    pub pay_date: Option<Date>,
    /// Bruto por cota, da época (como a Renda guarda).
    pub per_unit: Decimal,
    pub quantity: Decimal,
    pub gross: Decimal,
    pub net: Decimal,
    pub state: IncomeState,
    pub source: IncomeSource,
    pub unusual: Option<UnusualMark>,
}

/// O anúncio fora do padrão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusualDetail {
    pub record_date: Date,
    pub ex_date: Date,
    pub approved_on: Option<Date>,
    /// As datas de pagamento conhecidas (parcelas), em ordem.
    pub pay_dates: Vec<Date>,
    pub unusual: Unusual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    Income(IncomeDetail),
    Unusual(UnusualDetail),
    Document(Document),
    Closed(Closure),
}

/// Um evento da Agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub date: Date,
    pub kind: EventKind,
    /// Os ativos da carteira que o evento cita (ON e PN da mesma companhia
    /// recebem o mesmo fato relevante); vazio no dia sem pregão.
    pub assets: Vec<Uuid>,
    pub detail: Detail,
}

// ---------------------------------------------------------------------------
// Datas
// ---------------------------------------------------------------------------

/// O pregão anterior a `day`. Fora do calendário conhecido, pulam o fim de
/// semana (a mesma reserva de `income_auto::ex_date_after`).
pub fn previous_trading_day(day: Date) -> Date {
    let mut current = day;
    for _ in 0..10 {
        let Some(before) = current.previous_day() else {
            return current;
        };
        current = before;
        let open = is_trading_day(current)
            .unwrap_or_else(|_| !matches!(current.weekday(), Weekday::Saturday | Weekday::Sunday));
        if open {
            return current;
        }
    }
    current
}

/// A data com que o `external_ref` de um provento automático guarda
/// (`b3a:<isin>:<rótulo>:<data com>:<valor>[:<parcela>]`).
pub fn record_from_ref(reference: &str) -> Option<Date> {
    let rest = reference.strip_prefix(super::income_auto::REF_PREFIX)?;
    let text = rest.split(':').nth(2)?;
    let format = time::macros::format_description!("[year]-[month]-[day]");
    Date::parse(text, &format).ok()
}

/// A data com de um provento da Renda: a do anúncio no automático; no resto, o
/// pregão antes da data ex. Sem data ex, não há data com a mostrar.
pub fn record_date_of(ex_date: Option<Date>, external_ref: Option<&str>) -> Option<Date> {
    external_ref
        .and_then(record_from_ref)
        .or_else(|| ex_date.map(previous_trading_day))
}

/// Se um período pedido vale: começo antes do fim e no máximo
/// [`MAX_RANGE_DAYS`].
pub fn valid_range(from: Date, to: Date) -> bool {
    from <= to && (to - from) <= Duration::days(MAX_RANGE_DAYS)
}

// ---------------------------------------------------------------------------
// Proventos
// ---------------------------------------------------------------------------

/// Um provento da Renda, como a Agenda o lê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomeItem {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub kind: IncomeKind,
    pub ex_date: Option<Date>,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub gross: Decimal,
    pub net: Decimal,
    pub source: IncomeSource,
    pub external_ref: Option<String>,
}

impl IncomeItem {
    pub fn record_date(&self) -> Option<Date> {
        record_date_of(self.ex_date, self.external_ref.as_deref())
    }
}

/// Uma linha do anúncio da B3 (um rótulo, uma parcela).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredRow {
    pub label: String,
    /// Bruto por cota, da época.
    pub rate: Decimal,
    pub pay_date: Option<Date>,
    pub approved_on: Option<Date>,
}

/// Tudo o que a B3 anunciou para um ativo numa data com.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub asset_id: Uuid,
    pub record_date: Date,
    pub ex_date: Date,
    pub rows: Vec<DeclaredRow>,
    pub unusual: Option<Unusual>,
}

impl Declared {
    /// O dia do anúncio: a aprovação mais cedo, se não for depois da data com.
    pub fn announced_on(&self) -> Date {
        self.rows
            .iter()
            .filter_map(|r| r.approved_on)
            .min()
            .filter(|d| *d <= self.record_date)
            .unwrap_or(self.record_date)
    }

    fn pay_dates(&self) -> Vec<Date> {
        let mut dates: Vec<Date> = self.rows.iter().filter_map(|r| r.pay_date).collect();
        dates.sort_unstable();
        dates.dedup();
        dates
    }

    /// Se um provento da Renda é deste anúncio: mesmo ativo e a mesma data com
    /// (o lançado à mão pode guardar a data com ou a data ex no campo da data
    /// ex: qualquer dia entre as duas conta).
    pub fn covers(&self, item: &IncomeItem) -> bool {
        item.asset_id == self.asset_id
            && (item.record_date() == Some(self.record_date)
                || item
                    .ex_date
                    .is_some_and(|d| d >= self.record_date && d <= self.ex_date))
    }
}

/// O provento estimado de uma linha "a definir": o valor por cota vezes as
/// cotas do livro no fim da data com, com o IR do JCP pela alíquota de hoje (o
/// pagamento ainda vai cair). Sem cota na data com, nada.
pub fn undated_income(
    declared: &Declared,
    row: &DeclaredRow,
    class: AssetClass,
    entries: &[Entry],
    today: Date,
) -> Result<Option<IncomeDetail>, LedgerError> {
    let quantity = quantity_on(entries, declared.record_date)?;
    if quantity <= Decimal::ZERO {
        return Ok(None);
    }
    let kind = cash_kind(&row.label, class);
    let Ok(values) = amounts(Market::B3, kind, today, row.rate, quantity, None) else {
        return Ok(None);
    };
    Ok(Some(IncomeDetail {
        income_id: None,
        kind,
        record_date: Some(declared.record_date),
        ex_date: Some(declared.ex_date),
        pay_date: None,
        per_unit: row.rate.normalize(),
        quantity: quantity.normalize(),
        gross: values.gross,
        net: values.net,
        state: IncomeState::Undated,
        source: IncomeSource::B3Announcement,
        unusual: declared.unusual.as_ref().map(UnusualMark::from),
    }))
}

/// Se o provento da Renda já cobre a linha "a definir" (alguém lançou à mão o
/// mesmo valor por cota, a 1%).
fn covered(row: &DeclaredRow, items: &[&IncomeItem]) -> bool {
    items.iter().any(|item| {
        row.rate > Decimal::ZERO
            && (item.amount_per_unit - row.rate).abs() <= row.rate * VALUE_TOLERANCE
    })
}

// ---------------------------------------------------------------------------
// A Agenda
// ---------------------------------------------------------------------------

/// O que entra na conta.
#[derive(Debug, Clone)]
pub struct Input {
    pub today: Date,
    pub from: Date,
    pub to: Date,
    pub income: Vec<IncomeItem>,
    pub declared: Vec<Declared>,
    /// Os "a definir" já estimados, um por linha do anúncio.
    pub undated: Vec<(Uuid, IncomeDetail)>,
    /// Cada documento com os ativos que ele cita.
    pub documents: Vec<(Vec<Uuid>, Document)>,
    pub closures: Vec<(Date, Closure)>,
}

fn income_detail(item: &IncomeItem, today: Date, mark: Option<UnusualMark>) -> IncomeDetail {
    IncomeDetail {
        income_id: Some(item.id),
        kind: item.kind,
        record_date: item.record_date(),
        ex_date: item.ex_date,
        pay_date: Some(item.pay_date),
        per_unit: item.amount_per_unit,
        quantity: item.quantity,
        gross: item.gross,
        net: item.net,
        state: if item.pay_date <= today {
            IncomeState::Received
        } else {
            IncomeState::Upcoming
        },
        source: item.source,
        unusual: mark,
    }
}

/// A marca de fora do padrão que cabe a um provento da Renda.
pub fn mark_for<'a>(item: &IncomeItem, declared: &'a [Declared]) -> Option<&'a Declared> {
    declared
        .iter()
        .find(|d| d.unusual.is_some() && d.covers(item))
}

/// Os eventos de `[from, to]`, em ordem: dia, tipo, e os ativos.
pub fn build(input: &Input) -> Vec<Event> {
    let (today, from, to) = (input.today, input.from, input.to);
    let inside = |date: Date| date >= from && date <= to;
    let mut events = Vec::new();

    for item in &input.income {
        let mark = mark_for(item, &input.declared)
            .and_then(|d| d.unusual.as_ref())
            .map(UnusualMark::from);
        let detail = income_detail(item, today, mark);
        if let Some(record) = detail.record_date.filter(|d| inside(*d)) {
            events.push(Event {
                date: record,
                kind: EventKind::IncomeRecord,
                assets: vec![item.asset_id],
                detail: Detail::Income(detail.clone()),
            });
        }
        if inside(item.pay_date) {
            events.push(Event {
                date: item.pay_date,
                kind: EventKind::IncomePayment,
                assets: vec![item.asset_id],
                detail: Detail::Income(detail),
            });
        }
    }

    for (asset, detail) in &input.undated {
        let Some(record) = detail.record_date.filter(|d| inside(*d)) else {
            continue;
        };
        events.push(Event {
            date: record,
            kind: EventKind::IncomeRecord,
            assets: vec![*asset],
            detail: Detail::Income(detail.clone()),
        });
    }

    for declared in &input.declared {
        let Some(found) = declared.unusual else {
            continue;
        };
        let date = declared.announced_on();
        if !inside(date) {
            continue;
        }
        events.push(Event {
            date,
            kind: EventKind::UnusualIncome,
            assets: vec![declared.asset_id],
            detail: Detail::Unusual(UnusualDetail {
                record_date: declared.record_date,
                ex_date: declared.ex_date,
                approved_on: declared.rows.iter().filter_map(|r| r.approved_on).min(),
                pay_dates: declared.pay_dates(),
                unusual: found,
            }),
        });
    }

    for (assets, document) in &input.documents {
        if assets.is_empty() || !inside(document.delivered_on) {
            continue;
        }
        events.push(Event {
            date: document.delivered_on,
            kind: document.category.event_kind(),
            assets: assets.clone(),
            detail: Detail::Document(document.clone()),
        });
    }

    for (date, closure) in &input.closures {
        if inside(*date) {
            events.push(Event {
                date: *date,
                kind: EventKind::MarketClosed,
                assets: Vec::new(),
                detail: Detail::Closed(*closure),
            });
        }
    }

    events.sort_by(|a, b| {
        (a.date, a.kind, &a.assets, sort_key(&a.detail)).cmp(&(
            b.date,
            b.kind,
            &b.assets,
            sort_key(&b.detail),
        ))
    });
    events
}

/// O desempate dentro do mesmo dia, tipo e ativo: estável entre chamadas.
fn sort_key(detail: &Detail) -> (Option<Uuid>, String) {
    match detail {
        Detail::Income(i) => (i.income_id, i.per_unit.to_string()),
        Detail::Unusual(u) => (None, u.record_date.to_string()),
        Detail::Document(d) => (None, d.url.clone().unwrap_or_default()),
        Detail::Closed(_) => (None, String::new()),
    }
}

/// Os "a definir" de um anúncio que ainda não estão na Renda.
pub fn undated_rows<'a>(declared: &'a Declared, income: &[IncomeItem]) -> Vec<&'a DeclaredRow> {
    let mine: Vec<&IncomeItem> = income.iter().filter(|i| declared.covers(i)).collect();
    declared
        .rows
        .iter()
        .filter(|r| r.pay_date.is_none() && !covered(r, &mine))
        .collect()
}

#[cfg(test)]
mod tests;
