//! Agenda: a montagem dos eventos com dados fictícios.

use super::*;
use crate::domain::income::JCP_WITHHOLDING_RATE_2026;
use crate::domain::ledger::Kind;
use rust_decimal_macros::dec;
use time::macros::date;
use unusual::Payment;

const TODAY: Date = date!(2026 - 11 - 27);

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn buy(day: Date, quantity: Decimal) -> Entry {
    Entry {
        kind: Kind::Buy,
        trade_date: day,
        seq: 1,
        quantity,
        unit_price: Some(dec!(10)),
        fees: Decimal::ZERO,
        fx_rate: Decimal::ONE,
    }
}

fn item(asset: u128, ex: Option<Date>, pay: Date, reference: Option<&str>) -> IncomeItem {
    IncomeItem {
        id: id(1000 + asset),
        asset_id: id(asset),
        kind: IncomeKind::ReitIncome,
        ex_date: ex,
        pay_date: pay,
        amount_per_unit: dec!(0.80),
        quantity: dec!(100),
        gross: dec!(80),
        net: dec!(80),
        source: IncomeSource::B3Announcement,
        external_ref: reference.map(str::to_owned),
    }
}

fn input(from: Date, to: Date) -> Input {
    Input {
        today: TODAY,
        from,
        to,
        income: Vec::new(),
        declared: Vec::new(),
        undated: Vec::new(),
        documents: Vec::new(),
        closures: Vec::new(),
    }
}

fn row(rate: Decimal, pay: Option<Date>, approved: Option<Date>) -> DeclaredRow {
    DeclaredRow {
        label: "RENDIMENTO".to_owned(),
        rate,
        pay_date: pay,
        approved_on: approved,
    }
}

fn cut() -> Unusual {
    let history: Vec<Payment> = (1..=12)
        .map(|m| Payment {
            ex_date: Date::from_calendar_date(2025, time::Month::December, 1)
                .unwrap()
                .saturating_add(Duration::days(30 * (m - 1))),
            per_unit: dec!(0.80),
        })
        .collect();
    let declaration = unusual::Declaration {
        ex_date: date!(2026 - 12 - 01),
        by_kind: vec![(IncomeKind::ReitIncome, dec!(0.60))],
        total: dec!(0.60),
    };
    let history = unusual::History {
        by_kind: vec![(IncomeKind::ReitIncome, history.clone())],
        totals: history,
    };
    unusual::unusual(&declaration, &history, unusual::FUND_RATIO).unwrap()
}

#[test]
fn the_record_date_comes_from_the_reference_or_the_session_before_the_ex_date() {
    assert_eq!(
        record_from_ref("b3a:BRXXXXCTF000:RENDIMENTO:2026-11-30:0.8"),
        Some(date!(2026 - 11 - 30))
    );
    assert_eq!(
        record_from_ref("b3a:BRXXXXACNOR0:DIVIDENDO:2025-12-19:0.41:2"),
        Some(date!(2025 - 12 - 19))
    );
    assert_eq!(record_from_ref("outra:coisa"), None);
    // Data ex na segunda: a data com é a sexta.
    assert_eq!(
        record_date_of(Some(date!(2026 - 11 - 30)), None),
        Some(date!(2026 - 11 - 27))
    );
    // Data ex depois do feriado de 20/11 (sexta): a data com é a quinta 19/11.
    assert_eq!(
        record_date_of(Some(date!(2026 - 11 - 23)), None),
        Some(date!(2026 - 11 - 19))
    );
    // Fora do calendário, só o fim de semana.
    assert_eq!(
        previous_trading_day(date!(2030 - 01 - 07)),
        date!(2030 - 01 - 04)
    );
    assert_eq!(record_date_of(None, None), None);
    assert!(valid_range(date!(2026 - 01 - 01), date!(2026 - 12 - 31)));
    assert!(!valid_range(date!(2026 - 12 - 31), date!(2026 - 01 - 01)));
    assert!(!valid_range(date!(2025 - 01 - 01), date!(2026 - 12 - 31)));
}

#[test]
fn a_provento_gives_the_record_date_and_the_payment_inside_the_range() {
    let mut agenda = input(date!(2026 - 11 - 01), date!(2026 - 12 - 31));
    agenda.income = vec![
        item(
            1,
            Some(date!(2026 - 12 - 01)),
            date!(2026 - 12 - 15),
            Some("b3a:BRXXXXCTF000:RENDIMENTO:2026-11-30:0.8"),
        ),
        // Pago em outubro, data com em setembro: fora.
        item(2, Some(date!(2026 - 09 - 30)), date!(2026 - 10 - 15), None),
        // Lançado sem data ex: só o pagamento.
        item(3, None, date!(2026 - 11 - 10), None),
    ];
    let events = build(&agenda);
    let kinds: Vec<(Date, EventKind)> = events.iter().map(|e| (e.date, e.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            (date!(2026 - 11 - 10), EventKind::IncomePayment),
            (date!(2026 - 11 - 30), EventKind::IncomeRecord),
            (date!(2026 - 12 - 15), EventKind::IncomePayment),
        ]
    );
    let Detail::Income(paid) = &events[0].detail else {
        panic!("{:?}", events);
    };
    assert_eq!(paid.state, IncomeState::Received);
    let Detail::Income(next) = &events[2].detail else {
        panic!("{:?}", events);
    };
    assert_eq!(next.state, IncomeState::Upcoming);
    assert_eq!(next.record_date, Some(date!(2026 - 11 - 30)));
    assert_eq!(next.net, dec!(80));
}

#[test]
fn an_unusual_announcement_is_an_event_and_marks_the_provento() {
    let declared = Declared {
        asset_id: id(1),
        record_date: date!(2026 - 11 - 30),
        ex_date: date!(2026 - 12 - 01),
        rows: vec![row(
            dec!(0.60),
            Some(date!(2026 - 12 - 15)),
            Some(date!(2026 - 11 - 20)),
        )],
        unusual: Some(cut()),
    };
    let mut agenda = input(date!(2026 - 11 - 01), date!(2026 - 12 - 31));
    agenda.income = vec![item(
        1,
        Some(date!(2026 - 12 - 01)),
        date!(2026 - 12 - 15),
        Some("b3a:BRXXXXCTF000:RENDIMENTO:2026-11-30:0.6"),
    )];
    agenda.declared = vec![declared.clone()];
    let events = build(&agenda);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].date, date!(2026 - 11 - 20));
    assert_eq!(events[0].kind, EventKind::UnusualIncome);
    let Detail::Unusual(found) = &events[0].detail else {
        panic!("{:?}", events);
    };
    assert_eq!(found.unusual.direction, Direction::Cut);
    assert_eq!(found.pay_dates, vec![date!(2026 - 12 - 15)]);
    for event in &events[1..] {
        let Detail::Income(income) = &event.detail else {
            panic!("{:?}", event.detail);
        };
        assert_eq!(income.unusual.map(|u| u.direction), Some(Direction::Cut));
    }
    // A marca vem do mesmo ativo e da mesma data com: outro ativo não ganha.
    let other = item(2, Some(date!(2026 - 12 - 01)), date!(2026 - 12 - 15), None);
    assert!(mark_for(&other, &agenda.declared).is_none());
    // Lançado à mão com a data com no campo da data ex: é o mesmo anúncio.
    let manual = IncomeItem {
        source: IncomeSource::Manual,
        ..item(1, Some(date!(2026 - 11 - 30)), date!(2026 - 12 - 15), None)
    };
    assert!(mark_for(&manual, &agenda.declared).is_some());
    // Aprovação depois da data com (dado estranho): o evento fica na data com.
    let late = Declared {
        rows: vec![row(dec!(0.60), None, Some(date!(2026 - 12 - 05)))],
        ..declared
    };
    assert_eq!(late.announced_on(), date!(2026 - 11 - 30));
}

#[test]
fn an_undated_announcement_is_estimated_by_the_ledger_on_the_record_date() {
    let declared = Declared {
        asset_id: id(1),
        record_date: date!(2026 - 12 - 10),
        ex_date: date!(2026 - 12 - 11),
        rows: vec![DeclaredRow {
            label: "JRS CAP PROPRIO".to_owned(),
            rate: dec!(0.50),
            pay_date: None,
            approved_on: None,
        }],
        unusual: None,
    };
    let entries = vec![buy(date!(2026 - 01 - 05), dec!(200))];
    let rows = undated_rows(&declared, &[]);
    assert_eq!(rows.len(), 1);
    let detail = undated_income(&declared, rows[0], AssetClass::BrStocks, &entries, TODAY)
        .unwrap()
        .unwrap();
    assert_eq!(detail.state, IncomeState::Undated);
    assert_eq!(detail.kind, IncomeKind::Jcp);
    assert_eq!(detail.gross, dec!(100));
    assert_eq!(
        detail.net,
        dec!(100) * (Decimal::ONE - JCP_WITHHOLDING_RATE_2026)
    );
    assert_eq!(detail.pay_date, None);
    // Comprou depois da data com: nada.
    let late = vec![buy(date!(2026 - 12 - 11), dec!(200))];
    assert!(
        undated_income(&declared, rows[0], AssetClass::BrStocks, &late, TODAY)
            .unwrap()
            .is_none()
    );
    // Lançado à mão com o mesmo valor por cota: já está na Renda.
    let manual = IncomeItem {
        amount_per_unit: dec!(0.5),
        source: IncomeSource::Manual,
        ..item(1, Some(date!(2026 - 12 - 11)), date!(2027 - 01 - 15), None)
    };
    assert!(undated_rows(&declared, &[manual]).is_empty());
    // Na Agenda, só a data com.
    let mut agenda = input(date!(2026 - 12 - 01), date!(2026 - 12 - 31));
    agenda.undated = vec![(id(1), detail)];
    let events = build(&agenda);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, EventKind::IncomeRecord);
    assert_eq!(events[0].date, date!(2026 - 12 - 10));
}

#[test]
fn documents_and_closures_are_events_of_their_day() {
    let document = |category, delivered| Document {
        source: DocumentSource::CvmIpe,
        category,
        title: None,
        subject: Some("Assunto ficticio".to_owned()),
        reference_date: None,
        delivered_on: delivered,
        version: 1,
        url: Some("https://www.rad.cvm.gov.br/ENET/x".to_owned()),
        filing: None,
    };
    let mut agenda = input(date!(2026 - 11 - 01), date!(2026 - 11 - 30));
    agenda.documents = vec![
        (
            vec![id(1), id(2)],
            document(DocumentCategory::MaterialFact, date!(2026 - 11 - 20)),
        ),
        (
            vec![id(1)],
            document(DocumentCategory::Meeting, date!(2026 - 11 - 20)),
        ),
        (
            vec![id(1)],
            document(DocumentCategory::Results, date!(2026 - 11 - 11)),
        ),
        // Sem ativo da carteira, ou fora do período: fora.
        (
            vec![],
            document(DocumentCategory::MarketNotice, date!(2026 - 11 - 12)),
        ),
        (
            vec![id(1)],
            document(DocumentCategory::MarketNotice, date!(2026 - 12 - 01)),
        ),
    ];
    agenda.closures = vec![
        (date!(2026 - 11 - 20), Closure::BlackConsciousness),
        (date!(2026 - 12 - 25), Closure::Christmas),
    ];
    let events = build(&agenda);
    let kinds: Vec<(Date, EventKind, usize)> = events
        .iter()
        .map(|e| (e.date, e.kind, e.assets.len()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (date!(2026 - 11 - 11), EventKind::Results, 1),
            (date!(2026 - 11 - 20), EventKind::MarketClosed, 0),
            (date!(2026 - 11 - 20), EventKind::MaterialFact, 2),
            (date!(2026 - 11 - 20), EventKind::Notice, 1),
        ]
    );
}

#[test]
fn categories_round_trip() {
    for category in DocumentCategory::ALL {
        assert_eq!(DocumentCategory::parse(category.as_str()), Some(category));
    }
    assert_eq!(DocumentCategory::parse("outra"), None);
    for source in [
        DocumentSource::CvmIpe,
        DocumentSource::CvmFiling,
        DocumentSource::FundosNet,
    ] {
        assert_eq!(DocumentSource::parse(source.as_str()), Some(source));
    }
    assert_eq!(DocumentSource::parse("x"), None);
    assert_eq!(
        DocumentCategory::ShareholderNotice.event_kind(),
        EventKind::Notice
    );
    assert_eq!(
        DocumentCategory::ManagementReport.event_kind(),
        EventKind::Notice
    );
}
