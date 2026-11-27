//! A série da carteira refeita a partir do livro de lançamentos. Regra pura:
//! sem banco, sem rede. O método está em `docs/desempenho.md`.
//!
//! Para cada dia, desde o primeiro lançamento:
//!
//! - a **quantidade** de cada ativo é a soma do livro até aquele dia (o
//!   desdobramento, o grupamento e a bonificação já são lançamentos);
//! - o **preço** é o último observado em ou antes do dia, em reais (o
//!   fechamento real, ou o que a foto daquele dia guardou);
//! - o **fluxo** sai dos lançamentos do dia ([`movement`]): compra e venda pelo
//!   dinheiro que de fato entrou ou saiu, com as taxas; saldo inicial e ajuste
//!   de ativo cotado pelo preço do dia; evento societário sem dinheiro;
//! - o **provento** recebido no dia (data de pagamento, valor líquido) é
//!   retorno: entra na cota como dinheiro que a carteira rendeu e saiu para a
//!   pessoa, e não conta como aporte nem como retirada.
//!
//! Com isso o dia vira uma foto igual à da tarefa diária, e a cota anda pela
//! mesma regra de [`super::performance::advance`].

use rust_decimal::Decimal;
use time::Date;
use uuid::Uuid;

use super::allocation::round_cents;
use super::ledger::{sort, Entry, Kind};
use super::performance::{
    advance_day, market_value, net_flow, purchase_cost, Position, Purchase, Quota,
};

/// Um valor por data, em ordem crescente e sem data repetida.
pub type Series = Vec<(Date, Decimal)>;

/// Um ativo como a série o vê.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetHistory {
    pub asset_id: Uuid,
    /// Renda fixa por valor: a quantidade do livro já é o saldo em reais.
    pub by_value: bool,
    /// O livro inteiro do ativo, em qualquer ordem.
    pub entries: Vec<Entry>,
    /// Preço em reais observado em cada data; vale até a observação seguinte
    /// (fim de semana, feriado, dia sem negócio).
    pub prices: Series,
    /// O preço antes da primeira observação. Só o ativo de preço manual tem: o
    /// preço que a pessoa informou é a única coisa que se sabe dele. `None`: o
    /// ativo fica sem preço (fora da carteira) até a primeira observação.
    pub fallback_price: Option<Decimal>,
}

/// De onde a série continua: a foto do dia anterior ao primeiro dia refeito.
#[derive(Debug, Clone, Copy)]
pub struct Start<'a> {
    pub quota: Quota,
    pub positions: &'a [Position],
}

/// Um dia refeito: a foto que vai para `portfolio_snapshots`.
#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub date: Date,
    pub market_value: Decimal,
    /// Aporte (+) ou retirada (−) do dia, em centavos.
    pub net_flow: Decimal,
    /// Proventos líquidos recebidos no dia, em centavos.
    pub income: Decimal,
    pub quota: Quota,
    pub positions: Vec<Position>,
}

/// O dinheiro que um lançamento põe (+) ou tira (−) da carteira e quanto ele
/// muda a quantidade, em reais. `None`: o lançamento não é dinheiro que se
/// conhece — o saldo inicial e o ajuste de um ativo cotado entram pelo preço do
/// dia (é posição que veio de fora, como o cadastro), e o ajuste de renda fixa
/// por valor é o rendimento que a pessoa informou.
pub fn movement(entry: &Entry, by_value: bool) -> Option<Purchase> {
    let q = entry.quantity;
    let price = entry.unit_price.unwrap_or(Decimal::ZERO);
    let fx = entry.fx_rate;
    let gross = q * price * fx;
    let fees = entry.fees * fx;
    let (quantity, value) = match entry.kind {
        Kind::Buy => (q, gross + fees),
        Kind::Sell | Kind::Redemption => (-q, -(gross - fees)),
        // Devolução de capital: a quantidade fica, o dinheiro sai.
        Kind::Amortization => (Decimal::ZERO, -gross),
        // Evento societário: mais (ou menos) cotas, nenhum dinheiro.
        Kind::Split | Kind::Bonus => (q, Decimal::ZERO),
        Kind::ReverseSplit => (-q, Decimal::ZERO),
        // Na renda fixa por valor o saldo inicial é o próprio valor aplicado.
        Kind::Opening if by_value => (q, q),
        Kind::Opening | Kind::Adjustment => return None,
    };
    Some(Purchase {
        asset_id: Uuid::nil(),
        quantity,
        value,
    })
}

/// Desfaz o ajuste do Yahoo: o `close` de antes de cada desdobramento (ou
/// bonificação, que o Yahoo registra como desdobramento) vem dividido pelo
/// fator do evento. `splits` traz a data do evento e o fator (cotas novas por
/// cota antiga, `numerator / denominator`). O fechamento real de um dia é o
/// ajustado vezes o produto dos fatores dos eventos **depois** dele.
pub fn unadjust_splits(closes: &Series, splits: &[(Date, Decimal)]) -> Series {
    closes
        .iter()
        .map(|(date, close)| {
            let factor: Decimal = splits
                .iter()
                .filter(|(when, factor)| when > date && *factor > Decimal::ZERO)
                .map(|(_, factor)| *factor)
                .product();
            (*date, (close * factor).round_dp(8))
        })
        .collect()
}

/// O último valor em ou antes de `date`.
fn value_at(series: &Series, date: Date) -> Option<Decimal> {
    let after = series.partition_point(|(d, _)| *d <= date);
    after.checked_sub(1).map(|i| series[i].1)
}

/// O preço em reais nas datas em que o fechamento ou o câmbio mudou: cada um
/// vale até o próximo. Sem câmbio (`None`), o ativo já é em reais.
pub fn in_reais(closes: &Series, fx: Option<&Series>) -> Series {
    let Some(fx) = fx else {
        return closes.clone();
    };
    let Some((first, _)) = closes.first() else {
        return Vec::new();
    };
    let mut dates: Vec<Date> = closes
        .iter()
        .map(|(d, _)| *d)
        .chain(fx.iter().map(|(d, _)| *d).filter(|d| d >= first))
        .collect();
    dates.sort();
    dates.dedup();
    dates
        .into_iter()
        .filter_map(|date| {
            let close = value_at(closes, date)?;
            let rate = value_at(fx, date)?;
            Some((date, (close * rate).round_dp(8)))
        })
        .collect()
}

/// Junta duas fontes de preço: a primeira vale onde tem a data; a segunda
/// preenche as datas que faltam nela.
pub fn merge_prices(primary: &Series, secondary: &Series) -> Series {
    let mut out: Series = primary.clone();
    out.extend(
        secondary
            .iter()
            .filter(|(d, _)| primary.binary_search_by_key(d, |(p, _)| *p).is_err())
            .copied(),
    );
    out.sort_by_key(|(d, _)| *d);
    out
}

/// Um ativo durante a reconstrução: o livro em ordem e onde a leitura parou.
struct Cursor<'a> {
    asset: &'a AssetHistory,
    entries: Vec<Entry>,
    next: usize,
    quantity: Decimal,
}

impl<'a> Cursor<'a> {
    fn new(asset: &'a AssetHistory, from: Date) -> Self {
        let mut entries = asset.entries.clone();
        sort(&mut entries);
        let next = entries.partition_point(|e| e.trade_date < from);
        let quantity = entries[..next]
            .iter()
            .map(|e| e.kind.quantity_delta(e.quantity))
            .sum();
        Self {
            asset,
            entries,
            next,
            quantity,
        }
    }

    /// Aplica os lançamentos do dia; devolve se houve algum.
    fn advance_to(&mut self, date: Date, purchases: &mut Vec<Purchase>) -> bool {
        let start = self.next;
        while let Some(entry) = self.entries.get(self.next).filter(|e| e.trade_date <= date) {
            self.quantity += entry.kind.quantity_delta(entry.quantity);
            if let Some(m) = movement(entry, self.asset.by_value) {
                purchases.push(Purchase {
                    asset_id: self.asset.asset_id,
                    ..m
                });
            }
            self.next += 1;
        }
        self.next > start
    }

    fn price(&self, date: Date) -> Option<Decimal> {
        value_at(&self.asset.prices, date)
            .or(self.asset.fallback_price)
            .filter(|p| *p > Decimal::ZERO)
    }
}

/// Refaz a série de `from` a `to`, inclusive, um dia por data corrida (o
/// fim de semana repete o preço de sexta, como a foto diária). `start` é a foto
/// do dia anterior a `from`; sem ela, `from` é o primeiro dia da série e a cota
/// começa em 1.
pub fn rebuild(
    assets: &[AssetHistory],
    income: &[(Date, Decimal)],
    start: Option<Start<'_>>,
    from: Date,
    to: Date,
) -> Vec<Day> {
    let mut cursors: Vec<Cursor<'_>> = assets.iter().map(|a| Cursor::new(a, from)).collect();
    let mut previous: Vec<Position> = start.map(|s| s.positions.to_vec()).unwrap_or_default();
    let mut quota = start.map(|s| s.quota);
    let mut days = Vec::new();
    let mut date = from;
    while date <= to {
        let mut purchases = Vec::new();
        let mut positions = Vec::with_capacity(cursors.len());
        for cursor in &mut cursors {
            let touched = cursor.advance_to(date, &mut purchases);
            // Ativo zerado sai da foto, menos no dia em que zerou: a venda
            // daquele dia precisa da posição para virar retirada pelo valor
            // da venda, e não pelo valor de ontem.
            if cursor.quantity.is_zero() && !touched {
                continue;
            }
            let by_value = cursor.asset.by_value;
            positions.push(Position {
                asset_id: cursor.asset.asset_id,
                by_value,
                quantity: cursor.quantity.normalize(),
                unit_price: if by_value { None } else { cursor.price(date) },
            });
        }
        let value = market_value(&positions);
        let flow = net_flow(&previous, &positions, &purchases);
        let received = round_cents(
            income
                .iter()
                .filter(|(d, _)| *d == date)
                .map(|(_, v)| *v)
                .sum(),
        );
        // Provento é retorno: soma ao patrimônio do dia na conta da cota, como
        // o dinheiro que a carteira rendeu antes de sair para a pessoa. O custo
        // da compra (pago acima do fechamento, taxas) é de todo o dinheiro do
        // dia, não só do de ontem (`advance_day`).
        let cost = purchase_cost(&positions, &purchases);
        let next = advance_day(quota, value, flow - received, cost);
        days.push(Day {
            date,
            market_value: value,
            net_flow: flow,
            income: received,
            quota: next,
            positions: positions.clone(),
        });
        quota = Some(next);
        previous = positions;
        match date.next_day() {
            Some(next) => date = next,
            None => break,
        }
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::performance::{monthly_table, period_return, QuotaPoint};
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

    fn fees(mut e: Entry, fees: Decimal) -> Entry {
        e.fees = fees;
        e
    }

    fn asset(id: Uuid, entries: Vec<Entry>, prices: Series) -> AssetHistory {
        AssetHistory {
            asset_id: id,
            by_value: false,
            entries,
            prices,
            fallback_price: None,
        }
    }

    fn quotas(days: &[Day]) -> Vec<QuotaPoint> {
        days.iter()
            .map(|d| QuotaPoint {
                date: d.date,
                value: d.quota.value,
            })
            .collect()
    }

    const D1: Date = date!(2025 - 03 - 03);
    const D2: Date = date!(2025 - 03 - 04);
    const D3: Date = date!(2025 - 03 - 05);
    const D4: Date = date!(2025 - 03 - 06);
    const D5: Date = date!(2025 - 03 - 07);
    const SUNDAY: Date = date!(2025 - 03 - 09);

    /// O caso fictício de `docs/desempenho.md` ("Conferência passo a passo"),
    /// com cada número feito à mão.
    fn hand_case() -> Vec<AssetHistory> {
        let (fii, stock, cdb) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        vec![
            asset(
                fii,
                vec![
                    fees(entry(Kind::Buy, D1, 1, dec!(10), Some(dec!(100))), dec!(1)),
                    fees(
                        entry(Kind::Sell, D5, 6, dec!(5), Some(dec!(104))),
                        dec!(0.5),
                    ),
                ],
                vec![
                    (D1, dec!(101)),
                    (D2, dec!(102)),
                    (D3, dec!(103)),
                    (D4, dec!(103)),
                    (D5, dec!(104)),
                ],
            ),
            asset(
                stock,
                vec![
                    entry(Kind::Buy, D2, 3, dec!(20), Some(dec!(40))),
                    entry(Kind::Split, D4, 4, dec!(20), None),
                ],
                // O Yahoo entrega 20,50 e 21,00 antes do desdobro de 1 para 2;
                // desfeito o ajuste, é o que se pagava no dia.
                unadjust_splits(
                    &vec![
                        (D2, dec!(20.5)),
                        (D3, dec!(21)),
                        (D4, dec!(21.5)),
                        (D5, dec!(22)),
                    ],
                    &[(D4, dec!(2))],
                ),
            ),
            AssetHistory {
                by_value: true,
                ..asset(
                    cdb,
                    vec![
                        entry(Kind::Opening, D1, 2, dec!(1000), Some(dec!(1))),
                        entry(Kind::Adjustment, D4, 5, dec!(5), None),
                    ],
                    Vec::new(),
                )
            },
        ]
    }

    #[test]
    fn hand_computed_case() {
        let income = [(D3, dec!(10))];
        let days = rebuild(&hand_case(), &income, None, D1, SUNDAY);
        assert_eq!(days.len(), 7);
        let row = |d: &Day| (d.market_value, d.net_flow, d.income);
        assert_eq!(row(&days[0]), (dec!(2010), dec!(2001), dec!(0)));
        assert_eq!(row(&days[1]), (dec!(2840), dec!(800), dec!(0)));
        assert_eq!(row(&days[2]), (dec!(2870), dec!(0), dec!(10)));
        assert_eq!(row(&days[3]), (dec!(2895), dec!(0), dec!(0)));
        assert_eq!(row(&days[4]), (dec!(2405), dec!(-519.5), dec!(0)));
        // Fim de semana: o preço de sexta, nada se mexe.
        assert_eq!(row(&days[6]), (dec!(2405), dec!(0), dec!(0)));
        assert!((days[6].quota.value - days[4].quota.value).abs() < dec!(0.0000000001));
        // A cota, dia a dia: (patrimônio − fluxo + provento) / cotas de ontem.
        // Em D2 as 20 WEGE3 custaram 800 e valem 820 no fechamento (+20): o
        // preço anda sobre as cotas de ontem ((2.840 − 800 − 20) / 2.010) e o
        // ganho da compra é de todo o dinheiro do dia (× 2.840 / 2.820).
        let d2 = dec!(2020) / dec!(2010) * dec!(2840) / dec!(2820);
        let exact = [
            Decimal::ONE,
            d2,
            d2 * dec!(2880) / dec!(2840),
            d2 * dec!(2880) / dec!(2840) * dec!(2895) / dec!(2870),
            d2 * dec!(2880) / dec!(2840) * dec!(2924.5) / dec!(2870),
        ];
        for (day, want) in days.iter().zip(exact) {
            assert!(
                (day.quota.value - want).abs() < dec!(0.000000001),
                "{}: {} != {want}",
                day.date,
                day.quota.value
            );
        }
        let all = period_return(&quotas(&days), D1, SUNDAY).unwrap();
        assert_eq!((all.ret * dec!(100)).round_dp(4), dec!(4.5848));
        // Aplicado e ganho: 2001 + 800 − 519,50; ganho de preço 2405 − 2281,50.
        let invested: Decimal = days.iter().map(|d| d.net_flow).sum();
        assert_eq!(invested, dec!(2281.5));
        assert_eq!(days[6].market_value - invested, dec!(123.5));
        // O desdobro: 40 cotas a 21,50 no dia 4, 20 a 42 no dia 3.
        let stock = |d: &Day| {
            d.positions
                .iter()
                .find(|p| p.asset_id == Uuid::from_u128(2))
                .cloned()
        };
        assert_eq!(
            stock(&days[2]).map(|p| (p.quantity, p.unit_price)),
            Some((dec!(20), Some(dec!(42))))
        );
        assert_eq!(
            stock(&days[3]).map(|p| (p.quantity, p.unit_price)),
            Some((dec!(40), Some(dec!(21.5))))
        );
    }

    #[test]
    fn continuing_from_a_stored_day_gives_the_same_series() {
        let assets = hand_case();
        let income = [(D3, dec!(10))];
        let whole = rebuild(&assets, &income, None, D1, SUNDAY);
        let start = Start {
            quota: whole[2].quota,
            positions: &whole[2].positions,
        };
        let tail = rebuild(&assets, &income, Some(start), D4, SUNDAY);
        assert_eq!(tail, whole[3..].to_vec());
    }

    #[test]
    fn movements_by_kind() {
        let e = |kind, q, p| entry(kind, D1, 1, q, p);
        let m = |entry: Entry, by_value| movement(&entry, by_value).map(|p| (p.quantity, p.value));
        let mut dollar = fees(e(Kind::Buy, dec!(2), Some(dec!(10))), dec!(1));
        dollar.fx_rate = dec!(5);
        assert_eq!(m(dollar, false), Some((dec!(2), dec!(105))));
        assert_eq!(
            m(fees(e(Kind::Sell, dec!(2), Some(dec!(10))), dec!(1)), false),
            Some((dec!(-2), dec!(-19)))
        );
        assert_eq!(
            m(e(Kind::Redemption, dec!(500), Some(dec!(1))), true),
            Some((dec!(-500), dec!(-500)))
        );
        assert_eq!(
            m(e(Kind::Amortization, dec!(10), Some(dec!(3))), false),
            Some((dec!(0), dec!(-30)))
        );
        assert_eq!(
            m(e(Kind::Split, dec!(10), None), false),
            Some((dec!(10), dec!(0)))
        );
        assert_eq!(
            m(e(Kind::Bonus, dec!(1), Some(dec!(9))), false),
            Some((dec!(1), dec!(0)))
        );
        assert_eq!(
            m(e(Kind::ReverseSplit, dec!(9), None), false),
            Some((dec!(-9), dec!(0)))
        );
        assert_eq!(m(e(Kind::Opening, dec!(10), Some(dec!(5))), false), None);
        assert_eq!(
            m(e(Kind::Opening, dec!(700), None), true),
            Some((dec!(700), dec!(700)))
        );
        assert_eq!(m(e(Kind::Adjustment, dec!(-3), None), false), None);
        assert_eq!(m(e(Kind::Adjustment, dec!(30), None), true), None);
    }

    #[test]
    fn opening_and_adjustment_enter_at_the_price_of_the_day() {
        let id = Uuid::from_u128(9);
        // Saldo inicial com preço médio antigo (R$ 5): entra pelo preço do dia.
        let a = asset(
            id,
            vec![
                entry(Kind::Opening, D1, 1, dec!(10), Some(dec!(5))),
                entry(Kind::Adjustment, D2, 2, dec!(-4), None),
            ],
            vec![(D1, dec!(20)), (D2, dec!(25))],
        );
        let days = rebuild(&[a], &[], None, D1, D2);
        assert_eq!(days[0].net_flow, dec!(200));
        // Tirou 4 à mão: retirada pelo preço do dia.
        assert_eq!(days[1].net_flow, dec!(-100));
        assert_eq!(days[1].quota.value, dec!(1.25));
    }

    #[test]
    fn asset_without_price_stays_out_until_it_has_one() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let priced = asset(
            a,
            vec![entry(Kind::Buy, D1, 1, dec!(10), Some(dec!(10)))],
            vec![(D1, dec!(10)), (D2, dec!(11)), (D3, dec!(11))],
        );
        // Comprado no dia 1, mas só tem preço a partir do dia 3.
        let late = asset(
            b,
            vec![entry(Kind::Buy, D1, 2, dec!(5), Some(dec!(20)))],
            vec![(D3, dec!(30))],
        );
        let days = rebuild(&[priced, late], &[], None, D1, D3);
        assert_eq!(days[0].market_value, dec!(100));
        assert_eq!(days[0].net_flow, dec!(100));
        assert_eq!(days[1].quota.value, dec!(1.1));
        // Ganhou preço: entra inteiro como fluxo, sem virar ganho.
        assert_eq!(
            (days[2].market_value, days[2].net_flow),
            (dec!(260), dec!(150))
        );
        assert_eq!(days[2].quota.value, dec!(1.1));
    }

    #[test]
    fn selling_everything_leaves_at_the_sale_value() {
        let id = Uuid::from_u128(4);
        let a = asset(
            id,
            vec![
                entry(Kind::Buy, D1, 1, dec!(10), Some(dec!(10))),
                entry(Kind::Sell, D2, 2, dec!(10), Some(dec!(12))),
                entry(Kind::Buy, D4, 3, dec!(1), Some(dec!(50))),
            ],
            vec![(D1, dec!(10)), (D2, dec!(11)), (D4, dec!(50))],
        );
        let days = rebuild(&[a], &[], None, D1, D4);
        // Vendeu a 12 com o fechamento a 11: o dia mede a venda.
        assert_eq!(
            (days[1].market_value, days[1].net_flow),
            (dec!(0), dec!(-120))
        );
        assert_eq!(days[1].quota.value, dec!(1.2));
        assert_eq!(days[1].positions.len(), 1);
        // Carteira vazia: a cota espera o dinheiro voltar.
        assert!(days[2].positions.is_empty());
        assert_eq!(days[2].quota.value, dec!(1.2));
        assert_eq!(
            (days[3].market_value, days[3].net_flow),
            (dec!(50), dec!(50))
        );
        assert_eq!(days[3].quota.value, dec!(1.2));
    }

    #[test]
    fn amortization_is_money_out_and_income_after_selling_out_is_not_measured() {
        let id = Uuid::from_u128(5);
        let a = asset(
            id,
            vec![
                entry(Kind::Buy, D1, 1, dec!(10), Some(dec!(100))),
                entry(Kind::Amortization, D2, 2, dec!(10), Some(dec!(5))),
            ],
            // O preço cai o que foi devolvido.
            vec![(D1, dec!(100)), (D2, dec!(95))],
        );
        let days = rebuild(&[a], &[], None, D1, D2);
        assert_eq!(days[1].net_flow, dec!(-50));
        assert_eq!(days[1].quota.value, dec!(1));
    }

    #[test]
    fn manual_price_fills_the_days_before_any_observation() {
        let id = Uuid::from_u128(6);
        let a = AssetHistory {
            fallback_price: Some(dec!(7)),
            ..asset(
                id,
                vec![entry(Kind::Opening, D1, 1, dec!(10), None)],
                vec![(D3, dec!(8))],
            )
        };
        let days = rebuild(&[a], &[], None, D1, D3);
        assert_eq!(days[0].market_value, dec!(70));
        assert_eq!(days[2].market_value, dec!(80));
    }

    #[test]
    fn price_helpers() {
        let closes = vec![(D1, dec!(10)), (D3, dec!(11))];
        let fx = vec![(date!(2025 - 02 - 28), dec!(5)), (D2, dec!(5.5))];
        assert_eq!(
            in_reais(&closes, Some(&fx)),
            vec![(D1, dec!(50)), (D2, dec!(55)), (D3, dec!(60.5))]
        );
        assert_eq!(in_reais(&closes, None), closes);
        assert!(in_reais(&Vec::new(), Some(&fx)).is_empty());
        let snapshots = vec![(D1, dec!(9)), (D2, dec!(10.5))];
        assert_eq!(
            merge_prices(&closes, &snapshots),
            vec![(D1, dec!(10)), (D2, dec!(10.5)), (D3, dec!(11))]
        );
        // Dois eventos depois do dia: os fatores se multiplicam.
        let adjusted = vec![(D1, dec!(1)), (D3, dec!(2)), (D5, dec!(4))];
        let splits = [(D2, dec!(2)), (D4, dec!(1.05)), (D5, dec!(0))];
        assert_eq!(
            unadjust_splits(&adjusted, &splits),
            vec![(D1, dec!(2.1)), (D3, dec!(2.1)), (D5, dec!(4))]
        );
    }

    #[test]
    fn empty_range_and_no_assets() {
        assert!(rebuild(&[], &[], None, D2, D1).is_empty());
        let days = rebuild(&[], &[(D1, dec!(5))], None, D1, D1);
        assert_eq!(
            (days[0].market_value, days[0].quota.value),
            (dec!(0), dec!(1))
        );
    }

    // ------------------------------------------------------------ proptest --

    fn arb_prices(days: usize) -> impl Strategy<Value = Vec<Decimal>> {
        prop::collection::vec(100u32..100_000, days).prop_map(|c| {
            c.into_iter()
                .map(|c| Decimal::new(i64::from(c), 2))
                .collect()
        })
    }

    fn series(prices: &[Decimal]) -> Series {
        prices
            .iter()
            .enumerate()
            .map(|(i, p)| (D1 + Duration::days(i as i64), *p))
            .collect()
    }

    fn day(i: usize) -> Date {
        D1 + Duration::days(i as i64)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        /// Comprar pelo fechamento do dia, em qualquer dia e quantidade, não
        /// muda a cota de dia nenhum.
        #[test]
        fn contribution_does_not_change_the_quota(
            prices in arb_prices(20),
            initial in 1u32..500,
            buys in prop::collection::vec((1usize..20, 1u32..300), 0..6),
        ) {
            let id = Uuid::from_u128(1);
            let opening = entry(Kind::Opening, D1, 0, Decimal::from(initial), None);
            let without = asset(id, vec![opening.clone()], series(&prices));
            let mut entries = vec![opening];
            for (n, (at, q)) in buys.iter().enumerate() {
                entries.push(entry(Kind::Buy, day(*at), n as i64 + 1, Decimal::from(*q), Some(prices[*at])));
            }
            let with = asset(id, entries, series(&prices));
            let last = day(prices.len() - 1);
            let a = rebuild(&[without], &[], None, D1, last);
            let b = rebuild(&[with], &[], None, D1, last);
            for (x, y) in a.iter().zip(&b) {
                prop_assert!((x.quota.value - y.quota.value).abs() < dec!(0.000001), "{}: {} != {}", x.date, x.quota.value, y.quota.value);
            }
        }

        /// Desdobramento (ou grupamento) com o preço ajustado pelo Yahoo:
        /// desfeito o ajuste, o patrimônio e a cota são os de quem não teve
        /// evento nenhum.
        #[test]
        fn split_does_not_change_net_worth(
            prices in arb_prices(15),
            held in 1u32..200,
            at in 1usize..15,
            factor in 2u32..10,
            reverse in prop::bool::ANY,
        ) {
            let id = Uuid::from_u128(1);
            let held = Decimal::from(held * factor);
            let k = Decimal::from(factor);
            let opening = entry(Kind::Opening, D1, 0, held, None);
            let plain = asset(id, vec![opening.clone()], series(&prices));
            // Valor econômico igual; depois do evento, `k` vezes mais (ou menos) cotas.
            let (event, after, ratio) = if reverse {
                (entry(Kind::ReverseSplit, day(at), 1, held - held / k, None), held / k, Decimal::ONE / k)
            } else {
                (entry(Kind::Split, day(at), 1, held * (k - Decimal::ONE), None), held * k, k)
            };
            prop_assert!(after > Decimal::ZERO);
            // O que o Yahoo mostra hoje: todo o passado dividido pelo fator.
            let adjusted: Series = prices.iter().enumerate()
                .map(|(i, p)| (day(i), (p / ratio).round_dp(8)))
                .collect();
            let real = unadjust_splits(&adjusted, &[(day(at), ratio)]);
            let split = asset(id, vec![opening, event], real);
            let last = day(prices.len() - 1);
            let a = rebuild(&[plain], &[], None, D1, last);
            let b = rebuild(&[split], &[], None, D1, last);
            for (x, y) in a.iter().zip(&b) {
                prop_assert!((x.market_value - y.market_value).abs() <= dec!(0.01), "{}: {} != {}", x.date, x.market_value, y.market_value);
                prop_assert!(x.date == D1 || y.net_flow.is_zero(), "{}: fluxo {}", x.date, y.net_flow);
                prop_assert!((x.quota.value - y.quota.value).abs() < dec!(0.000001));
            }
        }

        /// Provento recebido com a carteira de pé aumenta a cota do dia e dali
        /// em diante, na mesma proporção.
        #[test]
        fn income_raises_the_quota(
            prices in arb_prices(12),
            held in 1u32..500,
            at in 1usize..12,
            cents in 1i64..1_000_000,
        ) {
            let id = Uuid::from_u128(1);
            let a = asset(id, vec![entry(Kind::Opening, D1, 0, Decimal::from(held), None)], series(&prices));
            let last = day(prices.len() - 1);
            let without = rebuild(std::slice::from_ref(&a), &[], None, D1, last);
            let with = rebuild(&[a], &[(day(at), Decimal::new(cents, 2))], None, D1, last);
            for (x, y) in without.iter().zip(&with) {
                if x.date < day(at) {
                    prop_assert_eq!(x.quota, y.quota);
                } else {
                    prop_assert!(y.quota.value > x.quota.value, "{}: {} <= {}", x.date, y.quota.value, x.quota.value);
                    prop_assert_eq!(x.market_value, y.market_value);
                    prop_assert_eq!(x.net_flow, y.net_flow);
                }
            }
            // E a tabela mês × ano continua batendo com a série inteira.
            let whole = period_return(&quotas(&with), D1, last).unwrap().ret;
            let table: Decimal = monthly_table(&quotas(&with)).iter()
                .fold(Decimal::ONE, |acc, row| acc * (Decimal::ONE + row.total));
            prop_assert!((table - Decimal::ONE - whole).abs() < dec!(0.0000000001));
        }
    }
}
