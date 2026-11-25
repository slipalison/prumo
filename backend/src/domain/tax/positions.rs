//! Os lançamentos viram resultados realizados e posições, pela regra da
//! Receita:
//!
//! - custo de aquisição pela **média ponderada** dos custos unitários, com
//!   corretagem e emolumentos somados à compra e abatidos da venda (IN RFB
//!   1.585/2015, art. 58; P&R 708, 719); a média é por ativo, mesmo em
//!   corretoras diferentes (P&R 720);
//! - venda não muda o preço médio;
//! - desdobramento e grupamento mudam só a quantidade; o custo total fica
//!   (IN RFB 1.585/2015, art. 58, par. 7, II; P&R 722);
//! - bonificação entra pelo valor capitalizado por ação que a empresa informa
//!   (P&R 721);
//! - **day trade**: compra e venda do mesmo ativo, no mesmo dia, na mesma
//!   instituição. A quantidade casada é day trade, casando a primeira compra
//!   com a primeira venda, sucessivamente (P&R 705; IN RFB 1.585/2015, art. 65,
//!   par. 1 e 3). O estoque de antes do dia não entra (art. 65, par. 2): a
//!   compra casada não mexe no preço médio, e o resto do dia é operação comum.
//!   Cripto não tem day trade (não é bolsa).
//!
//! Venda acima da posição é erro: o Prumo não tem venda a descoberto.
//! Valores sempre em reais (quem liga ao livro converte o que for em dólar).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;
use time::Date;

use super::{Instrument, TaxError};

/// Um lançamento do livro, do jeito que o imposto precisa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub date: Date,
    pub symbol: String,
    pub instrument: Instrument,
    /// Instituição intermediadora: o day trade é por instituição. `None` trata
    /// tudo como uma instituição só.
    pub broker: Option<String>,
    pub action: Action,
}

/// O que o lançamento faz. Quantidades e preços positivos; taxas e IR retido
/// não negativos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Buy {
        quantity: Decimal,
        unit_price: Decimal,
        /// Corretagem, emolumentos e demais custos da nota.
        fees: Decimal,
    },
    Sell {
        quantity: Decimal,
        unit_price: Decimal,
        fees: Decimal,
        /// IR retido na fonte informado na nota (0,005% da operação comum ou
        /// 1% do day trade).
        withheld: Decimal,
    },
    /// Desdobramento (fator > 1) ou grupamento (fator < 1): a quantidade é
    /// multiplicada pelo fator e o custo total não muda. O produto sai
    /// arredondado a [`QUANTITY_DECIMALS`] casas: um grupamento de 1 para 3 tem
    /// fator 0,333… e, sem o arredondamento, 99 cotas virariam 32,999… em vez
    /// de 33.
    Split { factor: Decimal },
    /// Bonificação: ações novas com o custo unitário informado pela empresa.
    Bonus {
        quantity: Decimal,
        unit_cost: Decimal,
    },
}

/// Casas decimais da quantidade depois de um desdobramento ou grupamento: a
/// precisão do livro (`numeric(28, 10)`).
pub const QUANTITY_DECIMALS: u32 = 10;

/// Posição de partida (antes do primeiro lançamento), com o custo total pelo
/// preço médio da Receita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpeningPosition {
    pub symbol: String,
    pub instrument: Instrument,
    pub quantity: Decimal,
    pub total_cost: Decimal,
}

/// Uma venda realizada (ou a parte dela que foi day trade).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Realized {
    pub date: Date,
    pub symbol: String,
    pub instrument: Instrument,
    pub day_trade: bool,
    pub quantity: Decimal,
    /// Valor de alienação bruto (quantidade × preço), o que soma no limite de
    /// isenção.
    pub proceeds: Decimal,
    /// Taxas da venda e, no day trade, da compra casada.
    pub fees: Decimal,
    /// Custo de aquisição da quantidade vendida.
    pub cost: Decimal,
}

impl Realized {
    /// Lucro (positivo) ou prejuízo (negativo).
    pub fn result(&self) -> Decimal {
        self.proceeds - self.fees - self.cost
    }
}

/// IR retido na fonte num dia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Withholding {
    pub date: Date,
    pub symbol: String,
    pub instrument: Instrument,
    pub amount: Decimal,
}

/// Posição de um ativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Holding {
    pub symbol: String,
    pub instrument: Instrument,
    pub quantity: Decimal,
    pub total_cost: Decimal,
}

impl Holding {
    /// Preço médio; zero sem quantidade.
    pub fn average_price(&self) -> Decimal {
        if self.quantity.is_zero() {
            Decimal::ZERO
        } else {
            self.total_cost / self.quantity
        }
    }
}

/// O que sai do livro até uma data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Replay {
    pub realized: Vec<Realized>,
    pub withholdings: Vec<Withholding>,
    /// Posições abertas (quantidade > 0), por símbolo.
    pub holdings: BTreeMap<String, Holding>,
}

/// Percorre posições de partida e lançamentos até `until` (inclusive).
/// Lançamentos do mesmo dia seguem a ordem da lista; no dia, eventos
/// (desdobramento, bonificação) vêm antes das compras e vendas.
pub fn replay(
    openings: &[OpeningPosition],
    operations: &[Operation],
    until: Date,
) -> Result<Replay, TaxError> {
    let mut book = Book::default();
    for opening in openings {
        book.open(opening)?;
    }
    for operation in operations {
        validate(operation)?;
        book.check_instrument(&operation.symbol, operation.instrument)?;
    }

    let mut by_day: BTreeMap<Date, Vec<(usize, &Operation)>> = BTreeMap::new();
    for (index, operation) in operations.iter().enumerate() {
        if operation.date <= until {
            by_day
                .entry(operation.date)
                .or_default()
                .push((index, operation));
        }
    }

    let mut out = Replay::default();
    for (date, day) in by_day {
        for &(_, operation) in &day {
            book.apply_event(operation)?;
        }
        let mut pieces = Vec::new();
        // Por (ativo, instituição): as compras e as vendas do dia, na ordem.
        let mut groups: BTreeMap<(&str, Option<&str>), DayLegs> = BTreeMap::new();
        for &(index, operation) in &day {
            let leg = match operation.action {
                Action::Buy {
                    quantity,
                    unit_price,
                    fees,
                } => Leg::new(index, operation, Side::Buy, quantity, unit_price, fees),
                Action::Sell {
                    quantity,
                    unit_price,
                    fees,
                    withheld,
                } => {
                    if !withheld.is_zero() {
                        out.withholdings.push(Withholding {
                            date,
                            symbol: operation.symbol.clone(),
                            instrument: operation.instrument,
                            amount: withheld,
                        });
                    }
                    Leg::new(index, operation, Side::Sell, quantity, unit_price, fees)
                }
                Action::Split { .. } | Action::Bonus { .. } => continue,
            };
            if operation.instrument == Instrument::Crypto {
                pieces.push(leg);
                continue;
            }
            let group = groups
                .entry((operation.symbol.as_str(), operation.broker.as_deref()))
                .or_default();
            match leg.side {
                Side::Buy => group.buys.push(leg),
                Side::Sell => group.sells.push(leg),
            }
        }
        for (_, legs) in groups {
            match_day_trades(date, legs.buys, legs.sells, &mut out.realized, &mut pieces);
        }
        pieces.sort_by_key(|leg| leg.index);
        for leg in pieces {
            if let Some(realized) = book.apply_common(date, &leg)? {
                out.realized.push(realized);
            }
        }
    }
    out.holdings = book
        .holdings
        .into_iter()
        .filter(|(_, holding)| holding.quantity > Decimal::ZERO)
        .collect();
    Ok(out)
}

fn validate(operation: &Operation) -> Result<(), TaxError> {
    let invalid = |reason| TaxError::InvalidOperation {
        symbol: operation.symbol.clone(),
        date: operation.date,
        reason,
    };
    match operation.action {
        Action::Buy {
            quantity,
            unit_price,
            fees,
        } => {
            if quantity <= Decimal::ZERO {
                return Err(invalid("quantidade precisa ser positiva"));
            }
            if unit_price < Decimal::ZERO || fees < Decimal::ZERO {
                return Err(invalid("preco e taxas nao podem ser negativos"));
            }
        }
        Action::Sell {
            quantity,
            unit_price,
            fees,
            withheld,
        } => {
            if quantity <= Decimal::ZERO {
                return Err(invalid("quantidade precisa ser positiva"));
            }
            if unit_price < Decimal::ZERO || fees < Decimal::ZERO || withheld < Decimal::ZERO {
                return Err(invalid("preco, taxas e IR retido nao podem ser negativos"));
            }
            if operation.instrument == Instrument::Crypto && !withheld.is_zero() {
                return Err(invalid("cripto nao tem IR retido na fonte"));
            }
        }
        Action::Split { factor } => {
            if factor <= Decimal::ZERO {
                return Err(invalid("fator de desdobramento precisa ser positivo"));
            }
        }
        Action::Bonus {
            quantity,
            unit_cost,
        } => {
            if quantity <= Decimal::ZERO {
                return Err(invalid("quantidade precisa ser positiva"));
            }
            if unit_cost < Decimal::ZERO {
                return Err(invalid("custo da bonificacao nao pode ser negativo"));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Buy,
    Sell,
}

/// Compras e vendas de um ativo numa instituição, num dia.
#[derive(Debug, Default)]
struct DayLegs {
    buys: Vec<Leg>,
    sells: Vec<Leg>,
}

/// Uma compra ou venda do dia, consumida aos poucos no casamento do day trade.
#[derive(Debug, Clone)]
struct Leg {
    index: usize,
    symbol: String,
    instrument: Instrument,
    side: Side,
    quantity: Decimal,
    unit_price: Decimal,
    fees: Decimal,
}

impl Leg {
    fn new(
        index: usize,
        operation: &Operation,
        side: Side,
        quantity: Decimal,
        unit_price: Decimal,
        fees: Decimal,
    ) -> Self {
        Self {
            index,
            symbol: operation.symbol.clone(),
            instrument: operation.instrument,
            side,
            quantity,
            unit_price,
            fees,
        }
    }

    /// Tira `quantity` da perna e devolve as taxas proporcionais (o resto das
    /// taxas fica com a última parte, para a soma bater).
    fn take(&mut self, quantity: Decimal) -> Decimal {
        let fees = if quantity >= self.quantity {
            self.fees
        } else {
            self.fees * quantity / self.quantity
        };
        self.quantity -= quantity;
        self.fees -= fees;
        fees
    }
}

/// Casa compras e vendas do mesmo ativo, dia e instituição na ordem (primeira
/// com primeira). O casado vira resultado de day trade; o que sobra vai para
/// `pieces` como operação comum.
fn match_day_trades(
    date: Date,
    mut buys: Vec<Leg>,
    mut sells: Vec<Leg>,
    realized: &mut Vec<Realized>,
    pieces: &mut Vec<Leg>,
) {
    let (mut b, mut s) = (0, 0);
    while b < buys.len() && s < sells.len() {
        let quantity = buys[b].quantity.min(sells[s].quantity);
        let buy_fees = buys[b].take(quantity);
        let sell_fees = sells[s].take(quantity);
        let (buy, sell) = (&buys[b], &sells[s]);
        realized.push(Realized {
            date,
            symbol: sell.symbol.clone(),
            instrument: sell.instrument,
            day_trade: true,
            quantity,
            proceeds: quantity * sell.unit_price,
            fees: buy_fees + sell_fees,
            cost: quantity * buy.unit_price,
        });
        if buys[b].quantity.is_zero() {
            b += 1;
        }
        if sells[s].quantity.is_zero() {
            s += 1;
        }
    }
    pieces.extend(
        buys.into_iter()
            .chain(sells)
            .filter(|leg| leg.quantity > Decimal::ZERO),
    );
}

#[derive(Debug, Default)]
struct Book {
    holdings: BTreeMap<String, Holding>,
    instruments: BTreeMap<String, Instrument>,
}

impl Book {
    fn check_instrument(&mut self, symbol: &str, instrument: Instrument) -> Result<(), TaxError> {
        match self.instruments.get(symbol) {
            Some(known) if *known != instrument => Err(TaxError::InstrumentMismatch {
                symbol: symbol.to_owned(),
            }),
            Some(_) => Ok(()),
            None => {
                self.instruments.insert(symbol.to_owned(), instrument);
                Ok(())
            }
        }
    }

    fn open(&mut self, opening: &OpeningPosition) -> Result<(), TaxError> {
        if opening.quantity < Decimal::ZERO || opening.total_cost < Decimal::ZERO {
            return Err(TaxError::InvalidOperation {
                symbol: opening.symbol.clone(),
                date: Date::MIN,
                reason: "posicao inicial nao pode ser negativa",
            });
        }
        self.check_instrument(&opening.symbol, opening.instrument)?;
        let holding = self.holding(&opening.symbol, opening.instrument);
        holding.quantity += opening.quantity;
        holding.total_cost += opening.total_cost;
        Ok(())
    }

    fn holding(&mut self, symbol: &str, instrument: Instrument) -> &mut Holding {
        self.holdings
            .entry(symbol.to_owned())
            .or_insert_with(|| Holding {
                symbol: symbol.to_owned(),
                instrument,
                quantity: Decimal::ZERO,
                total_cost: Decimal::ZERO,
            })
    }

    fn apply_event(&mut self, operation: &Operation) -> Result<(), TaxError> {
        let no_position = || TaxError::InvalidOperation {
            symbol: operation.symbol.clone(),
            date: operation.date,
            reason: "evento societario sem posicao",
        };
        match operation.action {
            Action::Split { factor } => {
                let holding = self
                    .holdings
                    .get_mut(&operation.symbol)
                    .filter(|holding| holding.quantity > Decimal::ZERO)
                    .ok_or_else(no_position)?;
                holding.quantity = (holding.quantity * factor).round_dp(QUANTITY_DECIMALS);
            }
            Action::Bonus {
                quantity,
                unit_cost,
            } => {
                let holding = self
                    .holdings
                    .get_mut(&operation.symbol)
                    .filter(|holding| holding.quantity > Decimal::ZERO)
                    .ok_or_else(no_position)?;
                holding.quantity += quantity;
                holding.total_cost += quantity * unit_cost;
            }
            Action::Buy { .. } | Action::Sell { .. } => {}
        }
        Ok(())
    }

    fn apply_common(&mut self, date: Date, leg: &Leg) -> Result<Option<Realized>, TaxError> {
        let holding = self.holding(&leg.symbol, leg.instrument);
        match leg.side {
            Side::Buy => {
                holding.quantity += leg.quantity;
                holding.total_cost += leg.quantity * leg.unit_price + leg.fees;
                Ok(None)
            }
            Side::Sell => {
                if leg.quantity > holding.quantity {
                    return Err(TaxError::SellExceedsPosition {
                        symbol: leg.symbol.clone(),
                        date,
                    });
                }
                let cost = if leg.quantity == holding.quantity {
                    holding.total_cost
                } else {
                    holding.total_cost * leg.quantity / holding.quantity
                };
                holding.quantity -= leg.quantity;
                holding.total_cost -= cost;
                // Uma posição zerada perde o custo que sobrou de arredondamento.
                if holding.quantity.is_zero() {
                    holding.total_cost = Decimal::ZERO;
                }
                Ok(Some(Realized {
                    date,
                    symbol: leg.symbol.clone(),
                    instrument: leg.instrument,
                    day_trade: false,
                    quantity: leg.quantity,
                    proceeds: leg.quantity * leg.unit_price,
                    fees: leg.fees,
                    cost,
                }))
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    pub(crate) fn buy(
        date: Date,
        symbol: &str,
        instrument: Instrument,
        quantity: Decimal,
        price: Decimal,
    ) -> Operation {
        Operation {
            date,
            symbol: symbol.to_owned(),
            instrument,
            broker: None,
            action: Action::Buy {
                quantity,
                unit_price: price,
                fees: Decimal::ZERO,
            },
        }
    }

    pub(crate) fn sell(
        date: Date,
        symbol: &str,
        instrument: Instrument,
        quantity: Decimal,
        price: Decimal,
    ) -> Operation {
        Operation {
            date,
            symbol: symbol.to_owned(),
            instrument,
            broker: None,
            action: Action::Sell {
                quantity,
                unit_price: price,
                fees: Decimal::ZERO,
                withheld: Decimal::ZERO,
            },
        }
    }

    pub(crate) fn with_fees(mut operation: Operation, value: Decimal) -> Operation {
        match &mut operation.action {
            Action::Buy { fees, .. } | Action::Sell { fees, .. } => *fees = value,
            Action::Split { .. } | Action::Bonus { .. } => {}
        }
        operation
    }

    pub(crate) fn with_withheld(mut operation: Operation, value: Decimal) -> Operation {
        if let Action::Sell { withheld, .. } = &mut operation.action {
            *withheld = value;
        }
        operation
    }

    const END: Date = date!(2026 - 12 - 31);

    #[test]
    fn average_cost_includes_fees_and_a_sale_keeps_it() {
        // P&R 708 e 719: taxas somam no custo; média ponderada.
        let operations = [
            with_fees(
                buy(
                    date!(2025 - 01 - 10),
                    "AAAA3",
                    Instrument::Stock,
                    dec!(100),
                    dec!(10),
                ),
                dec!(5),
            ),
            with_fees(
                buy(
                    date!(2025 - 02 - 10),
                    "AAAA3",
                    Instrument::Stock,
                    dec!(100),
                    dec!(12),
                ),
                dec!(5),
            ),
            with_fees(
                sell(
                    date!(2025 - 03 - 10),
                    "AAAA3",
                    Instrument::Stock,
                    dec!(50),
                    dec!(15),
                ),
                dec!(2),
            ),
        ];
        let replay = replay(&[], &operations, END).unwrap();
        let holding = &replay.holdings["AAAA3"];
        assert_eq!(holding.quantity, dec!(150));
        // custo 2.210 / 200 = 11,05 por ação; o PM não muda com a venda.
        assert_eq!(holding.average_price(), dec!(11.05));
        let sale = &replay.realized[0];
        assert_eq!(sale.cost, dec!(552.5));
        assert_eq!(sale.result(), dec!(750) - dec!(2) - dec!(552.5));
        assert!(!sale.day_trade);
    }

    #[test]
    fn same_asset_in_two_brokers_shares_one_average() {
        // P&R 720: média ponderada mesmo em custódias diferentes.
        let mut first = buy(
            date!(2025 - 01 - 10),
            "AAAA3",
            Instrument::Stock,
            dec!(10),
            dec!(10),
        );
        first.broker = Some("A".to_owned());
        let mut second = buy(
            date!(2025 - 01 - 11),
            "AAAA3",
            Instrument::Stock,
            dec!(10),
            dec!(20),
        );
        second.broker = Some("B".to_owned());
        let replay = replay(&[], &[first, second], END).unwrap();
        assert_eq!(replay.holdings["AAAA3"].average_price(), dec!(15));
    }

    #[test]
    fn split_keeps_the_total_cost() {
        // P&R 722: desdobramento aumenta a quantidade e mantém o custo total.
        let operations = [
            buy(
                date!(2025 - 01 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(100),
                dec!(30),
            ),
            Operation {
                date: date!(2025 - 05 - 02),
                symbol: "AAAA3".to_owned(),
                instrument: Instrument::Stock,
                broker: None,
                action: Action::Split { factor: dec!(3) },
            },
        ];
        let replay = replay(&[], &operations, END).unwrap();
        let holding = &replay.holdings["AAAA3"];
        assert_eq!(holding.quantity, dec!(300));
        assert_eq!(holding.total_cost, dec!(3000));
        assert_eq!(holding.average_price(), dec!(10));
    }

    #[test]
    fn reverse_split_with_an_inexact_factor_lands_on_whole_shares() {
        // Grupamento de 3 para 1: o fator 1/3 não cabe em decimal, e a venda
        // das 33 cotas novas precisa achar 33, não 32,999...
        let factor = dec!(33) / dec!(99);
        let operations = [
            buy(
                date!(2025 - 01 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(99),
                dec!(10),
            ),
            Operation {
                date: date!(2025 - 05 - 02),
                symbol: "AAAA3".to_owned(),
                instrument: Instrument::Stock,
                broker: None,
                action: Action::Split { factor },
            },
            sell(
                date!(2025 - 06 - 02),
                "AAAA3",
                Instrument::Stock,
                dec!(33),
                dec!(40),
            ),
        ];
        let replay = replay(&[], &operations, END).unwrap();
        assert!(replay.holdings.is_empty());
        assert_eq!(replay.realized[0].cost, dec!(990));
    }

    #[test]
    fn bonus_shares_enter_at_the_capitalized_value() {
        // P&R 721: custo da bonificação = valor capitalizado por ação.
        let operations = [
            buy(
                date!(2025 - 01 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(100),
                dec!(20),
            ),
            Operation {
                date: date!(2025 - 06 - 02),
                symbol: "AAAA3".to_owned(),
                instrument: Instrument::Stock,
                broker: None,
                action: Action::Bonus {
                    quantity: dec!(10),
                    unit_cost: dec!(4.50),
                },
            },
        ];
        let replay = replay(&[], &operations, END).unwrap();
        let holding = &replay.holdings["AAAA3"];
        assert_eq!(holding.quantity, dec!(110));
        assert_eq!(holding.total_cost, dec!(2045));
    }

    #[test]
    fn day_trade_matches_first_buy_with_first_sell() {
        // P&R 705: primeiro negócio de compra com o primeiro de venda.
        let day = date!(2025 - 04 - 07);
        let operations = [
            buy(day, "AAAA3", Instrument::Stock, dec!(100), dec!(10)),
            buy(day, "AAAA3", Instrument::Stock, dec!(100), dec!(11)),
            sell(day, "AAAA3", Instrument::Stock, dec!(150), dec!(12)),
        ];
        let replay = replay(&[], &operations, END).unwrap();
        let day_trades: Vec<_> = replay.realized.iter().filter(|r| r.day_trade).collect();
        assert_eq!(day_trades.len(), 2);
        let result: Decimal = day_trades.iter().map(|r| r.result()).sum();
        // 150 × 12 − (100 × 10 + 50 × 11) = 250
        assert_eq!(result, dec!(250));
        // Sobram 50 a R$ 11 como compra comum.
        let holding = &replay.holdings["AAAA3"];
        assert_eq!(holding.quantity, dec!(50));
        assert_eq!(holding.average_price(), dec!(11));
    }

    #[test]
    fn day_trade_ignores_the_previous_stock() {
        // IN RFB 1.585/2015, art. 65, par. 2: o estoque anterior não entra.
        let openings = [OpeningPosition {
            symbol: "AAAA3".to_owned(),
            instrument: Instrument::Stock,
            quantity: dec!(100),
            total_cost: dec!(1000),
        }];
        let day = date!(2025 - 04 - 07);
        let operations = [
            sell(day, "AAAA3", Instrument::Stock, dec!(100), dec!(13)),
            buy(day, "AAAA3", Instrument::Stock, dec!(100), dec!(12)),
        ];
        let replay = replay(&openings, &operations, END).unwrap();
        assert_eq!(replay.realized.len(), 1);
        assert!(replay.realized[0].day_trade);
        assert_eq!(replay.realized[0].result(), dec!(100));
        assert_eq!(replay.holdings["AAAA3"].average_price(), dec!(10));
    }

    #[test]
    fn day_trade_is_per_broker() {
        let day = date!(2025 - 04 - 07);
        let openings = [OpeningPosition {
            symbol: "AAAA3".to_owned(),
            instrument: Instrument::Stock,
            quantity: dec!(100),
            total_cost: dec!(1000),
        }];
        let mut purchase = buy(day, "AAAA3", Instrument::Stock, dec!(100), dec!(12));
        purchase.broker = Some("A".to_owned());
        let mut sale = sell(day, "AAAA3", Instrument::Stock, dec!(100), dec!(13));
        sale.broker = Some("B".to_owned());
        let replay = replay(&openings, &[purchase, sale], END).unwrap();
        assert_eq!(replay.realized.len(), 1);
        assert!(!replay.realized[0].day_trade);
        // Compra antes da venda (ordem da lista): PM (1000 + 1200) / 200 = 11.
        assert_eq!(replay.realized[0].cost, dec!(1100));
    }

    #[test]
    fn day_trade_fees_are_split_by_quantity() {
        let day = date!(2025 - 04 - 07);
        let operations = [
            with_fees(
                buy(day, "AAAA3", Instrument::Stock, dec!(100), dec!(10)),
                dec!(10),
            ),
            with_fees(
                sell(day, "AAAA3", Instrument::Stock, dec!(40), dec!(11)),
                dec!(4),
            ),
        ];
        let replay = replay(&[], &operations, END).unwrap();
        let trade = &replay.realized[0];
        assert!(trade.day_trade);
        assert_eq!(trade.fees, dec!(4) + dec!(4));
        // As 60 que sobram ficam com R$ 6 de taxa no custo.
        assert_eq!(replay.holdings["AAAA3"].total_cost, dec!(606));
    }

    #[test]
    fn crypto_has_no_day_trade() {
        let day = date!(2025 - 04 - 07);
        let operations = [
            buy(day, "BTC", Instrument::Crypto, dec!(1), dec!(300000)),
            sell(day, "BTC", Instrument::Crypto, dec!(0.5), dec!(310000)),
        ];
        let replay = replay(&[], &operations, END).unwrap();
        assert!(!replay.realized[0].day_trade);
        assert_eq!(replay.realized[0].result(), dec!(5000));
    }

    #[test]
    fn selling_more_than_held_is_an_error() {
        let operations = [
            buy(
                date!(2025 - 01 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(10),
                dec!(10),
            ),
            sell(
                date!(2025 - 01 - 11),
                "AAAA3",
                Instrument::Stock,
                dec!(11),
                dec!(10),
            ),
        ];
        assert_eq!(
            replay(&[], &operations, END),
            Err(TaxError::SellExceedsPosition {
                symbol: "AAAA3".to_owned(),
                date: date!(2025 - 01 - 11)
            })
        );
    }

    #[test]
    fn invalid_operations_are_rejected() {
        let day = date!(2025 - 01 - 10);
        let zero = buy(day, "AAAA3", Instrument::Stock, dec!(0), dec!(10));
        assert!(matches!(
            replay(&[], &[zero], END),
            Err(TaxError::InvalidOperation { .. })
        ));
        let negative_fee = with_fees(
            buy(day, "AAAA3", Instrument::Stock, dec!(1), dec!(10)),
            dec!(-1),
        );
        assert!(matches!(
            replay(&[], &[negative_fee], END),
            Err(TaxError::InvalidOperation { .. })
        ));
        let crypto_withholding = with_withheld(
            sell(day, "BTC", Instrument::Crypto, dec!(1), dec!(10)),
            dec!(1),
        );
        assert!(matches!(
            replay(&[], &[crypto_withholding], END),
            Err(TaxError::InvalidOperation { .. })
        ));
        let split_without_position = Operation {
            date: day,
            symbol: "AAAA3".to_owned(),
            instrument: Instrument::Stock,
            broker: None,
            action: Action::Split { factor: dec!(2) },
        };
        assert!(matches!(
            replay(&[], &[split_without_position], END),
            Err(TaxError::InvalidOperation { .. })
        ));
        let bad_factor = Operation {
            date: day,
            symbol: "AAAA3".to_owned(),
            instrument: Instrument::Stock,
            broker: None,
            action: Action::Split { factor: dec!(0) },
        };
        assert!(matches!(
            replay(&[], &[bad_factor], END),
            Err(TaxError::InvalidOperation { .. })
        ));
        let bad_bonus = Operation {
            date: day,
            symbol: "AAAA3".to_owned(),
            instrument: Instrument::Stock,
            broker: None,
            action: Action::Bonus {
                quantity: dec!(1),
                unit_cost: dec!(-1),
            },
        };
        assert!(matches!(
            replay(&[], &[bad_bonus], END),
            Err(TaxError::InvalidOperation { .. })
        ));
        let negative_opening = OpeningPosition {
            symbol: "AAAA3".to_owned(),
            instrument: Instrument::Stock,
            quantity: dec!(-1),
            total_cost: dec!(0),
        };
        assert!(matches!(
            replay(&[negative_opening], &[], END),
            Err(TaxError::InvalidOperation { .. })
        ));
    }

    #[test]
    fn one_symbol_cannot_have_two_instruments() {
        let day = date!(2025 - 01 - 10);
        let operations = [
            buy(day, "XXXX11", Instrument::RealEstateFund, dec!(1), dec!(10)),
            buy(day, "XXXX11", Instrument::Etf, dec!(1), dec!(10)),
        ];
        assert_eq!(
            replay(&[], &operations, END),
            Err(TaxError::InstrumentMismatch {
                symbol: "XXXX11".to_owned()
            })
        );
    }

    #[test]
    fn operations_after_the_cutoff_are_ignored_and_closed_positions_dropped() {
        let operations = [
            buy(
                date!(2025 - 01 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(10),
                dec!(10),
            ),
            sell(
                date!(2025 - 02 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(10),
                dec!(12),
            ),
            buy(
                date!(2026 - 01 - 10),
                "BBBB3",
                Instrument::Stock,
                dec!(10),
                dec!(10),
            ),
        ];
        let replay = replay(&[], &operations, date!(2025 - 12 - 31)).unwrap();
        assert!(replay.holdings.is_empty());
        assert_eq!(replay.realized.len(), 1);
        assert_eq!(
            Holding {
                symbol: "Z".to_owned(),
                instrument: Instrument::Stock,
                quantity: Decimal::ZERO,
                total_cost: Decimal::ZERO,
            }
            .average_price(),
            Decimal::ZERO
        );
    }

    #[test]
    fn withholding_is_recorded_per_sale() {
        let operations = [
            buy(
                date!(2025 - 01 - 10),
                "AAAA3",
                Instrument::Stock,
                dec!(10),
                dec!(10),
            ),
            with_withheld(
                sell(
                    date!(2025 - 02 - 10),
                    "AAAA3",
                    Instrument::Stock,
                    dec!(10),
                    dec!(12),
                ),
                dec!(0.01),
            ),
        ];
        let replay = replay(&[], &operations, END).unwrap();
        assert_eq!(replay.withholdings.len(), 1);
        assert_eq!(replay.withholdings[0].amount, dec!(0.01));
    }
}
