//! Desempenho por classe: o peso de cada classe no patrimônio e a
//! rentabilidade dela no período, ao lado do índice que serve de referência
//! para a classe (IFIX para FIIs, Ibovespa para Ações Nacionais, CDI para
//! Renda Fixa). A regra está em [`crate::domain::class_returns`]; o método, em
//! `docs/desempenho.md` ("Por classe").
//!
//! A janela é a mesma da rentabilidade da carteira no período (o fechamento
//! de base até o último ponto, que pode ser o de hoje, parcial). A série de
//! cada classe é refeita na hora, do livro e dos preços guardados: nada é
//! gravado.

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::{accrue, load_indexes, local_date, series, window};
use crate::domain::asset_class::AssetClass;
use crate::domain::class_returns::{self, ClassReturn};
use crate::domain::performance::{Accrued, Period, Window};
use crate::history::IndexCode;
use crate::store::{Store, StoreError};

/// O índice que a tela põe ao lado de cada classe. `None`: o Prumo não tem um
/// índice que represente a classe (ações e REITs dos EUA, cripto, renda fixa
/// internacional).
pub fn reference_index(class: AssetClass) -> Option<IndexCode> {
    match class {
        AssetClass::BrStocks => Some(IndexCode::Ibov),
        AssetClass::BrReits => Some(IndexCode::Ifix),
        AssetClass::FixedIncome => Some(IndexCode::Cdi),
        AssetClass::IntlStocks
        | AssetClass::UsReits
        | AssetClass::Crypto
        | AssetClass::IntlFixedIncome => None,
    }
}

/// Uma classe no período.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassRow {
    pub ret: ClassReturn,
    /// Fração do patrimônio no fim do período.
    pub weight: Decimal,
    /// O índice de referência na mesma janela da classe.
    pub benchmark: Option<(IndexCode, Accrued)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassesReport {
    pub period: Period,
    /// A janela da carteira; `None` sem série.
    pub window: Option<Window>,
    pub classes: Vec<ClassRow>,
}

/// As classes da carteira no período, na ordem da interface.
pub async fn class_report(
    store: &Store,
    user: Uuid,
    period: Period,
    now: OffsetDateTime,
) -> Result<ClassesReport, StoreError> {
    let today = local_date(now);
    let points = series(store, user, now).await?;
    let Some(window) = window(&points, period, today) else {
        return Ok(ClassesReport {
            period,
            window: None,
            classes: Vec::new(),
        });
    };
    let assets = super::asset_histories(store, user, window.from, window.to, now).await?;
    let income = store
        .received_income_by_asset(user, window.from, window.to)
        .await?;
    let returns = class_returns::class_returns(&assets, &income, window.from, window.to);
    let codes: Vec<IndexCode> = returns
        .iter()
        .filter_map(|r| reference_index(r.class))
        .collect();
    let indexes = load_indexes(store, &codes, &points, today).await?;
    let weights = class_returns::weights(&returns);
    let classes = returns
        .iter()
        .zip(weights)
        .map(|(ret, weight)| ClassRow {
            ret: *ret,
            weight,
            benchmark: reference_index(ret.class).map(|code| {
                let values = indexes.get(&code).map_or(&[][..], Vec::as_slice);
                (code, benchmark_window(code, values, ret.from, ret.to))
            }),
        })
        .collect();
    Ok(ClassesReport {
        period,
        window: Some(window),
        classes,
    })
}

/// O índice na janela da classe. Classe que ainda não andou (base e fim no
/// mesmo dia) não tem com o que comparar.
fn benchmark_window(code: IndexCode, values: &[(Date, Decimal)], from: Date, to: Date) -> Accrued {
    if from >= to {
        return Accrued {
            ret: Decimal::ZERO,
            as_of: None,
        };
    }
    accrue(code, values, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    #[test]
    fn each_class_has_its_reference() {
        assert_eq!(reference_index(AssetClass::BrReits), Some(IndexCode::Ifix));
        assert_eq!(reference_index(AssetClass::BrStocks), Some(IndexCode::Ibov));
        assert_eq!(
            reference_index(AssetClass::FixedIncome),
            Some(IndexCode::Cdi)
        );
        for class in [
            AssetClass::IntlStocks,
            AssetClass::UsReits,
            AssetClass::Crypto,
            AssetClass::IntlFixedIncome,
        ] {
            assert_eq!(reference_index(class), None);
        }
    }

    #[test]
    fn a_class_that_did_not_move_has_no_benchmark() {
        let ifix = [
            (date!(2026 - 11 - 23), dec!(3440.08)),
            (date!(2026 - 11 - 26), dec!(3578.98)),
        ];
        let same = benchmark_window(
            IndexCode::Ifix,
            &ifix,
            date!(2026 - 11 - 26),
            date!(2026 - 11 - 26),
        );
        assert_eq!(same.as_of, None);
        let moved = benchmark_window(
            IndexCode::Ifix,
            &ifix,
            date!(2026 - 11 - 23),
            date!(2026 - 11 - 26),
        );
        assert_eq!(moved.ret, dec!(3578.98) / dec!(3440.08) - Decimal::ONE);
        assert_eq!(moved.as_of, Some(date!(2026 - 11 - 26)));
    }
}
