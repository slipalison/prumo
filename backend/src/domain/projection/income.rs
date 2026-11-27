//! A renda projetada dos próximos meses, por ativo e por tipo, em três
//! cenários. Parte da estimativa da tela Renda (o que cada cota pagou, líquido
//! do IR retido, vezes a quantidade de hoje) e a leva para a frente:
//!
//! - **FII de papel** (pelo balanço da CVM) acompanha o CDI esperado: a renda
//!   de cada mês é a dos 12 meses × `CDI do mês ÷ CDI médio dos 12 meses que
//!   passaram` — a renda dos últimos 12 meses foi paga com o CDI deles.
//! - **FII de tijolo** acompanha o reajuste dos aluguéis pelo IPCA esperado:
//!   a renda dos 12 meses × o IPCA acumulado até o mês. O reajuste real é uma
//!   vez por ano por contrato; numa carteira de contratos ele se espalha.
//! - **O resto** (ações, FII híbrido ou de fundos, exterior) fica na média
//!   do que pagou, sem crescimento: a do habitual (até 5 anos) quando há, os
//!   12 meses quando não.
//!
//! Cada mês recebe o peso que o mesmo mês teve nos últimos 12 ([`Shape`]): o
//! ativo que paga em maio e em novembro continua pagando em maio e em
//! novembro. O pessimista e o otimista saem do base por
//! `(1 ∓ amplitude)^(k/12)` no mês `k`: a amplitude é uma taxa ao ano, e a
//! distância entre os cenários cresce com o tempo.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;
use time::Date;
use uuid::Uuid;

use super::rates::RatePath;
use super::{growth, to_f64};
use crate::domain::asset_class::AssetClass;
use crate::domain::income::YearMonth;

/// O tipo de FII de papel e de tijolo, como `sectors::cvm` grava no setor.
pub const PAPER_SECTOR: &str = "Papel";
pub const BRICK_SECTOR: &str = "Tijolo";

/// O que move a renda de um ativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Driver {
    /// O CDI esperado.
    Cdi,
    /// O IPCA esperado.
    Ipca,
    /// Nada: a média, parada.
    Flat,
}

/// O grupo do ativo na projeção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    PaperReits,
    BrickReits,
    /// FII híbrido, de fundos ou ainda sem tipo.
    OtherReits,
    BrStocks,
    /// Ação, REIT, ETF ou BDR do exterior.
    Foreign,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::PaperReits,
        Group::BrickReits,
        Group::OtherReits,
        Group::BrStocks,
        Group::Foreign,
    ];

    pub fn driver(self) -> Driver {
        match self {
            Group::PaperReits => Driver::Cdi,
            Group::BrickReits => Driver::Ipca,
            Group::OtherReits | Group::BrStocks | Group::Foreign => Driver::Flat,
        }
    }

    /// Os FIIs partem dos 12 meses (o rendimento é mensal e acompanha o juro
    /// e o aluguel de agora); o resto, da média do habitual.
    pub fn prefers_usual(self) -> bool {
        matches!(self, Group::BrStocks | Group::Foreign)
    }

    /// O grupo de um ativo pela classe e pelo tipo de FII (o setor que a CVM
    /// dá). `None` para o que não paga provento (renda fixa, cripto).
    pub fn of(class: AssetClass, sector: Option<&str>) -> Option<Group> {
        match class {
            AssetClass::BrReits => Some(match sector {
                Some(PAPER_SECTOR) => Group::PaperReits,
                Some(BRICK_SECTOR) => Group::BrickReits,
                _ => Group::OtherReits,
            }),
            AssetClass::BrStocks => Some(Group::BrStocks),
            AssetClass::IntlStocks | AssetClass::UsReits => Some(Group::Foreign),
            AssetClass::Crypto | AssetClass::FixedIncome | AssetClass::IntlFixedIncome => None,
        }
    }
}

/// O peso de cada mês do calendário (janeiro a dezembro) no ano de um ativo,
/// com média 1: quem paga todo mês igual tem 1 em todos; quem paga só em
/// maio tem 12 em maio e 0 no resto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape([f64; 12]);

impl Default for Shape {
    fn default() -> Self {
        Self::uniform()
    }
}

impl Shape {
    pub fn uniform() -> Self {
        Self([1.0; 12])
    }

    /// A partir dos pagamentos (data e valor) dos últimos 12 meses. Sem
    /// pagamento nenhum, uniforme.
    pub fn from_payments(payments: impl IntoIterator<Item = (Date, f64)>) -> Self {
        let mut by_month = [0.0f64; 12];
        for (date, amount) in payments {
            if amount.is_finite() && amount > 0.0 {
                by_month[usize::from(u8::from(date.month())) - 1] += amount;
            }
        }
        Self::normalized(by_month)
    }

    /// A soma ponderada de várias formas: a forma da carteira, com o peso de
    /// cada ativo pela renda dele.
    pub fn combine(parts: impl IntoIterator<Item = (Shape, f64)>) -> Self {
        let mut by_month = [0.0f64; 12];
        for (shape, weight) in parts {
            if weight.is_finite() && weight > 0.0 {
                for (slot, w) in by_month.iter_mut().zip(shape.0) {
                    *slot += w * weight;
                }
            }
        }
        Self::normalized(by_month)
    }

    fn normalized(by_month: [f64; 12]) -> Self {
        let total: f64 = by_month.iter().sum();
        if total <= 0.0 || !total.is_finite() {
            return Self::uniform();
        }
        Self(by_month.map(|m| m * 12.0 / total))
    }

    /// O peso do mês (1 a 12).
    pub fn weight(&self, month: u8) -> f64 {
        self.0
            .get(usize::from(month.clamp(1, 12)) - 1)
            .copied()
            .unwrap_or(1.0)
    }

    /// O menor peso do ano: a fração da média que o pior mês paga.
    pub fn min(&self) -> f64 {
        self.0.iter().copied().fold(f64::INFINITY, f64::min)
    }

    pub fn weights(&self) -> [f64; 12] {
        self.0
    }
}

/// Um ativo na projeção.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetInput {
    pub asset_id: Uuid,
    pub group: Group,
    /// A renda líquida por mês com a quantidade de hoje: o ponto de partida.
    pub monthly: Decimal,
    pub shape: Shape,
}

/// O que a projeção usa além dos ativos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IncomePremises {
    /// Amplitude dos cenários, ao ano (fração).
    pub spread: f64,
    /// O CDI médio dos 12 meses que passaram, ao ano: a base do FII de papel.
    /// `None` (sem CDI no banco): o papel fica parado.
    pub cdi_trailing: Option<f64>,
}

/// Um mês projetado.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthProjection {
    pub month: YearMonth,
    pub low: f64,
    pub base: f64,
    pub high: f64,
    /// O base de cada grupo.
    pub by_group: BTreeMap<Group, f64>,
}

/// O período inteiro de um ativo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetProjection {
    pub asset_id: Uuid,
    pub low: f64,
    pub base: f64,
    pub high: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IncomeProjection {
    pub months: Vec<MonthProjection>,
    pub assets: Vec<AssetProjection>,
}

impl IncomeProjection {
    /// A soma do período em cada cenário.
    pub fn totals(&self) -> (f64, f64, f64) {
        self.months.iter().fold((0.0, 0.0, 0.0), |(l, b, h), m| {
            (l + m.low, b + m.base, h + m.high)
        })
    }
}

/// O fator do mês `k` (1 = o mês que vem) sobre a renda de partida.
pub fn driver_factor(driver: Driver, path: &RatePath, k: usize, cdi_trailing: Option<f64>) -> f64 {
    match driver {
        Driver::Flat => 1.0,
        Driver::Ipca => path.ipca_factor(k),
        Driver::Cdi => match (path.at(k).and_then(|m| m.cdi), cdi_trailing) {
            (Some(cdi), Some(trailing)) if trailing > 0.0 => (cdi / trailing).max(0.0),
            _ => 1.0,
        },
    }
}

/// A projeção do caminho inteiro, mês a mês e por ativo.
pub fn project(
    assets: &[AssetInput],
    path: &RatePath,
    premises: IncomePremises,
) -> IncomeProjection {
    let spread = premises.spread.max(0.0);
    let mut months: Vec<MonthProjection> = path
        .months
        .iter()
        .map(|m| MonthProjection {
            month: m.month,
            low: 0.0,
            base: 0.0,
            high: 0.0,
            by_group: BTreeMap::new(),
        })
        .collect();
    let mut totals = Vec::with_capacity(assets.len());
    for asset in assets {
        let start = to_f64(asset.monthly).max(0.0);
        let driver = asset.group.driver();
        let mut sum = AssetProjection {
            asset_id: asset.asset_id,
            low: 0.0,
            base: 0.0,
            high: 0.0,
        };
        for (i, slot) in months.iter_mut().enumerate() {
            let k = i + 1;
            let base = start
                * driver_factor(driver, path, k, premises.cdi_trailing)
                * asset.shape.weight(slot.month.month);
            let elapsed = k as f64;
            let low = base * growth(-spread, elapsed);
            let high = base * growth(spread, elapsed);
            slot.base += base;
            slot.low += low;
            slot.high += high;
            *slot.by_group.entry(asset.group).or_insert(0.0) += base;
            sum.base += base;
            sum.low += low;
            sum.high += high;
        }
        totals.push(sum);
    }
    IncomeProjection {
        months,
        assets: totals,
    }
}

#[cfg(test)]
mod tests {
    use super::super::rates::{rate_path, Focus, PathInputs};
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const TODAY: Date = date!(2026 - 11 - 27);

    fn path(cdi: f64, ipca: f64, months: u32) -> RatePath {
        rate_path(
            &Focus::default(),
            TODAY,
            months,
            PathInputs {
                cdi_informed: Some(cdi),
                ipca_informed: Some(ipca),
                ..PathInputs::default()
            },
        )
    }

    fn asset(group: Group, monthly: Decimal) -> AssetInput {
        AssetInput {
            asset_id: Uuid::new_v4(),
            group,
            monthly,
            shape: Shape::uniform(),
        }
    }

    #[test]
    fn groups_follow_the_class_and_the_reit_type() {
        assert_eq!(
            Group::of(AssetClass::BrReits, Some("Papel")),
            Some(Group::PaperReits)
        );
        assert_eq!(
            Group::of(AssetClass::BrReits, Some("Tijolo")),
            Some(Group::BrickReits)
        );
        assert_eq!(
            Group::of(AssetClass::BrReits, Some("Híbrido")),
            Some(Group::OtherReits)
        );
        assert_eq!(
            Group::of(AssetClass::BrReits, None),
            Some(Group::OtherReits)
        );
        assert_eq!(Group::of(AssetClass::BrStocks, None), Some(Group::BrStocks));
        assert_eq!(Group::of(AssetClass::UsReits, None), Some(Group::Foreign));
        assert_eq!(
            Group::of(AssetClass::IntlStocks, None),
            Some(Group::Foreign)
        );
        assert_eq!(Group::of(AssetClass::FixedIncome, None), None);
        assert_eq!(Group::of(AssetClass::Crypto, None), None);
        assert_eq!(Group::PaperReits.driver(), Driver::Cdi);
        assert_eq!(Group::BrickReits.driver(), Driver::Ipca);
        assert_eq!(Group::BrStocks.driver(), Driver::Flat);
        assert!(Group::BrStocks.prefers_usual());
        assert!(!Group::PaperReits.prefers_usual());
        // O nome do setor é o que `sectors::cvm` grava.
        assert_eq!(PAPER_SECTOR, crate::sectors::cvm::PAPER);
        assert_eq!(BRICK_SECTOR, crate::sectors::cvm::BRICK);
    }

    #[test]
    fn shape_keeps_the_months_that_pay() {
        let shape = Shape::from_payments([
            (date!(2026 - 05 - 10), 3.0),
            (date!(2026 - 11 - 10), 1.0),
            (date!(2026 - 11 - 20), 2.0),
        ]);
        assert!((shape.weight(5) - 6.0).abs() < 1e-12);
        assert!((shape.weight(11) - 6.0).abs() < 1e-12);
        assert_eq!(shape.weight(1), 0.0);
        assert_eq!(shape.min(), 0.0);
        assert!((shape.weights().iter().sum::<f64>() - 12.0).abs() < 1e-9);
        assert_eq!(Shape::from_payments([]), Shape::uniform());
        assert_eq!(Shape::from_payments([(TODAY, -5.0)]), Shape::uniform());
        assert_eq!(Shape::uniform().min(), 1.0);
    }

    #[test]
    fn combined_shape_weighs_each_asset_by_its_income() {
        let may = Shape::from_payments([(date!(2026 - 05 - 10), 1.0)]);
        let combined = Shape::combine([(may, 100.0), (Shape::uniform(), 300.0)]);
        // Maio: (12 × 100 + 1 × 300) / 400; o resto: 300 / 400.
        assert!((combined.weight(5) - 1500.0 / 400.0).abs() < 1e-9);
        assert!((combined.weight(6) - 0.75).abs() < 1e-9);
        assert!((combined.min() - 0.75).abs() < 1e-9);
        assert_eq!(Shape::combine([]), Shape::uniform());
    }

    #[test]
    fn paper_follows_the_cdi_against_the_last_twelve_months() {
        let p = path(0.12, 0.04, 12);
        let premises = IncomePremises {
            spread: 0.0,
            cdi_trailing: Some(0.15),
        };
        let result = project(&[asset(Group::PaperReits, dec!(100))], &p, premises);
        // 100 × 12 / 15 = 80 em todo mês.
        assert!(result.months.iter().all(|m| (m.base - 80.0).abs() < 1e-9));
        assert!((result.assets[0].base - 960.0).abs() < 1e-6);
        // Sem o CDI médio, o papel fica parado.
        let flat = project(
            &[asset(Group::PaperReits, dec!(100))],
            &p,
            IncomePremises {
                spread: 0.0,
                cdi_trailing: None,
            },
        );
        assert!((flat.months[0].base - 100.0).abs() < 1e-9);
    }

    #[test]
    fn brick_follows_the_inflation_and_stocks_stay_flat() {
        let p = path(0.12, 0.06, 12);
        let premises = IncomePremises {
            spread: 0.0,
            cdi_trailing: Some(0.12),
        };
        let result = project(
            &[
                asset(Group::BrickReits, dec!(100)),
                asset(Group::BrStocks, dec!(50)),
            ],
            &p,
            premises,
        );
        let last = result.months.last().unwrap();
        assert!((last.by_group[&Group::BrickReits] - 106.0).abs() < 1e-6);
        assert!((last.by_group[&Group::BrStocks] - 50.0).abs() < 1e-9);
        assert!((last.base - 156.0).abs() < 1e-6);
    }

    #[test]
    fn scenarios_open_with_time() {
        let p = path(0.12, 0.0, 24);
        let result = project(
            &[asset(Group::BrStocks, dec!(100))],
            &p,
            IncomePremises {
                spread: 0.02,
                cdi_trailing: None,
            },
        );
        let year = &result.months[11];
        assert!((year.low - 98.0).abs() < 1e-9);
        assert!((year.high - 102.0).abs() < 1e-9);
        let two = &result.months[23];
        assert!((two.low - 100.0 * 0.98f64.powi(2)).abs() < 1e-9);
        let (low, base, high) = result.totals();
        assert!(low < base && base < high);
        assert!((base - 2400.0).abs() < 1e-6);
    }

    #[test]
    fn seasonality_moves_money_between_months_not_the_total() {
        let p = path(0.12, 0.0, 12);
        let mut semiannual = asset(Group::BrStocks, dec!(100));
        semiannual.shape =
            Shape::from_payments([(date!(2026 - 05 - 10), 1.0), (date!(2026 - 11 - 10), 1.0)]);
        let result = project(
            &[semiannual],
            &p,
            IncomePremises {
                spread: 0.0,
                cdi_trailing: None,
            },
        );
        let may = result.months.iter().find(|m| m.month.month == 5).unwrap();
        let june = result.months.iter().find(|m| m.month.month == 6).unwrap();
        assert!((may.base - 600.0).abs() < 1e-9);
        assert_eq!(june.base, 0.0);
        assert!((result.totals().1 - 1200.0).abs() < 1e-6);
    }

    proptest! {
        /// O pessimista nunca passa do base, nem o base do otimista, em mês
        /// nenhum; e o total de cada ativo fecha com a soma dos meses.
        #[test]
        fn scenarios_are_ordered(
            monthly in proptest::collection::vec(0i64..1_000_000, 1..6),
            groups in proptest::collection::vec(0usize..5, 6),
            spread in 0u32..=2000,
            cdi in 0u32..=3000,
            ipca in 0u32..=1500,
            horizon in 1u32..=24,
        ) {
            let p = path(f64::from(cdi) / 10_000.0, f64::from(ipca) / 10_000.0, horizon);
            let assets: Vec<AssetInput> = monthly
                .iter()
                .zip(&groups)
                .map(|(cents, g)| asset(Group::ALL[*g], Decimal::new(*cents, 2)))
                .collect();
            let result = project(&assets, &p, IncomePremises {
                spread: f64::from(spread) / 10_000.0,
                cdi_trailing: Some(0.13),
            });
            for m in &result.months {
                prop_assert!(m.low <= m.base + 1e-9 && m.base <= m.high + 1e-9);
                let groups: f64 = m.by_group.values().sum();
                prop_assert!((groups - m.base).abs() < 1e-6);
            }
            let by_asset: f64 = result.assets.iter().map(|a| a.base).sum();
            prop_assert!((by_asset - result.totals().1).abs() < 1e-4);
        }
    }
}
