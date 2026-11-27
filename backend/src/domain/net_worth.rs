//! Patrimônio × aportado: de onde veio a variação do patrimônio num período.
//! Regra pura; o método está em `docs/desempenho.md` ("Patrimônio × aportado").
//!
//! O patrimônio de um dia é o dinheiro que a pessoa pôs (menos o que tirou)
//! mais o que o preço fez com ele. Num período:
//!
//! `patrimônio no fim = patrimônio na base + aportes líquidos + mercado`
//!
//! - **aportes líquidos**: a soma dos fluxos do livro depois da base (compras
//!   com taxas menos vendas, resgates e amortizações);
//! - **mercado**: o resto, o que o preço fez (`fim − base − aportes`);
//! - **proventos**: o dinheiro que a carteira pagou no período. Ele saiu da
//!   carteira para a pessoa, então não está no patrimônio; o que ela
//!   reinvestiu voltou como aporte.
//!
//! Quando a janela começa na primeira foto (o período "Tudo", ou uma série mais
//! nova que o período), a base é o zero de antes do primeiro lançamento: o
//! aportado é o aplicado inteiro e o mercado, o ganho de preço inteiro.

use rust_decimal::Decimal;
use time::Date;

/// Um dia da série, no que a ponte precisa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowPoint {
    pub date: Date,
    pub market_value: Decimal,
    /// Aporte (+) ou retirada (−) do dia.
    pub net_flow: Decimal,
    /// Proventos recebidos no dia.
    pub income: Decimal,
}

/// A variação do patrimônio num período, separada por origem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bridge {
    /// O fechamento de base. `None`: a janela começa do zero, antes do
    /// primeiro lançamento.
    pub from: Option<Date>,
    pub to: Date,
    pub start_value: Decimal,
    pub contributions: Decimal,
    pub market: Decimal,
    pub income: Decimal,
    pub end_value: Decimal,
    /// Desde o começo da série: o aplicado no fim (a soma dos fluxos).
    pub invested: Decimal,
    /// Desde o começo da série: patrimônio − aplicado, o ganho de preço.
    pub gain: Decimal,
    /// Desde o começo da série: os proventos recebidos.
    pub income_total: Decimal,
}

/// A ponte de `base` (o fechamento de onde o período mede) ao último ponto.
/// `base` na primeira foto ou antes dela: a ponte começa do zero. `None` sem
/// série.
pub fn bridge(points: &[FlowPoint], base: Date) -> Option<Bridge> {
    let first = points.first()?;
    let last = points.last()?;
    let from_zero = base <= first.date;
    let start = (!from_zero)
        .then(|| points.iter().rev().find(|p| p.date <= base))
        .flatten();
    let in_window = |p: &&FlowPoint| start.is_none_or(|s| p.date > s.date);
    let contributions: Decimal = points.iter().filter(in_window).map(|p| p.net_flow).sum();
    let income: Decimal = points.iter().filter(in_window).map(|p| p.income).sum();
    let start_value = start.map_or(Decimal::ZERO, |s| s.market_value);
    let invested: Decimal = points.iter().map(|p| p.net_flow).sum();
    Some(Bridge {
        from: start.map(|s| s.date),
        to: last.date,
        start_value,
        contributions,
        market: last.market_value - start_value - contributions,
        income,
        end_value: last.market_value,
        invested,
        gain: last.market_value - invested,
        income_total: points.iter().map(|p| p.income).sum(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use time::Duration;

    fn point(date: Date, value: Decimal, flow: Decimal, income: Decimal) -> FlowPoint {
        FlowPoint {
            date,
            market_value: value,
            net_flow: flow,
            income,
        }
    }

    /// O caso de `docs/desempenho.md` ("Conferência passo a passo"): D1 a D5.
    fn hand_case() -> Vec<FlowPoint> {
        vec![
            point(date!(2025 - 03 - 03), dec!(2010), dec!(2001), dec!(0)),
            point(date!(2025 - 03 - 04), dec!(2840), dec!(800), dec!(0)),
            point(date!(2025 - 03 - 05), dec!(2870), dec!(0), dec!(10)),
            point(date!(2025 - 03 - 06), dec!(2895), dec!(0), dec!(0)),
            point(date!(2025 - 03 - 07), dec!(2405), dec!(-519.50), dec!(0)),
        ]
    }

    #[test]
    fn from_the_start_everything_is_contribution_or_market() {
        let all = bridge(&hand_case(), date!(2025 - 03 - 03)).unwrap();
        assert_eq!(all.from, None);
        assert_eq!(all.start_value, Decimal::ZERO);
        assert_eq!(all.contributions, dec!(2281.50));
        assert_eq!(all.market, dec!(123.50));
        assert_eq!(all.income, dec!(10));
        assert_eq!(all.end_value, dec!(2405));
        assert_eq!(
            (all.invested, all.gain, all.income_total),
            (dec!(2281.50), dec!(123.50), dec!(10))
        );
        // Base antes da série: também do zero.
        assert_eq!(bridge(&hand_case(), date!(2024 - 01 - 01)), Some(all));
    }

    #[test]
    fn a_window_starts_at_the_base_close() {
        // De D2 (fechamento de base, 2.840) a D5: só a venda é fluxo.
        let window = bridge(&hand_case(), date!(2025 - 03 - 04)).unwrap();
        assert_eq!(window.from, Some(date!(2025 - 03 - 04)));
        assert_eq!(window.start_value, dec!(2840));
        assert_eq!(window.contributions, dec!(-519.50));
        // 2.405 − 2.840 + 519,50: o preço fez +84,50 no período.
        assert_eq!(window.market, dec!(84.50));
        // O provento de D3 caiu dentro da janela; o de antes da base, não.
        assert_eq!(window.income, dec!(10));
        // O "desde o começo" não muda com a janela.
        assert_eq!(window.invested, dec!(2281.50));
        // Base num dia sem foto: vale a última de antes.
        let mut holes = hand_case();
        holes.remove(2);
        let gap = bridge(&holes, date!(2025 - 03 - 05)).unwrap();
        assert_eq!(gap.from, Some(date!(2025 - 03 - 04)));
        assert_eq!(gap.start_value, dec!(2840));
        assert!(bridge(&[], date!(2025 - 03 - 04)).is_none());
    }

    fn arb_points() -> impl Strategy<Value = Vec<FlowPoint>> {
        prop::collection::vec((0i64..1_000_000, -50_000i64..100_000, 0i64..5_000), 1..40).prop_map(
            |days| {
                let start = date!(2025 - 01 - 01);
                days.into_iter()
                    .enumerate()
                    .map(|(i, (value, flow, income))| {
                        point(
                            start + Duration::days(i as i64),
                            Decimal::new(value, 2),
                            Decimal::new(flow, 2),
                            Decimal::new(income, 2),
                        )
                    })
                    .collect()
            },
        )
    }

    proptest! {
        /// A ponte fecha (base + aportes + mercado = fim) e duas janelas
        /// seguidas somam o mesmo que a janela inteira.
        #[test]
        fn bridge_closes_and_windows_add_up(points in arb_points(), cut in 0usize..40) {
            let base = points[0].date;
            let middle = points[cut % points.len()].date;
            let whole = bridge(&points, base).unwrap();
            prop_assert_eq!(whole.start_value + whole.contributions + whole.market, whole.end_value);
            let upto: Vec<FlowPoint> = points.iter().copied().filter(|p| p.date <= middle).collect();
            let left = bridge(&upto, base).unwrap();
            let right = bridge(&points, middle).unwrap();
            prop_assert_eq!(left.end_value, if middle == base { points[0].market_value } else { right.start_value });
            if middle > base {
                prop_assert_eq!(left.contributions + right.contributions, whole.contributions);
                prop_assert_eq!(left.market + right.market, whole.market);
                prop_assert_eq!(left.income + right.income, whole.income);
            }
            prop_assert_eq!(whole.invested + whole.gain, whole.end_value);
        }
    }
}
