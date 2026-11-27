use std::collections::{BTreeMap, HashMap};

use proptest::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use time::macros::date;
use time::{Date, Duration};
use uuid::Uuid;

use super::heatmap::{heatmap, monthly_costs, range, HeatYear, MonthlyCosts};
use super::*;
use crate::domain::ledger::{Entry, Kind};

const TODAY: Date = date!(2026 - 11 - 25);

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// Um provento FICTÍCIO de um ativo da carteira.
fn event(asset: u128, class: AssetClass, pay: Date, gross: Decimal, net: Decimal) -> HistoryEvent {
    HistoryEvent {
        pay_date: pay,
        ex_date: None,
        gross,
        net,
        asset_id: Some(id(asset)),
        payer_key: id(asset).to_string(),
        payer_label: format!("ATV{asset}"),
        class: Some(class),
        segment: None,
    }
}

fn reit(asset: u128, pay: Date, amount: Decimal) -> HistoryEvent {
    event(asset, AssetClass::BrReits, pay, amount, amount)
}

/// Provento de ativo apagado da carteira: sem id, sem classe.
fn removed(pay: Date, amount: Decimal) -> HistoryEvent {
    HistoryEvent {
        asset_id: None,
        payer_key: "OLD3".into(),
        payer_label: "OLD3".into(),
        class: None,
        ..reit(0, pay, amount)
    }
}

fn options(
    interval: Interval,
    basis: DateBasis,
    grouping: Grouping,
    amount: AmountKind,
) -> HistoryOptions {
    HistoryOptions {
        interval,
        basis,
        grouping,
        amount,
    }
}

fn month(year: i32, month: u8) -> YearMonth {
    YearMonth { year, month }
}

fn labels(history: &History) -> Vec<String> {
    history.buckets.iter().map(|b| b.start.label()).collect()
}

// ----------------------------------------------------------------- intervalo

#[test]
fn intervals_start_on_their_first_month() {
    let may = month(2026, 5);
    assert_eq!(Interval::Month.start_of(may), may);
    assert_eq!(Interval::Quarter.start_of(may), month(2026, 4));
    assert_eq!(Interval::HalfYear.start_of(may), month(2026, 1));
    assert_eq!(Interval::HalfYear.start_of(month(2026, 7)), month(2026, 7));
    assert_eq!(Interval::Year.start_of(month(2026, 12)), month(2026, 1));
    assert_eq!(Interval::Quarter.start_of(month(2026, 12)), month(2026, 10));
    assert_eq!(Interval::Quarter.next(month(2026, 10)), month(2027, 1));
    assert_eq!(Interval::HalfYear.next(month(2026, 7)), month(2027, 1));
    assert_eq!(Interval::Year.next(month(2026, 1)), month(2027, 1));
}

#[test]
fn options_have_wire_names() {
    assert_eq!(
        serde_json::to_string(&Interval::HalfYear).unwrap(),
        "\"half_year\""
    );
    assert_eq!(
        serde_json::to_string(&DateBasis::Record).unwrap(),
        "\"record\""
    );
    assert_eq!(
        serde_json::to_string(&Grouping::Segment).unwrap(),
        "\"segment\""
    );
    assert_eq!(
        serde_json::to_string(&AmountKind::Gross).unwrap(),
        "\"gross\""
    );
    assert_eq!(
        HistoryOptions::default(),
        options(
            Interval::Month,
            DateBasis::Payment,
            Grouping::Total,
            AmountKind::Net
        )
    );
}

// ----------------------------------------------------------------- data com

#[test]
fn record_date_is_the_session_before_the_ex_date_or_the_payment() {
    // FII com data com na sexta 30/01/2026 e data ex na segunda 02/02: é janeiro.
    let mut e = reit(1, date!(2026 - 02 - 13), dec!(10));
    e.ex_date = Some(date!(2026 - 02 - 02));
    assert_eq!(e.record_date(), date!(2026 - 01 - 30));
    assert_eq!(e.date(DateBasis::Record), date!(2026 - 01 - 30));
    assert_eq!(e.date(DateBasis::Payment), date!(2026 - 02 - 13));
    // Sem data ex, vale o pagamento.
    e.ex_date = None;
    assert_eq!(e.record_date(), date!(2026 - 02 - 13));
}

#[test]
fn record_basis_moves_the_payment_to_the_month_of_the_record_date() {
    let mut e = reit(1, date!(2026 - 02 - 13), dec!(10));
    e.ex_date = Some(date!(2026 - 02 - 02));
    let by_pay = history(std::slice::from_ref(&e), HistoryOptions::default(), TODAY);
    let by_record = history(
        &[e],
        options(
            Interval::Month,
            DateBasis::Record,
            Grouping::Total,
            AmountKind::Net,
        ),
        TODAY,
    );
    assert_eq!(by_pay.buckets[0].start, month(2026, 2));
    assert_eq!(by_record.buckets[0].start, month(2026, 1));
    assert_eq!(by_record.buckets[0].total.received, dec!(10));
}

// ----------------------------------------------------------------- histórico

#[test]
fn empty_book_has_no_history() {
    let h = history(&[], HistoryOptions::default(), TODAY);
    assert!(h.buckets.is_empty());
    assert!(h.groups.is_empty());
}

#[test]
fn months_run_without_gaps_up_to_today() {
    let events = [
        reit(1, date!(2026 - 07 - 14), dec!(10)),
        reit(1, date!(2026 - 09 - 14), dec!(12)),
    ];
    let h = history(&events, HistoryOptions::default(), TODAY);
    assert_eq!(
        labels(&h),
        ["2026-07", "2026-08", "2026-09", "2026-10", "2026-11"]
    );
    assert_eq!(h.buckets[1].total, Split::default());
    assert!(h.buckets[1].groups.is_empty());
    assert_eq!(h.buckets[2].total.received, dec!(12));
}

#[test]
fn a_future_payment_extends_the_history_and_counts_as_upcoming() {
    let events = [
        reit(1, date!(2026 - 11 - 14), dec!(10)),
        reit(1, date!(2026 - 11 - 30), dec!(4)),
        reit(1, date!(2027 - 01 - 15), dec!(6)),
    ];
    let h = history(&events, HistoryOptions::default(), TODAY);
    assert_eq!(labels(&h), ["2026-11", "2026-12", "2027-01"]);
    assert_eq!(
        h.buckets[0].total,
        Split {
            received: dec!(10),
            upcoming: dec!(4)
        }
    );
    assert_eq!(
        h.buckets[2].total,
        Split {
            received: dec!(0),
            upcoming: dec!(6)
        }
    );
}

#[test]
fn quarters_halves_and_years_add_up_the_months() {
    let events = [
        reit(1, date!(2025 - 02 - 10), dec!(1)),
        reit(1, date!(2025 - 05 - 10), dec!(2)),
        reit(1, date!(2025 - 08 - 10), dec!(4)),
        reit(1, date!(2025 - 11 - 10), dec!(8)),
        reit(1, date!(2026 - 03 - 10), dec!(16)),
    ];
    let quarters = history(
        &events,
        options(
            Interval::Quarter,
            DateBasis::Payment,
            Grouping::Total,
            AmountKind::Net,
        ),
        TODAY,
    );
    assert_eq!(
        labels(&quarters),
        ["2025-01", "2025-04", "2025-07", "2025-10", "2026-01", "2026-04", "2026-07", "2026-10"]
    );
    let values: Vec<Decimal> = quarters.buckets.iter().map(|b| b.total.total()).collect();
    assert_eq!(
        values,
        [
            dec!(1),
            dec!(2),
            dec!(4),
            dec!(8),
            dec!(16),
            dec!(0),
            dec!(0),
            dec!(0)
        ]
    );

    let halves = history(
        &events,
        options(
            Interval::HalfYear,
            DateBasis::Payment,
            Grouping::Total,
            AmountKind::Net,
        ),
        TODAY,
    );
    let values: Vec<(String, Decimal)> = halves
        .buckets
        .iter()
        .map(|b| (b.start.label(), b.total.total()))
        .collect();
    assert_eq!(
        values,
        [
            ("2025-01".to_owned(), dec!(3)),
            ("2025-07".to_owned(), dec!(12)),
            ("2026-01".to_owned(), dec!(16)),
            ("2026-07".to_owned(), dec!(0)),
        ]
    );

    let years = history(
        &events,
        options(
            Interval::Year,
            DateBasis::Payment,
            Grouping::Total,
            AmountKind::Net,
        ),
        TODAY,
    );
    let values: Vec<Decimal> = years.buckets.iter().map(|b| b.total.total()).collect();
    assert_eq!(values, [dec!(15), dec!(16)]);
}

#[test]
fn gross_or_net_follow_the_book() {
    // JCP de 2026: 17,5% retidos; o líquido é o do livro.
    let jcp = event(
        2,
        AssetClass::BrStocks,
        date!(2026 - 03 - 10),
        dec!(100.00),
        dec!(82.50),
    );
    let net = history(std::slice::from_ref(&jcp), HistoryOptions::default(), TODAY);
    let gross = history(
        &[jcp],
        options(
            Interval::Month,
            DateBasis::Payment,
            Grouping::Total,
            AmountKind::Gross,
        ),
        TODAY,
    );
    assert_eq!(net.buckets[0].total.received, dec!(82.50));
    assert_eq!(gross.buckets[0].total.received, dec!(100.00));
    assert_eq!(gross.groups[0].total, dec!(100.00));
}

#[test]
fn classes_segments_and_assets_split_the_bars() {
    let mut bank = event(
        2,
        AssetClass::BrStocks,
        date!(2026 - 03 - 10),
        dec!(30),
        dec!(30),
    );
    bank.segment = Some("Bancos".into());
    let mut paper = reit(1, date!(2026 - 03 - 14), dec!(50));
    paper.segment = Some("Papel".into());
    let crypto = event(
        3,
        AssetClass::Crypto,
        date!(2026 - 03 - 20),
        dec!(5),
        dec!(5),
    );
    let gone = removed(date!(2026 - 03 - 25), dec!(7));
    let events = [bank, paper, crypto, gone];

    let by_class = history(
        &events,
        options(
            Interval::Month,
            DateBasis::Payment,
            Grouping::Class,
            AmountKind::Net,
        ),
        TODAY,
    );
    let keys: Vec<String> = by_class.groups.iter().map(|g| g.key.wire()).collect();
    assert_eq!(
        keys,
        [
            "class:br_reits",
            "class:br_stocks",
            "removed",
            "class:crypto"
        ]
    );
    let march = &by_class.buckets[0];
    assert_eq!(march.total.received, dec!(92));
    let parts: Vec<(String, Decimal)> = march
        .groups
        .iter()
        .map(|(k, s)| (k.wire(), s.received))
        .collect();
    assert_eq!(
        parts,
        [
            ("class:br_reits".to_owned(), dec!(50)),
            ("class:br_stocks".to_owned(), dec!(30)),
            ("removed".to_owned(), dec!(7)),
            ("class:crypto".to_owned(), dec!(5)),
        ]
    );

    let by_segment = history(
        &events,
        options(
            Interval::Month,
            DateBasis::Payment,
            Grouping::Segment,
            AmountKind::Net,
        ),
        TODAY,
    );
    let keys: Vec<String> = by_segment.groups.iter().map(|g| g.key.wire()).collect();
    assert_eq!(
        keys,
        [
            "segment:br_reits:Papel",
            "segment:br_stocks:Bancos",
            "removed",
            "segment:crypto:"
        ]
    );
    assert!(by_segment
        .groups
        .iter()
        .all(|g| g.label.is_none() && g.asset_id.is_none()));

    let by_asset = history(
        &events,
        options(
            Interval::Month,
            DateBasis::Payment,
            Grouping::Asset,
            AmountKind::Net,
        ),
        TODAY,
    );
    let named: Vec<(Option<&str>, Option<Uuid>)> = by_asset
        .groups
        .iter()
        .map(|g| (g.label.as_deref(), g.asset_id))
        .collect();
    assert_eq!(
        named,
        [
            (Some("ATV1"), Some(id(1))),
            (Some("ATV2"), Some(id(2))),
            (Some("OLD3"), None),
            (Some("ATV3"), Some(id(3))),
        ]
    );
    assert_eq!(by_asset.groups[2].key.wire(), "asset:OLD3");
}

#[test]
fn group_order_is_the_whole_history_and_ties_go_by_name() {
    let events = [
        reit(2, date!(2026 - 01 - 10), dec!(10)),
        reit(1, date!(2026 - 01 - 10), dec!(10)),
        // Quem mais pagou no histórico inteiro vem primeiro, mesmo que num mês
        // tenha pago menos.
        reit(3, date!(2026 - 01 - 10), dec!(1)),
        reit(3, date!(2026 - 02 - 10), dec!(100)),
    ];
    let h = history(
        &events,
        options(
            Interval::Month,
            DateBasis::Payment,
            Grouping::Asset,
            AmountKind::Net,
        ),
        TODAY,
    );
    let order: Vec<&str> = h.groups.iter().filter_map(|g| g.label.as_deref()).collect();
    assert_eq!(order, ["ATV3", "ATV1", "ATV2"]);
    let january: Vec<String> = h.buckets[0].groups.iter().map(|(k, _)| k.wire()).collect();
    assert_eq!(january[0], format!("asset:{}", id(3)));
}

#[test]
fn segment_comes_from_the_public_classification() {
    let info = |segment: Option<&str>, subsector: Option<&str>| SectorInfo {
        sector: "Financeiro".into(),
        subsector: subsector.map(str::to_owned),
        segment: segment.map(str::to_owned),
    };
    assert_eq!(
        segment_of(
            AssetClass::BrStocks,
            Some(&info(Some("Bancos"), Some("Intermediarios")))
        ),
        Some("Bancos".into())
    );
    assert_eq!(
        segment_of(
            AssetClass::BrStocks,
            Some(&info(None, Some("Intermediarios")))
        ),
        Some("Intermediarios".into())
    );
    assert_eq!(
        segment_of(AssetClass::BrStocks, Some(&info(None, None))),
        Some("Financeiro".into())
    );
    // FII: o tipo (o "setor" que o Prumo tira do balanço), não o que o
    // administrador declara.
    let fii = SectorInfo {
        sector: "Papel".into(),
        subsector: None,
        segment: Some("Multicategoria".into()),
    };
    assert_eq!(
        segment_of(AssetClass::BrReits, Some(&fii)),
        Some("Papel".into())
    );
    assert_eq!(segment_of(AssetClass::BrStocks, None), None);
    assert_eq!(segment_of(AssetClass::IntlStocks, Some(&fii)), None);
}

// ------------------------------------------------------------- custo por mês

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

#[test]
fn cost_is_the_average_of_the_days_with_a_position() {
    // Compra de R$ 1.000 em 01/10 e mais R$ 1.000 em 16/10 (31 dias): 15 dias
    // com 1.000 e 16 com 2.000.
    let ledgers = HashMap::from([(
        id(1),
        vec![
            entry(
                Kind::Buy,
                date!(2026 - 10 - 01),
                1,
                dec!(10),
                Some(dec!(100)),
            ),
            entry(
                Kind::Buy,
                date!(2026 - 10 - 16),
                2,
                dec!(10),
                Some(dec!(100)),
            ),
        ],
    )]);
    let costs = monthly_costs(&ledgers, month(2026, 9), month(2026, 11), TODAY);
    // Setembro sem posição: sem custo.
    assert!(!costs.months.contains_key(&month(2026, 9)));
    let october = &costs.months[&month(2026, 10)];
    assert_eq!(
        october.cost,
        ((dec!(1000) * dec!(15) + dec!(2000) * dec!(16)) / dec!(31)).round_dp(2)
    );
    // Novembro até hoje (25): o custo cheio.
    assert_eq!(costs.months[&month(2026, 11)].cost, dec!(2000));
    assert!(costs.broken.is_empty());
}

#[test]
fn first_purchase_mid_month_averages_only_the_days_held() {
    let ledgers = HashMap::from([(
        id(1),
        vec![entry(
            Kind::Buy,
            date!(2026 - 10 - 20),
            1,
            dec!(10),
            Some(dec!(100)),
        )],
    )]);
    let costs = monthly_costs(&ledgers, month(2026, 10), month(2026, 10), TODAY);
    assert_eq!(costs.months[&month(2026, 10)].cost, dec!(1000));
}

#[test]
fn the_opening_balance_counts_backwards_and_unknown_cost_is_reported() {
    let ledgers = HashMap::from([
        // Saldo inicial cadastrado em 2026 com preço: vale para trás.
        (
            id(1),
            vec![entry(
                Kind::Opening,
                date!(2026 - 10 - 15),
                1,
                dec!(10),
                Some(dec!(50)),
            )],
        ),
        // Saldo inicial sem preço: custo desconhecido.
        (
            id(2),
            vec![entry(
                Kind::Opening,
                date!(2026 - 10 - 15),
                1,
                dec!(5),
                None,
            )],
        ),
    ]);
    let costs = monthly_costs(&ledgers, month(2025, 1), month(2025, 1), TODAY);
    let january = &costs.months[&month(2025, 1)];
    assert_eq!(january.cost, dec!(500));
    assert_eq!(january.unknown.iter().copied().collect::<Vec<_>>(), [id(2)]);
}

#[test]
fn a_ledger_that_does_not_close_is_unknown_every_month() {
    let ledgers = HashMap::from([(
        id(1),
        vec![
            entry(Kind::Buy, date!(2026 - 01 - 10), 1, dec!(1), Some(dec!(10))),
            entry(
                Kind::Sell,
                date!(2026 - 01 - 11),
                2,
                dec!(5),
                Some(dec!(10)),
            ),
        ],
    )]);
    let costs = monthly_costs(&ledgers, month(2026, 1), month(2026, 2), TODAY);
    assert_eq!(costs.broken, [id(1)]);
    assert!(costs.months.is_empty());
}

#[test]
fn months_outside_the_calendar_have_no_cost() {
    let ledgers = HashMap::from([(
        id(1),
        vec![entry(
            Kind::Buy,
            date!(2026 - 01 - 10),
            1,
            dec!(1),
            Some(dec!(10)),
        )],
    )]);
    let costs = monthly_costs(&ledgers, month(2026, 13), month(2026, 13), TODAY);
    assert!(costs.months.is_empty());
    // Depois de hoje não há dia para contar.
    let future = monthly_costs(&ledgers, month(2027, 1), month(2027, 2), TODAY);
    assert!(future.months.is_empty());
}

// ------------------------------------------------------------ mapa de calor

fn values(entries: &[(YearMonth, Decimal)]) -> BTreeMap<YearMonth, Decimal> {
    entries.iter().copied().collect()
}

fn cost_of(entries: &[(YearMonth, Decimal)]) -> MonthlyCosts {
    MonthlyCosts {
        months: entries
            .iter()
            .map(|(m, c)| {
                (
                    *m,
                    super::heatmap::MonthCost {
                        cost: *c,
                        unknown: Default::default(),
                    },
                )
            })
            .collect(),
        broken: Vec::new(),
    }
}

fn year(years: &[HeatYear], y: i32) -> &HeatYear {
    years.iter().find(|h| h.year == y).unwrap()
}

#[test]
fn heatmap_covers_whole_years_from_the_first_income() {
    let events = [reit(1, date!(2025 - 06 - 14), dec!(10))];
    assert_eq!(
        range(&events, DateBasis::Payment, TODAY),
        Some((month(2025, 1), month(2026, 12)))
    );
    assert_eq!(range(&[], DateBasis::Payment, TODAY), None);
    let years = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &BTreeMap::new(),
        &MonthlyCosts::default(),
    );
    let order: Vec<i32> = years.iter().map(|y| y.year).collect();
    assert_eq!(order, [2026, 2025]);
    assert_eq!(year(&years, 2025).months[5].amount, dec!(10));
    assert_eq!(year(&years, 2025).total.amount, dec!(10));
    // Sem foto nem custo: nada de DY nem de yield on cost.
    assert_eq!(year(&years, 2025).months[5].dy, None);
    assert_eq!(year(&years, 2025).months[5].yield_on_cost, None);
    assert!(heatmap(
        &[],
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &BTreeMap::new(),
        &MonthlyCosts::default()
    )
    .is_empty());
}

#[test]
fn dy_is_the_month_income_over_the_average_portfolio() {
    let events = [
        reit(1, date!(2026 - 10 - 14), dec!(80)),
        event(
            2,
            AssetClass::BrStocks,
            date!(2026 - 10 - 20),
            dec!(40),
            dec!(33),
        ),
    ];
    let market = values(&[(month(2026, 10), dec!(10000)), (month(2026, 9), dec!(9000))]);
    let costs = cost_of(&[(month(2026, 10), dec!(8000))]);
    let net = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &market,
        &costs,
    );
    let october = &year(&net, 2026).months[9];
    assert_eq!(october.amount, dec!(113));
    assert_eq!(october.market_value, Some(dec!(10000)));
    assert_eq!(october.dy, Some(dec!(1.13)));
    assert_eq!(october.cost, Some(dec!(8000)));
    assert_eq!(october.yield_on_cost, Some(dec!(1.4125)));
    assert!(!october.dy_partial && !october.yield_on_cost_partial);
    // Setembro com foto e sem provento: DY zero.
    assert_eq!(year(&net, 2026).months[8].dy, Some(dec!(0)));

    let gross = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Gross,
        TODAY,
        &market,
        &costs,
    );
    assert_eq!(year(&gross, 2026).months[9].dy, Some(dec!(1.2)));
}

#[test]
fn removed_assets_and_unknown_costs_leave_the_cell_partial() {
    let events = [
        reit(1, date!(2026 - 10 - 14), dec!(80)),
        reit(2, date!(2026 - 10 - 14), dec!(20)),
        removed(date!(2026 - 10 - 20), dec!(10)),
    ];
    let market = values(&[(month(2026, 10), dec!(10000))]);
    let mut costs = cost_of(&[(month(2026, 10), dec!(8000))]);
    if let Some(october) = costs.months.get_mut(&month(2026, 10)) {
        october.unknown.insert(id(2));
    }
    let years = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &market,
        &costs,
    );
    let october = &year(&years, 2026).months[9];
    // R$ mostra tudo; o DY tira o apagado; o yield on cost tira também o de custo
    // desconhecido.
    assert_eq!(october.amount, dec!(110));
    assert_eq!(october.dy, Some(dec!(1)));
    assert!(october.dy_partial);
    assert_eq!(october.yield_on_cost, Some(dec!(1)));
    assert!(october.yield_on_cost_partial);
}

#[test]
fn a_broken_ledger_takes_the_asset_out_of_the_yield_on_cost() {
    let events = [
        reit(1, date!(2026 - 10 - 14), dec!(80)),
        reit(2, date!(2026 - 10 - 14), dec!(20)),
    ];
    let mut costs = cost_of(&[(month(2026, 10), dec!(1000))]);
    costs.broken.push(id(2));
    let years = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &BTreeMap::new(),
        &costs,
    );
    let october = &year(&years, 2026).months[9];
    assert_eq!(october.yield_on_cost, Some(dec!(8)));
    assert!(october.yield_on_cost_partial);
}

#[test]
fn the_year_divides_its_income_by_the_average_base_until_today() {
    let events = [
        reit(1, date!(2026 - 09 - 14), dec!(90)),
        reit(1, date!(2026 - 10 - 14), dec!(110)),
        // Fora do DY do ano: depois do mês de hoje.
        reit(1, date!(2026 - 12 - 14), dec!(500)),
    ];
    let market = values(&[
        (month(2026, 9), dec!(9000)),
        (month(2026, 10), dec!(11000)),
        (month(2026, 11), dec!(10000)),
    ]);
    let costs = cost_of(&[
        (month(2026, 9), dec!(5000)),
        (month(2026, 10), dec!(5000)),
        (month(2026, 11), dec!(5000)),
    ]);
    let years = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &market,
        &costs,
    );
    let total = &year(&years, 2026).total;
    assert_eq!(total.amount, dec!(700));
    assert_eq!(total.upcoming, dec!(500));
    assert_eq!(total.market_value, Some(dec!(10000)));
    assert_eq!(total.dy, Some(dec!(2)));
    assert!(!total.dy_partial);
    assert_eq!(total.cost, Some(dec!(5000)));
    assert_eq!(total.yield_on_cost, Some(dec!(4)));
    // Dezembro: a receber, sem base.
    let december = &year(&years, 2026).months[11];
    assert_eq!(
        (december.amount, december.upcoming, december.dy),
        (dec!(500), dec!(500), None)
    );
}

#[test]
fn a_month_with_income_and_no_base_makes_the_year_partial() {
    let events = [
        reit(1, date!(2026 - 03 - 14), dec!(50)),
        reit(1, date!(2026 - 10 - 14), dec!(100)),
    ];
    let market = values(&[(month(2026, 10), dec!(10000))]);
    let years = heatmap(
        &events,
        DateBasis::Payment,
        AmountKind::Net,
        TODAY,
        &market,
        &MonthlyCosts::default(),
    );
    let total = &year(&years, 2026).total;
    assert_eq!(total.dy, Some(dec!(1)));
    assert!(total.dy_partial);
    assert_eq!(total.yield_on_cost, None);
    assert!(total.yield_on_cost_partial);
}

// --------------------------------------------------------------- propriedades

fn arbitrary_events() -> impl Strategy<Value = Vec<HistoryEvent>> {
    proptest::collection::vec(
        (
            -900i64..120,
            0u8..4,
            1i64..100_000,
            0i64..30,
            0u8..3,
            prop::option::of(1i64..20),
        ),
        0..40,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(|(offset, payer, cents, retained, class, ex_before)| {
                let pay = TODAY.saturating_add(Duration::days(offset));
                let gross = Decimal::new(cents, 2);
                let net = (gross * Decimal::new(100 - retained, 2)).round_dp(2);
                let class = [
                    AssetClass::BrReits,
                    AssetClass::BrStocks,
                    AssetClass::IntlStocks,
                ][usize::from(class)];
                let mut e = if payer == 3 {
                    removed(pay, gross)
                } else {
                    event(u128::from(payer) + 1, class, pay, gross, net)
                };
                e.net = net;
                e.ex_date = ex_before.map(|days| pay.saturating_sub(Duration::days(days)));
                e
            })
            .collect()
    })
}

const INTERVALS: [Interval; 4] = [
    Interval::Month,
    Interval::Quarter,
    Interval::HalfYear,
    Interval::Year,
];
const BASES: [DateBasis; 2] = [DateBasis::Payment, DateBasis::Record];
const GROUPINGS: [Grouping; 4] = [
    Grouping::Total,
    Grouping::Class,
    Grouping::Segment,
    Grouping::Asset,
];
const AMOUNTS: [AmountKind; 2] = [AmountKind::Net, AmountKind::Gross];

proptest! {
    /// Nenhum provento some nem aparece em dobro: as barras somam o livro, cada
    /// barra é a soma dos grupos e a legenda soma o mesmo.
    #[test]
    fn history_adds_up_to_the_book(
        events in arbitrary_events(),
        interval in 0usize..4,
        basis in 0usize..2,
        grouping in 0usize..4,
        amount in 0usize..2,
    ) {
        let opts = options(INTERVALS[interval], BASES[basis], GROUPINGS[grouping], AMOUNTS[amount]);
        let h = history(&events, opts, TODAY);
        let book: Decimal = events.iter().map(|e| e.amount(opts.amount)).sum();
        let upcoming: Decimal = events.iter().filter(|e| e.upcoming(TODAY)).map(|e| e.amount(opts.amount)).sum();
        let bars: Decimal = h.buckets.iter().map(|b| b.total.total()).sum();
        prop_assert_eq!(bars, book);
        prop_assert_eq!(h.buckets.iter().map(|b| b.total.upcoming).sum::<Decimal>(), upcoming);
        prop_assert_eq!(h.groups.iter().map(|g| g.total).sum::<Decimal>(), book);
        for bucket in &h.buckets {
            let parts = bucket.groups.iter().fold(Split::default(), |acc, (_, s)| Split {
                received: acc.received + s.received,
                upcoming: acc.upcoming + s.upcoming,
            });
            prop_assert_eq!(parts, bucket.total);
            prop_assert_eq!(opts.interval.start_of(bucket.start), bucket.start);
        }
        // Sem buraco, um intervalo depois do outro.
        for pair in h.buckets.windows(2) {
            prop_assert_eq!(opts.interval.next(pair[0].start), pair[1].start);
        }
        if let Some(last) = h.buckets.last() {
            prop_assert!(last.start >= opts.interval.start_of(YearMonth::of(TODAY)));
        }
    }

    /// O trimestre (e o ano) é a soma dos meses que ele cobre.
    #[test]
    fn longer_intervals_are_sums_of_months(events in arbitrary_events(), basis in 0usize..2) {
        let months = history(&events, options(Interval::Month, BASES[basis], Grouping::Total, AmountKind::Net), TODAY);
        for interval in [Interval::Quarter, Interval::HalfYear, Interval::Year] {
            let longer = history(&events, options(interval, BASES[basis], Grouping::Total, AmountKind::Net), TODAY);
            for bucket in &longer.buckets {
                let inside: Decimal = months
                    .buckets
                    .iter()
                    .filter(|m| interval.start_of(m.start) == bucket.start)
                    .map(|m| m.total.total())
                    .sum();
                prop_assert_eq!(inside, bucket.total.total());
            }
        }
    }

    /// O mapa em reais é o histórico mensal, e o total do ano soma os meses.
    #[test]
    fn heatmap_amounts_match_the_monthly_history(events in arbitrary_events(), basis in 0usize..2, amount in 0usize..2) {
        let basis = BASES[basis];
        let amount = AMOUNTS[amount];
        let years = heatmap(&events, basis, amount, TODAY, &BTreeMap::new(), &MonthlyCosts::default());
        let months = history(&events, options(Interval::Month, basis, Grouping::Total, amount), TODAY);
        for bucket in &months.buckets {
            let Some(row) = years.iter().find(|y| y.year == bucket.start.year) else {
                return Err(TestCaseError::fail("ano faltando no mapa"));
            };
            let cell = &row.months[usize::from(bucket.start.month - 1)];
            prop_assert_eq!(cell.amount, bucket.total.total());
            prop_assert_eq!(cell.upcoming, bucket.total.upcoming);
        }
        for row in &years {
            prop_assert_eq!(row.months.iter().map(|m| m.amount).sum::<Decimal>(), row.total.amount);
        }
    }

    /// Dobrar o patrimônio corta o DY pela metade (a até um décimo de milésimo).
    #[test]
    fn doubling_the_portfolio_halves_the_dy(cents in 1i64..10_000_000, value in 1i64..100_000_000) {
        let events = [reit(1, date!(2026 - 10 - 14), Decimal::new(cents, 2))];
        let base = Decimal::new(value, 2);
        let one = heatmap(&events, DateBasis::Payment, AmountKind::Net, TODAY, &values(&[(month(2026, 10), base)]), &MonthlyCosts::default());
        let two = heatmap(&events, DateBasis::Payment, AmountKind::Net, TODAY, &values(&[(month(2026, 10), base * dec!(2))]), &MonthlyCosts::default());
        let (Some(one), Some(two)) = (year(&one, 2026).months[9].dy, year(&two, 2026).months[9].dy) else {
            return Err(TestCaseError::fail("DY esperado"));
        };
        prop_assert!((one / dec!(2) - two).abs() <= dec!(0.0001));
    }
}
