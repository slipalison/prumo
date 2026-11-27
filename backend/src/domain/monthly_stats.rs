//! Ano a ano: o melhor e o pior mês da série, quantos meses subiram e
//! quantos caíram, e em quantos a carteira ficou acima de um índice. Regra
//! pura sobre os meses de [`super::performance::monthly_cells`]; o método está
//! em `docs/desempenho.md` ("Ano a ano").

use rust_decimal::Decimal;
use time::Month;

use super::performance::MonthCell;

/// Um mês e a rentabilidade dele.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthMark {
    pub year: i32,
    pub month: Month,
    pub ret: Decimal,
}

impl From<&MonthCell> for MonthMark {
    fn from(cell: &MonthCell) -> Self {
        Self {
            year: cell.year,
            month: cell.month,
            ret: cell.ret,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonthStats {
    /// O mês de maior rentabilidade (no empate, o mais antigo).
    pub best: Option<MonthMark>,
    /// O de menor rentabilidade (no empate, o mais antigo).
    pub worst: Option<MonthMark>,
    pub positive: usize,
    pub negative: usize,
    /// Meses que ficaram exatamente no zero (a cota não andou).
    pub flat: usize,
}

/// As contas sobre todos os meses da série, inclusive o mês corrente (até
/// hoje).
pub fn month_stats(cells: &[MonthCell]) -> MonthStats {
    let mut stats = MonthStats::default();
    for cell in cells {
        match cell.ret.cmp(&Decimal::ZERO) {
            std::cmp::Ordering::Greater => stats.positive += 1,
            std::cmp::Ordering::Less => stats.negative += 1,
            std::cmp::Ordering::Equal => stats.flat += 1,
        }
        if stats.best.is_none_or(|b| cell.ret > b.ret) {
            stats.best = Some(cell.into());
        }
        if stats.worst.is_none_or(|w| cell.ret < w.ret) {
            stats.worst = Some(cell.into());
        }
    }
    stats
}

/// Em quantos meses a carteira rendeu mais que o índice, e em quantos dava
/// para comparar (o índice tinha dado publicado no mês). `index[i]` é o índice
/// na mesma janela de `cells[i]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    pub above: usize,
    pub compared: usize,
}

pub fn months_above(cells: &[MonthCell], index: &[Option<Decimal>]) -> Beat {
    let mut beat = Beat {
        above: 0,
        compared: 0,
    };
    for (cell, value) in cells.iter().zip(index) {
        if let Some(value) = value {
            beat.compared += 1;
            if cell.ret > *value {
                beat.above += 1;
            }
        }
    }
    beat
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn cell(year: i32, month: Month, ret: Decimal) -> MonthCell {
        MonthCell {
            year,
            month,
            from: date!(2025 - 01 - 01),
            to: date!(2025 - 01 - 31),
            ret,
            accumulated: Decimal::ZERO,
        }
    }

    #[test]
    fn best_worst_and_counts() {
        let cells = [
            cell(2025, Month::November, dec!(0.012)),
            cell(2025, Month::December, dec!(-0.004)),
            cell(2026, Month::January, dec!(0.031)),
            cell(2026, Month::February, Decimal::ZERO),
            cell(2026, Month::March, dec!(0.031)),
            cell(2026, Month::April, dec!(-0.020)),
        ];
        let stats = month_stats(&cells);
        assert_eq!(
            stats.best,
            Some(MonthMark {
                year: 2026,
                month: Month::January,
                ret: dec!(0.031)
            })
        );
        assert_eq!(stats.worst.map(|w| w.month), Some(Month::April));
        assert_eq!((stats.positive, stats.negative, stats.flat), (3, 2, 1));
        assert_eq!(month_stats(&[]), MonthStats::default());

        // CDI de 1% no mês: acima em janeiro e março; dezembro sem dado.
        let cdi = [
            Some(dec!(0.01)),
            None,
            Some(dec!(0.01)),
            Some(dec!(0.01)),
            Some(dec!(0.01)),
            Some(dec!(0.01)),
        ];
        assert_eq!(
            months_above(&cells, &cdi),
            Beat {
                above: 3,
                compared: 5
            }
        );
    }

    proptest! {
        /// Todo mês cai em uma das três contas, e o melhor não é menor que o pior.
        #[test]
        fn counts_cover_every_month(returns in prop::collection::vec(-5_000i64..5_000, 0..40)) {
            let cells: Vec<MonthCell> = returns
                .iter()
                .map(|r| cell(2026, Month::January, Decimal::new(*r, 4)))
                .collect();
            let stats = month_stats(&cells);
            prop_assert_eq!(stats.positive + stats.negative + stats.flat, cells.len());
            if let (Some(best), Some(worst)) = (stats.best, stats.worst) {
                prop_assert!(best.ret >= worst.ret);
                prop_assert!(cells.iter().all(|c| c.ret <= best.ret && c.ret >= worst.ret));
            }
            let all = vec![Some(Decimal::ZERO); cells.len()];
            let beat = months_above(&cells, &all);
            prop_assert_eq!(beat.compared, cells.len());
            prop_assert_eq!(beat.above, stats.positive);
        }
    }
}
