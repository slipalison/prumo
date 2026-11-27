//! Carteira e anúncios FICTÍCIOS: emissor `ABCD`, fundo `FICT`.

use super::*;
use crate::domain::income::DividendPoint;
use crate::domain::ledger::Kind;
use proptest::prelude::*;
use rust_decimal_macros::dec;
use time::macros::{date, datetime};
use time::Weekday;

const TODAY: Date = date!(2026 - 11 - 27);
const ISIN: &str = "BRABCDACNPR1";

fn cash(label: &str, record: Date, rate: Decimal, pay: Option<Date>) -> CashAnnouncement {
    CashAnnouncement {
        issuer: "ABCD".into(),
        isin: ISIN.into(),
        label: label.into(),
        record_date: record,
        rate,
        pay_date: pay,
        approved_on: None,
        related_to: None,
    }
}

fn stock(label: &str, record: Date, factor: Decimal) -> StockAnnouncement {
    StockAnnouncement {
        issuer: "ABCD".into(),
        isin: ISIN.into(),
        label: label.into(),
        record_date: record,
        factor,
        approved_on: None,
    }
}

fn point(ex_date: Date, amount: Decimal) -> UnitPayment {
    UnitPayment {
        ex_date,
        gross: amount,
        net: amount,
    }
}

fn days_ago(days: i64) -> Date {
    TODAY.saturating_sub(Duration::days(days))
}

fn entry(kind: Kind, day: Date, seq: i64, quantity: Decimal) -> Entry {
    Entry {
        kind,
        trade_date: day,
        seq,
        quantity,
        unit_price: Some(dec!(10)),
        fees: Decimal::ZERO,
        fx_rate: Decimal::ONE,
    }
}

#[test]
fn jcp_loses_the_withholding_of_its_pay_date() {
    let announced = [
        // Pago em 2025: 15%.
        cash(
            "JRS CAP PROPRIO",
            date!(2025 - 12 - 15),
            dec!(1),
            Some(date!(2025 - 12 - 22)),
        ),
        // Pago em 2026: 17,5%.
        cash(
            "JRS CAP PROPRIO",
            date!(2026 - 02 - 10),
            dec!(1),
            Some(date!(2026 - 03 - 01)),
        ),
        // A definir: o pagamento ainda vai cair, vale a alíquota de hoje.
        cash("JRS CAP PROPRIO", date!(2026 - 06 - 10), dec!(1), None),
        cash(
            "DIVIDENDO",
            date!(2026 - 07 - 10),
            dec!(1),
            Some(date!(2026 - 07 - 20)),
        ),
        cash(
            "RENDIMENTO",
            date!(2026 - 08 - 10),
            dec!(1),
            Some(date!(2026 - 08 - 20)),
        ),
    ];
    let payments = b3_unit_payments(&announced, &[], AssetClass::BrStocks, TODAY);
    let nets: Vec<Decimal> = payments.iter().map(|p| p.net).collect();
    assert_eq!(
        nets,
        [dec!(0.85), dec!(0.825), dec!(0.825), dec!(1), dec!(1)]
    );
    assert!(payments.iter().all(|p| p.gross == dec!(1)));
    let year = trailing(&payments, TODAY);
    assert_eq!(year.gross, dec!(5));
    assert_eq!(year.net, dec!(4.5));
    // No FII o rendimento é isento, como o dividendo.
    let reit = b3_unit_payments(&announced[4..], &[], AssetClass::BrReits, TODAY);
    assert_eq!(reit[0].net, dec!(1));
}

#[test]
fn stock_events_bring_the_rate_to_todays_shares() {
    let events = [
        // Bonificação de 2%: cada cota da época virou 1,02.
        stock("BONIFICACAO", date!(2025 - 12 - 18), dec!(2)),
        // Desdobramento 1:5 (a B3 publica 400%).
        stock("DESDOBRAMENTO", date!(2026 - 04 - 28), dec!(400)),
        // Outro rótulo não mexe na base.
        stock("RESGATE", date!(2026 - 05 - 10), dec!(50)),
        // Com data ex no futuro: a quantidade de hoje ainda não tem as cotas novas.
        stock("BONIFICACAO", date!(2026 - 11 - 27), dec!(10)),
    ];
    let rates = [
        (date!(2025 - 12 - 10), dec!(5.10)),
        // A mesma data com da bonificação: o dinheiro é só das cotas velhas.
        (date!(2025 - 12 - 18), dec!(5.10)),
        (date!(2026 - 01 - 15), dec!(5)),
        (date!(2026 - 06 - 15), dec!(1)),
    ];
    let announced: Vec<CashAnnouncement> = rates
        .iter()
        .map(|(record, rate)| cash("DIVIDENDO", *record, *rate, Some(*record)))
        .collect();
    let gross: Vec<Decimal> = b3_unit_payments(&announced, &events, AssetClass::BrStocks, TODAY)
        .iter()
        .map(|p| p.gross.round_dp(8))
        .collect();
    assert_eq!(gross, [dec!(1), dec!(1), dec!(1), dec!(1)]);

    // Grupamento de 10 para 1: a cota de hoje vale dez das de antes.
    let reverse = [stock("GRUPAMENTO", date!(2026 - 03 - 02), dec!(0.1))];
    assert_eq!(
        shares_after(date!(2026 - 01 - 10), &reverse, TODAY),
        dec!(0.1)
    );
    let before = b3_unit_payments(
        &[cash("DIVIDENDO", date!(2026 - 01 - 10), dec!(0.05), None)],
        &reverse,
        AssetClass::BrStocks,
        TODAY,
    );
    assert_eq!(before[0].gross, dec!(0.5));
    assert_eq!(
        shares_after(date!(2026 - 03 - 03), &reverse, TODAY),
        dec!(1)
    );
}

#[test]
fn the_window_goes_by_the_ex_date_after_the_record_date() {
    // Hoje é sexta, 27/11/2026: a janela é (27/11/2025, 27/11/2026].
    let rate = |record: Date| cash("DIVIDENDO", record, dec!(1), Some(record));
    let inside = |record: Date| {
        trailing(
            &b3_unit_payments(&[rate(record)], &[], AssetClass::BrStocks, TODAY),
            TODAY,
        )
        .gross
            == dec!(1)
    };
    // Data com 26/11/2025 → data ex 27/11/2025: a borda de baixo é aberta.
    assert!(!inside(date!(2025 - 11 - 26)));
    // Data com 27/11/2025 → data ex 28/11/2025: dentro.
    assert!(inside(date!(2025 - 11 - 27)));
    // Data com ontem, data ex hoje: dentro.
    assert!(inside(date!(2026 - 11 - 26)));
    // Data com hoje: a data ex é segunda que vem, ainda não aconteceu.
    assert!(!inside(TODAY));
    // 31/12 sem pregão e 1/1 feriado: a data com de 30/12 tem data ex em 2/1.
    let year_end = b3_unit_payments(
        &[rate(date!(2025 - 12 - 30))],
        &[],
        AssetClass::BrStocks,
        TODAY,
    );
    assert_eq!(year_end[0].ex_date, date!(2026 - 01 - 02));
    assert!(b3_window_start(TODAY) < date!(2025 - 11 - 26));
}

/// A data com no dia 28 de cada mês (ou na sexta antes, no fim de semana).
fn reit_on_the_28th() -> Vec<CashAnnouncement> {
    let mut list = Vec::new();
    for year in 2025..=2027 {
        for month in 1..=12u8 {
            let mut day =
                Date::from_calendar_date(year, time::Month::try_from(month).unwrap(), 28).unwrap();
            while matches!(day.weekday(), Weekday::Saturday | Weekday::Sunday) {
                day = day.previous_day().unwrap();
            }
            list.push(cash("RENDIMENTO", day, dec!(1), Some(day)));
        }
    }
    list
}

#[test]
fn a_monthly_reit_can_have_eleven_or_thirteen_payments_in_the_window() {
    // A data ex anda com os dias úteis: 365 dias podem ter dois pagamentos do
    // mesmo mês do ano, ou nenhum. É método, e está em
    // docs/proventos-automaticos.md (a janela por mês civil erra mais).
    let reit = reit_on_the_28th();
    let count = |today: Date| {
        b3_unit_payments(&reit, &[], AssetClass::BrReits, today)
            .iter()
            .filter(|p| in_window(p.ex_date, today))
            .count()
    };
    // Fevereiro de 2026: data com 27/02 (sexta), ex 02/03. Fevereiro de 2027:
    // data com 26/02, ex 01/03. Em 01/03/2027 os dois estão na janela.
    assert_eq!(count(date!(2027 - 03 - 01)), 13);
    // Maio de 2026: ex 29/05/2026, que acabou de sair; maio de 2027: ex
    // 31/05/2027, que ainda não chegou.
    assert_eq!(count(date!(2027 - 05 - 30)), 11);
    assert_eq!(count(date!(2027 - 05 - 31)), 12);
    assert_eq!(count(date!(2027 - 07 - 15)), 12);
}

#[test]
fn yahoo_is_the_reserve_when_the_b3_is_missing_or_down() {
    let now = datetime!(2026-11-27 12:00 UTC);
    let fresh = Some(datetime!(2026-11-26 10:00 UTC));
    let stale = Some(datetime!(2026-11-20 10:00 UTC));
    assert_eq!(pick_source(fresh, true, now), Some(EstimateSource::B3));
    assert_eq!(pick_source(fresh, false, now), Some(EstimateSource::B3));
    // A B3 fora do ar há uma semana: vale o Yahoo, que está em dia.
    assert_eq!(pick_source(stale, true, now), Some(EstimateSource::Yahoo));
    // Sem Yahoo, a B3 velha é melhor que nada.
    assert_eq!(pick_source(stale, false, now), Some(EstimateSource::B3));
    // A B3 não tem o ativo (sem ISIN ou emissor nunca buscado).
    assert_eq!(pick_source(None, true, now), Some(EstimateSource::Yahoo));
    assert_eq!(pick_source(None, false, now), None);
    // Exatamente no limite ainda é B3.
    assert_eq!(
        pick_source(Some(now - B3_FRESH_FOR), true, now),
        Some(EstimateSource::B3)
    );
}

#[test]
fn yahoo_points_are_converted_and_have_no_separate_jcp() {
    let points = [DividendPoint {
        ex_date: days_ago(10),
        amount: dec!(0.25),
    }];
    let payments = yahoo_unit_payments(&points, dec!(5.5));
    assert_eq!(payments[0].gross, dec!(1.375));
    assert_eq!(payments[0].net, dec!(1.375));
}

/// Um pagamento de `amount` por ano, há 100 dias, há 465, ... `years` vezes.
fn yearly(amount: Decimal, years: i64) -> Vec<UnitPayment> {
    (0..years)
        .map(|k| point(days_ago(100 + 365 * k), amount))
        .collect()
}

#[test]
fn the_usual_average_takes_only_whole_years() {
    // Seis anos de histórico: entram os cinco. O ano de agora é o da fonte (3),
    // os quatro de antes, os do histórico longo (1 cada).
    let found = usual(&yearly(dec!(1), 6), dec!(3), TODAY).unwrap();
    assert_eq!(found.years, 5);
    assert_eq!(found.per_unit_year, dec!(1.4));

    // Histórico de 2,5 anos: dois anos inteiros (o terceiro não chega inteiro).
    let short: Vec<UnitPayment> = (0..30)
        .map(|m| point(days_ago(5 + 30 * m), dec!(0.1)))
        .collect();
    let found = usual(&short, dec!(1.2), TODAY).unwrap();
    assert_eq!(found.years, 2);
    let second_year: Decimal = short
        .iter()
        .filter(|p| p.ex_date > days_ago(730) && p.ex_date <= days_ago(365))
        .map(|p| p.gross)
        .sum();
    assert_eq!(found.per_unit_year, (dec!(1.2) + second_year) / dec!(2));

    // Um ano só (ou nada): sem habitual.
    assert_eq!(usual(&yearly(dec!(1), 1), dec!(1), TODAY), None);
    assert_eq!(usual(&[], dec!(1), TODAY), None);
    // O histórico com um buraco de anos sem pagar conta zero nesses anos.
    let gap = [
        point(days_ago(100), dec!(2)),
        point(days_ago(1700), dec!(2)),
    ];
    let found = usual(&gap, dec!(2), TODAY).unwrap();
    assert_eq!((found.years, found.per_unit_year), (4, dec!(0.5)));
}

#[test]
fn above_usual_is_strictly_past_one_and_a_half_times_the_average() {
    let average = Usual {
        years: 5,
        per_unit_year: dec!(1),
    };
    assert!(!above_usual(dec!(1.5), Some(&average)));
    assert!(above_usual(dec!(1.50000001), Some(&average)));
    assert!(!above_usual(dec!(9), None));
    let zero = Usual {
        years: 5,
        per_unit_year: dec!(0),
    };
    assert!(!above_usual(dec!(0), Some(&zero)));
}

#[test]
fn the_asset_estimate_is_net_with_the_gross_beside_it() {
    let announced = [
        cash(
            "JRS CAP PROPRIO",
            date!(2026 - 03 - 10),
            dec!(1),
            Some(date!(2026 - 03 - 30)),
        ),
        cash(
            "DIVIDENDO",
            date!(2026 - 09 - 10),
            dec!(1),
            Some(date!(2026 - 09 - 30)),
        ),
    ];
    let payments = b3_unit_payments(&announced, &[], AssetClass::BrStocks, TODAY);
    let estimate = asset_estimate(
        EstimateSource::B3,
        &payments,
        &yearly(dec!(1), 6),
        dec!(120),
        TODAY,
    );
    assert_eq!(estimate.source, EstimateSource::B3);
    assert_eq!(
        estimate.per_unit_12m,
        GrossNet {
            gross: dec!(2),
            net: dec!(1.825)
        }
    );
    // 2,00 × 120 ÷ 12 = 20,00; 1,825 × 120 ÷ 12 = 18,25.
    assert_eq!(
        estimate.monthly,
        GrossNet {
            gross: dec!(20.00),
            net: dec!(18.25)
        }
    );
    // Média: (2 + 4 × 1) ÷ 5 = 1,20 por ano; o líquido usa a parte de JCP dos
    // 12 meses (1,825 ÷ 2).
    let usual = estimate.usual.unwrap();
    assert_eq!(usual.years, 5);
    assert_eq!(usual.per_unit_year.gross, dec!(1.2));
    assert_eq!(usual.per_unit_year.net, dec!(1.095));
    assert_eq!(usual.monthly.gross, dec!(12.00));
    assert_eq!(usual.monthly.net, dec!(10.95));
    // 2,00 > 1,5 × 1,20.
    assert!(estimate.above_usual);

    // Sem histórico longo: sem média e sem marca.
    let alone = asset_estimate(EstimateSource::B3, &payments, &[], dec!(120), TODAY);
    assert_eq!(alone.usual, None);
    assert!(!alone.above_usual);
    // Nada nos 12 meses: zero, e a média continua (o habitual existe).
    let quiet = asset_estimate(
        EstimateSource::Yahoo,
        &[],
        &yearly(dec!(1), 6),
        dec!(120),
        TODAY,
    );
    assert_eq!(quiet.monthly, GrossNet::default());
    assert_eq!(quiet.usual.unwrap().per_unit_year.net, dec!(0.8));
    assert!(!quiet.above_usual);
}

#[test]
fn trailing_income_uses_the_ledger_on_each_record_date() {
    let book = [
        entry(Kind::Buy, date!(2026 - 01 - 10), 1, dec!(100)),
        entry(Kind::Buy, date!(2026 - 06 - 15), 2, dec!(100)),
    ];
    let announced = [
        // Antes da primeira compra: nada.
        cash(
            "JRS CAP PROPRIO",
            date!(2025 - 12 - 20),
            dec!(0.5),
            Some(date!(2026 - 01 - 05)),
        ),
        cash(
            "DIVIDENDO",
            date!(2026 - 03 - 10),
            dec!(1),
            Some(date!(2026 - 03 - 25)),
        ),
        // A compra na data com conta: 200 cotas × 0,40 = 80, menos 17,5%.
        cash(
            "JRS CAP PROPRIO",
            date!(2026 - 06 - 15),
            dec!(0.4),
            Some(date!(2026 - 07 - 01)),
        ),
        // Fora da janela.
        cash(
            "DIVIDENDO",
            date!(2025 - 06 - 15),
            dec!(9),
            Some(date!(2025 - 07 - 01)),
        ),
    ];
    let total = b3_trailing_income(&announced, AssetClass::BrStocks, &book, TODAY).unwrap();
    assert_eq!(
        total,
        GrossNet {
            gross: dec!(180.00),
            net: dec!(166.00)
        }
    );
    // Livro que não fecha (venda sem posição) não vira número.
    let broken = [entry(Kind::Sell, date!(2026 - 01 - 10), 1, dec!(5))];
    assert!(b3_trailing_income(&announced, AssetClass::BrStocks, &broken, TODAY).is_err());
}

proptest! {
    /// O líquido nunca passa do bruto, e a soma da janela só conta o que está nela.
    #[test]
    fn net_never_exceeds_gross(
        items in proptest::collection::vec((0i64..800, 1i64..100_000, 0usize..3, proptest::option::of(0i64..60)), 0..30),
        bonus in proptest::option::of((0i64..800, 1i64..50)),
        quantity in 0i64..10_000,
    ) {
        let labels = ["DIVIDENDO", "JRS CAP PROPRIO", "RENDIMENTO"];
        let announced: Vec<CashAnnouncement> = items
            .iter()
            .map(|(days, cents, label, pay)| {
                let record = days_ago(*days);
                cash(
                    labels[*label],
                    record,
                    Decimal::new(*cents, 4),
                    pay.map(|d| record.saturating_add(Duration::days(d))),
                )
            })
            .collect();
        let events: Vec<StockAnnouncement> = bonus
            .iter()
            .map(|(days, factor)| stock("BONIFICACAO", days_ago(*days), Decimal::from(*factor)))
            .collect();
        let payments = b3_unit_payments(&announced, &events, AssetClass::BrStocks, TODAY);
        prop_assert_eq!(payments.len(), announced.len());
        for p in &payments {
            prop_assert!(p.net <= p.gross);
            prop_assert!(p.net >= p.gross * dec!(0.825) - dec!(0.000001));
        }
        let estimate = asset_estimate(EstimateSource::B3, &payments, &[], Decimal::from(quantity), TODAY);
        prop_assert!(estimate.monthly.net <= estimate.monthly.gross);
        prop_assert!(estimate.per_unit_12m.net <= estimate.per_unit_12m.gross);
        let inside: Decimal = payments
            .iter()
            .filter(|p| in_window(p.ex_date, TODAY))
            .map(|p| p.gross)
            .sum();
        prop_assert_eq!(estimate.per_unit_12m.gross, inside.round_dp(8));
    }
}
