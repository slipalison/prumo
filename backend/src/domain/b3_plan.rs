//! O plano da importação da B3: o que a prévia ([`super::b3_import`]) leu,
//! confrontado com o que já está na carteira, vira o que **seria gravado**.
//!
//! Regra pura: recebe a prévia, uma foto do livro da pessoa ([`Book`]) e as
//! decisões da tela ([`Decisions`]) e devolve o [`Plan`]. A rota de prévia
//! mostra o plano; a de gravação refaz o mesmo plano dentro da transação e só
//! grava se o resumo dele ([`Plan::digest`]) for o que a pessoa conferiu.
//! Detalhes e exemplos em `docs/importacao-b3.md`.
//!
//! # Cada linha: nova, já existe, conflita ou fica de fora
//!
//! - **Já existe** quando o `external_ref` já está no livro (reimportar não
//!   duplica) ou quando um lançamento digitado bate com ela: mesmo ativo,
//!   tipo, data e quantidade, e preço até [`PRICE_TOLERANCE`] de diferença (o
//!   "Já comprei" de um aporte que também aparece na B3). Esse lançamento
//!   ganha o `external_ref` da linha, para a próxima importação o achar direto.
//! - **Conflita** quando o livro recusaria a linha (venda além da posição com
//!   o saldo inicial escolhido, quantidade que a unidade do ativo não aceita).
//!   Conflito trava a gravação até a pessoa ajustar o saldo inicial ou deixar
//!   o ativo de fora.
//! - **Fica de fora** com um motivo: evento que não mexe na posição, título do
//!   Tesouro que não deu para identificar, provisionado sem data, ativo que a
//!   pessoa deixou de fora.
//!
//! # Saldo inicial
//!
//! A B3 começa em ~2019. Para cada ativo com lançamento novo, o plano propõe
//! um único saldo inicial (`opening`), datado na véspera do primeiro
//! lançamento do livro, e ele **substitui** os saldos iniciais e ajustes de
//! quantidade que já estavam lá (as quantidades que a pessoa digitou):
//!
//! - o bastante para nenhuma venda passar da posição (`covers_sales`);
//! - e, se a pessoa tinha digitado a quantidade, o que faltar para a posição
//!   na data do último saldo/ajuste digitado continuar a mesma
//!   (`keeps_quantity`): o que a B3 explica sai do saldo inicial, o resto fica.
//!
//! A pessoa pode trocar a quantidade e o preço médio. É a única entrada manual
//! do fluxo.

use std::collections::{BTreeMap, HashSet};

use aws_lc_rs::digest;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;
use time::Date;
use uuid::Uuid;

use super::allocation::round_cents;
use super::asset_class::{AssetClass, Market, UnitKind};
use super::b3_import::{
    fold, iso_date, AssetGroup, ClassBasis, Direction, EventKind, EventLine,
    IncomeKind as B3IncomeKind, IncomeLine, IncomeStatus as B3IncomeStatus, Preview, TradeSide,
};
use super::entitlement::{quantity_on, record_date};
use super::income::{IncomeKind, JCP_WITHHOLDING_RATE};
use super::ledger::{self, check_for_unit, Entry, Kind, LedgerError, Position};
use super::treasury::{Bond, BondKind};

/// Diferença de preço (relativa) que ainda conta como "o mesmo negócio" entre
/// um lançamento digitado e a linha da B3: o "Já comprei" grava o preço do
/// plano, e o negócio sai pelo preço do pregão.
pub const PRICE_TOLERANCE: Decimal = Decimal::from_parts(2, 0, 0, false, 2);

/// Distância máxima, em dias, entre o provisionado e o recebido do mesmo
/// provento (a empresa às vezes muda a data de pagamento).
pub const INCOME_WINDOW_DAYS: i64 = 20;

// ---------------------------------------------------------------------------
// Entrada: o livro da pessoa e as decisões da tela
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct BookAsset {
    pub id: Uuid,
    pub asset_class: AssetClass,
    pub symbol: Option<String>,
    pub display_name: String,
    pub listed_on_b3: bool,
    pub treasury: bool,
    pub quantity: Decimal,
}

impl BookAsset {
    pub fn market(&self) -> Market {
        self.asset_class.market_of(self.listed_on_b3, self.treasury)
    }
}

/// Um lançamento que já está no livro.
#[derive(Debug, Clone, PartialEq)]
pub struct BookEntry {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub entry: Entry,
    pub external_ref: Option<String>,
}

/// Um provento que já está gravado.
#[derive(Debug, Clone, PartialEq)]
pub struct BookIncome {
    pub id: Uuid,
    pub symbol: Option<String>,
    pub kind: IncomeKind,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub net: Decimal,
    /// Veio de uma importação da B3.
    pub imported: bool,
    /// Veio de "Proventos a receber": o recebido do mesmo provento o substitui.
    pub provisional: bool,
    pub external_ref: Option<String>,
}

/// A foto da carteira que o plano confronta.
#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    pub assets: Vec<BookAsset>,
    pub entries: Vec<BookEntry>,
    /// Todo `external_ref` do livro da pessoa, inclusive de ativo removido.
    pub transaction_refs: HashSet<String>,
    pub income: Vec<BookIncome>,
    /// Símbolos `TD-...` que a tabela do Tesouro conhece.
    pub treasury_symbols: Vec<String>,
    /// Hoje, no horário de Brasília: a data com de um provento a receber
    /// nunca passa de hoje (o livro não tem lançamento no futuro).
    pub today: Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpeningChoice {
    /// Zero: sem saldo inicial.
    pub quantity: Decimal,
    /// Preço médio em reais; `None` = custo desconhecido.
    pub unit_price: Option<Decimal>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetDecision {
    /// Classe de um ativo novo (vale só para ativo da B3 que ainda não existe).
    pub asset_class: Option<AssetClass>,
    /// Deixar o ativo inteiro de fora desta importação.
    pub skip: bool,
    /// O saldo inicial que a pessoa escolheu; `None` = o sugerido.
    pub opening: Option<OpeningChoice>,
}

/// As decisões da tela, pela chave do ativo na prévia (`PETR4`,
/// `TESOURO SELIC 2029`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decisions {
    pub assets: BTreeMap<String, AssetDecision>,
}

// ---------------------------------------------------------------------------
// Saída
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineState {
    New,
    Existing,
    Conflict,
    NotImported,
}

/// O destino de uma linha da prévia, pelo `external_ref` dela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineStatus {
    pub status: LineState,
    /// Por que fica de fora, conflita ou já existe.
    pub reason_code: Option<&'static str>,
}

impl LineStatus {
    fn new() -> Self {
        Self {
            status: LineState::New,
            reason_code: None,
        }
    }

    fn of(status: LineState, reason: &'static str) -> Self {
        Self {
            status,
            reason_code: Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetAction {
    /// O ativo nasce na carteira.
    Create,
    /// O ativo já existe e ganha lançamentos ou proventos.
    Update,
    /// A pessoa deixou de fora.
    Skip,
    /// Nada a gravar (tudo já existe, ou só fica de fora).
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningBasis {
    /// Cobre as vendas que a B3 mostra antes de qualquer compra.
    CoversSales,
    /// Mantém a quantidade que a pessoa tinha digitado.
    KeepsQuantity,
    /// Nada a sugerir.
    None,
}

/// O saldo inicial do ativo depois da importação.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpeningPlan {
    #[serde(with = "iso_date")]
    pub date: Date,
    #[serde(with = "rust_decimal::serde::float")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub unit_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float")]
    pub suggested_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub suggested_unit_price: Option<Decimal>,
    pub basis: OpeningBasis,
    /// A pessoa escolheu (em vez do sugerido).
    pub chosen: bool,
}

/// Um saldo inicial ou ajuste digitado que dá lugar ao saldo inicial novo.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplacedEntry {
    pub kind: Kind,
    #[serde(with = "iso_date")]
    pub date: Date,
    #[serde(with = "rust_decimal::serde::float")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub unit_price: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetConflict {
    /// `exceeds_position`, `unit`, `invalid`, `invalid_opening`.
    pub reason_code: &'static str,
    /// Texto do servidor (sem acento).
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AssetCounts {
    pub new_transactions: usize,
    pub existing_transactions: usize,
    pub new_income: usize,
    pub replaced_income: usize,
    pub existing_income: usize,
    pub not_imported: usize,
}

/// Um lançamento que vai para o livro, na ordem de gravação.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedEntry {
    pub kind: Kind,
    #[serde(with = "iso_date")]
    pub trade_date: Date,
    pub quantity: Decimal,
    pub unit_price: Option<Decimal>,
    pub external_ref: Option<String>,
}

/// Um provento a gravar (ou a trocar por cima de um provisionado).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IncomeWrite {
    /// O provisionado que esta linha substitui.
    pub replace: Option<Uuid>,
    pub kind: IncomeKind,
    #[serde(with = "iso_date")]
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub gross: Decimal,
    pub withholding: Decimal,
    pub net: Decimal,
    pub provisional: bool,
    pub external_ref: String,
}

/// O que a gravação faz no ativo.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AssetWrites {
    /// Saldos iniciais e ajustes digitados que o saldo inicial novo substitui.
    pub delete_entries: Vec<Uuid>,
    /// Lançamentos digitados que batem com uma linha: ganham o `external_ref`.
    pub adopt: Vec<(Uuid, String)>,
    pub entries: Vec<PlannedEntry>,
    pub income: Vec<IncomeWrite>,
}

impl AssetWrites {
    fn touches_ledger(&self) -> bool {
        !self.entries.is_empty() || !self.delete_entries.is_empty() || !self.adopt.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssetPlan {
    /// A chave do ativo na prévia.
    pub key: String,
    pub action: AssetAction,
    /// O ativo que já existe na carteira.
    pub asset_id: Option<Uuid>,
    /// O símbolo gravado (`PETR4`, `TD-SELIC-20290301`); `None` para título
    /// que não deu para identificar.
    pub symbol: Option<String>,
    pub display_name: String,
    pub asset_class: AssetClass,
    pub class_basis: ClassBasis,
    pub listed_on_b3: bool,
    pub treasury: bool,
    /// Só ativo novo da B3 aceita trocar a classe.
    pub class_editable: bool,
    /// Classe só pelo código e a pessoa ainda não confirmou: trava a gravação.
    pub class_needs_confirmation: bool,
    #[serde(with = "rust_decimal::serde::float")]
    pub current_quantity: Decimal,
    /// A quantidade depois de gravar. `None` com conflito ou sem mudança.
    #[serde(with = "rust_decimal::serde::float_option")]
    pub final_quantity: Option<Decimal>,
    pub opening: Option<OpeningPlan>,
    pub replaced: Vec<ReplacedEntry>,
    pub conflict: Option<AssetConflict>,
    pub counts: AssetCounts,
    #[serde(skip)]
    pub writes: AssetWrites,
}

impl AssetPlan {
    fn blocks(&self) -> bool {
        self.action != AssetAction::Skip
            && (self.conflict.is_some() || self.class_needs_confirmation)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlanSummary {
    pub assets_created: usize,
    pub assets_updated: usize,
    pub assets_skipped: usize,
    pub transactions_new: usize,
    pub transactions_existing: usize,
    pub income_new: usize,
    pub income_replaced: usize,
    pub income_existing: usize,
    pub openings: usize,
    pub not_imported: usize,
    pub conflicts: usize,
    pub needs_confirmation: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    /// Resumo (SHA-256, 128 bits) de tudo o que seria gravado. A gravação só
    /// acontece se o plano refeito na hora tiver o mesmo.
    pub digest: String,
    /// Sem conflito e sem classe por confirmar.
    pub ready: bool,
    pub assets: Vec<AssetPlan>,
    pub lines: BTreeMap<String, LineStatus>,
    pub summary: PlanSummary,
}

// ---------------------------------------------------------------------------
// O plano
// ---------------------------------------------------------------------------

/// O plano da importação. Determinístico: a mesma prévia, o mesmo livro e as
/// mesmas decisões dão o mesmo plano (e o mesmo `digest`).
pub fn plan(preview: &Preview, book: &Book, decisions: &Decisions) -> Plan {
    let mut lines: BTreeMap<String, LineStatus> = BTreeMap::new();
    let mut used = Used::default();
    let none = AssetDecision::default();
    let mut assets: Vec<AssetPlan> = preview
        .assets
        .iter()
        .map(|group| {
            let decision = decisions.assets.get(&group.key).unwrap_or(&none);
            plan_group(group, book, decision, &mut used, &mut lines)
        })
        .collect();
    assets.sort_by(|a, b| a.key.cmp(&b.key));

    let mut summary = PlanSummary::default();
    for asset in &assets {
        match asset.action {
            AssetAction::Create => summary.assets_created += 1,
            AssetAction::Update => summary.assets_updated += 1,
            AssetAction::Skip => summary.assets_skipped += 1,
            AssetAction::Nothing => {}
        }
        if asset.action == AssetAction::Skip {
            continue;
        }
        summary.transactions_new += asset.counts.new_transactions;
        summary.transactions_existing += asset.counts.existing_transactions;
        summary.income_new += asset.counts.new_income;
        summary.income_replaced += asset.counts.replaced_income;
        summary.income_existing += asset.counts.existing_income;
        if asset.writes.entries.iter().any(|e| e.kind == Kind::Opening) {
            summary.openings += 1;
        }
        if asset.conflict.is_some() {
            summary.conflicts += 1;
        }
        if asset.class_needs_confirmation {
            summary.needs_confirmation += 1;
        }
    }
    summary.not_imported = lines
        .values()
        .filter(|l| l.status == LineState::NotImported)
        .count();
    let ready = !assets.iter().any(AssetPlan::blocks);
    let digest = digest_of(&assets, &lines);
    Plan {
        digest,
        ready,
        assets,
        lines,
        summary,
    }
}

fn digest_of(assets: &[AssetPlan], lines: &BTreeMap<String, LineStatus>) -> String {
    let writes: Vec<&AssetWrites> = assets.iter().map(|a| &a.writes).collect();
    let canonical = serde_json::to_vec(&(assets, &writes, lines)).unwrap_or_default();
    let hash = digest::digest(&digest::SHA256, &canonical);
    hash.as_ref()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Lançamentos e proventos já usados por outra linha: cada um casa uma vez só.
#[derive(Default)]
struct Used {
    entries: HashSet<Uuid>,
    income: HashSet<Uuid>,
}

/// Para onde vão as linhas do grupo.
enum Target<'a> {
    Existing(&'a BookAsset),
    New {
        symbol: String,
        display_name: String,
        asset_class: AssetClass,
        listed_on_b3: bool,
        treasury: bool,
    },
    /// Título do Tesouro que não deu para ligar a nenhum título conhecido.
    Unidentified,
}

fn plan_group(
    group: &AssetGroup,
    book: &Book,
    decision: &AssetDecision,
    used: &mut Used,
    lines: &mut BTreeMap<String, LineStatus>,
) -> AssetPlan {
    let target = resolve_target(group, book, decision);
    let mut plan = AssetPlan {
        key: group.key.clone(),
        action: AssetAction::Nothing,
        asset_id: None,
        symbol: None,
        display_name: group.name.clone().unwrap_or_else(|| group.key.clone()),
        asset_class: group.suggested_class,
        class_basis: group.class_basis,
        listed_on_b3: group.listed_on_b3,
        treasury: group.treasury,
        class_editable: false,
        class_needs_confirmation: false,
        current_quantity: Decimal::ZERO,
        final_quantity: None,
        opening: None,
        replaced: Vec::new(),
        conflict: None,
        counts: AssetCounts::default(),
        writes: AssetWrites::default(),
    };
    match &target {
        Target::Existing(asset) => {
            plan.asset_id = Some(asset.id);
            plan.symbol = asset.symbol.clone();
            plan.display_name = asset.display_name.clone();
            plan.asset_class = asset.asset_class;
            plan.listed_on_b3 = asset.listed_on_b3;
            plan.treasury = asset.treasury;
            plan.current_quantity = asset.quantity;
        }
        Target::New {
            symbol,
            display_name,
            asset_class,
            listed_on_b3,
            treasury,
        } => {
            plan.symbol = Some(symbol.clone());
            plan.display_name = display_name.clone();
            plan.asset_class = *asset_class;
            plan.listed_on_b3 = *listed_on_b3;
            plan.treasury = *treasury;
            plan.class_editable = !treasury;
        }
        Target::Unidentified => {}
    }

    let refs = lines_of(group);
    if decision.skip {
        for r in refs {
            lines.insert(r, LineStatus::of(LineState::NotImported, "asset_skipped"));
        }
        plan.counts.not_imported = lines_of(group).len();
        plan.action = AssetAction::Skip;
        return plan;
    }
    if matches!(target, Target::Unidentified) {
        for r in refs {
            lines.insert(
                r,
                LineStatus::of(LineState::NotImported, "treasury_unidentified"),
            );
        }
        plan.counts.not_imported = lines_of(group).len();
        return plan;
    }

    let unit = match &target {
        Target::Existing(asset) => asset.market().unit_kind(),
        Target::New {
            asset_class,
            listed_on_b3,
            treasury,
            ..
        } => asset_class.market_of(*listed_on_b3, *treasury).unit_kind(),
        Target::Unidentified => UnitKind::Whole,
    };
    let context = Context {
        group,
        book,
        decision,
        target: &target,
        unit,
    };
    let ledger_after = plan_ledger(&context, used, lines, &mut plan);
    plan_income(group, book, &ledger_after, used, lines, &mut plan);

    let ledger_writes = plan.writes.touches_ledger();
    plan.action = match &target {
        Target::Existing(_) if ledger_writes || !plan.writes.income.is_empty() => {
            AssetAction::Update
        }
        Target::New { .. } if ledger_writes => AssetAction::Create,
        _ => AssetAction::Nothing,
    };
    if plan.action == AssetAction::Create {
        plan.class_needs_confirmation =
            group.class_basis == ClassBasis::Code && decision.asset_class.is_none();
    }
    if plan.action == AssetAction::Nothing {
        // Proventos de ativo que não está na carteira entram sem ativo ligado.
        plan.class_editable = false;
    }
    plan
}

fn lines_of(group: &AssetGroup) -> Vec<String> {
    group
        .trades
        .iter()
        .map(|t| t.external_ref.clone())
        .chain(group.events.iter().map(|e| e.external_ref.clone()))
        .chain(group.income.iter().map(|i| i.external_ref.clone()))
        .collect()
}

fn resolve_target<'a>(group: &AssetGroup, book: &'a Book, decision: &AssetDecision) -> Target<'a> {
    if group.treasury {
        return match treasury_target(&group.key, book) {
            Some(TreasuryTarget::Existing(asset)) => Target::Existing(asset),
            Some(TreasuryTarget::New(bond)) => Target::New {
                symbol: bond.symbol(),
                display_name: bond.name(),
                asset_class: AssetClass::FixedIncome,
                listed_on_b3: false,
                treasury: true,
            },
            None => Target::Unidentified,
        };
    }
    let mut same: Vec<&BookAsset> = book
        .assets
        .iter()
        .filter(|a| !a.treasury && a.symbol.as_deref() == Some(group.key.as_str()))
        .collect();
    // Dois ativos com o mesmo código em classes diferentes: vale o da B3.
    same.sort_by_key(|a| a.market() != Market::B3);
    if let Some(asset) = same.first() {
        return Target::Existing(asset);
    }
    Target::New {
        symbol: group.key.clone(),
        display_name: group
            .name
            .clone()
            .unwrap_or_else(|| group.key.clone())
            .chars()
            .take(120)
            .collect(),
        asset_class: decision.asset_class.unwrap_or(group.suggested_class),
        listed_on_b3: true,
        treasury: false,
    }
}

enum TreasuryTarget<'a> {
    Existing(&'a BookAsset),
    New(Bond),
}

/// `TESOURO SELIC 2029` → o tipo e o ano. A B3 não traz o dia do vencimento:
/// o título é o único daquele tipo e ano na carteira ou na tabela do Tesouro.
pub fn treasury_kind_and_year(product: &str) -> Option<(BondKind, i32)> {
    let text = product.trim();
    let (title, year) = text.rsplit_once(' ')?;
    let year: i32 = year.parse().ok().filter(|_| year.len() == 4)?;
    let kind = BondKind::from_title(title)?;
    Some((kind, year))
}

fn treasury_target<'a>(key: &str, book: &'a Book) -> Option<TreasuryTarget<'a>> {
    let (kind, year) = treasury_kind_and_year(key)?;
    let matches = |bond: &Bond| bond.kind == kind && bond.maturity.year() == year;
    let held: Vec<&BookAsset> = book
        .assets
        .iter()
        .filter(|a| a.treasury)
        .filter(|a| {
            a.symbol
                .as_deref()
                .and_then(Bond::from_symbol)
                .is_some_and(|b| matches(&b))
        })
        .collect();
    match held.as_slice() {
        [one] => return Some(TreasuryTarget::Existing(one)),
        [] => {}
        _ => return None,
    }
    let mut known: Vec<Bond> = book
        .treasury_symbols
        .iter()
        .filter_map(|s| Bond::from_symbol(s))
        .filter(matches)
        .collect();
    known.sort();
    known.dedup();
    match known.as_slice() {
        [one] => Some(TreasuryTarget::New(*one)),
        _ => None,
    }
}

// --- livro ------------------------------------------------------------------

/// Quantidade que depende da posição naquela data.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Conversion {
    Fixed,
    /// Grupamento em que a B3 só credita as cotas novas: a quantidade que
    /// deixa de existir é a posição menos esse total.
    ReverseToTotal(Decimal),
    /// Amortização sem a quantidade de cotas: todas as da posição receberam.
    AmortizeHeld(Decimal),
}

#[derive(Debug, Clone)]
struct Candidate {
    /// As linhas da prévia que formam este lançamento (o grupamento pode
    /// juntar o crédito e o débito do mesmo dia).
    members: Vec<String>,
    store_ref: String,
    kind: Kind,
    date: Date,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    conversion: Conversion,
    /// Desempate no mesmo dia: o que soma vem antes do que tira.
    rank: u8,
    line: u32,
}

fn rank_of(kind: Kind) -> u8 {
    match kind {
        Kind::Opening => 0,
        Kind::Buy => 1,
        Kind::Split => 2,
        Kind::Bonus => 3,
        Kind::Adjustment => 4,
        Kind::Amortization => 5,
        Kind::ReverseSplit => 6,
        Kind::Sell => 7,
        Kind::Redemption => 8,
    }
}

fn price8(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(8, RoundingStrategy::MidpointAwayFromZero)
}

fn per_unit(value: Option<Decimal>, quantity: Decimal) -> Option<Decimal> {
    value
        .filter(|_| quantity > Decimal::ZERO)
        .and_then(|v| v.checked_div(quantity))
        .map(price8)
        .filter(|p| *p > Decimal::ZERO)
}

/// O lançamento que cada linha vira, ou por que ela fica de fora.
fn candidates(group: &AssetGroup) -> (Vec<Candidate>, Vec<(String, &'static str)>) {
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for trade in &group.trades {
        let kind = match trade.side {
            TradeSide::Buy => Kind::Buy,
            TradeSide::Sell => Kind::Sell,
        };
        out.push(Candidate {
            members: vec![trade.external_ref.clone()],
            store_ref: trade.external_ref.clone(),
            kind,
            date: trade.date,
            quantity: trade.quantity,
            unit_price: Some(price8(trade.unit_price)),
            conversion: Conversion::Fixed,
            rank: rank_of(kind),
            line: trade.line,
        });
    }
    // Grupamento: crédito e débito do mesmo dia são um evento só.
    let mut reverse: BTreeMap<Date, Vec<&EventLine>> = BTreeMap::new();
    for event in &group.events {
        if event.kind == EventKind::ReverseSplit {
            reverse.entry(event.date).or_default().push(event);
            continue;
        }
        match event_candidate(event) {
            Ok(candidate) => out.push(candidate),
            Err(reason) => skipped.push((event.external_ref.clone(), reason)),
        }
    }
    for (date, events) in reverse {
        let members: Vec<String> = events.iter().map(|e| e.external_ref.clone()).collect();
        let sum = |direction: Direction| -> Decimal {
            events
                .iter()
                .filter(|e| e.direction == direction)
                .filter_map(|e| e.quantity)
                .sum()
        };
        let (credited, debited) = (sum(Direction::Credit), sum(Direction::Debit));
        let (quantity, conversion) = if debited > Decimal::ZERO && credited > Decimal::ZERO {
            (debited - credited, Conversion::Fixed)
        } else if debited > Decimal::ZERO {
            (debited, Conversion::Fixed)
        } else {
            (credited, Conversion::ReverseToTotal(credited))
        };
        if quantity <= Decimal::ZERO {
            for r in members {
                skipped.push((r, "reverse_split_unclear"));
            }
            continue;
        }
        let store_ref = if members.len() == 1 {
            members[0].clone()
        } else {
            combined_ref(&members)
        };
        out.push(Candidate {
            store_ref,
            members,
            kind: Kind::ReverseSplit,
            date,
            quantity,
            unit_price: None,
            conversion,
            rank: rank_of(Kind::ReverseSplit),
            line: events.iter().map(|e| e.line).min().unwrap_or(0),
        });
    }
    out.sort_by_key(|a| (a.date, a.rank, a.line));
    (out, skipped)
}

/// O `external_ref` de um lançamento que junta várias linhas (sem depender da
/// ordem delas).
fn combined_ref(members: &[String]) -> String {
    let mut sorted = members.to_vec();
    sorted.sort();
    let hash = digest::digest(&digest::SHA256, sorted.join("|").as_bytes());
    let hex: String = hash
        .as_ref()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("b3:{}:event:{hex}", super::b3_import::REF_VERSION)
}

fn event_candidate(event: &EventLine) -> Result<Candidate, &'static str> {
    let fixed = |kind: Kind, quantity: Decimal, unit_price: Option<Decimal>| Candidate {
        members: vec![event.external_ref.clone()],
        store_ref: event.external_ref.clone(),
        kind,
        date: event.date,
        quantity,
        unit_price,
        conversion: Conversion::Fixed,
        rank: rank_of(kind),
        line: event.line,
    };
    let credit = event.direction == Direction::Credit;
    match event.kind {
        EventKind::Split if credit => Ok(fixed(
            Kind::Split,
            event.quantity.ok_or("missing_quantity")?,
            None,
        )),
        EventKind::Bonus if credit => Ok(fixed(
            Kind::Bonus,
            event.quantity.ok_or("missing_quantity")?,
            event.unit_price.filter(|p| *p > Decimal::ZERO).map(price8),
        )),
        EventKind::Split | EventKind::Bonus => Err("event_debit"),
        EventKind::FractionInKind => Err("fraction_in_kind"),
        EventKind::Amortization if credit => {
            let value = event.value.ok_or("missing_value")?;
            match event.quantity {
                Some(quantity) => {
                    let price = event
                        .unit_price
                        .filter(|p| *p > Decimal::ZERO)
                        .map(price8)
                        .or_else(|| per_unit(Some(value), quantity))
                        .ok_or("missing_value")?;
                    Ok(fixed(Kind::Amortization, quantity, Some(price)))
                }
                None => Ok(Candidate {
                    conversion: Conversion::AmortizeHeld(value),
                    ..fixed(Kind::Amortization, Decimal::ZERO, None)
                }),
            }
        }
        EventKind::Amortization => Err("event_debit"),
        EventKind::Redemption | EventKind::Maturity | EventKind::TreasuryMovement => {
            let Some(quantity) = event.quantity else {
                return Err(if event.treasury {
                    "treasury_interest"
                } else {
                    "missing_quantity"
                });
            };
            let price = event
                .unit_price
                .filter(|p| *p > Decimal::ZERO)
                .map(price8)
                .or_else(|| per_unit(event.value, quantity))
                .ok_or("missing_value")?;
            let words = fold(&event.description);
            let kind = if event.kind != EventKind::TreasuryMovement {
                Kind::Redemption
            } else if credit {
                Kind::Buy
            } else if words.contains("resgate") || words.contains("vencimento") {
                Kind::Redemption
            } else {
                Kind::Sell
            };
            Ok(fixed(kind, quantity, Some(price)))
        }
        EventKind::ReverseSplit => Err("reverse_split_unclear"),
    }
}

fn prices_close(a: Option<Decimal>, b: Option<Decimal>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let base = a.abs().max(b.abs());
            (a - b).abs() <= base * PRICE_TOLERANCE || (a - b).abs() <= Decimal::new(1, 2)
        }
        _ => false,
    }
}

/// O menor passo da unidade: a folga para o grupamento não zerar a posição.
fn unit_step(unit: UnitKind) -> Decimal {
    match unit {
        UnitKind::Whole => Decimal::ONE,
        UnitKind::Hundredth | UnitKind::Value => Decimal::new(1, 2),
        UnitKind::Fractional => Decimal::new(1, 8),
    }
}

/// Arredonda para cima no passo da unidade (o saldo de ação é inteiro).
fn ceil_to_unit(value: Decimal, unit: UnitKind) -> Decimal {
    let places = match unit {
        UnitKind::Whole => 0,
        UnitKind::Hundredth | UnitKind::Value => 2,
        UnitKind::Fractional => 10,
    };
    value.round_dp_with_strategy(places, RoundingStrategy::AwayFromZero)
}

/// Um lançamento do livro simulado: o que já está (com o `seq` do banco) ou o
/// que vai entrar (com o `seq` que o banco vai dar, na ordem de gravação).
#[derive(Debug, Clone)]
struct Simulated {
    entry: Entry,
    conversion: Conversion,
    /// Índice em `candidates` quando é linha nova.
    candidate: Option<usize>,
}

/// O que o plano de um ativo consulta.
struct Context<'a> {
    group: &'a AssetGroup,
    book: &'a Book,
    decision: &'a AssetDecision,
    target: &'a Target<'a>,
    unit: UnitKind,
}

/// Planeja os lançamentos do ativo e devolve o livro dele como fica depois
/// de gravar (o de hoje, se nada entra ou se conflita): é dele que sai a
/// quantidade na data com dos proventos sem quantidade.
fn plan_ledger(
    context: &Context<'_>,
    used: &mut Used,
    lines: &mut BTreeMap<String, LineStatus>,
    plan: &mut AssetPlan,
) -> Vec<Entry> {
    let Context {
        group,
        book,
        decision,
        target,
        unit,
    } = *context;
    let (all, skipped) = candidates(group);
    for (r, reason) in skipped {
        lines.insert(r, LineStatus::of(LineState::NotImported, reason));
        plan.counts.not_imported += 1;
    }
    let asset_id = match target {
        Target::Existing(asset) => Some(asset.id),
        _ => None,
    };
    let mut current: Vec<&BookEntry> = book
        .entries
        .iter()
        .filter(|e| Some(e.asset_id) == asset_id)
        .collect();
    current.sort_by(|a, b| ledger::ledger_order(&a.entry, &b.entry));
    let unchanged: Vec<Entry> = current.iter().map(|e| e.entry.clone()).collect();

    // Já existe? Pelo `external_ref` ou pelo lançamento digitado igual.
    let mut fresh: Vec<Candidate> = Vec::new();
    for candidate in all {
        let status = if book.transaction_refs.contains(&candidate.store_ref) {
            Some("same_ref")
        } else if candidate.conversion == Conversion::Fixed {
            let found = current.iter().find(|e| {
                e.external_ref.is_none()
                    && !used.entries.contains(&e.id)
                    && e.entry.kind == candidate.kind
                    && e.entry.trade_date == candidate.date
                    && e.entry.quantity == candidate.quantity
                    && prices_close(e.entry.unit_price, candidate.unit_price)
            });
            found.map(|e| {
                used.entries.insert(e.id);
                plan.writes.adopt.push((e.id, candidate.store_ref.clone()));
                "same_entry"
            })
        } else {
            None
        };
        match status {
            Some(reason) => {
                plan.counts.existing_transactions += 1;
                for r in &candidate.members {
                    lines.insert(r.clone(), LineStatus::of(LineState::Existing, reason));
                }
            }
            None => fresh.push(candidate),
        }
    }
    if fresh.is_empty() {
        return unchanged;
    }
    for candidate in &fresh {
        for r in &candidate.members {
            lines.insert(r.clone(), LineStatus::new());
        }
    }

    // Saldos iniciais e ajustes digitados dão lugar a um saldo inicial só.
    let replay_now = ledger::replay(&unchanged);
    let mut checkpoint: Option<(Date, Decimal)> = None;
    if let Ok(steps) = &replay_now {
        for step in steps {
            if matches!(step.entry.kind, Kind::Opening | Kind::Adjustment) {
                checkpoint = Some((step.entry.trade_date, step.position.quantity));
            }
        }
    }
    let mut replaced_openings: Vec<(Decimal, Option<Decimal>)> = Vec::new();
    let kept: Vec<&BookEntry> = current
        .iter()
        .copied()
        .filter(|e| {
            let typed = matches!(e.entry.kind, Kind::Opening | Kind::Adjustment);
            if typed {
                plan.writes.delete_entries.push(e.id);
                plan.replaced.push(ReplacedEntry {
                    kind: e.entry.kind,
                    date: e.entry.trade_date,
                    quantity: e.entry.quantity,
                    unit_price: e.entry.unit_price,
                });
                if e.entry.kind == Kind::Opening {
                    replaced_openings.push((e.entry.quantity, e.entry.unit_price));
                }
            }
            !typed
        })
        .collect();

    // O livro simulado sem saldo inicial: o que já fica e o que entra.
    let base_seq = current.iter().map(|e| e.entry.seq).max().unwrap_or(0) + 1;
    let mut simulated: Vec<Simulated> = kept
        .iter()
        .map(|e| Simulated {
            entry: e.entry.clone(),
            conversion: Conversion::Fixed,
            candidate: None,
        })
        .collect();
    for (i, candidate) in fresh.iter().enumerate() {
        simulated.push(Simulated {
            entry: Entry {
                kind: candidate.kind,
                trade_date: candidate.date,
                seq: base_seq + 1 + i64::try_from(i).unwrap_or(0),
                quantity: candidate.quantity,
                unit_price: candidate.unit_price,
                fees: Decimal::ZERO,
                fx_rate: Decimal::ONE,
            },
            conversion: candidate.conversion,
            candidate: Some(i),
        });
    }
    simulated.sort_by(|a, b| ledger::ledger_order(&a.entry, &b.entry));

    let suggestion = suggest_opening(&simulated, checkpoint, unit);
    let suggested_price = average_of(&replaced_openings);
    let first_date = simulated
        .first()
        .map(|s| s.entry.trade_date)
        .unwrap_or(group_first_date(&fresh));
    let opening_date = first_date.previous_day().unwrap_or(first_date);
    let (quantity, unit_price, chosen) = match decision.opening {
        Some(choice) => (choice.quantity, choice.unit_price, true),
        None => (suggestion.0, suggested_price, false),
    };
    if quantity > Decimal::ZERO || suggestion.0 > Decimal::ZERO || chosen {
        plan.opening = Some(OpeningPlan {
            date: opening_date,
            quantity,
            unit_price: unit_price.filter(|_| quantity > Decimal::ZERO),
            suggested_quantity: suggestion.0,
            suggested_unit_price: suggested_price,
            basis: suggestion.1,
            chosen,
        });
    }
    if let Err(detail) = check_opening(quantity, unit_price, unit) {
        plan.conflict = Some(AssetConflict {
            reason_code: "invalid_opening",
            detail,
        });
        return unchanged;
    }
    if quantity > Decimal::ZERO {
        simulated.insert(
            0,
            Simulated {
                entry: Entry {
                    kind: Kind::Opening,
                    trade_date: opening_date,
                    seq: base_seq,
                    quantity,
                    unit_price,
                    fees: Decimal::ZERO,
                    fx_rate: Decimal::ONE,
                },
                conversion: Conversion::Fixed,
                candidate: None,
            },
        );
        plan.writes.entries.push(PlannedEntry {
            kind: Kind::Opening,
            trade_date: opening_date,
            quantity,
            unit_price,
            external_ref: None,
        });
    }

    // O livro de verdade, com as quantidades que dependem da posição.
    let mut position = Position::EMPTY;
    let mut applied: Vec<Entry> = Vec::with_capacity(simulated.len());
    let mut resolved: Vec<Option<PlannedEntry>> = vec![None; fresh.len()];
    for step in &simulated {
        let mut entry = step.entry.clone();
        match step.conversion {
            Conversion::Fixed => {}
            Conversion::ReverseToTotal(total) => entry.quantity = position.quantity - total,
            Conversion::AmortizeHeld(value) => {
                entry.quantity = position.quantity;
                entry.unit_price = per_unit(Some(value), position.quantity);
            }
        }
        let refused = |plan: &mut AssetPlan,
                       lines: &mut BTreeMap<String, LineStatus>,
                       code: &'static str,
                       detail: String| {
            if let Some(i) = step.candidate {
                for r in &fresh[i].members {
                    lines.insert(r.clone(), LineStatus::of(LineState::Conflict, code));
                }
            }
            plan.conflict = Some(AssetConflict {
                reason_code: code,
                detail,
            });
        };
        if step.candidate.is_some() {
            if let Err(message) = check_for_unit(entry.kind, entry.quantity, entry.unit_price, unit)
            {
                refused(plan, lines, "unit", message.to_owned());
                return unchanged;
            }
        }
        match ledger::apply(position, &entry) {
            Ok((next, _)) => position = next,
            Err(error) => {
                let code = match error {
                    LedgerError::ExceedsPosition { .. } => "exceeds_position",
                    LedgerError::Invalid { .. } | LedgerError::Overflow { .. } => "invalid",
                };
                refused(plan, lines, code, error.to_string());
                return unchanged;
            }
        }
        if let Some(i) = step.candidate {
            resolved[i] = Some(PlannedEntry {
                kind: entry.kind,
                trade_date: entry.trade_date,
                quantity: entry.quantity,
                unit_price: entry.unit_price,
                external_ref: Some(fresh[i].store_ref.clone()),
            });
        }
        applied.push(entry);
    }
    // Ordem de gravação = a ordem do `seq` simulado.
    plan.writes.entries.extend(resolved.into_iter().flatten());
    plan.counts.new_transactions += fresh.len();
    plan.final_quantity = Some(position.quantity);
    applied
}

fn group_first_date(fresh: &[Candidate]) -> Date {
    fresh.iter().map(|c| c.date).min().unwrap_or(Date::MIN)
}

fn check_opening(
    quantity: Decimal,
    unit_price: Option<Decimal>,
    unit: UnitKind,
) -> Result<(), String> {
    if quantity < Decimal::ZERO {
        return Err("O saldo inicial nao pode ser negativo.".into());
    }
    if unit_price.is_some_and(|p| p < Decimal::ZERO) {
        return Err("O preco medio nao pode ser negativo.".into());
    }
    if quantity > Decimal::ZERO {
        check_for_unit(Kind::Opening, quantity, unit_price, unit).map_err(str::to_owned)?;
    }
    Ok(())
}

/// Preço médio dos saldos iniciais substituídos, quando todos tinham preço.
fn average_of(openings: &[(Decimal, Option<Decimal>)]) -> Option<Decimal> {
    if openings.is_empty() || openings.iter().any(|(_, p)| p.is_none()) {
        return None;
    }
    let quantity: Decimal = openings.iter().map(|(q, _)| *q).sum();
    let cost: Decimal = openings
        .iter()
        .map(|(q, p)| *q * p.unwrap_or_default())
        .sum();
    cost.checked_div(quantity).map(price8)
}

/// O saldo inicial que faz o livro fechar e, havendo quantidade digitada, a
/// mantém. Caminha o livro sem saldo inicial guardando a maior falta; depois
/// de um grupamento "até o total" a posição deixa de depender do saldo
/// inicial e a conta para ali.
fn suggest_opening(
    simulated: &[Simulated],
    checkpoint: Option<(Date, Decimal)>,
    unit: UnitKind,
) -> (Decimal, OpeningBasis) {
    let step = unit_step(unit);
    let mut held = Decimal::ZERO;
    let mut need = Decimal::ZERO;
    let mut reset = false;
    let mut at_checkpoint: Option<Decimal> = None;
    let mut reset_before_checkpoint = false;
    for s in simulated {
        let e = &s.entry;
        if let Some((date, _)) = checkpoint {
            if at_checkpoint.is_none() && e.trade_date > date {
                at_checkpoint = Some(held);
                reset_before_checkpoint = reset;
            }
        }
        match s.conversion {
            Conversion::ReverseToTotal(total) => {
                if !reset {
                    need = need.max(total + step - held);
                }
                held = total;
                reset = true;
                continue;
            }
            Conversion::AmortizeHeld(_) => {
                if !reset {
                    need = need.max(step - held);
                }
                continue;
            }
            Conversion::Fixed => {}
        }
        let before = held;
        held += e.kind.quantity_delta(e.quantity);
        if reset {
            continue;
        }
        match e.kind {
            Kind::Amortization => need = need.max(e.quantity - before),
            Kind::ReverseSplit => need = need.max(step - held),
            _ => need = need.max(-held),
        }
    }
    if checkpoint.is_some() && at_checkpoint.is_none() {
        at_checkpoint = Some(held);
        reset_before_checkpoint = reset;
    }
    let keep = match (checkpoint, at_checkpoint) {
        (Some((_, typed)), Some(explained)) if !reset_before_checkpoint => typed - explained,
        _ => Decimal::ZERO,
    };
    let need = ceil_to_unit(need.max(Decimal::ZERO), unit);
    let keep = ceil_to_unit(keep.max(Decimal::ZERO), unit);
    if keep > need {
        (keep, OpeningBasis::KeepsQuantity)
    } else if need > Decimal::ZERO {
        (need, OpeningBasis::CoversSales)
    } else {
        (Decimal::ZERO, OpeningBasis::None)
    }
}

// --- proventos --------------------------------------------------------------

/// O tipo de provento do Prumo para o tipo da B3.
pub fn income_kind(kind: B3IncomeKind) -> IncomeKind {
    match kind {
        B3IncomeKind::Dividend => IncomeKind::Dividend,
        B3IncomeKind::Jcp => IncomeKind::Jcp,
        B3IncomeKind::Yield => IncomeKind::ReitIncome,
        B3IncomeKind::Reimbursement | B3IncomeKind::FractionAuction => IncomeKind::Other,
    }
}

/// Os valores gravados de um provento importado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncomeValues {
    /// Valor por cota da planilha; sem ele, líquido ÷ quantidade; sem os dois, zero.
    pub amount_per_unit: Decimal,
    /// Zero quando a planilha não traz.
    pub quantity: Decimal,
    pub gross: Decimal,
    pub withholding: Decimal,
    pub net: Decimal,
}

/// Os valores gravados de uma linha de provento. A B3 manda o **líquido**; no
/// JCP o bruto é o líquido ÷ 0,85 (o IR de 15% retido na fonte) e a retenção
/// é a diferença. Nos outros tipos, bruto = líquido.
///
/// A quantidade é a da B3 quando ela traz; sem ela, `held`: as cotas que o
/// livro tinha na data com (a mesma regra de `GET /income/entitlement`).
pub fn income_values(line: &IncomeLine, held: Option<Decimal>) -> IncomeValues {
    let net = round_cents(line.net_amount);
    let quantity = line
        .quantity
        .or(held.filter(|q| *q > Decimal::ZERO))
        .unwrap_or(Decimal::ZERO);
    let amount_per_unit = line
        .unit_price
        .map(price8)
        .or_else(|| per_unit(Some(net), quantity))
        .unwrap_or(Decimal::ZERO);
    let gross = if line.kind == B3IncomeKind::Jcp {
        net.checked_div(Decimal::ONE - JCP_WITHHOLDING_RATE)
            .map(round_cents)
            .unwrap_or(net)
    } else {
        net
    };
    IncomeValues {
        amount_per_unit,
        quantity,
        gross,
        withholding: gross - net,
        net,
    }
}

/// O mesmo provento: tipo igual, datas perto e o mesmo valor por cota (quando
/// os dois têm); sem o valor por cota, só a mesma data.
fn same_event(
    kind_a: IncomeKind,
    per_unit_a: Decimal,
    date_a: Date,
    kind_b: IncomeKind,
    per_unit_b: Decimal,
    date_b: Date,
) -> bool {
    if kind_a != kind_b {
        return false;
    }
    let distance = (date_a - date_b).whole_days().abs();
    if per_unit_a > Decimal::ZERO && per_unit_b > Decimal::ZERO {
        distance <= INCOME_WINDOW_DAYS && price8(per_unit_a) == price8(per_unit_b)
    } else {
        distance == 0
    }
}

fn plan_income(
    group: &AssetGroup,
    book: &Book,
    ledger_after: &[Entry],
    used: &mut Used,
    lines: &mut BTreeMap<String, LineStatus>,
    plan: &mut AssetPlan,
) {
    let refs: HashSet<&str> = book
        .income
        .iter()
        .filter_map(|i| i.external_ref.as_deref())
        .collect();
    let mine: Vec<&BookIncome> = book
        .income
        .iter()
        .filter(|i| i.symbol.as_deref() == Some(group.key.as_str()))
        .collect();
    // Recebidos primeiro: o provisionado do mesmo envio pode já ter virado recebido.
    let mut ordered: Vec<&IncomeLine> = group.income.iter().collect();
    ordered.sort_by_key(|l| (l.status == B3IncomeStatus::Provisioned, l.date, l.line));
    let mut received_here: Vec<(IncomeKind, Decimal, Date)> = Vec::new();
    for line in ordered {
        let kind = income_kind(line.kind);
        let provisional = line.status == B3IncomeStatus::Provisioned;
        let mut set = |status: LineStatus| {
            lines.insert(line.external_ref.clone(), status);
        };
        if refs.contains(line.external_ref.as_str()) {
            plan.counts.existing_income += 1;
            set(LineStatus::of(LineState::Existing, "same_ref"));
            continue;
        }
        let Some(date) = line.date else {
            plan.counts.not_imported += 1;
            set(LineStatus::of(
                LineState::NotImported,
                "provisioned_without_date",
            ));
            continue;
        };
        // Sem data ex na B3: a data com é a do pagamento, até hoje.
        let held = line
            .quantity
            .is_none()
            .then(|| quantity_on(ledger_after, record_date(None, date, book.today)).ok())
            .flatten();
        let values = income_values(line, held);
        let (amount_per_unit, net) = (values.amount_per_unit, values.net);
        if provisional
            && received_here
                .iter()
                .any(|(k, p, d)| same_event(*k, *p, *d, kind, amount_per_unit, date))
        {
            plan.counts.not_imported += 1;
            set(LineStatus::of(
                LineState::NotImported,
                "superseded_by_received",
            ));
            continue;
        }
        if !provisional {
            received_here.push((kind, amount_per_unit, date));
        }
        // O provisionado de antes (da mesma pessoa e do mesmo ativo) que esta
        // linha substitui: o mais perto na data.
        let replace = mine
            .iter()
            .filter(|i| i.imported && i.provisional && !used.income.contains(&i.id))
            .filter(|i| {
                same_event(
                    i.kind,
                    i.amount_per_unit,
                    i.pay_date,
                    kind,
                    amount_per_unit,
                    date,
                )
            })
            .min_by_key(|i| ((i.pay_date - date).whole_days().abs(), (i.net - net).abs()))
            .map(|i| i.id);
        if let Some(id) = replace {
            used.income.insert(id);
        } else {
            // Já recebido (de outra importação) ou digitado à mão.
            let known = mine.iter().find(|i| {
                !used.income.contains(&i.id)
                    && ((provisional
                        && !i.provisional
                        && same_event(
                            i.kind,
                            i.amount_per_unit,
                            i.pay_date,
                            kind,
                            amount_per_unit,
                            date,
                        ))
                        || (!i.imported
                            && i.kind == kind
                            && i.pay_date == date
                            && (i.net - net).abs() <= Decimal::new(1, 2)))
            });
            if let Some(found) = known {
                used.income.insert(found.id);
                plan.counts.existing_income += 1;
                set(LineStatus::of(LineState::Existing, "same_income"));
                continue;
            }
        }
        if replace.is_some() {
            plan.counts.replaced_income += 1;
            set(LineStatus::of(LineState::New, "replaces_provisioned"));
        } else {
            plan.counts.new_income += 1;
            set(LineStatus::new());
        }
        plan.writes.income.push(IncomeWrite {
            replace,
            kind,
            pay_date: date,
            amount_per_unit,
            quantity: values.quantity,
            gross: values.gross,
            withholding: values.withholding,
            net,
            provisional,
            external_ref: line.external_ref.clone(),
        });
    }
}

#[cfg(test)]
mod tests;
