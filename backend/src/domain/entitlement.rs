//! Quantas cotas davam direito a um provento: a posição do livro na **data
//! com**, e não a de hoje.
//!
//! A data com é o último dia em que comprar ainda dava direito ao pagamento; a
//! data ex é o primeiro dia sem direito. Então vale o que o livro tinha no fim
//! do dia anterior à data ex: lançamentos datados na data ex ou depois não
//! contam. Num provento sem data ex (lançamento manual sem ela), a melhor
//! referência é a data de pagamento.
//!
//! Duas conversões de quantidade:
//!
//! - [`quantity_on`]: as cotas do livro naquele dia, na base daquela época.
//!   É o que serve para o provento *lançado* (o valor por cota que a pessoa
//!   digita é o do extrato, na base da época).
//! - [`quantity_in_provider_shares`]: as mesmas cotas na base de hoje, depois
//!   dos desdobramentos, grupamentos e bonificações que vieram depois. O
//!   histórico por cota do Yahoo é **ajustado por desdobramento** (o dividendo
//!   antigo de uma ação que dobrou aparece pela metade), então a estimativa
//!   precisa multiplicar pelas cotas na mesma base, ou conta metade do que a
//!   pessoa recebeu. A bonificação o Yahoo trata como desdobramento: a de 2% da
//!   ITSA4 em 12/2025 aparece no `events=split` como `102:100`, e todo
//!   dividendo anterior vem dividido por 1,02 (medido em 27/11/2026 em ITSA4,
//!   ITUB4, BBDC4, RADL3, UNIP6, SBSP3 e EGIE3: todas as bonificações dos
//!   últimos 5 anos estão lá como desdobramento).
//!
//! **Saldo inicial.** O lançamento `opening` é "o que já existia antes do
//! livro": a data dele é a do cadastro, não a da compra. Para a quantidade na
//! data com ele conta desde sempre, senão todo provento anterior ao cadastro
//! ficaria sem cota nenhuma. Compras, vendas e ajustes valem na data deles.

use rust_decimal::Decimal;
use time::{Date, Duration};

use super::income::DividendPoint;
use super::ledger::{replay, Entry, Kind, LedgerError};

/// A janela da estimativa, a mesma de `income::trailing_per_unit`.
const TRAILING_DAYS: i64 = 365;

/// A data com de um provento: o dia anterior à data ex. Sem data ex, vale o dia
/// do pagamento (nunca depois de `today`: o livro não tem lançamento no futuro).
pub fn record_date(ex_date: Option<Date>, pay_date: Date, today: Date) -> Date {
    match ex_date {
        Some(ex) => ex.saturating_sub(Duration::days(1)),
        None => pay_date.min(today),
    }
}

/// O livro com o saldo inicial no começo dos tempos.
fn normalized(entries: &[Entry]) -> Vec<Entry> {
    entries
        .iter()
        .map(|e| {
            let mut entry = e.clone();
            if entry.kind == Kind::Opening {
                entry.trade_date = Date::MIN;
            }
            entry
        })
        .collect()
}

/// As cotas que o livro tinha no fim de `date`, na base daquela época.
pub fn quantity_on(entries: &[Entry], date: Date) -> Result<Decimal, LedgerError> {
    let upto: Vec<Entry> = normalized(entries)
        .into_iter()
        .filter(|e| e.trade_date <= date)
        .collect();
    Ok(replay(&upto)?
        .last()
        .map_or(Decimal::ZERO, |step| step.position.quantity))
}

/// Quantas vezes a quantidade foi multiplicada por desdobramentos e
/// bonificações (e dividida por grupamentos) depois de `after`: os eventos que
/// o provedor desconta do histórico por cota.
fn split_factor_after(entries: &[Entry], after: Date) -> Result<Decimal, LedgerError> {
    let steps = replay(&normalized(entries))?;
    let mut factor = Decimal::ONE;
    let mut held = Decimal::ZERO;
    for step in steps {
        let now = step.position.quantity;
        if step.entry.trade_date > after && held > Decimal::ZERO {
            if let Some(ratio) = match step.entry.kind {
                Kind::Split | Kind::ReverseSplit | Kind::Bonus => now.checked_div(held),
                _ => None,
            } {
                factor = factor.checked_mul(ratio).unwrap_or(factor);
            }
        }
        held = now;
    }
    Ok(factor)
}

/// As cotas com direito ao provento de data ex `ex_date`, na base do histórico
/// do provedor (a de hoje): a quantidade na data com, vezes os desdobramentos e
/// as bonificações posteriores à data ex.
pub fn quantity_in_provider_shares(
    entries: &[Entry],
    ex_date: Date,
) -> Result<Decimal, LedgerError> {
    let com = ex_date.saturating_sub(Duration::days(1));
    let held = quantity_on(entries, com)?;
    if held.is_zero() {
        return Ok(Decimal::ZERO);
    }
    let factor = split_factor_after(entries, ex_date)?;
    Ok(held.checked_mul(factor).unwrap_or(held))
}

/// O que a posição teria rendido nos últimos 365 dias (bruto, na moeda do
/// provedor): cada dividendo do histórico vezes as cotas que o livro tinha na
/// data com dele, na base do provedor. Dividendo de um dia em que a pessoa não
/// tinha cota não conta.
pub fn trailing_income(
    points: &[DividendPoint],
    today: Date,
    entries: &[Entry],
) -> Result<Decimal, LedgerError> {
    let since = today.saturating_sub(Duration::days(TRAILING_DAYS));
    let mut total = Decimal::ZERO;
    for point in points
        .iter()
        .filter(|p| p.ex_date > since && p.ex_date <= today)
    {
        let quantity = quantity_in_provider_shares(entries, point.ex_date)?;
        total += point.amount.checked_mul(quantity).unwrap_or(Decimal::ZERO);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn book() -> Vec<Entry> {
        vec![
            entry(
                Kind::Buy,
                date!(2026 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(10)),
            ),
            entry(
                Kind::Buy,
                date!(2026 - 03 - 10),
                2,
                dec!(50),
                Some(dec!(12)),
            ),
            entry(
                Kind::Sell,
                date!(2026 - 05 - 10),
                3,
                dec!(30),
                Some(dec!(14)),
            ),
        ]
    }

    #[test]
    fn record_date_is_the_day_before_the_ex_date() {
        let today = date!(2026 - 11 - 25);
        assert_eq!(
            record_date(Some(date!(2026 - 03 - 10)), date!(2026 - 03 - 20), today),
            date!(2026 - 03 - 09)
        );
        assert_eq!(
            record_date(None, date!(2026 - 03 - 20), today),
            date!(2026 - 03 - 20)
        );
        // Pagamento no futuro: o livro só vai até hoje.
        assert_eq!(record_date(None, date!(2026 - 12 - 20), today), today);
    }

    #[test]
    fn quantity_follows_the_book_on_the_date() {
        let book = book();
        assert_eq!(quantity_on(&book, date!(2026 - 01 - 09)).unwrap(), dec!(0));
        assert_eq!(
            quantity_on(&book, date!(2026 - 01 - 10)).unwrap(),
            dec!(100)
        );
        assert_eq!(
            quantity_on(&book, date!(2026 - 03 - 09)).unwrap(),
            dec!(100)
        );
        assert_eq!(
            quantity_on(&book, date!(2026 - 03 - 10)).unwrap(),
            dec!(150)
        );
        assert_eq!(
            quantity_on(&book, date!(2026 - 05 - 09)).unwrap(),
            dec!(150)
        );
        assert_eq!(
            quantity_on(&book, date!(2027 - 01 - 01)).unwrap(),
            dec!(120)
        );
    }

    #[test]
    fn a_purchase_on_the_ex_date_does_not_count() {
        // Comprou no dia da data ex: sem direito. Comprou na data com: com direito.
        let book = book();
        let ex = date!(2026 - 03 - 10);
        assert_eq!(quantity_in_provider_shares(&book, ex).unwrap(), dec!(100));
        assert_eq!(
            quantity_in_provider_shares(&book, ex.saturating_add(Duration::days(1))).unwrap(),
            dec!(150)
        );
    }

    #[test]
    fn opening_balance_counts_since_always() {
        // O saldo inicial é do cadastro (hoje), mas as cotas já existiam.
        let book = vec![
            entry(Kind::Opening, date!(2026 - 11 - 25), 1, dec!(40), None),
            entry(Kind::Buy, date!(2026 - 11 - 25), 2, dec!(10), Some(dec!(5))),
        ];
        assert_eq!(quantity_on(&book, date!(2024 - 06 - 01)).unwrap(), dec!(40));
        assert_eq!(quantity_on(&book, date!(2026 - 11 - 25)).unwrap(), dec!(50));
        // A compra de hoje não vale para um provento antigo.
        assert_eq!(
            quantity_in_provider_shares(&book, date!(2025 - 06 - 02)).unwrap(),
            dec!(40)
        );
    }

    #[test]
    fn split_before_the_record_date_is_already_in_the_quantity() {
        // 100 cotas, desdobramento 1:2 em março (mais 100), provento em junho: 200.
        let book = vec![
            entry(
                Kind::Buy,
                date!(2026 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(20)),
            ),
            entry(Kind::Split, date!(2026 - 03 - 01), 2, dec!(100), None),
        ];
        let ex = date!(2026 - 06 - 15);
        assert_eq!(
            quantity_on(&book, date!(2026 - 06 - 14)).unwrap(),
            dec!(200)
        );
        // Desdobramento anterior à data ex: o provedor não ajustou esse
        // provento, e o fator é 1.
        assert_eq!(quantity_in_provider_shares(&book, ex).unwrap(), dec!(200));
    }

    #[test]
    fn split_after_the_ex_date_converts_to_the_providers_basis() {
        // Provento em fevereiro com 100 cotas; o desdobramento 1:2 veio depois.
        // O Yahoo ajustou o valor por cota (metade): 100 cotas da época são
        // 200 cotas na base dele.
        let book = vec![
            entry(
                Kind::Buy,
                date!(2026 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(20)),
            ),
            entry(Kind::Split, date!(2026 - 03 - 01), 2, dec!(100), None),
        ];
        let ex = date!(2026 - 02 - 15);
        assert_eq!(
            quantity_on(&book, date!(2026 - 02 - 14)).unwrap(),
            dec!(100)
        );
        assert_eq!(quantity_in_provider_shares(&book, ex).unwrap(), dec!(200));
    }

    #[test]
    fn reverse_split_after_the_ex_date_shrinks_the_basis() {
        // 100 cotas, grupamento 10:1 (saem 90) depois do provento.
        let book = vec![
            entry(
                Kind::Buy,
                date!(2026 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(1)),
            ),
            entry(Kind::ReverseSplit, date!(2026 - 04 - 01), 2, dec!(90), None),
        ];
        assert_eq!(
            quantity_in_provider_shares(&book, date!(2026 - 02 - 15)).unwrap(),
            dec!(10)
        );
    }

    #[test]
    fn two_splits_multiply() {
        let book = vec![
            entry(
                Kind::Buy,
                date!(2026 - 01 - 10),
                1,
                dec!(10),
                Some(dec!(20)),
            ),
            entry(Kind::Split, date!(2026 - 02 - 01), 2, dec!(10), None),
            entry(Kind::Split, date!(2026 - 03 - 01), 3, dec!(60), None),
        ];
        // 10 → 20 (×2) → 80 (×4): na base de hoje, 10 cotas de janeiro são 80.
        assert_eq!(
            quantity_in_provider_shares(&book, date!(2026 - 01 - 20)).unwrap(),
            dec!(80)
        );
    }

    #[test]
    fn bonus_after_the_ex_date_converts_to_the_providers_basis() {
        // 100 cotas no provento de fevereiro; bonificação de 10% em março (mais
        // 10). O Yahoo dividiu o valor antigo por 1,1: as 100 cotas da época são
        // 110 na base dele, e 110 × (1,10 ÷ 1,1) devolve o que foi pago.
        let book = vec![
            entry(
                Kind::Buy,
                date!(2026 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(20)),
            ),
            entry(Kind::Bonus, date!(2026 - 03 - 01), 2, dec!(10), None),
        ];
        assert_eq!(
            quantity_in_provider_shares(&book, date!(2026 - 02 - 15)).unwrap(),
            dec!(110)
        );
        // Bonificação anterior à data ex já está na quantidade da época.
        assert_eq!(
            quantity_in_provider_shares(&book, date!(2026 - 03 - 15)).unwrap(),
            dec!(110)
        );
        let points = [DividendPoint {
            ex_date: date!(2026 - 02 - 15),
            amount: dec!(1),
        }];
        assert_eq!(
            trailing_income(&points, date!(2026 - 11 - 25), &book).unwrap(),
            dec!(110)
        );
    }

    #[test]
    fn trailing_income_uses_the_quantity_of_each_record_date() {
        let today = date!(2026 - 11 - 25);
        let points = [
            // Antes de comprar: sem direito.
            DividendPoint {
                ex_date: date!(2026 - 01 - 05),
                amount: dec!(1),
            },
            // Com 100 cotas.
            DividendPoint {
                ex_date: date!(2026 - 02 - 05),
                amount: dec!(1),
            },
            // Com 150 cotas.
            DividendPoint {
                ex_date: date!(2026 - 04 - 05),
                amount: dec!(2),
            },
            // Fora da janela de 365 dias.
            DividendPoint {
                ex_date: date!(2025 - 11 - 25),
                amount: dec!(9),
            },
        ];
        // 1×100 + 2×150, não 3 × 120 (a quantidade de hoje).
        assert_eq!(trailing_income(&points, today, &book()).unwrap(), dec!(400));
    }

    #[test]
    fn nothing_held_means_nothing_entitled() {
        assert_eq!(
            quantity_in_provider_shares(&book(), date!(2025 - 01 - 01)).unwrap(),
            dec!(0)
        );
        assert_eq!(quantity_on(&[], date!(2026 - 01 - 01)).unwrap(), dec!(0));
    }
}
