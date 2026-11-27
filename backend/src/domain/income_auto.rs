//! Proventos automáticos: o anúncio da B3 vezes a quantidade do livro na data
//! com. Regra pura, sem banco e sem rede; o porquê de cada decisão está em
//! `docs/proventos-automaticos.md`.
//!
//! - **Fonte.** A página de eventos corporativos de cada emissor na B3 traz,
//!   por ISIN, o valor por cota (`rate`), a data com (`lastDatePrior`) e a data
//!   de pagamento. Os textos vêm em pt-BR (`0,02424250000`, `31/08/2026`).
//! - **Quantidade.** A posição do livro no fim da data com, pela mesma regra do
//!   provento lançado à mão ([`quantity_on`]): saldo inicial conta desde
//!   sempre, compra na data ex não conta.
//! - **Derivado.** O conjunto de automáticos da pessoa é refeito a cada rodada:
//!   [`plan`] devolve o que inserir, trocar e apagar para ele ficar igual ao que
//!   os anúncios e o livro dizem hoje. O que veio da pessoa (`manual`) ou do
//!   extrato (`b3_import`) nunca entra no plano, e sempre vence: um automático
//!   igual a um deles não é criado.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;
use time::{Date, Duration, Month, Weekday};
use uuid::Uuid;

use super::asset_class::{AssetClass, Market};
use super::entitlement::quantity_on;
use super::income::{amounts, jcp_withholding_rate, IncomeAmounts, IncomeKind, IncomeSource};
use super::ledger::{Entry, Kind};
use super::tax::calendar::is_trading_day;

/// O começo do `external_ref` de um provento automático.
pub const REF_PREFIX: &str = "b3a:";

/// Quanto o valor por cota de dois proventos pode diferir e eles ainda serem o
/// mesmo: 1%. Cobre o arredondamento de outra ferramenta (2 casas no lugar de
/// 11) sem juntar dois proventos de valores de fato diferentes.
pub const VALUE_TOLERANCE: Decimal = Decimal::from_parts(1, 0, 0, false, 2);

/// Até quantos dias para trás um evento em ações vira pendência: o histórico
/// da B3 cobre de 12 a 18 meses, e o que é mais velho do que isso já devia
/// estar no livro (ou no saldo inicial).
pub const PENDING_WINDOW_DAYS: i64 = 548;

/// Quantos dias depois da data com um lançamento de bonificação, desdobramento
/// ou grupamento ainda conta como "já lançado": a B3 credita as cotas novas
/// dias depois da data ex, e a pessoa pode datar por uma ou por outra.
pub const PENDING_MATCH_DAYS: i64 = 90;

// ---------------------------------------------------------------------------
// Texto da B3
// ---------------------------------------------------------------------------

/// `0,02424250000` ou `1.234,56` → número. Vazio ou ilegível → `None`.
pub fn parse_br_decimal(text: &str) -> Option<Decimal> {
    let text = text.trim();
    if text.is_empty()
        || !text
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    {
        return None;
    }
    text.replace('.', "").replace(',', ".").parse().ok()
}

/// `31/08/2026` → data. Vazio, ilegível ou o "sem data" da B3 (ano 9999) →
/// `None`.
pub fn parse_br_date(text: &str) -> Option<Date> {
    let mut parts = text.trim().split('/');
    let day: u8 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let year: i32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1900..=2200).contains(&year) {
        return None;
    }
    Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()
}

// ---------------------------------------------------------------------------
// Código de negociação e ISIN
// ---------------------------------------------------------------------------

/// Um código de negociação da B3: o emissor (`ITSA`) e o número (`4`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticker {
    pub code: String,
    pub issuer: String,
    pub number: u8,
}

/// `ITSA4`, `TAEE11`, `B3SA3`, `AAPL34` (e o fracionário `ITSA4F`) → partes.
/// O emissor são os quatro primeiros caracteres; o resto, um ou dois dígitos.
pub fn parse_ticker(symbol: &str) -> Option<Ticker> {
    let upper = symbol.trim().to_ascii_uppercase();
    let base = upper
        .strip_suffix(".SA")
        .unwrap_or(&upper)
        .trim_end_matches('F');
    if !base.is_ascii() || !(5..=6).contains(&base.len()) {
        return None;
    }
    let (issuer, number) = base.split_at(4);
    let valid_issuer = issuer.chars().all(|c| c.is_ascii_alphanumeric())
        && issuer.chars().any(|c| c.is_ascii_alphabetic());
    if !valid_issuer || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(Ticker {
        code: base.to_owned(),
        issuer: issuer.to_owned(),
        number: number.parse().ok()?,
    })
}

/// Qual página da B3 tem os anúncios do ativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Venue {
    /// Empresa listada (ação, unit, BDR).
    Company,
    /// Fundo listado (FII, ETF).
    Fund,
}

impl Venue {
    pub fn as_str(self) -> &'static str {
        match self {
            Venue::Company => "company",
            Venue::Fund => "fund",
        }
    }

    /// A página de um ativo: FII é fundo; Ações Nacionais é empresa (unit
    /// inclusive); fora das duas classes, o que a B3 negocia com final 11 é
    /// ETF (fundo) e o resto é BDR (empresa).
    pub fn of(class: AssetClass, ticker: &Ticker) -> Self {
        match class {
            AssetClass::BrReits => Venue::Fund,
            AssetClass::BrStocks => Venue::Company,
            _ if ticker.number == 11 => Venue::Fund,
            _ => Venue::Company,
        }
    }
}

/// Os tipos de papel no meio do ISIN que o número do código costuma ter.
/// É a regra de reserva, para quando o cadastro da B3 (`otherCodes`) não diz:
/// `BR` + emissor + tipo + série + dígito (`BRITSAACNPR7` é ITSA, `ACNPR`
/// ação preferencial).
fn isin_kinds(number: u8, venue: Venue) -> &'static [&'static str] {
    match (number, venue) {
        (3, _) => &["ACNOR"],
        (4, _) => &["ACNPR"],
        (5, _) => &["ACNPA"],
        (6, _) => &["ACNPB"],
        (7, _) => &["ACNPC"],
        (8, _) => &["ACNPD"],
        (11, Venue::Fund) => &["CTF"],
        (11, Venue::Company) => &["CDAM", "UNT"],
        (31..=39, _) => &["BDR"],
        _ => &[],
    }
}

/// O ISIN de um código. Vale o cadastro da B3 (`listing`); sem ele, a regra do
/// tipo de papel entre os ISIN que o emissor anunciou (`known`), e só quando
/// um único bate — dois candidatos é dúvida, e dúvida não vira provento.
pub fn isin_for(
    ticker: &Ticker,
    venue: Venue,
    listing: Option<&str>,
    known: &[&str],
) -> Option<String> {
    if let Some(isin) = listing {
        return Some(isin.to_owned());
    }
    let prefixes: Vec<String> = isin_kinds(ticker.number, venue)
        .iter()
        .map(|kind| format!("BR{}{kind}", ticker.issuer))
        .collect();
    let mut found: Vec<&str> = known
        .iter()
        .copied()
        .filter(|isin| prefixes.iter().any(|p| isin.starts_with(p.as_str())))
        .collect();
    found.sort_unstable();
    found.dedup();
    match found.as_slice() {
        [only] => Some((*only).to_owned()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Anúncios
// ---------------------------------------------------------------------------

/// Um provento em dinheiro anunciado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashAnnouncement {
    pub issuer: String,
    pub isin: String,
    pub label: String,
    /// Data com.
    pub record_date: Date,
    /// Valor bruto por cota.
    pub rate: Decimal,
    /// `None`: a empresa ainda não definiu.
    pub pay_date: Option<Date>,
    pub approved_on: Option<Date>,
    pub related_to: Option<String>,
}

/// Um evento em ações anunciado (bonificação, desdobramento, grupamento...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockAnnouncement {
    pub issuer: String,
    pub isin: String,
    pub label: String,
    pub record_date: Date,
    /// Percentual na bonificação e no desdobramento; multiplicador no grupamento.
    pub factor: Decimal,
    pub approved_on: Option<Date>,
}

/// O tipo do provento pelo rótulo da B3. "RENDIMENTO" é rendimento de FII só
/// no FII; numa ação é a correção do valor entre a data com e o pagamento, e
/// num ETF a distribuição do fundo — os dois sem regra de IR que a B3 informe,
/// e por isso "outro". O que a B3 chamar de outro jeito também é "outro".
pub fn cash_kind(label: &str, class: AssetClass) -> IncomeKind {
    let label = label.trim().to_ascii_uppercase();
    match label.as_str() {
        "DIVIDENDO" => IncomeKind::Dividend,
        "JRS CAP PROPRIO" => IncomeKind::Jcp,
        "RENDIMENTO" if class == AssetClass::BrReits => IncomeKind::ReitIncome,
        _ if label.starts_with("AMORTIZ") => IncomeKind::Amortization,
        _ => IncomeKind::Other,
    }
}

/// O evento em ações que vira pendência. Os outros rótulos da B3 (resgate,
/// cisão com redução de capital) ficam de fora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StockEventKind {
    Bonus,
    Split,
    ReverseSplit,
}

impl StockEventKind {
    pub fn of_label(label: &str) -> Option<Self> {
        match label.trim().to_ascii_uppercase().as_str() {
            "BONIFICACAO" => Some(StockEventKind::Bonus),
            "DESDOBRAMENTO" => Some(StockEventKind::Split),
            "GRUPAMENTO" => Some(StockEventKind::ReverseSplit),
            _ => None,
        }
    }

    /// O lançamento do livro que resolve a pendência.
    pub fn ledger_kind(self) -> Kind {
        match self {
            StockEventKind::Bonus => Kind::Bonus,
            StockEventKind::Split => Kind::Split,
            StockEventKind::ReverseSplit => Kind::ReverseSplit,
        }
    }
}

/// Os feriados de data fixa do fim de ano em que a B3 nunca abre: 24/12,
/// 25/12, 31/12 e 1/1. É a reserva para ano fora do calendário.
fn year_end_closed(day: Date) -> bool {
    matches!(
        (u8::from(day.month()), day.day()),
        (12, 24) | (12, 25) | (12, 31) | (1, 1)
    )
}

/// A data ex: o primeiro **pregão** da B3 depois da data com
/// ([`is_trading_day`]: sem pregão em feriado nacional, Carnaval, Corpus
/// Christi, 24/12 e 31/12). Fora do calendário conhecido, pulam o sábado, o
/// domingo e os feriados fixos do fim de ano.
pub fn ex_date_after(record_date: Date) -> Date {
    let mut day = record_date;
    for _ in 0..10 {
        let Some(next) = day.next_day() else {
            return day;
        };
        day = next;
        let open = is_trading_day(day).unwrap_or_else(|_| {
            !matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) && !year_end_closed(day)
        });
        if open {
            return day;
        }
    }
    day
}

/// O identificador estável do automático: o mesmo anúncio dá sempre o mesmo,
/// e a rodada repetida não duplica. A data de pagamento não entra, para a
/// empresa poder mudá-la sem o automático virar outro.
///
/// `installment` é a ordem da parcela entre os anúncios com o mesmo ISIN,
/// rótulo, data com e valor (um provento pago em parcelas vem da B3 numa linha
/// por parcela, como o dividendo de 12/2025 da WEGE3 em três pagamentos
/// anuais), da data de pagamento mais cedo à mais tarde: a primeira (`0`) fica
/// com o identificador de sempre, a segunda ganha `:2` no fim, e assim por
/// diante.
pub fn external_ref(announcement: &CashAnnouncement, installment: usize) -> String {
    let label: String = announcement
        .label
        .trim()
        .to_ascii_uppercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let base = format!(
        "{REF_PREFIX}{}:{label}:{}:{}",
        announcement.isin,
        announcement.record_date,
        announcement.rate.normalize()
    );
    match installment {
        0 => base,
        n => format!("{base}:{}", n + 1),
    }
}

// ---------------------------------------------------------------------------
// Mesmo provento
// ---------------------------------------------------------------------------

/// Tipos que contam como iguais na comparação: dividendo, rendimento e outro
/// são "dinheiro distribuído" (a importação da B3 chama tudo de Rendimento,
/// outras ferramentas lançam rendimento de FII como dividendo); JCP e
/// amortização são cada um o seu.
fn family(kind: IncomeKind) -> u8 {
    match kind {
        IncomeKind::Jcp => 1,
        IncomeKind::Amortization => 2,
        IncomeKind::Dividend | IncomeKind::ReitIncome | IncomeKind::Other => 0,
    }
}

/// Os valores de um provento, para comparar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payment {
    pub kind: IncomeKind,
    pub pay_date: Date,
    /// Por cota; zero quando quem lançou não informou.
    pub per_unit: Decimal,
    pub gross: Decimal,
    pub net: Decimal,
}

fn close(value: Decimal, reference: Decimal) -> bool {
    reference > Decimal::ZERO && (value - reference).abs() <= reference * VALUE_TOLERANCE
}

/// Se um provento já gravado (`existing`) tem o mesmo tipo e o mesmo valor do
/// automático (`auto`, valor por cota bruto). No JCP o valor por cota gravado
/// pode ser o líquido (o extrato da B3 e muitas planilhas dão o líquido): os
/// dois contam. Sem valor por cota no gravado, compara o líquido e o bruto.
pub fn same_value(existing: &Payment, auto: &Payment) -> bool {
    if family(existing.kind) != family(auto.kind) {
        return false;
    }
    if existing.per_unit > Decimal::ZERO {
        let mut references = vec![auto.per_unit];
        if auto.kind == IncomeKind::Jcp {
            references.push(auto.per_unit * (Decimal::ONE - jcp_withholding_rate(auto.pay_date)));
        }
        references
            .into_iter()
            .any(|reference| close(existing.per_unit, reference))
    } else {
        close(existing.net, auto.net) || close(existing.gross, auto.gross)
    }
}

// ---------------------------------------------------------------------------
// O plano da rodada
// ---------------------------------------------------------------------------

/// Um ativo da B3 da pessoa, com o ISIN já resolvido e o livro.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoAsset {
    pub id: Uuid,
    pub symbol: String,
    pub display_name: String,
    pub class: AssetClass,
    pub isin: Option<String>,
    pub entries: Vec<Entry>,
}

/// Um provento que já está gravado (de qualquer origem).
#[derive(Debug, Clone, PartialEq)]
pub struct KnownIncome {
    pub id: Uuid,
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub kind: IncomeKind,
    pub ex_date: Option<Date>,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub gross: Decimal,
    pub withholding: Decimal,
    pub net: Decimal,
    pub source: IncomeSource,
    pub external_ref: Option<String>,
    pub display_name: String,
}

/// Um automático como ele deve estar gravado.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoIncome {
    pub asset_id: Uuid,
    pub symbol: String,
    pub display_name: String,
    pub kind: IncomeKind,
    pub ex_date: Option<Date>,
    pub pay_date: Date,
    pub amount_per_unit: Decimal,
    pub quantity: Decimal,
    pub amounts: IncomeAmounts,
    pub external_ref: String,
}

impl AutoIncome {
    fn matches(&self, known: &KnownIncome) -> bool {
        known.asset_id == Some(self.asset_id)
            && known.symbol.as_deref() == Some(self.symbol.as_str())
            && known.display_name == self.display_name
            && known.kind == self.kind
            && known.ex_date == self.ex_date
            && known.pay_date == self.pay_date
            && known.amount_per_unit == self.amount_per_unit
            && known.quantity == self.quantity
            && known.gross == self.amounts.gross
            && known.withholding == self.amounts.withholding
            && known.net == self.amounts.net
    }
}

/// O que a rodada grava para o conjunto de automáticos ficar certo.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoPlan {
    pub insert: Vec<AutoIncome>,
    /// O automático gravado (id) e como ele deve ficar.
    pub update: Vec<(Uuid, AutoIncome)>,
    pub delete: Vec<Uuid>,
    /// Quantos anúncios já estavam lançados à mão ou importados.
    pub covered: usize,
    /// Ativos cujo livro não fecha: os automáticos deles ficam como estão.
    pub skipped: Vec<Uuid>,
}

impl AutoPlan {
    pub fn is_empty(&self) -> bool {
        self.insert.is_empty() && self.update.is_empty() && self.delete.is_empty()
    }
}

/// Se o gravado é do mesmo ativo: pelo id, ou pelo código quando o provento
/// ficou sem ativo.
fn same_asset(known: &KnownIncome, asset: &AutoAsset) -> bool {
    match known.asset_id {
        Some(id) => id == asset.id,
        None => known.symbol.as_deref() == Some(asset.symbol.as_str()),
    }
}

/// Como o gravado e o automático caem no mesmo dia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SameDay {
    /// A mesma data de pagamento.
    Payment,
    /// A data com (o gravado pode guardar a data com ou a data ex no campo
    /// `ex_date`: qualquer dia entre as duas conta).
    Record,
}

/// Se o gravado é o mesmo provento do automático: mesmo ativo, mesmo valor
/// ([`same_value`]) e o mesmo dia, pelo critério `day`.
fn same_income(
    known: &KnownIncome,
    asset: &AutoAsset,
    auto: &AutoIncome,
    record: Date,
    day: SameDay,
) -> bool {
    if !same_asset(known, asset) {
        return false;
    }
    let same_day = match day {
        SameDay::Payment => known.pay_date == auto.pay_date,
        SameDay::Record => known
            .ex_date
            .is_some_and(|d| d >= record && d <= auto.ex_date.unwrap_or(record)),
    };
    same_day
        && same_value(
            &Payment {
                kind: known.kind,
                pay_date: known.pay_date,
                per_unit: known.amount_per_unit,
                gross: known.gross,
                net: known.net,
            },
            &Payment {
                kind: auto.kind,
                pay_date: auto.pay_date,
                per_unit: auto.amount_per_unit,
                gross: auto.amounts.gross,
                net: auto.amounts.net,
            },
        )
}

/// As parcelas que a pessoa lançou de um anúncio que a B3 publica numa linha só:
/// dois ou mais proventos do ativo, ainda sem par, da mesma família e da mesma
/// data com do anúncio, cuja soma tem o valor dele ([`same_value`]). A AMBEV
/// anunciou em 12/2025 um JCP de 0,269 com pagamento "até 31/12/2026" e pagou
/// em três vezes (0,075 + 0,0755 + 0,1185); quem lançou as três não pode ver o
/// automático de 0,269 em dobro.
fn installments_of(
    winners: &[&KnownIncome],
    used: &HashSet<Uuid>,
    asset: &AutoAsset,
    auto: &AutoIncome,
    record: Date,
) -> Option<Vec<Uuid>> {
    let parts: Vec<&KnownIncome> = winners
        .iter()
        .copied()
        .filter(|k| {
            !used.contains(&k.id)
                && same_asset(k, asset)
                && family(k.kind) == family(auto.kind)
                && k.ex_date
                    .is_some_and(|d| d >= record && d <= auto.ex_date.unwrap_or(record))
        })
        .collect();
    if parts.len() < 2 {
        return None;
    }
    let sum = |field: fn(&KnownIncome) -> Decimal| parts.iter().map(|k| field(k)).sum::<Decimal>();
    // Sem valor por cota em alguma parcela, a soma por cota não vale: compara
    // o líquido e o bruto.
    let per_unit = if parts.iter().all(|k| k.amount_per_unit > Decimal::ZERO) {
        sum(|k| k.amount_per_unit)
    } else {
        Decimal::ZERO
    };
    let together = Payment {
        kind: parts[0].kind,
        pay_date: auto.pay_date,
        per_unit,
        gross: sum(|k| k.gross),
        net: sum(|k| k.net),
    };
    let announced = Payment {
        kind: auto.kind,
        pay_date: auto.pay_date,
        per_unit: auto.amount_per_unit,
        gross: auto.amounts.gross,
        net: auto.amounts.net,
    };
    same_value(&together, &announced).then(|| parts.iter().map(|k| k.id).collect())
}

/// Se dois anúncios são parcelas do mesmo provento: mesmo ISIN, rótulo, data
/// com e valor.
fn same_announcement(a: &CashAnnouncement, b: &CashAnnouncement) -> bool {
    a.isin == b.isin && a.label == b.label && a.record_date == b.record_date && a.rate == b.rate
}

/// O automático de um anúncio, ou `None` quando não há o que gravar: sem data
/// de pagamento (a rodada espera a empresa definir; inventar a data poria o
/// dinheiro no mês errado), ou sem cota na data com.
fn derive(
    asset: &AutoAsset,
    announcement: &CashAnnouncement,
    installment: usize,
) -> Result<Option<AutoIncome>, super::ledger::LedgerError> {
    let Some(pay_date) = announcement.pay_date else {
        return Ok(None);
    };
    let quantity = quantity_on(&asset.entries, announcement.record_date)?;
    if quantity <= Decimal::ZERO {
        return Ok(None);
    }
    let kind = cash_kind(&announcement.label, asset.class);
    let Ok(values) = amounts(
        Market::B3,
        kind,
        pay_date,
        announcement.rate,
        quantity,
        None,
    ) else {
        return Ok(None);
    };
    if values.gross <= Decimal::ZERO {
        return Ok(None);
    }
    let ex_date = Some(ex_date_after(announcement.record_date)).filter(|ex| *ex <= pay_date);
    Ok(Some(AutoIncome {
        asset_id: asset.id,
        symbol: asset.symbol.clone(),
        display_name: asset.display_name.clone(),
        kind,
        ex_date,
        pay_date,
        amount_per_unit: announcement.rate.round_dp(8).normalize(),
        quantity: quantity.normalize(),
        amounts: values,
        external_ref: external_ref(announcement, installment),
    }))
}

/// O plano da rodada de uma pessoa: os automáticos que os anúncios e o livro
/// pedem hoje, contra os que estão gravados.
///
/// - Anúncio sem data de pagamento ou sem cota na data com: nada.
/// - Igual a um provento `manual` ou `b3_import` do mesmo ativo
///   ([`same_income`], um para um): nada — o da pessoa vence.
/// - Apagado pela pessoa (`dismissed`): nada.
/// - O resto é inserido, ou trocado se o gravado com o mesmo `external_ref`
///   mudou (a quantidade do livro, a data de pagamento); automático gravado
///   que o plano não pede mais (a posição zerou, o anúncio sumiu, um
///   importado chegou) é apagado.
pub fn plan(
    assets: &[AutoAsset],
    announcements: &[CashAnnouncement],
    known: &[KnownIncome],
    dismissed: &HashSet<String>,
) -> AutoPlan {
    let mut result = AutoPlan::default();
    let current: HashMap<&str, &KnownIncome> = known
        .iter()
        .filter(|k| k.source == IncomeSource::B3Announcement)
        .filter_map(|k| k.external_ref.as_deref().map(|r| (r, k)))
        .collect();
    let winners: Vec<&KnownIncome> = known
        .iter()
        .filter(|k| matches!(k.source, IncomeSource::Manual | IncomeSource::B3Import))
        .collect();
    let mut used: HashSet<Uuid> = HashSet::new();
    let mut wanted: HashMap<String, AutoIncome> = HashMap::new();
    let mut kept: HashSet<String> = HashSet::new();

    let mut by_isin: HashMap<&str, Vec<&CashAnnouncement>> = HashMap::new();
    for announcement in announcements {
        by_isin
            .entry(announcement.isin.as_str())
            .or_default()
            .push(announcement);
    }
    for list in by_isin.values_mut() {
        // As parcelas de um anúncio ficam juntas, da data de pagamento mais
        // cedo à mais tarde, e a que ainda não tem data por último: quando ela
        // ganhar data, as de antes não mudam de ordem.
        list.sort_by_key(|a| {
            (
                a.record_date,
                a.label.clone(),
                a.rate,
                a.pay_date.is_none(),
                a.pay_date,
            )
        });
    }

    let mut ordered: Vec<&AutoAsset> = assets.iter().collect();
    ordered.sort_by_key(|a| a.id);
    for asset in ordered {
        let Some(isin) = asset.isin.as_deref() else {
            continue;
        };
        let mut derived = Vec::new();
        let mut broken = false;
        let list = by_isin.get(isin).map_or(&[][..], Vec::as_slice);
        for (index, announcement) in list.iter().enumerate() {
            let installment = list[..index]
                .iter()
                .rev()
                .take_while(|before| same_announcement(before, announcement))
                .count();
            match derive(asset, announcement, installment) {
                Ok(Some(auto)) => derived.push((announcement.record_date, auto)),
                Ok(None) => {}
                Err(_) => {
                    broken = true;
                    break;
                }
            }
        }
        if broken {
            // Livro que não fecha: não dá para saber a quantidade. Os
            // automáticos deste ativo ficam como estão até o livro ser corrigido.
            result.skipped.push(asset.id);
            for k in known.iter().filter(|k| {
                k.source == IncomeSource::B3Announcement && k.asset_id == Some(asset.id)
            }) {
                if let Some(r) = &k.external_ref {
                    kept.insert(r.clone());
                }
            }
            continue;
        }
        // Primeiro o um para um pela data de pagamento, em todos os anúncios;
        // depois pela data com; só o que sobrar procura as parcelas da pessoa.
        // Assim a parcela de 2027 lançada à mão não cobre a de 2026 (as duas têm
        // a mesma data com), e um grupo nunca toma o provento que outro anúncio
        // casaria sozinho.
        let mut unmatched = derived;
        for day in [SameDay::Payment, SameDay::Record] {
            let mut left = Vec::new();
            for (record, auto) in unmatched {
                let winner = winners
                    .iter()
                    .find(|k| !used.contains(&k.id) && same_income(k, asset, &auto, record, day));
                if let Some(winner) = winner {
                    used.insert(winner.id);
                    result.covered += 1;
                } else {
                    left.push((record, auto));
                }
            }
            unmatched = left;
        }
        for (record, auto) in unmatched {
            if let Some(parts) = installments_of(&winners, &used, asset, &auto, record) {
                used.extend(parts);
                result.covered += 1;
                continue;
            }
            if dismissed.contains(&auto.external_ref) || wanted.contains_key(&auto.external_ref) {
                continue;
            }
            wanted.insert(auto.external_ref.clone(), auto);
        }
    }

    let mut wanted: Vec<AutoIncome> = wanted.into_values().collect();
    wanted.sort_by(|a, b| a.external_ref.cmp(&b.external_ref));
    for auto in &wanted {
        kept.insert(auto.external_ref.clone());
    }
    for auto in wanted {
        match current.get(auto.external_ref.as_str()) {
            Some(existing) if auto.matches(existing) => {}
            Some(existing) => result.update.push((existing.id, auto)),
            None => result.insert.push(auto),
        }
    }
    let mut stale: Vec<Uuid> = current
        .iter()
        .filter(|(reference, _)| !kept.contains(**reference))
        .map(|(_, k)| k.id)
        .collect();
    stale.sort_unstable();
    result.delete = stale;
    result
}

// ---------------------------------------------------------------------------
// Eventos em ações: pendência, nunca lançamento
// ---------------------------------------------------------------------------

/// Um evento em ações que a B3 anunciou, a pessoa tinha cota na data com e o
/// livro ainda não tem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingStockEvent {
    pub asset_id: Uuid,
    pub symbol: String,
    pub kind: StockEventKind,
    pub record_date: Date,
    pub factor: Decimal,
    /// As cotas do livro no fim da data com.
    pub quantity: Decimal,
}

/// As pendências de um ativo. Fica de fora o evento:
///
/// - mais velho que [`PENDING_WINDOW_DAYS`] ou com data com no futuro;
/// - anterior ao saldo inicial (o saldo inicial é a quantidade do dia do
///   cadastro, que já inclui o evento);
/// - sem cota na data com;
/// - já lançado: um lançamento do mesmo tipo entre a data com e
///   [`PENDING_MATCH_DAYS`] depois.
pub fn pending_stock_events(
    asset: &AutoAsset,
    events: &[StockAnnouncement],
    today: Date,
) -> Vec<PendingStockEvent> {
    let Some(isin) = asset.isin.as_deref() else {
        return Vec::new();
    };
    let since = today.saturating_sub(Duration::days(PENDING_WINDOW_DAYS));
    let opening = asset
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Opening)
        .map(|e| e.trade_date)
        .max();
    let mut pending: Vec<PendingStockEvent> = events
        .iter()
        .filter(|e| e.isin == isin && e.record_date > since && e.record_date <= today)
        .filter(|e| opening.is_none_or(|o| e.record_date >= o))
        .filter_map(|e| {
            let kind = StockEventKind::of_label(&e.label)?;
            let until = e
                .record_date
                .saturating_add(Duration::days(PENDING_MATCH_DAYS));
            let launched = asset.entries.iter().any(|entry| {
                entry.kind == kind.ledger_kind()
                    && entry.trade_date >= e.record_date
                    && entry.trade_date <= until
            });
            if launched {
                return None;
            }
            let quantity = quantity_on(&asset.entries, e.record_date).ok()?;
            (quantity > Decimal::ZERO).then(|| PendingStockEvent {
                asset_id: asset.id,
                symbol: asset.symbol.clone(),
                kind,
                record_date: e.record_date,
                factor: e.factor.normalize(),
                quantity: quantity.normalize(),
            })
        })
        .collect();
    pending.sort_by(|a, b| (b.record_date, &a.symbol).cmp(&(a.record_date, &b.symbol)));
    pending.dedup_by(|a, b| {
        a.kind == b.kind && a.record_date == b.record_date && a.asset_id == b.asset_id
    });
    pending
}

#[cfg(test)]
mod tests;
