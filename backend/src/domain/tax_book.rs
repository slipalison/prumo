//! A ponte entre o livro de lançamentos (`domain::ledger`), os proventos
//! (`domain::income`) e o imposto (`domain::tax`).
//!
//! O IR não vê a carteira como a carteira se vê:
//!
//! - **Instrumento fiscal** ([`TaxInstrument`]): o que o ativo é para a
//!   Receita. Um ETF de bitcoin da B3 é cripto na carteira e ETF no IR; um ETF
//!   de IMA-B é renda fixa nos dois e fica fora ([`TaxInstrument::Unsupported`]).
//!   O padrão sai da classe, da marca "negociado na B3" e do nome (as mesmas
//!   pistas que `quotes::classify_b3` usa para separar ETF e BDR); a pessoa
//!   pode trocar na folha do ativo.
//! - **Eventos**: a B3 informa o desdobramento e o grupamento em cotas
//!   recebidas ou perdidas, o livro guarda assim, e o IR quer o fator. A
//!   conversão usa a posição do próprio IR naquele dia, que processa os
//!   eventos antes dos negócios do dia — assim a quantidade do IR no fim de
//!   cada dia é a do livro.
//! - **O que o IR do Prumo não sabe fazer** sai da conta e vira um
//!   [`Skipped`] com o motivo: amortização e resgate, saldo inicial sem custo,
//!   ajuste manual que zera a posição, renda fixa, exterior fora de BDR.
//!
//! Tudo em reais: preço e taxa de cada lançamento multiplicados pelo câmbio
//! gravado nele (`fx_rate`, 1 em reais).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::Date;

use super::asset_class::AssetClass;
use super::income::IncomeKind as PaymentKind;
use super::ledger::{ledger_order, Entry, Kind};
use super::tax::irpf::{AnnualReport, Income, IncomeKind, Payer};
use super::tax::monthly::MonthReport;
use super::tax::positions::{Action, OpeningPosition, Operation, QUANTITY_DECIMALS};
use super::tax::rules::{ExemptionId, Pool};
use super::tax::Instrument;
use crate::quotes::{is_bdr, is_index_fund, tracks_fixed_income};

/// O que o ativo é para o imposto, como a pessoa (ou o padrão) decidiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxInstrument {
    Stock,
    Etf,
    Bdr,
    RealEstateFund,
    AgribusinessFund,
    Crypto,
    /// Fora do IR do Prumo: renda fixa, Tesouro, ETF de renda fixa, ações e
    /// ETFs no exterior (fora BDR), cripto em exchange estrangeira.
    Unsupported,
}

impl TaxInstrument {
    pub const ALL: [TaxInstrument; 7] = [
        TaxInstrument::Stock,
        TaxInstrument::Etf,
        TaxInstrument::Bdr,
        TaxInstrument::RealEstateFund,
        TaxInstrument::AgribusinessFund,
        TaxInstrument::Crypto,
        TaxInstrument::Unsupported,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TaxInstrument::Stock => "stock",
            TaxInstrument::Etf => "etf",
            TaxInstrument::Bdr => "bdr",
            TaxInstrument::RealEstateFund => "real_estate_fund",
            TaxInstrument::AgribusinessFund => "agribusiness_fund",
            TaxInstrument::Crypto => "crypto",
            TaxInstrument::Unsupported => "unsupported",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        TaxInstrument::ALL.into_iter().find(|i| i.as_str() == value)
    }

    /// O instrumento do domínio do IR; `None` fora do que o Prumo calcula.
    pub fn instrument(self) -> Option<Instrument> {
        match self {
            TaxInstrument::Stock => Some(Instrument::Stock),
            TaxInstrument::Etf => Some(Instrument::Etf),
            TaxInstrument::Bdr => Some(Instrument::Bdr),
            TaxInstrument::RealEstateFund => Some(Instrument::RealEstateFund),
            TaxInstrument::AgribusinessFund => Some(Instrument::AgribusinessFund),
            TaxInstrument::Crypto => Some(Instrument::Crypto),
            TaxInstrument::Unsupported => None,
        }
    }

    /// O padrão de um ativo. A migração 0015 faz a mesma conta em SQL para os
    /// ativos que já existiam (e o teste dela confere que dá o mesmo).
    ///
    /// - Renda fixa (inclusive Tesouro e ETF de renda fixa): fora.
    /// - Cripto: cripto; negociado na B3, é ETF (o ETF de bitcoin).
    /// - Ações internacionais e REITs: fora (exterior); na B3, BDR pelo
    ///   código ou nome, senão ETF (o de S&P 500), e ETF de renda fixa fica fora.
    /// - Ações nacionais: BDR pelo código ou nome, ETF pelo nome ("fundo de
    ///   índice", "ETF"; o de renda fixa fica fora), senão ação.
    /// - FIIs: ETF pelo nome (o de IFIX), Fiagro com "agro" no nome, senão FII.
    pub fn default_for(
        class: AssetClass,
        listed_on_b3: bool,
        symbol: Option<&str>,
        name: &str,
    ) -> Self {
        let upper = name.to_uppercase();
        let base = symbol.unwrap_or_default().trim().to_uppercase();
        let bdr = is_bdr(&base, &upper);
        let index_fund = is_index_fund("", &upper, &upper) || has_word(&upper, "ETF");
        let fixed_income_fund = index_fund && tracks_fixed_income(&upper, &upper);
        match class {
            AssetClass::FixedIncome | AssetClass::IntlFixedIncome => TaxInstrument::Unsupported,
            AssetClass::Crypto if listed_on_b3 => TaxInstrument::Etf,
            AssetClass::Crypto => TaxInstrument::Crypto,
            AssetClass::IntlStocks | AssetClass::UsReits if !listed_on_b3 => {
                TaxInstrument::Unsupported
            }
            AssetClass::IntlStocks | AssetClass::UsReits if bdr => TaxInstrument::Bdr,
            AssetClass::BrStocks if bdr => TaxInstrument::Bdr,
            AssetClass::IntlStocks | AssetClass::UsReits | AssetClass::BrStocks
                if fixed_income_fund =>
            {
                TaxInstrument::Unsupported
            }
            AssetClass::IntlStocks | AssetClass::UsReits => TaxInstrument::Etf,
            AssetClass::BrStocks if index_fund => TaxInstrument::Etf,
            AssetClass::BrStocks => TaxInstrument::Stock,
            AssetClass::BrReits if index_fund => TaxInstrument::Etf,
            AssetClass::BrReits if upper.contains("AGRO") => TaxInstrument::AgribusinessFund,
            AssetClass::BrReits => TaxInstrument::RealEstateFund,
        }
    }
}

/// A palavra inteira aparece no texto (já em maiúsculas).
fn has_word(upper: &str, word: &str) -> bool {
    upper
        .split(|c: char| !c.is_alphanumeric())
        .any(|piece| piece == word)
}

// ------------------------------------------------------------ o que ficou fora --

/// Por que algo do livro ou dos proventos ficou fora da conta (ou entrou com
/// uma suposição que a pessoa precisa conferir).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Ativo fora do IR do Prumo (renda fixa, exterior, ETF de renda fixa).
    UnsupportedInstrument,
    /// Saldo inicial sem preço: sem custo não há preço médio nem lucro.
    UnknownOpeningCost,
    /// Resgate: regra de renda fixa, que o Prumo não calcula. O ativo sai.
    Redemption,
    /// Amortização: o IR do Prumo não abate do custo; o lançamento sai, e o
    /// custo (e o lucro de vendas depois) fica maior que o real.
    Amortization,
    /// Ajuste manual de quantidade: entra como no livro (muda a quantidade,
    /// o custo total fica). Se foi compra ou venda, falta o lançamento com preço.
    Adjustment,
    /// Um ajuste manual zerou a posição: se foi venda, o IR não sabe o preço.
    /// O ativo sai.
    AdjustmentClosesPosition,
    /// Desdobramento, grupamento, bonificação ou ajuste sem posição no dia (ou
    /// venda maior que ela, pela ordem do dia). O ativo sai.
    EventWithoutPosition,
    /// O mesmo ticker com dois instrumentos fiscais diferentes. O ticker sai.
    ConflictingInstruments,
    /// Provento de amortização (devolução de capital): não é rendimento.
    IncomeAmortization,
    /// Provento "outro": o Prumo não sabe em que ficha entra.
    IncomeOther,
    /// Dividendo de BDR: rendimento do exterior (Lei 14.754/2023).
    IncomeFromAbroad,
    /// Provento de ETF, cripto ou ativo fora do IR do Prumo.
    IncomeOutOfScope,
}

/// Uma linha de "não calculado".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    /// O ticker (ou o nome, sem ticker).
    pub subject: String,
    /// O lançamento ou o pagamento; `None` quando é o ativo inteiro.
    pub date: Option<Date>,
    pub reason: SkipReason,
    /// O valor envolvido, em reais, quando há um (amortização, provento).
    pub amount: Option<Decimal>,
}

// ---------------------------------------------------------------- o livro --

/// Um lançamento do livro com o que o IR precisa saber do ativo.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerLine {
    /// O ticker (o preço médio da Receita é por ativo) ou, sem ticker, o nome.
    pub key: String,
    pub instrument: TaxInstrument,
    pub entry: Entry,
}

/// O livro do jeito que `domain::tax` recebe.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Book {
    pub openings: Vec<OpeningPosition>,
    pub operations: Vec<Operation>,
    pub skipped: Vec<Skipped>,
}

/// Converte o livro inteiro. Cada ticker é convertido sozinho; o que não dá
/// para converter tira o ticker inteiro da conta (e diz por quê).
pub fn book(lines: &[LedgerLine]) -> Book {
    let mut groups: BTreeMap<&str, Vec<&LedgerLine>> = BTreeMap::new();
    for line in lines {
        groups.entry(line.key.as_str()).or_default().push(line);
    }
    let mut out = Book::default();
    for (key, group) in groups {
        let first = group[0].instrument;
        if group.iter().any(|line| line.instrument != first) {
            out.skipped
                .push(whole(key, SkipReason::ConflictingInstruments));
            continue;
        }
        let mut entries: Vec<Entry> = group.iter().map(|line| line.entry.clone()).collect();
        entries.sort_by(ledger_order);
        let Some(instrument) = first.instrument() else {
            out.skipped
                .push(whole(key, SkipReason::UnsupportedInstrument));
            continue;
        };
        match convert(key, instrument, &entries) {
            Ok(converted) => {
                out.openings.extend(converted.openings);
                out.operations.extend(converted.operations);
                out.skipped.extend(converted.notes);
            }
            Err(skipped) => out.skipped.push(skipped),
        }
    }
    out
}

fn whole(key: &str, reason: SkipReason) -> Skipped {
    Skipped {
        subject: key.to_owned(),
        date: None,
        reason,
        amount: None,
    }
}

#[derive(Default)]
struct Converted {
    openings: Vec<OpeningPosition>,
    operations: Vec<Operation>,
    notes: Vec<Skipped>,
}

fn is_event(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Split | Kind::ReverseSplit | Kind::Bonus | Kind::Adjustment | Kind::Amortization
    )
}

/// Um ticker, na ordem do livro. `Err` tira o ticker da conta.
fn convert(key: &str, instrument: Instrument, entries: &[Entry]) -> Result<Converted, Skipped> {
    if let Some(entry) = entries.iter().find(|e| e.kind == Kind::Redemption) {
        return Err(skipped_at(key, entry, SkipReason::Redemption));
    }
    if let Some(entry) = entries
        .iter()
        .find(|e| e.kind == Kind::Opening && e.unit_price.is_none())
    {
        return Err(skipped_at(key, entry, SkipReason::UnknownOpeningCost));
    }

    let mut conversion = Conversion {
        key,
        instrument,
        held: Decimal::ZERO,
        out: Converted::default(),
    };
    // O saldo inicial que abre o livro é a posição de partida do IR.
    let leading = entries
        .iter()
        .take_while(|e| e.kind == Kind::Opening)
        .count();
    for entry in &entries[..leading] {
        conversion.open(entry);
    }
    for day in entries[leading..].chunk_by(|a, b| a.trade_date == b.trade_date) {
        // O IR aplica os eventos do dia antes dos negócios (positions::replay).
        let trades = conversion.events_of_day(day)?;
        for entry in trades {
            conversion.trade(entry)?;
        }
    }
    Ok(conversion.out)
}

/// O lançamento que tira o ticker da conta (ou vira nota), com a data dele.
fn skipped_at(key: &str, entry: &Entry, reason: SkipReason) -> Skipped {
    Skipped {
        subject: key.to_owned(),
        date: Some(entry.trade_date),
        reason,
        amount: None,
    }
}

/// A conversão de um ticker em andamento: as cotas que o IR vê até aqui e o
/// que já saiu.
struct Conversion<'a> {
    key: &'a str,
    instrument: Instrument,
    held: Decimal,
    out: Converted,
}

impl Conversion<'_> {
    fn skipped(&self, entry: &Entry, reason: SkipReason) -> Skipped {
        skipped_at(self.key, entry, reason)
    }

    fn push(&mut self, entry: &Entry, action: Action) {
        self.out.operations.push(Operation {
            date: entry.trade_date,
            symbol: self.key.to_owned(),
            instrument: self.instrument,
            broker: None,
            action,
        });
    }

    /// Um saldo inicial do começo do livro: posição de partida.
    fn open(&mut self, entry: &Entry) {
        self.held += entry.quantity;
        self.out.openings.push(OpeningPosition {
            symbol: self.key.to_owned(),
            instrument: self.instrument,
            quantity: entry.quantity,
            total_cost: gross(entry) + fees(entry),
        });
    }

    /// Aplica os eventos de um dia e devolve os negócios dele, que o IR aplica
    /// depois dos eventos.
    fn events_of_day<'e>(&mut self, day: &'e [Entry]) -> Result<Vec<&'e Entry>, Skipped> {
        let mut trades: Vec<&Entry> = Vec::new();
        for entry in day {
            if entry.kind == Kind::Opening && self.held > Decimal::ZERO {
                self.opening_mid_book(entry);
            } else if is_event(entry.kind) {
                self.event(entry)?;
            } else {
                // Negócio, ou saldo inicial de quem estava zerado.
                trades.push(entry);
            }
        }
        Ok(trades)
    }

    /// Saldo inicial no meio do livro: cotas que entram com custo e sem
    /// negócio — a conta da bonificação.
    fn opening_mid_book(&mut self, entry: &Entry) {
        let cost = gross(entry) + fees(entry);
        self.held += entry.quantity;
        self.push(
            entry,
            Action::Bonus {
                quantity: entry.quantity,
                unit_cost: cost / entry.quantity,
            },
        );
    }

    fn event(&mut self, entry: &Entry) -> Result<(), Skipped> {
        match entry.kind {
            Kind::Amortization => {
                let note = Skipped {
                    amount: Some(gross(entry)),
                    ..self.skipped(entry, SkipReason::Amortization)
                };
                self.out.notes.push(note);
            }
            Kind::Bonus => {
                if self.held <= Decimal::ZERO {
                    return Err(self.skipped(entry, SkipReason::EventWithoutPosition));
                }
                self.held += entry.quantity;
                self.push(
                    entry,
                    Action::Bonus {
                        quantity: entry.quantity,
                        unit_cost: entry.unit_price.unwrap_or_default() * entry.fx_rate,
                    },
                );
            }
            _ => self.reshape(entry)?,
        }
        Ok(())
    }

    /// Desdobramento, grupamento e ajuste: a quantidade muda e o custo total
    /// fica. O livro diz quantas cotas, o IR quer o fator.
    fn reshape(&mut self, entry: &Entry) -> Result<(), Skipped> {
        let kind = entry.kind;
        let held = self.held;
        let after = held + kind.quantity_delta(entry.quantity);
        if kind == Kind::Adjustment && after.is_zero() && held > Decimal::ZERO {
            return Err(self.skipped(entry, SkipReason::AdjustmentClosesPosition));
        }
        if held <= Decimal::ZERO || after <= Decimal::ZERO {
            return Err(self.skipped(entry, SkipReason::EventWithoutPosition));
        }
        if kind == Kind::Adjustment {
            let note = self.skipped(entry, SkipReason::Adjustment);
            self.out.notes.push(note);
        }
        let factor = after / held;
        self.held = (held * factor).round_dp(QUANTITY_DECIMALS);
        self.push(entry, Action::Split { factor });
        Ok(())
    }

    fn trade(&mut self, entry: &Entry) -> Result<(), Skipped> {
        let action = match entry.kind {
            Kind::Sell => {
                if entry.quantity > self.held {
                    return Err(self.skipped(entry, SkipReason::EventWithoutPosition));
                }
                self.held -= entry.quantity;
                Action::Sell {
                    quantity: entry.quantity,
                    unit_price: price(entry),
                    fees: fees(entry),
                }
            }
            // Compra, ou saldo inicial de quem estava zerado: entra como compra.
            _ => {
                self.held += entry.quantity;
                Action::Buy {
                    quantity: entry.quantity,
                    unit_price: price(entry),
                    fees: fees(entry),
                }
            }
        };
        self.push(entry, action);
        Ok(())
    }
}

fn price(entry: &Entry) -> Decimal {
    entry.unit_price.unwrap_or_default() * entry.fx_rate
}

fn gross(entry: &Entry) -> Decimal {
    entry.quantity * price(entry)
}

fn fees(entry: &Entry) -> Decimal {
    entry.fees * entry.fx_rate
}

// ------------------------------------------------------------- proventos --

/// Um provento recebido, com o instrumento do ativo que pagou (`None` se o
/// ativo já saiu da carteira) e a fonte pagadora que a CVM dá para ele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payment {
    pub payer: String,
    pub kind: PaymentKind,
    pub instrument: Option<TaxInstrument>,
    pub pay_date: Date,
    pub gross: Decimal,
    pub withheld: Decimal,
    /// CNPJ e nome da fonte pagadora (cadastro da CVM); `None` sem cadastro.
    pub source: Option<Payer>,
}

/// De que cadastro da CVM sai o CNPJ de quem pagou o provento: companhia
/// (dividendo e JCP) ou fundo (rendimento de FII e Fiagro). `None` para o que
/// não vai à declaração como rendimento.
pub fn payer_registry(payment: &Payment) -> Option<PayerRegistry> {
    let fund = matches!(
        payment.instrument,
        Some(TaxInstrument::RealEstateFund | TaxInstrument::AgribusinessFund)
    );
    let company = matches!(payment.instrument, None | Some(TaxInstrument::Stock));
    match payment.kind {
        PaymentKind::Dividend | PaymentKind::ReitIncome if fund => Some(PayerRegistry::Fund),
        PaymentKind::ReitIncome if payment.instrument.is_none() => Some(PayerRegistry::Fund),
        PaymentKind::Dividend | PaymentKind::Jcp if company => Some(PayerRegistry::Company),
        _ => None,
    }
}

/// O cadastro da CVM de uma fonte pagadora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayerRegistry {
    /// Companhia aberta (cadastro de companhias).
    Company,
    /// FII ou Fiagro (informe mensal).
    Fund,
}

/// Os proventos do jeito que o IRPF recebe, e os que ficam fora.
pub fn incomes(payments: &[Payment]) -> (Vec<Income>, Vec<Skipped>) {
    let mut kept = Vec::new();
    let mut skipped = Vec::new();
    for payment in payments {
        let fund = matches!(
            payment.instrument,
            Some(TaxInstrument::RealEstateFund | TaxInstrument::AgribusinessFund)
        );
        let company = matches!(payment.instrument, None | Some(TaxInstrument::Stock));
        let kind = match (payment.kind, payment.instrument) {
            (PaymentKind::Amortization, _) => Err(SkipReason::IncomeAmortization),
            (PaymentKind::Other, _) => Err(SkipReason::IncomeOther),
            (_, Some(TaxInstrument::Bdr)) => Err(SkipReason::IncomeFromAbroad),
            (PaymentKind::Dividend | PaymentKind::ReitIncome, _) if fund => {
                Ok(IncomeKind::FundIncome)
            }
            (PaymentKind::ReitIncome, None) => Ok(IncomeKind::FundIncome),
            (PaymentKind::Dividend, _) if company => Ok(IncomeKind::Dividend),
            (PaymentKind::Jcp, _) if company => Ok(IncomeKind::InterestOnEquity),
            _ => Err(SkipReason::IncomeOutOfScope),
        };
        match kind {
            Ok(kind) => kept.push(Income {
                pay_date: payment.pay_date,
                symbol: payment.payer.clone(),
                kind,
                gross: payment.gross,
                withheld: payment.withheld,
                payer: payment.source.clone(),
            }),
            Err(reason) => skipped.push(Skipped {
                subject: payment.payer.clone(),
                date: Some(payment.pay_date),
                reason,
                amount: Some(payment.gross),
            }),
        }
    }
    (kept, skipped)
}

// ----------------------------------------------- pontos a confirmar (§ 5) --

/// As dúvidas de `docs/ir.md` § 5 que mexem num número da tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Doubt {
    /// § 5.1: as vendas em day trade somam no limite de R$ 20 mil das ações.
    DayTradeInStockExemption,
    /// § 5.2: prejuízo de Fiagro só compensa com Fiagro (separado do FII).
    AgribusinessSeparateFromRealEstate,
    /// § 5.3: códigos de Bens e Direitos e de rendimentos.
    IrpfCodes,
    /// § 5.4: cripto apurada por alienação; prejuízo não compensa lucro.
    CryptoPerSale,
    /// § 5.5: 31/12 tratado como dia sem expediente.
    YearEnd,
    /// § 5.6: onde informar a sobra de IR retido.
    WithholdingLeftover,
    /// § 5.7: dividendo com retenção de 10% a partir de 2026.
    DividendWithholding,
    /// § 5.8: JCP pelo valor líquido.
    InterestOnEquityNet,
}

/// As dúvidas que mexem num número do mês.
pub fn month_doubts(report: &MonthReport) -> Vec<Doubt> {
    let mut doubts = Vec::new();
    if report
        .sales
        .iter()
        .any(|s| s.instrument == Instrument::Stock && s.day_trade_sales > Decimal::ZERO)
        && report
            .exemptions
            .iter()
            .any(|e| e.id == ExemptionId::StockSales)
    {
        doubts.push(Doubt::DayTradeInStockExemption);
    }
    if report.pools.iter().any(|p| {
        p.pool == Pool::AgribusinessFund
            && (!p.result.is_zero() || !p.loss_before.is_zero() || !p.loss_after.is_zero())
    }) {
        doubts.push(Doubt::AgribusinessSeparateFromRealEstate);
    }
    let capital = &report.capital_gain;
    if capital.gains > Decimal::ZERO && capital.losses > Decimal::ZERO {
        doubts.push(Doubt::CryptoPerSale);
    }
    if report.period.month == 11 && report.darfs.iter().any(|d| d.payable) {
        doubts.push(Doubt::YearEnd);
    }
    doubts
}

/// As dúvidas que mexem num número da declaração do ano.
pub fn annual_doubts(report: &AnnualReport) -> Vec<Doubt> {
    let mut doubts = vec![Doubt::IrpfCodes];
    if report.withholding_left > Decimal::ZERO {
        doubts.push(Doubt::WithholdingLeftover);
    }
    let dividends_withheld = report.exempt_income.iter().any(|line| {
        line.code.code == "09"
            && (line.withheld > Decimal::ZERO
                || line.by_payer.iter().any(|p| !p.above_threshold.is_empty()))
    });
    if dividends_withheld {
        doubts.push(Doubt::DividendWithholding);
    }
    if report
        .exclusive_income
        .iter()
        .any(|line| line.gross > Decimal::ZERO)
    {
        doubts.push(Doubt::InterestOnEquityNet);
    }
    doubts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tax::monthly::{assess, Carryforward, TaxInput};
    use crate::domain::tax::positions;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn entry(kind: Kind, day: Date, seq: i64, quantity: Decimal, price: Option<Decimal>) -> Entry {
        Entry {
            kind,
            trade_date: day,
            seq,
            quantity,
            unit_price: price,
            fees: Decimal::ZERO,
            fx_rate: Decimal::ONE,
        }
    }

    fn line(key: &str, instrument: TaxInstrument, entry: Entry) -> LedgerLine {
        LedgerLine {
            key: key.to_owned(),
            instrument,
            entry,
        }
    }

    #[test]
    fn instrument_names_round_trip() {
        for instrument in TaxInstrument::ALL {
            assert_eq!(TaxInstrument::parse(instrument.as_str()), Some(instrument));
        }
        assert_eq!(TaxInstrument::parse("option"), None);
        assert_eq!(TaxInstrument::Unsupported.instrument(), None);
        assert_eq!(
            TaxInstrument::AgribusinessFund.instrument(),
            Some(Instrument::AgribusinessFund)
        );
    }

    #[test]
    fn default_instrument_reads_class_market_and_name() {
        use AssetClass::*;
        use TaxInstrument as T;
        let d = |class, listed, symbol: &str, name: &str| {
            TaxInstrument::default_for(class, listed, Some(symbol), name)
        };
        assert_eq!(d(BrStocks, true, "WEGE3", "WEG S.A."), T::Stock);
        assert_eq!(d(BrStocks, true, "TAEE11", "Taesa UNT N2"), T::Stock);
        assert_eq!(
            d(BrStocks, true, "BOVA11", "iShares Ibovespa Fundo de Índice"),
            T::Etf
        );
        assert_eq!(d(BrStocks, true, "XPTO11", "Xpto Small Caps ETF"), T::Etf);
        assert_eq!(d(BrStocks, true, "AAPL34", "Apple"), T::Bdr);
        assert_eq!(
            d(BrStocks, true, "IMAB11", "It Now IMA-B Fundo de Índice"),
            T::Unsupported
        );
        assert_eq!(
            d(BrReits, true, "KNRI11", "Kinea Renda Imobiliária"),
            T::RealEstateFund
        );
        assert_eq!(
            d(BrReits, true, "AGRX11", "Fiagro Exemplo Agro"),
            T::AgribusinessFund
        );
        assert_eq!(
            d(BrReits, true, "XFIX11", "Trend IFIX Fundo de Índice"),
            T::Etf
        );
        // ETF de bitcoin da B3 é ETF no IR; o bitcoin é cripto.
        assert_eq!(d(Crypto, true, "QBTC11", "QR Bitcoin"), T::Etf);
        assert_eq!(d(Crypto, false, "BTC", "Bitcoin"), T::Crypto);
        // Exterior: fora, salvo BDR e ETF da B3.
        assert_eq!(d(IntlStocks, false, "AAPL", "Apple Inc."), T::Unsupported);
        assert_eq!(d(UsReits, false, "O", "Realty Income"), T::Unsupported);
        assert_eq!(d(IntlStocks, true, "AAPL34", "Apple DRN"), T::Bdr);
        assert_eq!(
            d(
                IntlStocks,
                true,
                "IVVB11",
                "iShares S&P 500 Fundo de Índice"
            ),
            T::Etf
        );
        assert_eq!(d(UsReits, true, "REIT11", "Realty BDR DR3"), T::Bdr);
        assert_eq!(
            d(
                IntlStocks,
                true,
                "B5P211",
                "It Now Tesouro IPCA Fundo de Índice"
            ),
            T::Unsupported
        );
        // Renda fixa, inclusive o ETF de IMA-B cadastrado em Renda Fixa e o Tesouro.
        assert_eq!(
            d(FixedIncome, true, "IMAB11", "It Now IMA-B"),
            T::Unsupported
        );
        assert_eq!(
            d(FixedIncome, false, "TD-IPCA-20350515", "Tesouro IPCA+ 2035"),
            T::Unsupported
        );
        assert_eq!(
            TaxInstrument::default_for(IntlFixedIncome, false, None, "Treasury"),
            T::Unsupported
        );
        // "ETF" só como palavra.
        assert_eq!(d(BrStocks, true, "PETF3", "Petfood S.A."), T::Stock);
    }

    #[test]
    fn plain_book_becomes_operations_in_reais() {
        let usd = Entry {
            fx_rate: dec!(5),
            fees: dec!(1),
            ..entry(
                Kind::Buy,
                date!(2025 - 02 - 03),
                2,
                dec!(0.5),
                Some(dec!(60000)),
            )
        };
        let lines = [
            line(
                "WEGE3",
                TaxInstrument::Stock,
                entry(
                    Kind::Opening,
                    date!(2024 - 12 - 01),
                    1,
                    dec!(100),
                    Some(dec!(30)),
                ),
            ),
            line(
                "WEGE3",
                TaxInstrument::Stock,
                entry(
                    Kind::Sell,
                    date!(2025 - 03 - 03),
                    3,
                    dec!(40),
                    Some(dec!(45)),
                ),
            ),
            line("BTC", TaxInstrument::Crypto, usd),
        ];
        let book = book(&lines);
        assert!(book.skipped.is_empty());
        assert_eq!(
            book.openings,
            vec![OpeningPosition {
                symbol: "WEGE3".into(),
                instrument: Instrument::Stock,
                quantity: dec!(100),
                total_cost: dec!(3000),
            }]
        );
        assert_eq!(book.operations.len(), 2);
        let btc = book.operations.iter().find(|o| o.symbol == "BTC").unwrap();
        assert_eq!(
            btc.action,
            Action::Buy {
                quantity: dec!(0.5),
                unit_price: dec!(300000),
                fees: dec!(5),
            }
        );
        let replay =
            positions::replay(&book.openings, &book.operations, date!(2025 - 12 - 31)).unwrap();
        assert_eq!(replay.realized[0].cost, dec!(1200));
        assert_eq!(replay.holdings["BTC"].total_cost, dec!(150005));
    }

    #[test]
    fn splits_and_reverse_splits_become_factors() {
        let d = date!(2025 - 01 - 10);
        let lines = [
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Buy, d, 1, dec!(99), Some(dec!(10))),
            ),
            // A B3 diz "recebeu 198 cotas" (desdobramento de 1 para 3).
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Split, date!(2025 - 02 - 10), 2, dec!(198), None),
            ),
            // E "perdeu 198" no grupamento de volta (3 para 1).
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::ReverseSplit,
                    date!(2025 - 03 - 10),
                    3,
                    dec!(198),
                    None,
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Sell,
                    date!(2025 - 04 - 10),
                    4,
                    dec!(99),
                    Some(dec!(12)),
                ),
            ),
        ];
        let book = book(&lines);
        assert!(book.skipped.is_empty(), "{:?}", book.skipped);
        assert_eq!(book.operations[1].action, Action::Split { factor: dec!(3) });
        let Action::Split { factor } = book.operations[2].action else {
            panic!("grupamento vira fator");
        };
        assert_eq!(factor, dec!(99) / dec!(297));
        let replay = positions::replay(&[], &book.operations, date!(2025 - 12 - 31)).unwrap();
        assert!(replay.holdings.is_empty());
        assert_eq!(replay.realized[0].cost, dec!(990));
    }

    #[test]
    fn split_on_a_trading_day_uses_the_position_before_the_trades() {
        // No livro: compra de 10 e, depois, desdobramento que dá +30 sobre as
        // 30 que havia (10 + 20). O IR aplica o evento antes da compra: o
        // fator sai sobre 20, e a quantidade no fim do dia fecha com o livro.
        let lines = [
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Buy,
                    date!(2025 - 01 - 02),
                    1,
                    dec!(20),
                    Some(dec!(10)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Buy,
                    date!(2025 - 02 - 03),
                    2,
                    dec!(10),
                    Some(dec!(10)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Split, date!(2025 - 02 - 03), 3, dec!(30), None),
            ),
        ];
        let book = book(&lines);
        let replay = positions::replay(&[], &book.operations, date!(2025 - 12 - 31)).unwrap();
        assert_eq!(replay.holdings["AAAA3"].quantity, dec!(60));
        assert_eq!(replay.holdings["AAAA3"].total_cost, dec!(300));
    }

    #[test]
    fn bonus_and_mid_book_opening_enter_with_cost() {
        let lines = [
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Buy,
                    date!(2025 - 01 - 02),
                    1,
                    dec!(100),
                    Some(dec!(20)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Bonus,
                    date!(2025 - 03 - 03),
                    2,
                    dec!(10),
                    Some(dec!(5)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Bonus, date!(2025 - 04 - 01), 3, dec!(10), None),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                Entry {
                    fees: dec!(2),
                    ..entry(
                        Kind::Opening,
                        date!(2025 - 05 - 02),
                        4,
                        dec!(20),
                        Some(dec!(24)),
                    )
                },
            ),
        ];
        let book = book(&lines);
        assert!(book.openings.is_empty());
        let replay = positions::replay(&[], &book.operations, date!(2025 - 12 - 31)).unwrap();
        let holding = &replay.holdings["AAAA3"];
        assert_eq!(holding.quantity, dec!(140));
        assert_eq!(holding.total_cost, dec!(2000) + dec!(50) + dec!(482));
    }

    #[test]
    fn opening_after_the_position_closed_is_a_purchase() {
        let lines = [
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Buy,
                    date!(2025 - 01 - 02),
                    1,
                    dec!(10),
                    Some(dec!(20)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Sell,
                    date!(2025 - 02 - 03),
                    2,
                    dec!(10),
                    Some(dec!(25)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Opening,
                    date!(2025 - 03 - 03),
                    3,
                    dec!(5),
                    Some(dec!(30)),
                ),
            ),
        ];
        let book = book(&lines);
        assert!(matches!(book.operations[2].action, Action::Buy { .. }));
    }

    #[test]
    fn what_the_tax_cannot_follow_is_listed() {
        let d = date!(2025 - 01 - 02);
        let lines = [
            // Renda fixa: fora.
            line(
                "CDB",
                TaxInstrument::Unsupported,
                entry(Kind::Buy, d, 1, dec!(1000), Some(dec!(1))),
            ),
            // Saldo inicial sem custo: fora.
            line(
                "XXXX3",
                TaxInstrument::Stock,
                entry(Kind::Opening, d, 2, dec!(5), None),
            ),
            // Resgate: fora.
            line(
                "YYYY3",
                TaxInstrument::Stock,
                entry(Kind::Buy, d, 3, dec!(5), Some(dec!(1))),
            ),
            line(
                "YYYY3",
                TaxInstrument::Stock,
                entry(
                    Kind::Redemption,
                    date!(2025 - 02 - 03),
                    4,
                    dec!(5),
                    Some(dec!(2)),
                ),
            ),
            // Amortização: o lançamento sai, o ativo fica.
            line(
                "KNRI11",
                TaxInstrument::RealEstateFund,
                entry(Kind::Buy, d, 5, dec!(10), Some(dec!(100))),
            ),
            line(
                "KNRI11",
                TaxInstrument::RealEstateFund,
                entry(
                    Kind::Amortization,
                    date!(2025 - 02 - 03),
                    6,
                    dec!(10),
                    Some(dec!(2)),
                ),
            ),
            // Ajuste que reduz: entra como no livro, com aviso.
            line(
                "KNRI11",
                TaxInstrument::RealEstateFund,
                entry(Kind::Adjustment, date!(2025 - 03 - 03), 7, dec!(-4), None),
            ),
            // Ajuste que zera: fora.
            line(
                "ZZZZ3",
                TaxInstrument::Stock,
                entry(Kind::Buy, d, 8, dec!(5), Some(dec!(1))),
            ),
            line(
                "ZZZZ3",
                TaxInstrument::Stock,
                entry(Kind::Adjustment, date!(2025 - 03 - 03), 9, dec!(-5), None),
            ),
            // Ajuste sem posição: fora.
            line(
                "WWWW3",
                TaxInstrument::Stock,
                entry(Kind::Adjustment, d, 10, dec!(5), None),
            ),
            // Bonificação sem posição: fora.
            line(
                "VVVV3",
                TaxInstrument::Stock,
                entry(Kind::Bonus, d, 11, dec!(5), None),
            ),
            // Mesmo ticker, dois instrumentos: fora.
            line(
                "DUPL11",
                TaxInstrument::Etf,
                entry(Kind::Buy, d, 12, dec!(1), Some(dec!(1))),
            ),
            line(
                "DUPL11",
                TaxInstrument::RealEstateFund,
                entry(Kind::Buy, d, 13, dec!(1), Some(dec!(1))),
            ),
        ];
        let book = book(&lines);
        let reasons: Vec<(&str, SkipReason)> = book
            .skipped
            .iter()
            .map(|s| (s.subject.as_str(), s.reason))
            .collect();
        assert_eq!(
            reasons,
            vec![
                ("CDB", SkipReason::UnsupportedInstrument),
                ("DUPL11", SkipReason::ConflictingInstruments),
                ("KNRI11", SkipReason::Amortization),
                ("KNRI11", SkipReason::Adjustment),
                ("VVVV3", SkipReason::EventWithoutPosition),
                ("WWWW3", SkipReason::EventWithoutPosition),
                ("XXXX3", SkipReason::UnknownOpeningCost),
                ("YYYY3", SkipReason::Redemption),
                ("ZZZZ3", SkipReason::AdjustmentClosesPosition),
            ]
        );
        assert_eq!(book.skipped[2].amount, Some(dec!(20)));
        // Só o KNRI11 sobra, com 6 cotas e o custo inteiro (a amortização não abate).
        let replay =
            positions::replay(&book.openings, &book.operations, date!(2025 - 12 - 31)).unwrap();
        assert_eq!(replay.holdings.len(), 1);
        assert_eq!(replay.holdings["KNRI11"].quantity, dec!(6));
        assert_eq!(replay.holdings["KNRI11"].total_cost, dec!(1000));
    }

    #[test]
    fn sale_beyond_the_tax_position_takes_the_ticker_out() {
        // No livro: vende as 10, recompra 10 e grupa de 2 para 1 no mesmo dia.
        // O IR aplica o grupamento antes dos negócios e a venda de 10 não
        // cabe nas 5 que sobram.
        let d = date!(2025 - 02 - 03);
        let lines = [
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(
                    Kind::Buy,
                    date!(2025 - 01 - 02),
                    1,
                    dec!(10),
                    Some(dec!(10)),
                ),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Sell, d, 2, dec!(10), Some(dec!(10))),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Buy, d, 3, dec!(10), Some(dec!(10))),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::ReverseSplit, d, 4, dec!(5), None),
            ),
        ];
        let book = book(&lines);
        assert!(book.operations.is_empty());
        assert_eq!(book.skipped[0].reason, SkipReason::EventWithoutPosition);
        assert_eq!(book.skipped[0].date, Some(d));
    }

    #[test]
    fn payments_land_in_their_irpf_lines() {
        let payment = |kind, instrument| Payment {
            payer: "X".into(),
            kind,
            instrument,
            pay_date: date!(2025 - 05 - 02),
            gross: dec!(100),
            withheld: Decimal::ZERO,
            source: None,
        };
        let cases = [
            (
                payment(PaymentKind::Dividend, Some(TaxInstrument::Stock)),
                Ok(IncomeKind::Dividend),
            ),
            (
                payment(PaymentKind::Dividend, None),
                Ok(IncomeKind::Dividend),
            ),
            (
                payment(PaymentKind::Jcp, Some(TaxInstrument::Stock)),
                Ok(IncomeKind::InterestOnEquity),
            ),
            (
                payment(PaymentKind::ReitIncome, Some(TaxInstrument::RealEstateFund)),
                Ok(IncomeKind::FundIncome),
            ),
            (
                payment(PaymentKind::Dividend, Some(TaxInstrument::AgribusinessFund)),
                Ok(IncomeKind::FundIncome),
            ),
            (
                payment(PaymentKind::ReitIncome, None),
                Ok(IncomeKind::FundIncome),
            ),
            (
                payment(PaymentKind::Dividend, Some(TaxInstrument::Bdr)),
                Err(SkipReason::IncomeFromAbroad),
            ),
            (
                payment(PaymentKind::Dividend, Some(TaxInstrument::Etf)),
                Err(SkipReason::IncomeOutOfScope),
            ),
            (
                payment(PaymentKind::Jcp, Some(TaxInstrument::RealEstateFund)),
                Err(SkipReason::IncomeOutOfScope),
            ),
            (
                payment(PaymentKind::ReitIncome, Some(TaxInstrument::Stock)),
                Err(SkipReason::IncomeOutOfScope),
            ),
            (
                payment(
                    PaymentKind::Amortization,
                    Some(TaxInstrument::RealEstateFund),
                ),
                Err(SkipReason::IncomeAmortization),
            ),
            (
                payment(PaymentKind::Other, Some(TaxInstrument::Stock)),
                Err(SkipReason::IncomeOther),
            ),
        ];
        for (payment, expected) in cases {
            let (kept, skipped) = incomes(std::slice::from_ref(&payment));
            let got = match (kept.first(), skipped.first()) {
                (Some(income), None) => Ok(income.kind),
                (None, Some(skip)) => {
                    assert_eq!(skip.amount, Some(dec!(100)));
                    Err(skip.reason)
                }
                other => panic!("um ou outro: {other:?}"),
            };
            assert_eq!(got, expected, "{payment:?}");
        }
    }

    #[test]
    fn payer_registry_follows_the_irpf_line() {
        let payment = |kind, instrument| Payment {
            payer: "X".into(),
            kind,
            instrument,
            pay_date: date!(2025 - 05 - 02),
            gross: dec!(100),
            withheld: Decimal::ZERO,
            source: None,
        };
        use PayerRegistry::{Company, Fund};
        let cases = [
            (
                PaymentKind::Dividend,
                Some(TaxInstrument::Stock),
                Some(Company),
            ),
            (PaymentKind::Jcp, None, Some(Company)),
            (
                PaymentKind::ReitIncome,
                Some(TaxInstrument::RealEstateFund),
                Some(Fund),
            ),
            (
                PaymentKind::Dividend,
                Some(TaxInstrument::AgribusinessFund),
                Some(Fund),
            ),
            (PaymentKind::ReitIncome, None, Some(Fund)),
            (PaymentKind::Dividend, Some(TaxInstrument::Bdr), None),
            (PaymentKind::Dividend, Some(TaxInstrument::Etf), None),
            (PaymentKind::Amortization, Some(TaxInstrument::Stock), None),
        ];
        for (kind, instrument, expected) in cases {
            assert_eq!(
                payer_registry(&payment(kind, instrument)),
                expected,
                "{kind:?} {instrument:?}"
            );
        }
        // O CNPJ vai junto para o rendimento da declaração.
        let with_source = Payment {
            source: Some(Payer {
                cnpj: "00.000.000/0001-00".into(),
                name: "EMPRESA EXEMPLO S.A.".into(),
                source: crate::domain::tax::irpf::PayerSource::CvmCompany,
            }),
            ..payment(PaymentKind::Dividend, Some(TaxInstrument::Stock))
        };
        let (kept, _) = incomes(&[with_source]);
        assert_eq!(
            kept[0].payer.as_ref().map(|p| p.name.as_str()),
            Some("EMPRESA EXEMPLO S.A.")
        );
    }

    #[test]
    fn doubts_flag_the_numbers_they_touch() {
        let day = |m, d| {
            time::Date::from_calendar_date(2025, time::Month::try_from(m).unwrap(), d).unwrap()
        };
        let lines = [
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Buy, day(3, 3), 1, dec!(100), Some(dec!(100))),
            ),
            // Day trade em março: soma no limite.
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Buy, day(3, 10), 2, dec!(10), Some(dec!(100))),
            ),
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Sell, day(3, 10), 3, dec!(10), Some(dec!(101))),
            ),
            // Fiagro com prejuízo em abril.
            line(
                "AGRX11",
                TaxInstrument::AgribusinessFund,
                entry(Kind::Buy, day(4, 1), 4, dec!(10), Some(dec!(10))),
            ),
            line(
                "AGRX11",
                TaxInstrument::AgribusinessFund,
                entry(Kind::Sell, day(4, 15), 5, dec!(10), Some(dec!(9))),
            ),
            // Cripto com lucro e prejuízo em maio, acima de R$ 35 mil.
            line(
                "BTC",
                TaxInstrument::Crypto,
                entry(Kind::Buy, day(5, 2), 6, dec!(1), Some(dec!(40000))),
            ),
            line(
                "ETH",
                TaxInstrument::Crypto,
                entry(Kind::Buy, day(5, 2), 7, dec!(10), Some(dec!(1000))),
            ),
            line(
                "BTC",
                TaxInstrument::Crypto,
                entry(Kind::Sell, day(5, 20), 8, dec!(1), Some(dec!(45000))),
            ),
            line(
                "ETH",
                TaxInstrument::Crypto,
                entry(Kind::Sell, day(5, 20), 9, dec!(10), Some(dec!(900))),
            ),
            // Lucro tributável em novembro: DARF que vence perto de 31/12.
            line(
                "AAAA3",
                TaxInstrument::Stock,
                entry(Kind::Sell, day(11, 10), 10, dec!(100), Some(dec!(300))),
            ),
        ];
        let book = book(&lines);
        let input = TaxInput {
            openings: &book.openings,
            operations: &book.operations,
            since_year: 2025,
            carryforward: Carryforward::default(),
            informed_withholding: Default::default(),
        };
        let assessment = assess(&input, 2025).unwrap();
        let doubts = |month: usize| month_doubts(&assessment.months[month - 1]);
        assert_eq!(doubts(1), vec![]);
        assert_eq!(doubts(3), vec![Doubt::DayTradeInStockExemption]);
        assert_eq!(doubts(4), vec![Doubt::AgribusinessSeparateFromRealEstate]);
        assert!(doubts(5).contains(&Doubt::CryptoPerSale));
        assert!(doubts(11).contains(&Doubt::YearEnd));
    }

    #[test]
    fn annual_doubts_follow_the_lines() {
        use crate::domain::tax::irpf::annual_report;
        let input = TaxInput {
            openings: &[],
            operations: &[],
            since_year: 2025,
            carryforward: Carryforward::default(),
            informed_withholding: Default::default(),
        };
        let quiet = annual_report(&input, &[], 2026).unwrap();
        assert_eq!(annual_doubts(&quiet), vec![Doubt::IrpfCodes]);
        let incomes = [
            Income {
                pay_date: date!(2026 - 03 - 02),
                symbol: "AAAA3".into(),
                kind: IncomeKind::Dividend,
                gross: dec!(60000),
                withheld: dec!(6000),
                payer: None,
            },
            Income {
                pay_date: date!(2026 - 03 - 02),
                symbol: "AAAA3".into(),
                kind: IncomeKind::InterestOnEquity,
                gross: dec!(100),
                withheld: dec!(17.5),
                payer: None,
            },
        ];
        let busy = annual_report(&input, &incomes, 2026).unwrap();
        assert_eq!(
            annual_doubts(&busy),
            vec![
                Doubt::IrpfCodes,
                Doubt::DividendWithholding,
                Doubt::InterestOnEquityNet
            ]
        );
    }
}
