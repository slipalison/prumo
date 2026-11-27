//! Painel da Carteira: a faixa de indicadores do mês e a referência do dia.
//! Regra pura, sem banco nem rede; método e limites em `docs/painel.md`.
//!
//! - **Mês**: tudo sai da série diária da carteira (`portfolio_snapshots`).
//!   A rentabilidade é a variação da cota (TWR, `domain::performance`), então
//!   aporte não vira rendimento. Proventos e aportes são a soma dos dias do mês.
//! - **Dia**: o fechamento anterior de um ativo é o preço guardado na última
//!   foto diária anterior à data da cotação dele. O painel compara a cotação ao
//!   vivo com esse número. Para o resultado em reais, vai junto o que os
//!   lançamentos depois desse fechamento mudaram: a quantidade que já havia
//!   nele e o dinheiro das compras e vendas. Assim a compra do dia não vira
//!   ganho (ela entra pelo que se pagou, não pelo fechamento de ontem).

use rust_decimal::Decimal;
use time::{Date, Duration};

use super::allocation::round_cents;
use super::cash::sale_proceeds;
use super::ledger::{Entry, Kind};
use super::performance::{period_return, Position, QuotaPoint};

/// Quantos dias a foto de referência pode ficar atrás da data da cotação: além
/// disso a "variação do dia" mediria outra coisa (série com buraco).
pub const MAX_REFERENCE_GAP_DAYS: i64 = 7;
/// Cotação mais velha que isto não é "de hoje": o ativo fica fora do painel do dia.
pub const MAX_STALE_PRICE_DAYS: i64 = 5;

/// Um dia da série da carteira, no que o painel usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayPoint {
    pub date: Date,
    pub market_value: Decimal,
    /// Aporte (positivo) ou retirada (negativo) do dia.
    pub net_flow: Decimal,
    /// Proventos líquidos recebidos no dia.
    pub income: Decimal,
    pub quota_value: Decimal,
}

/// O patrimônio no último dia de um mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthEnd {
    pub date: Date,
    pub value: Decimal,
}

/// A rentabilidade (TWR) de um mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthReturn {
    /// Fechamento de onde se mediu.
    pub from: Date,
    pub to: Date,
    /// Fração (0,02 = 2%).
    pub ret: Decimal,
    /// `false`: a série começou no meio do mês e a janela ficou mais curta.
    pub full: bool,
}

/// O que entrou num mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonthTotals {
    /// Proventos líquidos recebidos.
    pub income: Decimal,
    /// Aportes menos retiradas.
    pub contribution: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthSummary {
    /// O último dia da série (hoje, com a foto parcial).
    pub as_of: Date,
    pub patrimony: Decimal,
    /// O patrimônio no fim do mês anterior; `None` se a série começou depois.
    pub previous_month_end: Option<MonthEnd>,
    pub current_return: Option<MonthReturn>,
    pub previous_return: Option<MonthReturn>,
    pub current: MonthTotals,
    pub previous: MonthTotals,
}

fn first_of_month(date: Date) -> Date {
    date.replace_day(1).unwrap_or(date)
}

fn totals(points: &[DayPoint], from: Date, to: Date) -> MonthTotals {
    points
        .iter()
        .filter(|p| p.date >= from && p.date <= to)
        .fold(MonthTotals::default(), |acc, p| MonthTotals {
            income: acc.income + p.income,
            contribution: acc.contribution + p.net_flow,
        })
}

fn month_return(quotas: &[QuotaPoint], from: Date, to: Date) -> Option<MonthReturn> {
    period_return(quotas, from, to)
        .filter(|w| w.to > w.from)
        .map(|w| MonthReturn {
            from: w.from,
            to: w.to,
            ret: w.ret,
            full: w.from <= from,
        })
}

/// O resumo do mês de `today` contra o mês anterior. `None` sem série.
/// `points` vem em ordem de data.
pub fn month_summary(points: &[DayPoint], today: Date) -> Option<MonthSummary> {
    let last = points.iter().rev().find(|p| p.date <= today)?;
    let month_start = first_of_month(today);
    let previous_end = month_start - Duration::days(1);
    let previous_start = first_of_month(previous_end);
    let before_previous_end = previous_start - Duration::days(1);
    let quotas: Vec<QuotaPoint> = points
        .iter()
        .map(|p| QuotaPoint {
            date: p.date,
            value: p.quota_value,
        })
        .collect();
    Some(MonthSummary {
        as_of: last.date,
        patrimony: last.market_value,
        previous_month_end: points
            .iter()
            .rev()
            .find(|p| p.date <= previous_end)
            .map(|p| MonthEnd {
                date: p.date,
                value: p.market_value,
            }),
        current_return: month_return(&quotas, previous_end, today),
        previous_return: month_return(&quotas, before_previous_end, previous_end),
        current: totals(points, month_start, today),
        previous: totals(points, previous_start, previous_end),
    })
}

/// O que os lançamentos de um ativo depois do fechamento de referência mudaram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DayTrades {
    /// Cotas que entraram (positivo) ou saíram (negativo) por compra, venda,
    /// resgate, desdobramento, grupamento ou bonificação. Saldo inicial e
    /// ajuste não contam: dizem a quantidade que a pessoa já tinha, então ela
    /// já estava lá no fechamento.
    pub quantity: Decimal,
    /// O que as compras custaram, com as taxas, em reais.
    pub bought: Decimal,
    /// O líquido das vendas e resgates (preço × quantidade − taxas), em reais.
    pub sold: Decimal,
}

/// Soma os lançamentos com data depois de `after`. Desdobramento, grupamento e
/// bonificação mudam a quantidade sem dinheiro (o preço se ajusta); amortização
/// fica de fora, como provento: devolve capital sem mudar as cotas.
pub fn day_trades<'a>(entries: impl IntoIterator<Item = &'a Entry>, after: Date) -> DayTrades {
    let mut trades = DayTrades::default();
    for entry in entries.into_iter().filter(|e| e.trade_date > after) {
        match entry.kind {
            Kind::Buy => {
                trades.quantity += entry.quantity;
                let price = entry.unit_price.unwrap_or(Decimal::ZERO);
                trades.bought += (entry.quantity * price + entry.fees) * entry.fx_rate;
            }
            Kind::Sell | Kind::Redemption => {
                trades.quantity -= entry.quantity;
                trades.sold += sale_proceeds(entry).unwrap_or(Decimal::ZERO);
            }
            Kind::Split | Kind::Bonus => trades.quantity += entry.quantity,
            Kind::ReverseSplit => trades.quantity -= entry.quantity,
            Kind::Opening | Kind::Adjustment | Kind::Amortization => {}
        }
    }
    trades.bought = round_cents(trades.bought);
    trades.sold = round_cents(trades.sold);
    trades
}

/// A referência do dia de um ativo: o fechamento anterior, de que dia ele é, e
/// o que mudou desde então.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayReference {
    /// O preço na foto. `None` só para o ativo que não estava nela (ou estava
    /// sem preço) e teve compra ou venda depois: o dia dele é só o negócio.
    pub previous_close: Option<Decimal>,
    pub date: Date,
    /// As cotas que já havia no fechamento: a quantidade de agora menos o que
    /// os negócios depois dele trouxeram ou levaram.
    pub previous_quantity: Decimal,
    pub bought: Decimal,
    pub sold: Decimal,
}

/// A referência do dia de um ativo: o preço dele na foto de `snapshot_date`
/// (a última anterior à data da cotação, `price_date`), a quantidade que havia
/// nela e o dinheiro dos negócios depois dela (`trades`). `quantity` é a
/// quantidade de agora.
///
/// `None` quando a foto não é anterior à cotação, quando está longe demais
/// dela, quando a própria cotação é velha (`today` − `price_date`), quando o
/// ativo era por valor na foto, e quando não há preço de mercado na foto nem
/// negócio depois dela (nada a medir).
pub fn day_reference(
    position: Option<&Position>,
    snapshot_date: Date,
    price_date: Date,
    today: Date,
    quantity: Decimal,
    trades: DayTrades,
) -> Option<DayReference> {
    let gap = (price_date - snapshot_date).whole_days();
    let stale = (today - price_date).whole_days();
    if !(1..=MAX_REFERENCE_GAP_DAYS).contains(&gap) || stale > MAX_STALE_PRICE_DAYS {
        return None;
    }
    if position.is_some_and(|p| p.by_value) {
        return None;
    }
    let close = position.and_then(|p| p.unit_price.filter(|price| *price > Decimal::ZERO));
    if close.is_none() && trades.bought.is_zero() && trades.sold.is_zero() {
        return None;
    }
    Some(DayReference {
        previous_close: close,
        date: snapshot_date,
        previous_quantity: (quantity - trades.quantity).max(Decimal::ZERO),
        bought: trades.bought,
        sold: trades.sold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal_macros::dec;
    use time::macros::date;
    use uuid::Uuid;

    fn point(
        date: Date,
        value: Decimal,
        flow: Decimal,
        income: Decimal,
        quota: Decimal,
    ) -> DayPoint {
        DayPoint {
            date,
            market_value: value,
            net_flow: flow,
            income,
            quota_value: quota,
        }
    }

    /// Outubro e novembro de uma carteira fictícia.
    fn series() -> Vec<DayPoint> {
        vec![
            point(
                date!(2026 - 09 - 30),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 10 - 15),
                dec!(1250),
                dec!(100),
                dec!(5),
                dec!(1.10),
            ),
            point(
                date!(2026 - 10 - 31),
                dec!(1300),
                dec!(0),
                dec!(7),
                dec!(1.15),
            ),
            point(
                date!(2026 - 11 - 10),
                dec!(1500),
                dec!(200),
                dec!(4),
                dec!(1.20),
            ),
            point(
                date!(2026 - 11 - 25),
                dec!(1450),
                dec!(0),
                dec!(0),
                dec!(1.265),
            ),
        ]
    }

    #[test]
    fn summarizes_the_month_against_the_previous() {
        let s = month_summary(&series(), date!(2026 - 11 - 25)).unwrap();
        assert_eq!(s.as_of, date!(2026 - 11 - 25));
        assert_eq!(s.patrimony, dec!(1450));
        assert_eq!(
            s.previous_month_end,
            Some(MonthEnd {
                date: date!(2026 - 10 - 31),
                value: dec!(1300)
            })
        );
        // Novembro: cota de 1,15 (31/10) a 1,265.
        let current = s.current_return.unwrap();
        assert_eq!(current.ret, dec!(0.10));
        assert!(current.full);
        // Outubro: cota de 1,00 (30/09) a 1,15.
        let previous = s.previous_return.unwrap();
        assert_eq!(previous.ret, dec!(0.15));
        assert!(previous.full);
        assert_eq!(s.current.income, dec!(4));
        assert_eq!(s.current.contribution, dec!(200));
        assert_eq!(s.previous.income, dec!(12));
        assert_eq!(s.previous.contribution, dec!(100));
    }

    #[test]
    fn a_series_that_starts_mid_month_has_a_shorter_window() {
        let points = vec![
            point(
                date!(2026 - 10 - 20),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 10 - 31),
                dec!(1100),
                dec!(0),
                dec!(0),
                dec!(1.1),
            ),
            point(
                date!(2026 - 11 - 02),
                dec!(1100),
                dec!(0),
                dec!(0),
                dec!(1.1),
            ),
        ];
        let s = month_summary(&points, date!(2026 - 11 - 02)).unwrap();
        let previous = s.previous_return.unwrap();
        assert!(!previous.full);
        assert_eq!(previous.from, date!(2026 - 10 - 20));
        assert_eq!(previous.ret, dec!(0.1));
        assert_eq!(s.current_return.unwrap().ret, dec!(0));
    }

    #[test]
    fn a_series_born_this_month_has_no_previous_month() {
        let points = vec![
            point(
                date!(2026 - 11 - 03),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 11 - 25),
                dec!(1050),
                dec!(0),
                dec!(2),
                dec!(1.05),
            ),
        ];
        let s = month_summary(&points, date!(2026 - 11 - 25)).unwrap();
        assert_eq!(s.previous_month_end, None);
        assert_eq!(s.previous_return, None);
        assert_eq!(s.previous, MonthTotals::default());
        let current = s.current_return.unwrap();
        assert!(!current.full);
        assert_eq!(current.ret, dec!(0.05));
    }

    #[test]
    fn a_single_day_has_no_return_to_measure() {
        let points = vec![point(
            date!(2026 - 11 - 25),
            dec!(1000),
            dec!(1000),
            dec!(0),
            dec!(1),
        )];
        let s = month_summary(&points, date!(2026 - 11 - 25)).unwrap();
        assert_eq!(s.current_return, None);
        assert_eq!(s.current.contribution, dec!(1000));
        assert_eq!(month_summary(&[], date!(2026 - 11 - 25)), None);
    }

    #[test]
    fn january_looks_back_to_december_and_november() {
        let points = vec![
            point(
                date!(2026 - 11 - 28),
                dec!(1000),
                dec!(1000),
                dec!(0),
                dec!(1),
            ),
            point(
                date!(2026 - 12 - 30),
                dec!(1020),
                dec!(0),
                dec!(3),
                dec!(1.02),
            ),
            point(
                date!(2027 - 01 - 15),
                dec!(1071),
                dec!(0),
                dec!(1),
                dec!(1.071),
            ),
        ];
        let s = month_summary(&points, date!(2027 - 01 - 15)).unwrap();
        assert_eq!(s.previous_month_end.unwrap().date, date!(2026 - 12 - 30));
        assert_eq!(s.current_return.unwrap().ret, dec!(0.05));
        assert_eq!(s.previous_return.unwrap().ret, dec!(0.02));
        assert_eq!(s.previous.income, dec!(3));
    }

    fn position(by_value: bool, price: Option<Decimal>) -> Position {
        Position {
            asset_id: Uuid::nil(),
            by_value,
            quantity: dec!(10),
            unit_price: price,
        }
    }

    /// A referência sem negócio depois da foto, com a quantidade de agora.
    fn reference(
        position: Option<&Position>,
        snapshot_date: Date,
        price_date: Date,
        today: Date,
    ) -> Option<DayReference> {
        day_reference(
            position,
            snapshot_date,
            price_date,
            today,
            dec!(10),
            DayTrades::default(),
        )
    }

    fn entry(kind: Kind, day: Date, quantity: Decimal, price: Option<Decimal>) -> Entry {
        Entry {
            kind,
            trade_date: day,
            seq: 1,
            quantity,
            unit_price: price,
            fees: Decimal::ZERO,
            fx_rate: Decimal::ONE,
        }
    }

    #[test]
    fn day_reference_is_the_price_of_the_previous_snapshot() {
        let p = position(false, Some(dec!(10.5)));
        let r = reference(
            Some(&p),
            date!(2026 - 11 - 24),
            date!(2026 - 11 - 25),
            date!(2026 - 11 - 25),
        )
        .unwrap();
        assert_eq!(r.previous_close, Some(dec!(10.5)));
        assert_eq!(r.date, date!(2026 - 11 - 24));
        assert_eq!(r.previous_quantity, dec!(10));
        assert_eq!((r.bought, r.sold), (dec!(0), dec!(0)));
        // Fim de semana: a cotação é de sexta, a referência é a quinta.
        assert!(reference(
            Some(&p),
            date!(2026 - 11 - 19),
            date!(2026 - 11 - 20),
            date!(2026 - 11 - 22)
        )
        .is_some());
    }

    #[test]
    fn day_reference_refuses_what_would_measure_something_else() {
        let today = date!(2026 - 11 - 25);
        let ok = position(false, Some(dec!(10)));
        // Sem posição na foto, por valor, sem preço, preço zero (e sem negócio).
        assert!(reference(None, date!(2026 - 11 - 24), today, today).is_none());
        assert!(reference(
            Some(&position(true, Some(dec!(10)))),
            date!(2026 - 11 - 24),
            today,
            today
        )
        .is_none());
        assert!(reference(
            Some(&position(false, None)),
            date!(2026 - 11 - 24),
            today,
            today
        )
        .is_none());
        assert!(reference(
            Some(&position(false, Some(dec!(0)))),
            date!(2026 - 11 - 24),
            today,
            today
        )
        .is_none());
        // A foto não é anterior à cotação.
        assert!(reference(Some(&ok), today, today, today).is_none());
        // Foto longe demais; cotação velha.
        assert!(reference(Some(&ok), date!(2026 - 11 - 10), today, today).is_none());
        assert!(reference(
            Some(&ok),
            date!(2026 - 11 - 14),
            date!(2026 - 11 - 15),
            today
        )
        .is_none());
        // Por valor na foto continua de fora, mesmo com negócio depois.
        let bought = DayTrades {
            quantity: dec!(1),
            bought: dec!(10),
            sold: dec!(0),
        };
        assert!(day_reference(
            Some(&position(true, Some(dec!(10)))),
            date!(2026 - 11 - 24),
            today,
            today,
            dec!(11),
            bought
        )
        .is_none());
    }

    #[test]
    fn trades_after_the_close_split_quantity_from_money() {
        let close = date!(2026 - 11 - 24);
        let today = date!(2026 - 11 - 25);
        let mut buy = entry(Kind::Buy, today, dec!(5), Some(dec!(10.4)));
        buy.fees = dec!(1.2);
        let mut sell = entry(Kind::Sell, today, dec!(2), Some(dec!(11)));
        sell.fees = dec!(0.5);
        let entries = [
            // Antes ou no dia da foto: já está nela.
            entry(Kind::Buy, close, dec!(100), Some(dec!(9))),
            buy,
            sell,
            entry(Kind::Bonus, today, dec!(1), Some(dec!(3))),
            entry(Kind::Split, today, dec!(4), None),
            entry(Kind::ReverseSplit, today, dec!(3), None),
            // Saldo inicial, ajuste e amortização: nem quantidade nem dinheiro.
            entry(Kind::Opening, today, dec!(7), Some(dec!(8))),
            entry(Kind::Adjustment, today, dec!(-1), None),
            entry(Kind::Amortization, today, dec!(10), Some(dec!(0.3))),
        ];
        let trades = day_trades(&entries, close);
        // +5 −2 +1 +4 −3
        assert_eq!(trades.quantity, dec!(5));
        assert_eq!(trades.bought, dec!(53.20));
        assert_eq!(trades.sold, dec!(21.50));
        assert_eq!(day_trades(&entries, today), DayTrades::default());
    }

    #[test]
    fn foreign_trades_are_in_reais() {
        let close = date!(2026 - 11 - 24);
        let mut buy = entry(Kind::Buy, date!(2026 - 11 - 25), dec!(2), Some(dec!(100)));
        buy.fees = dec!(1);
        buy.fx_rate = dec!(5.5);
        let mut sell = entry(Kind::Sell, date!(2026 - 11 - 25), dec!(1), Some(dec!(101)));
        sell.fx_rate = dec!(5.5);
        let trades = day_trades(&[buy, sell], close);
        assert_eq!(trades.bought, dec!(1105.50));
        assert_eq!(trades.sold, dec!(555.50));
    }

    #[test]
    fn day_reference_carries_what_the_trades_changed() {
        let close = date!(2026 - 11 - 24);
        let today = date!(2026 - 11 - 25);
        let p = position(false, Some(dec!(10)));
        // Tinha 10, comprou 5 hoje: no fechamento eram 10.
        let trades = DayTrades {
            quantity: dec!(5),
            bought: dec!(52),
            sold: dec!(0),
        };
        let r = day_reference(Some(&p), close, today, today, dec!(15), trades).unwrap();
        assert_eq!(r.previous_close, Some(dec!(10)));
        assert_eq!(r.previous_quantity, dec!(10));
        assert_eq!(r.bought, dec!(52));
        // Ativo que não estava na foto e foi comprado hoje: entra sem fechamento.
        let new = day_reference(None, close, today, today, dec!(5), trades).unwrap();
        assert_eq!(new.previous_close, None);
        assert_eq!(new.previous_quantity, dec!(0));
        // Na foto sem preço, com venda depois: entra, também sem fechamento.
        let sold = DayTrades {
            quantity: dec!(-2),
            bought: dec!(0),
            sold: dec!(20),
        };
        let unpriced = position(false, None);
        let r = day_reference(Some(&unpriced), close, today, today, dec!(8), sold).unwrap();
        assert_eq!(r.previous_close, None);
        assert_eq!(r.previous_quantity, dec!(10));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// O total de cada mês é a soma dos dias daquele mês (nada some nem conta
        /// em dobro), venha a data de onde vier.
        #[test]
        fn month_totals_partition_the_series(
            flows in proptest::collection::vec((-500i64..2000, 0i64..100), 1..60),
        ) {
            let start = date!(2026 - 09 - 20);
            let mut quota = dec!(1);
            let points: Vec<DayPoint> = flows
                .iter()
                .enumerate()
                .map(|(i, (flow, income))| {
                    quota += dec!(0.001);
                    point(start + Duration::days(i as i64), dec!(1000), Decimal::from(*flow), Decimal::from(*income), quota)
                })
                .collect();
            let today = points.last().unwrap().date;
            let s = month_summary(&points, today).unwrap();
            let (year, month) = (today.year(), u8::from(today.month()));
            let key = |p: &DayPoint| (p.date.year(), u8::from(p.date.month()));
            let previous_key = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
            let sum = |wanted: (i32, u8), pick: fn(&DayPoint) -> Decimal| -> Decimal {
                points.iter().filter(|p| key(p) == wanted).map(pick).sum()
            };
            prop_assert_eq!(s.current.income, sum((year, month), |p| p.income));
            prop_assert_eq!(s.current.contribution, sum((year, month), |p| p.net_flow));
            prop_assert_eq!(s.previous.income, sum(previous_key, |p| p.income));
            prop_assert_eq!(s.previous.contribution, sum(previous_key, |p| p.net_flow));
            if let Some(r) = s.current_return {
                prop_assert!(r.from < r.to);
            }
            if let Some(r) = s.previous_return {
                prop_assert!(r.from < r.to);
            }
        }

        /// A conta que a tela faz com a referência (valor de agora − valor no
        /// fechamento − comprado + vendido) é a variação do preço nas cotas que
        /// já havia mais o ganho de cada negócio contra o próprio preço, menos
        /// as taxas: a compra do dia não ganha o pulo desde o fechamento.
        #[test]
        fn the_day_result_charges_each_trade_its_own_price(
            held in 0i64..500,
            close in 1i64..10_000,
            now in 1i64..10_000,
            trades in proptest::collection::vec((any::<bool>(), 1i64..50, 1i64..10_000, 0i64..500), 0..8),
        ) {
            let day = date!(2026 - 11 - 25);
            let cents = |v: i64| Decimal::new(v, 2);
            let (close, now) = (cents(close), cents(now));
            let mut quantity = Decimal::from(held);
            let mut entries = Vec::new();
            let mut expected = Decimal::from(held) * (now - close);
            for (buy, q, price, fees) in trades {
                let (q, price, fees) = (Decimal::from(q), cents(price), cents(fees));
                // Venda só do que há.
                if !buy && q > quantity {
                    continue;
                }
                let mut e = entry(if buy { Kind::Buy } else { Kind::Sell }, day, q, Some(price));
                e.fees = fees;
                if buy {
                    quantity += q;
                    expected += q * (now - price) - fees;
                } else {
                    quantity -= q;
                    // Venda com taxa maior que o valor rende zero, nunca negativo.
                    expected += (q * price - fees).max(Decimal::ZERO) - q * now;
                }
                entries.push(e);
            }
            let p = Position { asset_id: Uuid::nil(), by_value: false, quantity: Decimal::from(held), unit_price: Some(close) };
            let t = day_trades(&entries, day - Duration::days(1));
            if let Some(r) = day_reference(Some(&p), day - Duration::days(1), day, day, quantity, t) {
                prop_assert_eq!(r.previous_quantity, Decimal::from(held));
                let result = quantity * now - r.previous_quantity * r.previous_close.unwrap_or(now) - r.bought + r.sold;
                prop_assert_eq!(result, expected);
            } else {
                prop_assert!(false, "com preço na foto há referência");
            }
        }

        /// Sem foto anterior à cotação, nunca há referência.
        #[test]
        fn no_reference_unless_the_snapshot_precedes_the_price(gap in -10i64..15, stale in 0i64..10) {
            let price_date = date!(2026 - 11 - 20);
            let snapshot_date = price_date - Duration::days(gap);
            let today = price_date + Duration::days(stale);
            let p = position(false, Some(dec!(10)));
            let r = reference(Some(&p), snapshot_date, price_date, today);
            if !(1..=MAX_REFERENCE_GAP_DAYS).contains(&gap) || stale > MAX_STALE_PRICE_DAYS {
                prop_assert!(r.is_none());
            } else {
                prop_assert!(r.is_some());
            }
        }
    }
}
