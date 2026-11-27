//! Desempenho por classe: o peso de cada classe e a rentabilidade dela no
//! período, pelo mesmo método de cotas da carteira. Regra pura; o método está
//! em `docs/desempenho.md` ("Por classe").
//!
//! Cada classe é tratada como uma carteira só dela: os ativos da classe, com o
//! mesmo livro, os mesmos preços (`price_history` e o que as fotos guardaram) e
//! os proventos dos ativos dela, refeitos dia a dia por
//! [`super::retro::rebuild`] a partir do fechamento de base do período. Na base
//! a cota da classe vale 1; o aporte compra cotas pelo valor do dia e não mexe
//! nela. Por isso a rentabilidade da classe não depende de quanto nem de quando
//! se aportou nela (sem viés de aporte), e a classe que entrou no meio do
//! período mede desde o primeiro dia em que teve dinheiro.

use std::collections::{BTreeMap, HashMap};

use rust_decimal::Decimal;
use time::Date;
use uuid::Uuid;

use super::asset_class::AssetClass;
use super::retro::{self, AssetHistory, Series};

/// A rentabilidade de uma classe no período.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassReturn {
    pub class: AssetClass,
    /// O fechamento de onde a classe mede: a base do período ou, se a classe
    /// ainda não tinha dinheiro nela, o dia em que recebeu o primeiro.
    pub from: Date,
    /// O último dia medido: o fim do período ou o dia em que a classe zerou.
    pub to: Date,
    /// Fração (0,05 = 5%).
    pub ret: Decimal,
    /// O patrimônio da classe no fim do período (0 se zerou).
    pub market_value: Decimal,
}

/// A rentabilidade de cada classe que teve dinheiro em algum dia de `from` a
/// `to`, na ordem de [`AssetClass::ALL`]. `assets` são todos os ativos da
/// carteira com a classe de cada um; `income`, os proventos recebidos de cada
/// ativo (data de pagamento, valor líquido).
pub fn class_returns(
    assets: &[(AssetClass, AssetHistory)],
    income: &HashMap<Uuid, Series>,
    from: Date,
    to: Date,
) -> Vec<ClassReturn> {
    AssetClass::ALL
        .into_iter()
        .filter_map(|class| {
            let members: Vec<AssetHistory> = assets
                .iter()
                .filter(|(c, _)| *c == class)
                .map(|(_, a)| a.clone())
                .collect();
            if members.is_empty() {
                return None;
            }
            let mut received: BTreeMap<Date, Decimal> = BTreeMap::new();
            for asset in &members {
                for (date, value) in income.get(&asset.asset_id).into_iter().flatten() {
                    *received.entry(*date).or_default() += *value;
                }
            }
            let received: Series = received.into_iter().collect();
            measure(class, &retro::rebuild(&members, &received, None, from, to))
        })
        .collect()
}

/// A rentabilidade de uma série refeita do zero: da primeira cota com
/// dinheiro até o último dia em que havia cotas para medir.
fn measure(class: AssetClass, days: &[retro::Day]) -> Option<ClassReturn> {
    let base = days.iter().position(|d| d.quota.quotas > Decimal::ZERO)?;
    // Um dia é medido quando o dia anterior tinha cotas: o preço andou sobre
    // elas. Depois de zerar, a cota fica parada e não há o que medir.
    let last = (base + 1..days.len())
        .rev()
        .find(|&i| days[i - 1].quota.quotas > Decimal::ZERO)
        .unwrap_or(base);
    let start = days[base].quota.value;
    let end = days[last].quota.value;
    Some(ClassReturn {
        class,
        from: days[base].date,
        to: days[last].date,
        ret: if start > Decimal::ZERO {
            end / start - Decimal::ONE
        } else {
            Decimal::ZERO
        },
        market_value: days.last().map_or(Decimal::ZERO, |d| d.market_value),
    })
}

/// O peso de cada classe no patrimônio do fim do período, em fração. Sem
/// patrimônio, tudo zero.
pub fn weights(classes: &[ClassReturn]) -> Vec<Decimal> {
    let total: Decimal = classes.iter().map(|c| c.market_value).sum();
    classes
        .iter()
        .map(|c| {
            if total > Decimal::ZERO {
                c.market_value / total
            } else {
                Decimal::ZERO
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ledger::{Entry, Kind};
    use crate::domain::performance::{period_return, QuotaPoint};
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use time::Duration;

    fn entry(kind: Kind, date: Date, seq: i64, quantity: Decimal, price: Option<Decimal>) -> Entry {
        Entry {
            kind,
            trade_date: date,
            seq,
            quantity,
            unit_price: price,
            fees: Decimal::ZERO,
            fx_rate: Decimal::ONE,
        }
    }

    fn asset(id: u128, entries: Vec<Entry>, prices: Series) -> AssetHistory {
        AssetHistory {
            asset_id: Uuid::from_u128(id),
            by_value: false,
            entries,
            prices,
            fallback_price: None,
        }
    }

    const D1: Date = date!(2025 - 03 - 03);
    const D2: Date = date!(2025 - 03 - 04);
    const D3: Date = date!(2025 - 03 - 05);
    const D4: Date = date!(2025 - 03 - 06);
    const D5: Date = date!(2025 - 03 - 07);

    /// Um FII e uma ação fictícios. O FII já existia na base (D1), paga
    /// rendimento em D3 e é vendido inteiro em D4; a ação entra em D2.
    fn case() -> (Vec<(AssetClass, AssetHistory)>, HashMap<Uuid, Series>) {
        let fii = asset(
            1,
            vec![
                entry(
                    Kind::Buy,
                    date!(2025 - 02 - 20),
                    1,
                    dec!(10),
                    Some(dec!(95)),
                ),
                entry(Kind::Sell, D4, 4, dec!(10), Some(dec!(104))),
            ],
            vec![
                (date!(2025 - 02 - 20), dec!(95)),
                (D1, dec!(100)),
                (D2, dec!(102)),
                (D3, dec!(101)),
                (D4, dec!(104)),
            ],
        );
        let stock = asset(
            2,
            vec![
                entry(Kind::Buy, D2, 2, dec!(20), Some(dec!(40))),
                // Aporte grande no meio: não muda a cota da classe.
                entry(Kind::Buy, D4, 3, dec!(200), Some(dec!(42))),
            ],
            vec![
                (D2, dec!(40)),
                (D3, dec!(41)),
                (D4, dec!(42)),
                (D5, dec!(43.05)),
            ],
        );
        let income = HashMap::from([(Uuid::from_u128(1), vec![(D3, dec!(10))])]);
        (
            vec![(AssetClass::BrStocks, stock), (AssetClass::BrReits, fii)],
            income,
        )
    }

    #[test]
    fn each_class_measures_its_own_days() {
        let (assets, income) = case();
        let classes = class_returns(&assets, &income, D1, D5);
        assert_eq!(classes.len(), 2);
        // Ordem da interface: Ações Nacionais antes de FIIs.
        let stock = classes[0];
        assert_eq!(stock.class, AssetClass::BrStocks);
        // A ação começa em D2 (primeiro dia com dinheiro), não na base.
        assert_eq!((stock.from, stock.to), (D2, D5));
        // 40 → 43,05: +7,625%, com ou sem o aporte de D4 pelo preço do dia.
        assert_eq!(stock.ret, dec!(0.07625));
        assert_eq!(stock.market_value, dec!(220) * dec!(43.05));
        let fii = classes[1];
        assert_eq!(fii.class, AssetClass::BrReits);
        // O FII mede de D1 até a venda em D4 e para ali.
        assert_eq!((fii.from, fii.to), (D1, D4));
        // D2: 1.020/1.000; D3: (1.010 + 10 de rendimento)/1.020; D4: a venda a 1.040/1.010.
        let expected =
            (dec!(1020) / dec!(1000)) * (dec!(1020) / dec!(1020)) * (dec!(1040) / dec!(1010))
                - Decimal::ONE;
        assert!(
            (fii.ret - expected).abs() < dec!(0.000000001),
            "{} != {expected}",
            fii.ret
        );
        assert_eq!(fii.market_value, Decimal::ZERO);
        assert_eq!(weights(&classes), vec![Decimal::ONE, Decimal::ZERO]);
    }

    #[test]
    fn class_without_money_in_the_window_is_left_out() {
        let (assets, income) = case();
        // A janela inteira antes da compra da ação e depois de o FII existir.
        let early = class_returns(
            &assets,
            &income,
            date!(2025 - 02 - 20),
            date!(2025 - 02 - 28),
        );
        assert_eq!(early.len(), 1);
        assert_eq!(early[0].class, AssetClass::BrReits);
        // Comprada no último dia: a base e o fim são o mesmo dia, sem rentabilidade.
        let last_day = class_returns(&assets, &HashMap::new(), D1, D2);
        assert_eq!(last_day[0].class, AssetClass::BrStocks);
        assert_eq!(
            (last_day[0].from, last_day[0].to, last_day[0].ret),
            (D2, D2, Decimal::ZERO)
        );
        assert!(class_returns(&[], &income, D1, D5).is_empty());
        assert!(weights(&[]).is_empty());
    }

    fn quota_points(days: &[retro::Day]) -> Vec<QuotaPoint> {
        days.iter()
            .map(|d| QuotaPoint {
                date: d.date,
                value: d.quota.value,
            })
            .collect()
    }

    /// Um ativo fictício: compra antes da janela, preços diários e, se pedido,
    /// compras no meio dela.
    fn arb_asset(id: u128, days: usize, buys: bool) -> impl Strategy<Value = AssetHistory> {
        (
            1u32..200,
            prop::collection::vec(1_000u32..20_000, days),
            prop::collection::vec((0usize..30, 0u32..50), 0..4),
        )
            .prop_map(move |(held, cents, extra)| {
                let start = date!(2025 - 01 - 01);
                let prices: Series = cents
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        (
                            start + Duration::days(i as i64),
                            Decimal::new(i64::from(*c), 2),
                        )
                    })
                    .collect();
                let mut entries = vec![entry(
                    Kind::Buy,
                    start,
                    1,
                    Decimal::from(held),
                    Some(prices[0].1),
                )];
                if buys {
                    for (n, (day, quantity)) in extra.iter().enumerate() {
                        let day = 1 + day % (prices.len() - 1);
                        let (date, price) = prices[day];
                        entries.push(entry(
                            Kind::Buy,
                            date,
                            10 + n as i64,
                            Decimal::from(*quantity),
                            Some(price),
                        ));
                    }
                }
                asset(id, entries, prices)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(120))]

        /// Com uma classe só, a rentabilidade da classe na janela é a da
        /// carteira inteira na mesma janela: medir do zero na base dá o mesmo
        /// que a série desde o primeiro lançamento.
        #[test]
        fn one_class_is_the_portfolio(
            a in arb_asset(1, 30, true),
            b in arb_asset(2, 30, true),
            cut in 1usize..28,
        ) {
            let start = date!(2025 - 01 - 01);
            let end = start + Duration::days(29);
            let from = start + Duration::days(cut as i64);
            let whole = retro::rebuild(&[a.clone(), b.clone()], &[], None, start, end);
            let portfolio = period_return(&quota_points(&whole), from, end).unwrap().ret;
            let classes = class_returns(
                &[(AssetClass::BrStocks, a), (AssetClass::BrStocks, b)],
                &HashMap::new(),
                from,
                end,
            );
            prop_assert_eq!(classes.len(), 1);
            prop_assert!((classes[0].ret - portfolio).abs() < dec!(0.0000001), "{} != {}", classes[0].ret, portfolio);
            prop_assert_eq!(classes[0].market_value, whole.last().unwrap().market_value);
        }

        /// Sem aporte nem provento na janela, a rentabilidade da carteira é a
        /// média das classes pesada pelo patrimônio de cada uma na base.
        #[test]
        fn without_flows_the_portfolio_is_the_weighted_classes(
            a in arb_asset(1, 20, false),
            b in arb_asset(2, 20, false),
            c in arb_asset(3, 20, false),
        ) {
            let start = date!(2025 - 01 - 01);
            let end = start + Duration::days(19);
            let assets = vec![
                (AssetClass::BrStocks, a),
                (AssetClass::BrReits, b),
                (AssetClass::BrReits, c),
            ];
            let all: Vec<AssetHistory> = assets.iter().map(|(_, h)| h.clone()).collect();
            let whole = retro::rebuild(&all, &[], None, start, end);
            let portfolio = period_return(&quota_points(&whole), start, end).unwrap().ret;
            let classes = class_returns(&assets, &HashMap::new(), start, end);
            let base: Vec<Decimal> = assets
                .iter()
                .map(|(_, h)| h.entries[0].quantity * h.prices[0].1)
                .collect();
            let base_total: Decimal = base.iter().sum();
            let weight = |class: AssetClass| -> Decimal {
                assets.iter().zip(&base).filter(|((c, _), _)| *c == class).map(|(_, v)| *v).sum::<Decimal>() / base_total
            };
            let mixed: Decimal = classes.iter().map(|r| weight(r.class) * r.ret).sum();
            prop_assert!((mixed - portfolio).abs() < dec!(0.0000001), "{mixed} != {portfolio}");
            let total: Decimal = weights(&classes).iter().sum();
            prop_assert!((total - Decimal::ONE).abs() < dec!(0.0000000001));
        }
    }
}
