//! O livro de lançamentos: a posição de um ativo — quantidade, custo, preço
//! médio e lucro realizado — a partir da lista de lançamentos.
//!
//! A regra é a da Receita para renda variável: **custo médio ponderado**, com
//! as taxas da compra somadas ao custo. A venda tira da posição o custo médio
//! do que saiu e **não muda o preço médio**; o lucro realizado é o que entrou
//! menos esse custo (e menos as taxas da venda). Desdobramento e grupamento
//! mudam a quantidade e **preservam o custo total**; bonificação soma ao custo
//! o valor atribuído pela empresa (zero se ela não atribuiu).
//!
//! Tudo em reais: o preço guarda a moeda original e o câmbio do dia
//! (`fx_rate`, reais por unidade da moeda), e o custo entra convertido. Assim
//! o preço médio de um ativo em dólar é o que a pessoa pagou em reais, com o
//! câmbio de cada compra.
//!
//! A ordem é a data do negócio e, no mesmo dia, a ordem em que os lançamentos
//! entraram no livro (`seq`). Qualquer ordem de entrada dá a mesma posição.
//!
//! Custo **desconhecido** (`None`) é diferente de custo zero: é o saldo
//! inicial que a pessoa ainda não disse quanto custou. Ele contamina o custo
//! até a posição zerar — sem saber quanto custou parte das cotas, não há preço
//! médio honesto — e o lucro de qualquer venda feita nesse meio-tempo. Quando
//! a pessoa informa o preço do saldo inicial, o livro é refeito do começo e
//! tudo fica conhecido de novo.

use std::cmp::Ordering;

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use time::Date;

use super::asset_class::UnitKind;

/// O tipo do lançamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Saldo inicial: a quantidade que já existia antes do livro. Preço
    /// opcional — sem ele, o custo fica desconhecido.
    Opening,
    /// Compra. Preço obrigatório; taxas somam ao custo.
    Buy,
    /// Venda. Não passa da posição; não muda o preço médio.
    Sell,
    /// Desdobramento: `quantity` são as cotas novas recebidas. Custo igual.
    Split,
    /// Grupamento: `quantity` são as cotas que deixam de existir. Custo igual;
    /// a posição não pode zerar (sobra de grupamento sai em leilão, que é venda).
    ReverseSplit,
    /// Bonificação: cotas novas; o preço, opcional, é o custo atribuído por
    /// cota (sem ele, custo zero).
    Bonus,
    /// Amortização: devolução de capital. `quantity` são as cotas que
    /// receberam e o preço, o valor por cota. A quantidade não muda; o custo
    /// cai, e o que passar do custo é lucro realizado.
    Amortization,
    /// Resgate (renda fixa, Tesouro): a mesma conta da venda.
    Redemption,
    /// Ajuste manual de quantidade, com sinal: corrige a quantidade sem mexer
    /// no custo. Na renda fixa por valor é a atualização do saldo (rendimento
    /// ou marcação a mercado). É o que a edição da quantidade na folha do
    /// ativo grava.
    Adjustment,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Opening,
        Kind::Buy,
        Kind::Sell,
        Kind::Split,
        Kind::ReverseSplit,
        Kind::Bonus,
        Kind::Amortization,
        Kind::Redemption,
        Kind::Adjustment,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Opening => "opening",
            Kind::Buy => "buy",
            Kind::Sell => "sell",
            Kind::Split => "split",
            Kind::ReverseSplit => "reverse_split",
            Kind::Bonus => "bonus",
            Kind::Amortization => "amortization",
            Kind::Redemption => "redemption",
            Kind::Adjustment => "adjustment",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|k| k.as_str() == value)
    }

    /// O preço é obrigatório, opcional ou proibido.
    pub fn price_rule(self) -> PriceRule {
        match self {
            Kind::Buy | Kind::Sell | Kind::Redemption | Kind::Amortization => PriceRule::Required,
            Kind::Opening | Kind::Bonus => PriceRule::Optional,
            Kind::Split | Kind::ReverseSplit | Kind::Adjustment => PriceRule::Forbidden,
        }
    }

    /// Aceita taxas (corretagem, emolumentos).
    pub fn takes_fees(self) -> bool {
        matches!(
            self,
            Kind::Opening | Kind::Buy | Kind::Sell | Kind::Redemption
        )
    }

    /// Só o ajuste tem quantidade com sinal; o resto é sempre positivo.
    pub fn signed(self) -> bool {
        self == Kind::Adjustment
    }

    /// Faz sentido em renda fixa por valor, onde a "quantidade" é o saldo em
    /// reais: aplicação, resgate, saldo inicial e atualização do saldo.
    pub fn applies_to_value(self) -> bool {
        matches!(
            self,
            Kind::Opening | Kind::Buy | Kind::Redemption | Kind::Adjustment
        )
    }

    /// Quanto o lançamento muda a quantidade (amortização não muda).
    pub fn quantity_delta(self, quantity: Decimal) -> Decimal {
        match self {
            Kind::Opening | Kind::Buy | Kind::Split | Kind::Bonus | Kind::Adjustment => quantity,
            Kind::Sell | Kind::Redemption | Kind::ReverseSplit => -quantity,
            Kind::Amortization => Decimal::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceRule {
    Required,
    Optional,
    Forbidden,
}

/// Um lançamento, como o livro o vê.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    pub trade_date: Date,
    /// Ordem de entrada no livro; desempata o mesmo dia.
    pub seq: i64,
    pub quantity: Decimal,
    /// Na moeda original. `None` só onde o tipo deixa.
    pub unit_price: Option<Decimal>,
    /// Na mesma moeda do preço.
    pub fees: Decimal,
    /// Reais por unidade da moeda do preço; `1` em reais.
    pub fx_rate: Decimal,
}

/// A posição depois de um trecho do livro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub quantity: Decimal,
    /// Custo total em reais. `None`: parte da quantidade tem custo desconhecido.
    /// Com quantidade zero é sempre `Some(0)`.
    pub cost_basis: Option<Decimal>,
    /// Lucro realizado acumulado, em reais. `None`: alguma venda ou
    /// amortização saiu de custo desconhecido.
    pub realized_gain: Option<Decimal>,
}

impl Position {
    /// Nada ainda: zero cotas, custo zero, lucro zero.
    pub const EMPTY: Position = Position {
        quantity: Decimal::ZERO,
        cost_basis: Some(Decimal::ZERO),
        realized_gain: Some(Decimal::ZERO),
    };

    /// Custo por unidade, em reais. `None` sem posição ou com custo desconhecido.
    pub fn average_price(&self) -> Option<Decimal> {
        if self.quantity <= Decimal::ZERO {
            return None;
        }
        self.cost_basis.and_then(|c| c.checked_div(self.quantity))
    }
}

impl Default for Position {
    fn default() -> Self {
        Position::EMPTY
    }
}

/// Por que o livro recusou um lançamento.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    /// Venda, resgate, grupamento, amortização ou ajuste que passa do que
    /// havia naquela data.
    #[error("{kind} em {date}: {quantity} passa da posicao de {held} naquela data")]
    ExceedsPosition {
        date: Date,
        kind: &'static str,
        quantity: Decimal,
        held: Decimal,
    },
    #[error("lancamento invalido em {date}: {reason}")]
    Invalid { date: Date, reason: &'static str },
    #[error("numero grande demais em {date}")]
    Overflow { date: Date },
}

/// Um passo do livro: o lançamento, a posição depois dele e o lucro que ele
/// realizou (zero nos que não vendem; `None` quando o custo era desconhecido).
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub entry: Entry,
    pub position: Position,
    pub realized: Option<Decimal>,
}

/// A ordem do livro: data do negócio e, no mesmo dia, ordem de entrada.
pub fn ledger_order(a: &Entry, b: &Entry) -> Ordering {
    (a.trade_date, a.seq).cmp(&(b.trade_date, b.seq))
}

/// Ordena no lugar, na ordem do livro.
pub fn sort(entries: &mut [Entry]) {
    entries.sort_by(ledger_order);
}

fn mul(a: Decimal, b: Decimal, date: Date) -> Result<Decimal, LedgerError> {
    a.checked_mul(b).ok_or(LedgerError::Overflow { date })
}

fn add(a: Decimal, b: Decimal, date: Date) -> Result<Decimal, LedgerError> {
    a.checked_add(b).ok_or(LedgerError::Overflow { date })
}

fn sub(a: Decimal, b: Decimal, date: Date) -> Result<Decimal, LedgerError> {
    a.checked_sub(b).ok_or(LedgerError::Overflow { date })
}

fn exceeds(entry: &Entry, held: Decimal) -> LedgerError {
    LedgerError::ExceedsPosition {
        date: entry.trade_date,
        kind: entry.kind.as_str(),
        quantity: entry.quantity.abs(),
        held,
    }
}

fn invalid(entry: &Entry, reason: &'static str) -> LedgerError {
    LedgerError::Invalid {
        date: entry.trade_date,
        reason,
    }
}

/// O valor em reais de `quantidade × preço` (o preço já na moeda original).
fn gross(entry: &Entry, price: Decimal) -> Result<Decimal, LedgerError> {
    let date = entry.trade_date;
    mul(mul(entry.quantity, price, date)?, entry.fx_rate, date)
}

fn fees_brl(entry: &Entry) -> Result<Decimal, LedgerError> {
    mul(entry.fees, entry.fx_rate, entry.trade_date)
}

/// Confere a forma do lançamento (sem olhar a posição).
fn check_shape(entry: &Entry) -> Result<(), LedgerError> {
    if entry.kind.signed() {
        if entry.quantity.is_zero() {
            return Err(invalid(entry, "ajuste de quantidade zero"));
        }
    } else if entry.quantity <= Decimal::ZERO {
        return Err(invalid(entry, "quantidade precisa ser positiva"));
    }
    if entry.fx_rate <= Decimal::ZERO {
        return Err(invalid(entry, "cambio precisa ser positivo"));
    }
    if entry.fees < Decimal::ZERO {
        return Err(invalid(entry, "taxa negativa"));
    }
    if !entry.kind.takes_fees() && !entry.fees.is_zero() {
        return Err(invalid(entry, "esse tipo nao tem taxa"));
    }
    match (entry.kind.price_rule(), entry.unit_price) {
        (PriceRule::Required, None) => Err(invalid(entry, "esse tipo precisa de preco")),
        (PriceRule::Forbidden, Some(_)) => Err(invalid(entry, "esse tipo nao tem preco")),
        (_, Some(price)) if price < Decimal::ZERO => Err(invalid(entry, "preco negativo")),
        (PriceRule::Required, Some(price)) if price.is_zero() => {
            Err(invalid(entry, "preco precisa ser positivo"))
        }
        _ => Ok(()),
    }
}

/// Tira `quantity` da posição pelo custo médio (venda, resgate). Devolve a
/// posição e o lucro realizado.
fn reduce_at_average(
    position: Position,
    entry: &Entry,
    price: Decimal,
) -> Result<(Position, Option<Decimal>), LedgerError> {
    let date = entry.trade_date;
    let q = entry.quantity;
    if q > position.quantity {
        return Err(exceeds(entry, position.quantity));
    }
    let remaining = sub(position.quantity, q, date)?;
    // Saiu tudo: o custo que sai é o custo inteiro, sem a divisão (que
    // deixaria resto de arredondamento num ativo zerado).
    let removed = match position.cost_basis {
        Some(cost) if remaining.is_zero() => Some(cost),
        Some(cost) => Some(
            mul(cost, q, date)?
                .checked_div(position.quantity)
                .ok_or(LedgerError::Overflow { date })?,
        ),
        None => None,
    };
    let proceeds = sub(gross(entry, price)?, fees_brl(entry)?, date)?;
    let realized = match removed {
        Some(removed) => Some(sub(proceeds, removed, date)?),
        None => None,
    };
    let cost_basis = if remaining.is_zero() {
        Some(Decimal::ZERO)
    } else {
        match (position.cost_basis, removed) {
            (Some(cost), Some(removed)) => Some(sub(cost, removed, date)?),
            _ => None,
        }
    };
    Ok((
        Position {
            quantity: remaining,
            cost_basis,
            realized_gain: accumulate(position.realized_gain, realized, date)?,
        },
        realized,
    ))
}

fn accumulate(
    total: Option<Decimal>,
    delta: Option<Decimal>,
    date: Date,
) -> Result<Option<Decimal>, LedgerError> {
    match (total, delta) {
        (Some(total), Some(delta)) => Ok(Some(add(total, delta, date)?)),
        _ => Ok(None),
    }
}

/// Aplica um lançamento à posição. Devolve a posição nova e o lucro que ele
/// realizou.
pub fn apply(
    position: Position,
    entry: &Entry,
) -> Result<(Position, Option<Decimal>), LedgerError> {
    check_shape(entry)?;
    let date = entry.trade_date;
    let q = entry.quantity;
    let zero = Some(Decimal::ZERO);
    let grow = |cost_added: Option<Decimal>| -> Result<Position, LedgerError> {
        Ok(Position {
            quantity: add(position.quantity, q, date)?,
            cost_basis: match (position.cost_basis, cost_added) {
                (Some(cost), Some(added)) => Some(add(cost, added, date)?),
                _ => None,
            },
            realized_gain: position.realized_gain,
        })
    };
    match entry.kind {
        Kind::Opening => {
            let added = match entry.unit_price {
                Some(price) => Some(add(gross(entry, price)?, fees_brl(entry)?, date)?),
                None => None,
            };
            Ok((grow(added)?, zero))
        }
        Kind::Buy => {
            let price = entry.unit_price.unwrap_or_default();
            let added = add(gross(entry, price)?, fees_brl(entry)?, date)?;
            Ok((grow(Some(added))?, zero))
        }
        Kind::Bonus => {
            let added = gross(entry, entry.unit_price.unwrap_or_default())?;
            Ok((grow(Some(added))?, zero))
        }
        Kind::Split => Ok((grow(zero)?, zero)),
        Kind::ReverseSplit => {
            if q >= position.quantity {
                return Err(exceeds(entry, position.quantity));
            }
            Ok((
                Position {
                    quantity: sub(position.quantity, q, date)?,
                    ..position
                },
                zero,
            ))
        }
        Kind::Sell | Kind::Redemption => {
            reduce_at_average(position, entry, entry.unit_price.unwrap_or_default())
        }
        Kind::Amortization => {
            if q > position.quantity {
                return Err(exceeds(entry, position.quantity));
            }
            let returned = gross(entry, entry.unit_price.unwrap_or_default())?;
            let (cost_basis, realized) = match position.cost_basis {
                Some(cost) if returned <= cost => (Some(sub(cost, returned, date)?), zero),
                Some(cost) => (Some(Decimal::ZERO), Some(sub(returned, cost, date)?)),
                None => (None, None),
            };
            Ok((
                Position {
                    quantity: position.quantity,
                    cost_basis,
                    realized_gain: accumulate(position.realized_gain, realized, date)?,
                },
                realized,
            ))
        }
        Kind::Adjustment => {
            let quantity = add(position.quantity, q, date)?;
            if quantity < Decimal::ZERO {
                return Err(exceeds(entry, position.quantity));
            }
            let cost_basis = if quantity.is_zero() {
                zero
            } else {
                position.cost_basis
            };
            Ok((
                Position {
                    quantity,
                    cost_basis,
                    realized_gain: position.realized_gain,
                },
                zero,
            ))
        }
    }
}

/// O livro passo a passo, na ordem do livro (a entrada pode vir em qualquer
/// ordem). Para a primeira data em que a posição ficaria negativa, erro.
pub fn replay(entries: &[Entry]) -> Result<Vec<Step>, LedgerError> {
    let mut sorted = entries.to_vec();
    sort(&mut sorted);
    let mut position = Position::EMPTY;
    let mut steps = Vec::with_capacity(sorted.len());
    for entry in sorted {
        let (next, realized) = apply(position, &entry)?;
        position = next;
        steps.push(Step {
            entry,
            position,
            realized,
        });
    }
    Ok(steps)
}

/// A posição no fim do livro.
pub fn position(entries: &[Entry]) -> Result<Position, LedgerError> {
    Ok(replay(entries)?
        .last()
        .map(|s| s.position)
        .unwrap_or(Position::EMPTY))
}

/// O que a tela mostra de uma posição, já com o valor de hoje.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Performance {
    /// Custo por unidade em reais. `None` sem posição, com custo desconhecido
    /// ou em renda fixa por valor (lá não existe "unidade").
    pub average_price: Option<Decimal>,
    /// Custo total em reais, em centavos. `None`: desconhecido.
    pub cost_basis: Option<Decimal>,
    /// Valor de hoje − custo. `None` sem preço ou com custo desconhecido.
    pub unrealized_gain: Option<Decimal>,
    pub realized_gain: Option<Decimal>,
    /// `unrealized_gain / cost_basis × 100`. `None` quando qualquer um dos dois
    /// falta ou o custo é zero.
    pub return_percent: Option<Decimal>,
}

fn cents(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Preço médio, custo, ganho e rentabilidade de uma posição. `current_value` é
/// o valor de hoje em reais (`None` se o ativo está sem preço).
pub fn performance(
    position: &Position,
    current_value: Option<Decimal>,
    unit: UnitKind,
) -> Performance {
    let cost = position.cost_basis;
    let average_price = match unit {
        UnitKind::Value => None,
        UnitKind::Whole | UnitKind::Fractional | UnitKind::Hundredth => position
            .average_price()
            .map(|p| p.round_dp_with_strategy(8, RoundingStrategy::MidpointAwayFromZero)),
    };
    let unrealized = match (current_value, cost) {
        (Some(value), Some(cost)) => value.checked_sub(cost),
        _ => None,
    };
    let return_percent = match (unrealized, cost) {
        (Some(gain), Some(cost)) if cost > Decimal::ZERO => gain
            .checked_mul(Decimal::ONE_HUNDRED)
            .and_then(|g| g.checked_div(cost))
            .map(cents),
        _ => None,
    };
    Performance {
        average_price,
        cost_basis: cost.map(cents),
        unrealized_gain: unrealized.map(cents),
        realized_gain: position.realized_gain.map(cents),
        return_percent,
    }
}

/// Confere um lançamento contra a unidade do ativo, antes de ele entrar no
/// livro. A mensagem vai para a pessoa (sem acento: atravessa log).
pub fn check_for_unit(
    kind: Kind,
    quantity: Decimal,
    unit_price: Option<Decimal>,
    unit: UnitKind,
) -> Result<(), &'static str> {
    match unit {
        UnitKind::Value => {
            if !kind.applies_to_value() {
                return Err("Em renda fixa por valor os lancamentos sao saldo inicial, aplicacao, resgate e ajuste.");
            }
            if unit_price.is_some_and(|p| p != Decimal::ONE) {
                return Err("Em renda fixa por valor informe so o valor, sem preco por unidade.");
            }
        }
        UnitKind::Whole => {
            // Amortização não mexe na quantidade, mas conta cotas inteiras.
            if quantity.fract() != Decimal::ZERO {
                return Err("Esse ativo so aceita quantidade inteira.");
            }
        }
        UnitKind::Hundredth => {
            // Título do Tesouro: o menor pedaço é 0,01 título.
            if (quantity * Decimal::ONE_HUNDRED).fract() != Decimal::ZERO {
                return Err("Titulo do Tesouro vai em centesimos: no maximo duas casas.");
            }
        }
        UnitKind::Fractional => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn entry(kind: Kind, day: u8, seq: i64, quantity: Decimal, price: Option<Decimal>) -> Entry {
        Entry {
            kind,
            trade_date: Date::from_calendar_date(2026, time::Month::March, day)
                .unwrap_or(date!(2026 - 03 - 01)),
            seq,
            quantity,
            unit_price: price,
            fees: Decimal::ZERO,
            fx_rate: Decimal::ONE,
        }
    }

    fn with_fees(mut e: Entry, fees: Decimal) -> Entry {
        e.fees = fees;
        e
    }

    #[test]
    fn receita_weighted_average_with_fees() {
        // Duas compras com taxa, uma venda: o PM não muda com a venda.
        let entries = [
            with_fees(entry(Kind::Buy, 2, 1, dec!(100), Some(dec!(10))), dec!(10)),
            with_fees(entry(Kind::Buy, 5, 2, dec!(100), Some(dec!(12))), dec!(10)),
            with_fees(entry(Kind::Sell, 9, 3, dec!(50), Some(dec!(15))), dec!(5)),
        ];
        let steps = replay(&entries).unwrap();
        assert_eq!(steps[0].position.average_price(), Some(dec!(10.10)));
        assert_eq!(steps[1].position.cost_basis, Some(dec!(2220)));
        assert_eq!(steps[1].position.average_price(), Some(dec!(11.10)));
        let last = steps[2].position;
        assert_eq!(last.quantity, dec!(150));
        assert_eq!(last.cost_basis, Some(dec!(1665)));
        assert_eq!(last.average_price(), Some(dec!(11.10)));
        // 50 × 15 − 5 − 50 × 11,10
        assert_eq!(steps[2].realized, Some(dec!(190)));
        assert_eq!(last.realized_gain, Some(dec!(190)));
    }

    #[test]
    fn corporate_events() {
        let base = [entry(Kind::Buy, 1, 1, dec!(100), Some(dec!(20)))];
        // Desdobramento 1:2 — cem cotas novas, custo igual, PM pela metade.
        let mut split = base.to_vec();
        split.push(entry(Kind::Split, 2, 2, dec!(100), None));
        let p = position(&split).unwrap();
        assert_eq!((p.quantity, p.cost_basis), (dec!(200), Some(dec!(2000))));
        assert_eq!(p.average_price(), Some(dec!(10)));
        // Grupamento 10:1 — noventa cotas a menos, custo igual.
        let mut reverse = base.to_vec();
        reverse.push(entry(Kind::ReverseSplit, 2, 2, dec!(90), None));
        let p = position(&reverse).unwrap();
        assert_eq!((p.quantity, p.average_price()), (dec!(10), Some(dec!(200))));
        // Grupamento que zera a posição não existe.
        let mut gone = base.to_vec();
        gone.push(entry(Kind::ReverseSplit, 2, 2, dec!(100), None));
        assert!(matches!(
            position(&gone),
            Err(LedgerError::ExceedsPosition { .. })
        ));
        // Bonificação de 10% com custo atribuído de R$ 5 por cota.
        let mut bonus = base.to_vec();
        bonus.push(entry(Kind::Bonus, 2, 2, dec!(10), Some(dec!(5))));
        let p = position(&bonus).unwrap();
        assert_eq!((p.quantity, p.cost_basis), (dec!(110), Some(dec!(2050))));
        // Sem custo atribuído: só dilui o PM.
        let mut free = base.to_vec();
        free.push(entry(Kind::Bonus, 2, 2, dec!(10), None));
        assert_eq!(position(&free).unwrap().cost_basis, Some(dec!(2000)));
        // Amortização de R$ 3 por cota: o custo cai, a quantidade fica.
        let mut amortized = base.to_vec();
        amortized.push(entry(Kind::Amortization, 2, 2, dec!(100), Some(dec!(3))));
        let p = position(&amortized).unwrap();
        assert_eq!(
            (p.quantity, p.cost_basis, p.realized_gain),
            (dec!(100), Some(dec!(1700)), Some(dec!(0)))
        );
        // A que passa do custo vira lucro realizado.
        amortized.push(entry(Kind::Amortization, 3, 3, dec!(100), Some(dec!(20))));
        let p = position(&amortized).unwrap();
        assert_eq!(
            (p.cost_basis, p.realized_gain),
            (Some(dec!(0)), Some(dec!(300)))
        );
    }

    #[test]
    fn opening_without_cost_is_unknown_until_the_position_closes() {
        let entries = [
            entry(Kind::Opening, 1, 1, dec!(10), None),
            entry(Kind::Buy, 2, 2, dec!(5), Some(dec!(20))),
            entry(Kind::Sell, 3, 3, dec!(15), Some(dec!(30))),
            entry(Kind::Buy, 4, 4, dec!(2), Some(dec!(25))),
        ];
        let steps = replay(&entries).unwrap();
        assert_eq!(steps[0].position.cost_basis, None);
        assert_eq!(steps[0].position.average_price(), None);
        assert_eq!(steps[1].position.cost_basis, None);
        // Vendeu tudo: o lucro dessa venda é desconhecido, mas o custo zera.
        assert_eq!(steps[2].realized, None);
        assert_eq!(
            steps[2].position,
            Position {
                quantity: dec!(0),
                cost_basis: Some(dec!(0)),
                realized_gain: None
            }
        );
        // Ciclo novo, custo conhecido de novo.
        assert_eq!(steps[3].position.average_price(), Some(dec!(25)));
        assert_eq!(steps[3].position.realized_gain, None);
        // Com o preço do saldo inicial, tudo fica conhecido.
        let mut known = entries.to_vec();
        known[0].unit_price = Some(dec!(18));
        let p = position(&known).unwrap();
        // 10 × 18 + 5 × 20 = 280 por 15 cotas; vendidas a 30: 450 − 280.
        assert_eq!(p.realized_gain, Some(dec!(170)));
    }

    #[test]
    fn dollar_assets_cost_in_reais_at_each_trade_rate() {
        let mut first = with_fees(entry(Kind::Buy, 1, 1, dec!(10), Some(dec!(200))), dec!(1));
        first.fx_rate = dec!(5.5);
        let mut second = entry(Kind::Buy, 2, 2, dec!(10), Some(dec!(210)));
        second.fx_rate = dec!(5);
        let p = position(&[first, second]).unwrap();
        // (2000 + 1) × 5,5 + 2100 × 5
        assert_eq!(p.cost_basis, Some(dec!(21505.5)));
        assert_eq!(p.average_price(), Some(dec!(1075.275)));
    }

    #[test]
    fn adjustment_changes_quantity_and_keeps_cost() {
        let entries = [
            entry(Kind::Buy, 1, 1, dec!(50000), Some(dec!(1))),
            entry(Kind::Adjustment, 2, 2, dec!(2000), None),
        ];
        let p = position(&entries).unwrap();
        assert_eq!((p.quantity, p.cost_basis), (dec!(52000), Some(dec!(50000))));
        let perf = performance(&p, Some(dec!(52000)), UnitKind::Value);
        assert_eq!(perf.average_price, None, "renda fixa por valor nao tem PM");
        assert_eq!(perf.unrealized_gain, Some(dec!(2000)));
        assert_eq!(perf.return_percent, Some(dec!(4)));
        let mut down = entries.to_vec();
        down.push(entry(Kind::Adjustment, 3, 3, dec!(-52000), None));
        assert_eq!(
            position(&down).unwrap(),
            Position {
                quantity: dec!(0),
                cost_basis: Some(dec!(0)),
                realized_gain: Some(dec!(0))
            }
        );
        down.push(entry(Kind::Adjustment, 4, 4, dec!(-1), None));
        assert!(matches!(
            position(&down),
            Err(LedgerError::ExceedsPosition { .. })
        ));
    }

    #[test]
    fn selling_more_than_held_on_that_date_fails() {
        // A compra de depois não cobre a venda de antes.
        let entries = [
            entry(Kind::Buy, 1, 1, dec!(10), Some(dec!(10))),
            entry(Kind::Sell, 2, 2, dec!(15), Some(dec!(10))),
            entry(Kind::Buy, 3, 3, dec!(10), Some(dec!(10))),
        ];
        let error = position(&entries).unwrap_err();
        assert_eq!(
            error,
            LedgerError::ExceedsPosition {
                date: date!(2026 - 03 - 02),
                kind: "sell",
                quantity: dec!(15),
                held: dec!(10)
            }
        );
        assert!(error.to_string().contains("passa da posicao"));
        // No mesmo dia, a ordem de entrada decide.
        let same_day = [
            entry(Kind::Sell, 1, 1, dec!(5), Some(dec!(10))),
            entry(Kind::Buy, 1, 2, dec!(10), Some(dec!(10))),
        ];
        assert!(position(&same_day).is_err());
        let fixed = [
            entry(Kind::Sell, 1, 3, dec!(5), Some(dec!(10))),
            entry(Kind::Buy, 1, 2, dec!(10), Some(dec!(10))),
        ];
        assert_eq!(position(&fixed).unwrap().quantity, dec!(5));
        let amortize_more = [
            entry(Kind::Buy, 1, 1, dec!(10), Some(dec!(10))),
            entry(Kind::Amortization, 2, 2, dec!(11), Some(dec!(1))),
        ];
        assert!(position(&amortize_more).is_err());
    }

    #[test]
    fn shape_is_checked() {
        let bad = [
            (entry(Kind::Buy, 1, 1, dec!(0), Some(dec!(1))), "quantidade"),
            (
                entry(Kind::Buy, 1, 1, dec!(-1), Some(dec!(1))),
                "quantidade",
            ),
            (entry(Kind::Adjustment, 1, 1, dec!(0), None), "ajuste"),
            (entry(Kind::Buy, 1, 1, dec!(1), None), "precisa de preco"),
            (entry(Kind::Buy, 1, 1, dec!(1), Some(dec!(0))), "positivo"),
            (
                entry(Kind::Bonus, 1, 1, dec!(1), Some(dec!(-1))),
                "negativo",
            ),
            (
                entry(Kind::Split, 1, 1, dec!(1), Some(dec!(1))),
                "nao tem preco",
            ),
            (
                with_fees(entry(Kind::Split, 1, 1, dec!(1), None), dec!(1)),
                "taxa",
            ),
            (
                with_fees(entry(Kind::Buy, 1, 1, dec!(1), Some(dec!(1))), dec!(-1)),
                "taxa",
            ),
            (
                Entry {
                    fx_rate: dec!(0),
                    ..entry(Kind::Buy, 1, 1, dec!(1), Some(dec!(1)))
                },
                "cambio",
            ),
        ];
        for (e, why) in bad {
            let error = apply(Position::EMPTY, &e).unwrap_err();
            assert!(error.to_string().contains(why), "{why}: {error}");
        }
        let huge = entry(Kind::Buy, 1, 1, Decimal::MAX, Some(Decimal::MAX));
        assert!(matches!(
            apply(Position::EMPTY, &huge),
            Err(LedgerError::Overflow { .. })
        ));
        assert!(LedgerError::Overflow {
            date: date!(2026 - 01 - 01)
        }
        .to_string()
        .contains("grande"));
    }

    #[test]
    fn performance_needs_price_and_known_cost() {
        let p = Position {
            quantity: dec!(10),
            cost_basis: Some(dec!(100)),
            realized_gain: Some(dec!(0)),
        };
        let perf = performance(&p, Some(dec!(125)), UnitKind::Whole);
        assert_eq!(perf.average_price, Some(dec!(10)));
        assert_eq!(perf.unrealized_gain, Some(dec!(25)));
        assert_eq!(perf.return_percent, Some(dec!(25)));
        assert_eq!(performance(&p, None, UnitKind::Whole).unrealized_gain, None);
        let unknown = Position {
            cost_basis: None,
            ..p
        };
        let perf = performance(&unknown, Some(dec!(125)), UnitKind::Fractional);
        assert_eq!(
            (perf.average_price, perf.cost_basis, perf.return_percent),
            (None, None, None)
        );
        // Sem posição: custo zero, sem rentabilidade (divisão por zero).
        let perf = performance(&Position::EMPTY, Some(dec!(0)), UnitKind::Whole);
        assert_eq!(
            (
                perf.average_price,
                perf.unrealized_gain,
                perf.return_percent
            ),
            (None, Some(dec!(0)), None)
        );
    }

    #[test]
    fn unit_rules() {
        assert!(check_for_unit(Kind::Buy, dec!(1.5), Some(dec!(1)), UnitKind::Whole).is_err());
        assert!(check_for_unit(Kind::Buy, dec!(1.5), Some(dec!(1)), UnitKind::Fractional).is_ok());
        assert!(check_for_unit(Kind::Buy, dec!(0.01), Some(dec!(1)), UnitKind::Hundredth).is_ok());
        assert!(check_for_unit(Kind::Sell, dec!(1.37), Some(dec!(1)), UnitKind::Hundredth).is_ok());
        assert!(
            check_for_unit(Kind::Buy, dec!(0.001), Some(dec!(1)), UnitKind::Hundredth).is_err()
        );
        assert!(check_for_unit(Kind::Adjustment, dec!(-0.015), None, UnitKind::Hundredth).is_err());
        assert!(check_for_unit(Kind::Split, dec!(1), None, UnitKind::Value).is_err());
        assert!(check_for_unit(Kind::Buy, dec!(100), Some(dec!(2)), UnitKind::Value).is_err());
        assert!(
            check_for_unit(Kind::Redemption, dec!(100), Some(dec!(1)), UnitKind::Value).is_ok()
        );
        assert!(check_for_unit(Kind::Adjustment, dec!(-100.5), None, UnitKind::Value).is_ok());
    }

    #[test]
    fn kinds_round_trip() {
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
        }
        assert_eq!(Kind::parse("swap"), None);
        assert_eq!(Kind::Sell.quantity_delta(dec!(3)), dec!(-3));
        assert_eq!(Kind::Amortization.quantity_delta(dec!(3)), dec!(0));
        assert_eq!(Position::default(), Position::EMPTY);
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn arb_kind() -> impl Strategy<Value = Kind> {
            prop::sample::select(Kind::ALL.to_vec())
        }

        fn arb_entry() -> impl Strategy<Value = Entry> {
            (
                arb_kind(),
                0u16..60,
                1i64..10_000,
                1i64..2_000,
                prop::option::weighted(0.85, 1i64..100_000),
                0i64..500,
                prop::bool::weighted(0.3),
                prop::bool::ANY,
            )
                .prop_map(
                    |(kind, day, seq, qty, price_cents, fee_cents, dollar, negative)| {
                        let quantity = Decimal::new(qty, 1);
                        let quantity = if kind.signed() && negative {
                            -quantity
                        } else {
                            quantity
                        };
                        let unit_price = match kind.price_rule() {
                            PriceRule::Required => Some(Decimal::new(price_cents.unwrap_or(1), 2)),
                            PriceRule::Optional => price_cents.map(|c| Decimal::new(c, 2)),
                            PriceRule::Forbidden => None,
                        };
                        Entry {
                            kind,
                            trade_date: date!(2026 - 01 - 01)
                                + time::Duration::days(i64::from(day)),
                            seq,
                            quantity,
                            unit_price,
                            fees: if kind.takes_fees() {
                                Decimal::new(fee_cents, 2)
                            } else {
                                Decimal::ZERO
                            },
                            fx_rate: if dollar { dec!(5.4321) } else { Decimal::ONE },
                        }
                    },
                )
        }

        /// Seq único, como o banco garante.
        fn unique_seq(mut entries: Vec<Entry>) -> Vec<Entry> {
            for (i, e) in entries.iter_mut().enumerate() {
                e.seq = e.seq * 100 + i64::try_from(i).unwrap_or(0);
            }
            entries
        }

        fn close(a: Decimal, b: Decimal) -> bool {
            (a - b).abs() <= a.abs().max(Decimal::ONE) * dec!(0.000000000000000001)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(400))]

            /// O livro recusa exatamente o que deixaria a quantidade negativa
            /// (ou zeraria num grupamento, ou amortizaria cota que não há) em
            /// alguma data; o que ele aceita nunca fica negativo e soma o
            /// que os lançamentos somam.
            #[test]
            fn quantity_never_negative_and_matches_the_signed_sum(entries in prop::collection::vec(arb_entry(), 0..25)) {
                let entries = unique_seq(entries);
                let mut sorted = entries.clone();
                sort(&mut sorted);
                let mut held = Decimal::ZERO;
                let mut naive_ok = true;
                for e in &sorted {
                    let next = held + e.kind.quantity_delta(e.quantity);
                    let breaks = match e.kind {
                        Kind::ReverseSplit => next <= Decimal::ZERO,
                        Kind::Amortization => e.quantity > held,
                        _ => next < Decimal::ZERO,
                    };
                    if breaks { naive_ok = false; break; }
                    held = next;
                }
                match replay(&entries) {
                    Ok(steps) => {
                        prop_assert!(naive_ok);
                        for s in &steps {
                            prop_assert!(s.position.quantity >= Decimal::ZERO);
                            if s.position.quantity.is_zero() {
                                prop_assert_eq!(s.position.cost_basis, Some(Decimal::ZERO));
                            }
                            if let Some(cost) = s.position.cost_basis {
                                prop_assert!(cost >= Decimal::ZERO, "custo negativo: {cost}");
                            }
                        }
                        let last = steps.last().map(|s| s.position.quantity).unwrap_or_default();
                        prop_assert_eq!(last, held);
                    }
                    Err(error) => {
                        prop_assert!(!naive_ok, "recusou um livro valido: {}", error);
                        let is_position_error = matches!(error, LedgerError::ExceedsPosition { .. });
                        prop_assert!(is_position_error);
                    }
                }
            }

            /// A ordem de entrada não muda nada: data, depois sequência.
            #[test]
            fn order_is_deterministic(entries in prop::collection::vec(arb_entry(), 0..20), seed in any::<u64>()) {
                let entries = unique_seq(entries);
                let mut shuffled = entries.clone();
                // Embaralha de forma reproduzível a partir da semente.
                let n = shuffled.len();
                let mut state = seed | 1;
                for i in (1..n).rev() {
                    state ^= state << 13; state ^= state >> 7; state ^= state << 17;
                    let j = usize::try_from(state % (u64::try_from(i).unwrap_or(1) + 1)).unwrap_or(0);
                    shuffled.swap(i, j);
                }
                prop_assert_eq!(replay(&entries), replay(&shuffled));
            }

            /// A venda não muda o preço médio.
            #[test]
            fn sale_keeps_the_average_price(
                buys in prop::collection::vec((1i64..1_000, 1i64..1_000_000, 0i64..1_000), 1..8),
                sell_fraction in 1u32..100,
                sell_price in 1i64..1_000_000,
            ) {
                let mut entries: Vec<Entry> = buys.iter().enumerate().map(|(i, (q, p, f))| Entry {
                    kind: Kind::Buy,
                    trade_date: date!(2026 - 01 - 01),
                    seq: i64::try_from(i).unwrap_or(0),
                    quantity: Decimal::new(*q, 0),
                    unit_price: Some(Decimal::new(*p, 2)),
                    fees: Decimal::new(*f, 2),
                    fx_rate: Decimal::ONE,
                }).collect();
                let before = position(&entries).unwrap();
                let sold = (before.quantity * Decimal::from(sell_fraction) / dec!(100)).floor().max(Decimal::ONE);
                prop_assume!(sold < before.quantity);
                entries.push(Entry {
                    kind: Kind::Sell,
                    trade_date: date!(2026 - 02 - 01),
                    seq: 1_000,
                    quantity: sold,
                    unit_price: Some(Decimal::new(sell_price, 2)),
                    fees: Decimal::ZERO,
                    fx_rate: Decimal::ONE,
                });
                let after = position(&entries).unwrap();
                let (pm_before, pm_after) = (before.average_price().unwrap(), after.average_price().unwrap());
                prop_assert!(close(pm_before, pm_after), "{pm_before} -> {pm_after}");
                prop_assert_eq!(after.quantity, before.quantity - sold);
            }

            /// Desdobramento e grupamento preservam o custo total, exato.
            #[test]
            fn split_preserves_the_total_cost(
                entries in prop::collection::vec(arb_entry(), 0..15),
                new_shares in 1i64..10_000,
                reverse in prop::bool::ANY,
            ) {
                let mut entries = unique_seq(entries);
                let Ok(before) = position(&entries) else { return Ok(()); };
                let quantity = if reverse {
                    prop_assume!(before.quantity > Decimal::ONE);
                    (before.quantity / dec!(2)).floor().max(dec!(0.1))
                } else {
                    Decimal::new(new_shares, 1)
                };
                entries.push(Entry {
                    kind: if reverse { Kind::ReverseSplit } else { Kind::Split },
                    trade_date: date!(2027 - 01 - 01),
                    seq: i64::MAX,
                    quantity,
                    unit_price: None,
                    fees: Decimal::ZERO,
                    fx_rate: Decimal::ONE,
                });
                let after = position(&entries).unwrap();
                prop_assert_eq!(after.cost_basis, before.cost_basis);
                prop_assert_eq!(after.realized_gain, before.realized_gain);
            }
        }
    }
}
