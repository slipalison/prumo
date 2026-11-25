//! Caixa: o dinheiro que a carteira gerou e a pessoa ainda não reaplicou.
//!
//! **Entra** o provento líquido recebido (`pay_date` até hoje) e o valor
//! líquido das vendas e resgates do livro (preço × quantidade − taxas). **Sai**
//! o que cada aporte registrado consumiu do caixa. O saldo é *calculado*, não
//! guardado: provento e lançamento do livro podem ser editados ou apagados, e
//! uma tabela de saldo ficaria errada na primeira edição.
//!
//! O cálculo é um replay em ordem de data, e o saldo **nunca fica negativo**: se
//! uma saída passa do que havia (porque um provento que a sustentava foi
//! apagado depois, por exemplo), o saldo vai a zero e a diferença fica
//! registrada em `adjustment`, em vez de comer as entradas futuras. Num dia,
//! as entradas vêm antes das saídas: o provento que cai no dia do aporte já
//! estava no caixa quando a pessoa aportou.
//!
//! Só conta o que é de `since` em diante (o caixa não herda o passado). O
//! motor do aporte não sabe de nada disso: o caixa só muda o *valor de
//! entrada* que a pessoa digita.
//!
//! **Dinheiro de fora.** O aporte tem um valor (`amount`) e uma parte que veio
//! do caixa (`cash_used`, no máximo o saldo). A diferença é dinheiro novo da
//! pessoa e **não passa** pelo caixa: não entra, não sai, e o saldo não
//! muda. Aportar mais do que o caixa, ou aportar sem tocar nele, é sempre
//! válido.

use rust_decimal::Decimal;
use time::Date;

use super::allocation::round_cents;
use super::ledger::{Entry, Kind};

/// De onde veio, ou para onde foi, um movimento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementKind {
    /// Provento líquido recebido (entra).
    Income,
    /// Venda ou resgate do livro, líquido de taxas (entra).
    Sale,
    /// Parte do aporte que saiu do caixa (sai).
    Contribution,
}

impl MovementKind {
    pub fn is_inflow(self) -> bool {
        matches!(self, MovementKind::Income | MovementKind::Sale)
    }
}

/// Um movimento de caixa. `amount` é sempre positivo; o sentido vem do tipo.
#[derive(Debug, Clone, PartialEq)]
pub struct Movement {
    pub date: Date,
    pub kind: MovementKind,
    pub amount: Decimal,
    /// O que a tela mostra ("KNRI11", "Aporte").
    pub label: String,
}

/// O que o caixa tem e de onde veio.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CashState {
    pub balance: Decimal,
    /// Soma dos proventos líquidos que entraram.
    pub income_in: Decimal,
    pub income_count: usize,
    /// Soma do líquido das vendas que entrou.
    pub sales_in: Decimal,
    pub sales_count: usize,
    /// Soma do que os aportes tiraram do caixa.
    pub contributions_out: Decimal,
    /// Saída que passou do saldo e foi cortada em zero (normalmente `0`).
    /// `balance = income_in + sales_in − contributions_out + adjustment`.
    pub adjustment: Decimal,
}

/// O líquido de uma venda ou resgate do livro, em reais: preço × quantidade −
/// taxas (na moeda do lançamento, convertidas pelo câmbio do dia). `None` nos
/// outros tipos. Venda com taxa maior que o valor rende zero, nunca negativo.
///
/// A amortização do livro não entra: ela devolve capital que a pessoa lança
/// também como provento (`kind = amortization`), e contar as duas dobraria o
/// dinheiro.
pub fn sale_proceeds(entry: &Entry) -> Option<Decimal> {
    if !matches!(entry.kind, Kind::Sell | Kind::Redemption) {
        return None;
    }
    let gross = entry
        .quantity
        .checked_mul(entry.unit_price?)?
        .checked_mul(entry.fx_rate)?;
    let fees = entry.fees.checked_mul(entry.fx_rate)?;
    Some(round_cents(gross.checked_sub(fees)?.max(Decimal::ZERO)))
}

/// Refaz o caixa dos movimentos de `since` em diante.
pub fn replay(movements: &[Movement], since: Date) -> CashState {
    let mut ordered: Vec<&Movement> = movements.iter().filter(|m| m.date >= since).collect();
    // Entradas antes das saídas no mesmo dia; `sort_by_key` é estável.
    ordered.sort_by_key(|m| (m.date, !m.kind.is_inflow()));
    let mut state = CashState::default();
    for movement in ordered {
        match movement.kind {
            MovementKind::Income => {
                state.balance += movement.amount;
                state.income_in += movement.amount;
                state.income_count += 1;
            }
            MovementKind::Sale => {
                state.balance += movement.amount;
                state.sales_in += movement.amount;
                state.sales_count += 1;
            }
            MovementKind::Contribution => {
                state.contributions_out += movement.amount;
                if movement.amount > state.balance {
                    state.adjustment += movement.amount - state.balance;
                    state.balance = Decimal::ZERO;
                } else {
                    state.balance -= movement.amount;
                }
            }
        }
    }
    state
}

/// Por que um aporte não pode tirar `cash_used` do caixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashUseError {
    /// Mais que o valor do aporte: o resto do aporte é dinheiro de fora, mas o
    /// caixa não pode valer mais do que o aporte inteiro.
    AboveAmount,
    /// Mais que o saldo na hora do registro.
    AboveBalance,
}

/// Confere o caixa que um aporte quer consumir: não passa do valor do aporte
/// nem do saldo (que nunca fica negativo). A parte do aporte acima do caixa
/// é dinheiro de fora e não precisa de conferência.
pub fn check_use(
    amount: Decimal,
    cash_used: Decimal,
    balance: Decimal,
) -> Result<(), CashUseError> {
    if cash_used > amount {
        return Err(CashUseError::AboveAmount);
    }
    if cash_used > balance {
        return Err(CashUseError::AboveBalance);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn movement(date: Date, kind: MovementKind, amount: Decimal) -> Movement {
        Movement {
            date,
            kind,
            amount,
            label: "X".to_owned(),
        }
    }

    const START: Date = date!(2026 - 01 - 01);

    #[test]
    fn income_and_sales_come_in_and_contributions_go_out() {
        let state = replay(
            &[
                movement(date!(2026 - 02 - 01), MovementKind::Income, dec!(100)),
                movement(date!(2026 - 02 - 05), MovementKind::Sale, dec!(250.50)),
                movement(date!(2026 - 02 - 10), MovementKind::Contribution, dec!(300)),
                movement(date!(2026 - 02 - 12), MovementKind::Income, dec!(20)),
            ],
            START,
        );
        assert_eq!(state.balance, dec!(70.50));
        assert_eq!(state.income_in, dec!(120));
        assert_eq!((state.income_count, state.sales_count), (2, 1));
        assert_eq!(state.sales_in, dec!(250.50));
        assert_eq!(state.contributions_out, dec!(300));
        assert_eq!(state.adjustment, dec!(0));
    }

    #[test]
    fn what_came_before_since_is_not_cash() {
        let state = replay(
            &[
                movement(date!(2025 - 12 - 31), MovementKind::Income, dec!(900)),
                movement(date!(2026 - 01 - 01), MovementKind::Income, dec!(10)),
            ],
            START,
        );
        assert_eq!(state.balance, dec!(10));
        assert_eq!(state.income_count, 1);
    }

    #[test]
    fn inflows_of_the_day_are_there_when_the_contribution_happens() {
        // O mesmo dia, entrada lançada depois da saída: o replay põe a entrada antes.
        let day = date!(2026 - 03 - 10);
        let state = replay(
            &[
                movement(day, MovementKind::Contribution, dec!(100)),
                movement(day, MovementKind::Income, dec!(100)),
            ],
            START,
        );
        assert_eq!(state.balance, dec!(0));
        assert_eq!(state.adjustment, dec!(0));
    }

    #[test]
    fn balance_never_goes_negative_and_later_inflows_still_count() {
        // O provento de 100 que sustentava o aporte foi apagado: a saída de 100
        // passa do saldo (0). Vira ajuste, e o provento seguinte vale inteiro.
        let state = replay(
            &[
                movement(date!(2026 - 02 - 10), MovementKind::Contribution, dec!(100)),
                movement(date!(2026 - 03 - 01), MovementKind::Income, dec!(50)),
            ],
            START,
        );
        assert_eq!(state.balance, dec!(50));
        assert_eq!(state.adjustment, dec!(100));
        assert_eq!(
            state.balance,
            state.income_in + state.sales_in - state.contributions_out + state.adjustment
        );
    }

    #[test]
    fn using_the_whole_cash_zeroes_it_and_outside_money_does_not_touch_it() {
        // Aporte de 1000 com 200 de caixa: o caixa zera. Outro aporte de 500 só
        // com dinheiro de fora (cash_used = 0) nem aparece nos movimentos.
        let state = replay(
            &[
                movement(date!(2026 - 02 - 01), MovementKind::Income, dec!(200)),
                movement(date!(2026 - 02 - 02), MovementKind::Contribution, dec!(200)),
            ],
            START,
        );
        assert_eq!(state.balance, dec!(0));
        assert_eq!(state.contributions_out, dec!(200));
    }

    #[test]
    fn check_use_enforces_the_amount_and_the_balance() {
        assert_eq!(check_use(dec!(1000), dec!(0), dec!(0)), Ok(()));
        assert_eq!(check_use(dec!(1000), dec!(200), dec!(200)), Ok(()));
        // Dinheiro de fora: o aporte é maior que o caixa usado.
        assert_eq!(check_use(dec!(1000), dec!(150), dec!(200)), Ok(()));
        assert_eq!(
            check_use(dec!(100), dec!(150), dec!(500)),
            Err(CashUseError::AboveAmount)
        );
        assert_eq!(
            check_use(dec!(1000), dec!(200.01), dec!(200)),
            Err(CashUseError::AboveBalance)
        );
    }

    fn sell(quantity: Decimal, price: Decimal, fees: Decimal, fx: Decimal) -> Entry {
        Entry {
            kind: Kind::Sell,
            trade_date: date!(2026 - 02 - 01),
            seq: 1,
            quantity,
            unit_price: Some(price),
            fees,
            fx_rate: fx,
        }
    }

    #[test]
    fn sale_proceeds_are_net_of_fees_and_in_reais() {
        assert_eq!(
            sale_proceeds(&sell(dec!(10), dec!(25.50), dec!(1.25), dec!(1))),
            Some(dec!(253.75))
        );
        // Em dólar: preço e taxa × câmbio.
        assert_eq!(
            sale_proceeds(&sell(dec!(2), dec!(100), dec!(1), dec!(5))),
            Some(dec!(995))
        );
        // Taxa maior que o valor: zero, nunca negativo.
        assert_eq!(
            sale_proceeds(&sell(dec!(1), dec!(1), dec!(5), dec!(1))),
            Some(dec!(0))
        );
    }

    #[test]
    fn only_sales_and_redemptions_bring_cash() {
        let mut e = sell(dec!(1), dec!(10), dec!(0), dec!(1));
        for kind in Kind::ALL {
            e.kind = kind;
            let expected = matches!(kind, Kind::Sell | Kind::Redemption);
            assert_eq!(sale_proceeds(&e).is_some(), expected, "{kind:?}");
        }
        // Resgate de renda fixa por valor: quantidade é o valor, preço 1.
        e.kind = Kind::Redemption;
        e.quantity = dec!(300);
        e.unit_price = Some(dec!(1));
        assert_eq!(sale_proceeds(&e), Some(dec!(300)));
    }

    fn arbitrary_movements() -> impl Strategy<Value = Vec<Movement>> {
        prop::collection::vec((0u32..40, 0u8..3, 0i64..100_000), 0..40).prop_map(|raw| {
            raw.into_iter()
                .map(|(day, kind, cents)| {
                    let kind = match kind {
                        0 => MovementKind::Income,
                        1 => MovementKind::Sale,
                        _ => MovementKind::Contribution,
                    };
                    movement(
                        START.saturating_add(time::Duration::days(i64::from(day))),
                        kind,
                        Decimal::new(cents, 2),
                    )
                })
                .collect()
        })
    }

    proptest! {
        #[test]
        fn balance_is_never_negative_and_always_adds_up(movements in arbitrary_movements()) {
            let state = replay(&movements, START);
            prop_assert!(state.balance >= Decimal::ZERO);
            prop_assert!(state.adjustment >= Decimal::ZERO);
            prop_assert_eq!(
                state.balance,
                state.income_in + state.sales_in - state.contributions_out + state.adjustment
            );
        }

        #[test]
        fn input_order_does_not_matter(movements in arbitrary_movements()) {
            let mut reversed = movements.clone();
            reversed.reverse();
            prop_assert_eq!(replay(&movements, START), replay(&reversed, START));
        }
    }
}
