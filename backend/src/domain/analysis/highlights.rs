//! Destaques de 12 meses: quem mais subiu e quem mais caiu, pelo **preço** em
//! 12 meses e pela **rentabilidade desde a compra** (preço de hoje contra o
//! preço médio do livro).
//!
//! São fatos do passado, não recomendação: a lista não diz comprar nem vender.
//! Só entra na lista de melhores quem subiu e na de piores quem caiu.

use rust_decimal::Decimal;
use time::Date;
use uuid::Uuid;

use crate::domain::allocation::round_cents;
use crate::domain::asset_class::AssetClass;
use crate::domain::ledger::{self, Entry, Kind};

/// Quantos entram em cada lista.
pub const TOP: usize = 3;

/// Um ativo e a variação dele, em percentual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mover {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    pub change_percent: Decimal,
}

/// As duas listas de uma medida.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ranking {
    /// Os que mais subiram, do maior para o menor; só quem subiu.
    pub best: Vec<Mover>,
    /// Os que mais caíram, do pior para o menos pior; só quem caiu.
    pub worst: Vec<Mover>,
}

/// Separa os [`TOP`] melhores e os [`TOP`] piores. O desempate é pelo nome, para
/// a saída ser estável.
pub fn rank(movers: Vec<Mover>) -> Ranking {
    let mut best: Vec<Mover> = movers
        .iter()
        .filter(|m| m.change_percent > Decimal::ZERO)
        .cloned()
        .collect();
    best.sort_by(|a, b| {
        b.change_percent
            .cmp(&a.change_percent)
            .then_with(|| a.label.cmp(&b.label))
    });
    best.truncate(TOP);
    let mut worst: Vec<Mover> = movers
        .into_iter()
        .filter(|m| m.change_percent < Decimal::ZERO)
        .collect();
    worst.sort_by(|a, b| {
        a.change_percent
            .cmp(&b.change_percent)
            .then_with(|| a.label.cmp(&b.label))
    });
    worst.truncate(TOP);
    Ranking { best, worst }
}

/// Variação em percentual de `base` para `now`, com duas casas. `None` com base
/// zero ou negativa.
pub fn change_percent(now: Decimal, base: Decimal) -> Option<Decimal> {
    (base > Decimal::ZERO).then(|| round_cents((now / base - Decimal::ONE) * Decimal::ONE_HUNDRED))
}

/// Quantas cotas novas passou a haver por cota antiga **depois** de `after`:
/// o produto das razões de quantidade dos desdobramentos, grupamentos e
/// bonificações do livro. O preço de antes do evento não é comparável ao de
/// hoje sem dividir por esse fator. `None` se o livro não fecha.
pub fn share_factor_after(entries: &[Entry], after: Date) -> Option<Decimal> {
    let steps = ledger::replay(entries).ok()?;
    let mut before = Decimal::ZERO;
    let mut factor = Decimal::ONE;
    for step in &steps {
        let after_quantity = step.position.quantity;
        let changes_shares = matches!(
            step.entry.kind,
            Kind::Split | Kind::ReverseSplit | Kind::Bonus
        );
        if changes_shares && step.entry.trade_date > after && before > Decimal::ZERO {
            factor = factor.checked_mul(after_quantity / before)?;
        }
        before = after_quantity;
    }
    Some(factor)
}

/// A variação do preço em 12 meses já ajustada por desdobramentos: `base` é o
/// fechamento de então (do dia, sem ajuste), `now` o preço de hoje.
pub fn price_change(now: Decimal, base: Decimal, factor: Decimal) -> Option<Decimal> {
    if factor <= Decimal::ZERO {
        return None;
    }
    change_percent(now, base / factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn mover(label: &str, change: Decimal) -> Mover {
        Mover {
            asset_id: Uuid::new_v4(),
            label: label.to_owned(),
            asset_class: AssetClass::BrStocks,
            change_percent: change,
        }
    }

    fn entry(kind: Kind, day: Date, seq: i64, quantity: Decimal, price: Option<Decimal>) -> Entry {
        Entry {
            kind,
            trade_date: day,
            seq,
            quantity,
            unit_price: price,
            fx_rate: Decimal::ONE,
            fees: Decimal::ZERO,
        }
    }

    #[test]
    fn best_are_the_risers_and_worst_the_fallers() {
        let ranking = rank(vec![
            mover("A", dec!(10)),
            mover("B", dec!(-5)),
            mover("C", dec!(30)),
            mover("D", dec!(0)),
            mover("E", dec!(-20)),
            mover("F", dec!(2)),
            mover("G", dec!(7)),
        ]);
        let labels = |list: &[Mover]| list.iter().map(|m| m.label.clone()).collect::<Vec<_>>();
        assert_eq!(labels(&ranking.best), ["C", "A", "G"]);
        assert_eq!(labels(&ranking.worst), ["E", "B"]);
    }

    #[test]
    fn ties_break_by_name() {
        let ranking = rank(vec![mover("B", dec!(5)), mover("A", dec!(5))]);
        assert_eq!(ranking.best[0].label, "A");
    }

    #[test]
    fn a_split_after_the_base_date_is_taken_out_of_the_price_change() {
        // 100 cotas compradas, desdobramento 1 para 2 em 01/03.
        let entries = [
            entry(
                Kind::Buy,
                date!(2025 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(40)),
            ),
            entry(Kind::Split, date!(2026 - 03 - 01), 2, dec!(100), None),
        ];
        let factor = share_factor_after(&entries, date!(2025 - 12 - 01)).unwrap();
        assert_eq!(factor, dec!(2));
        // Preço de 40 antes do desdobramento vale 20 de hoje; agora custa 22.
        assert_eq!(price_change(dec!(22), dec!(40), factor), Some(dec!(10)));
        // Sem o ajuste pareceria uma queda de 45%.
        assert_eq!(change_percent(dec!(22), dec!(40)), Some(dec!(-45)));
        // Base depois do evento: nada a ajustar.
        assert_eq!(
            share_factor_after(&entries, date!(2026 - 03 - 02)),
            Some(Decimal::ONE)
        );
    }

    #[test]
    fn a_reverse_split_and_a_bonus_move_the_factor_the_other_way() {
        let entries = [
            entry(
                Kind::Buy,
                date!(2025 - 01 - 10),
                1,
                dec!(100),
                Some(dec!(4)),
            ),
            entry(Kind::ReverseSplit, date!(2026 - 01 - 02), 2, dec!(90), None),
            entry(Kind::Bonus, date!(2026 - 02 - 02), 3, dec!(1), None),
        ];
        // 100 -> 10 (0,1) e 10 -> 11 (1,1).
        let factor = share_factor_after(&entries, date!(2025 - 06 - 01)).unwrap();
        assert_eq!(factor, dec!(0.11));
    }

    #[test]
    fn a_ledger_that_does_not_close_has_no_factor() {
        let broken = [entry(
            Kind::Sell,
            date!(2025 - 01 - 10),
            1,
            dec!(5),
            Some(dec!(4)),
        )];
        assert_eq!(share_factor_after(&broken, date!(2024 - 01 - 01)), None);
    }

    #[test]
    fn change_needs_a_positive_base() {
        assert_eq!(change_percent(dec!(10), dec!(0)), None);
        assert_eq!(change_percent(dec!(11), dec!(10)), Some(dec!(10)));
        assert_eq!(price_change(dec!(11), dec!(10), dec!(0)), None);
    }

    proptest! {
        /// Cada lista tem no máximo TOP, só do seu sinal, e ninguém está nas duas.
        #[test]
        fn the_lists_are_bounded_signed_and_disjoint(
            changes in proptest::collection::vec(-5000i64..5000, 0..20)
        ) {
            let movers: Vec<Mover> = changes
                .iter()
                .enumerate()
                .map(|(i, c)| mover(&format!("A{i:02}"), Decimal::new(*c, 2)))
                .collect();
            let ranking = rank(movers);
            prop_assert!(ranking.best.len() <= TOP && ranking.worst.len() <= TOP);
            prop_assert!(ranking.best.iter().all(|m| m.change_percent > Decimal::ZERO));
            prop_assert!(ranking.worst.iter().all(|m| m.change_percent < Decimal::ZERO));
            prop_assert!(ranking.best.windows(2).all(|w| w[0].change_percent >= w[1].change_percent));
            prop_assert!(ranking.worst.windows(2).all(|w| w[0].change_percent <= w[1].change_percent));
        }
    }
}
