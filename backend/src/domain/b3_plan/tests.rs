//! Testes do plano da importação: carteira e planilha fictícias.

use rust_decimal_macros::dec;
use time::macros::date;

use super::*;
use crate::domain::b3_import::{
    AssetTotals, EventLine, FileKind, IncomeLine, PreviewSummary, TradeLine, TradeMarket,
};

fn trade(r: &str, day: Date, side: TradeSide, quantity: Decimal, price: Decimal) -> TradeLine {
    TradeLine {
        external_ref: format!("b3:v1:trade:{r}"),
        date: day,
        side,
        market: TradeMarket::Spot,
        code: "WEGE3".into(),
        institution: None,
        quantity,
        unit_price: price,
        total: quantity * price,
        line: 2,
    }
}

fn event(r: &str, day: Date, kind: EventKind, direction: Direction) -> EventLine {
    EventLine {
        external_ref: format!("b3:v1:event:{r}"),
        date: day,
        kind,
        direction,
        code: Some("WEGE3".into()),
        product: "WEGE3 - WEG S.A.".into(),
        quantity: None,
        unit_price: None,
        value: None,
        institution: None,
        description: "evento".into(),
        treasury: false,
        source: FileKind::Movements,
        line: 3,
    }
}

fn income(
    r: &str,
    kind: B3IncomeKind,
    status: B3IncomeStatus,
    day: Option<Date>,
    net: Decimal,
) -> IncomeLine {
    IncomeLine {
        external_ref: format!("b3:v1:income:{r}"),
        kind,
        status,
        date: day,
        code: "WEGE3".into(),
        product: "WEGE3 - WEG S.A.".into(),
        quantity: Some(dec!(100)),
        unit_price: Some(net / dec!(100)),
        net_amount: net,
        institution: None,
        source: FileKind::IncomeReceived,
        line: 4,
    }
}

fn group(key: &str) -> AssetGroup {
    AssetGroup {
        key: key.into(),
        name: Some("WEG S.A.".into()),
        suggested_class: AssetClass::BrStocks,
        class_basis: ClassBasis::Name,
        listed_on_b3: true,
        treasury: false,
        totals: AssetTotals::default(),
        trades: Vec::new(),
        events: Vec::new(),
        income: Vec::new(),
    }
}

fn preview(groups: Vec<AssetGroup>) -> Preview {
    Preview {
        files: Vec::new(),
        assets: groups,
        unrecognized: Vec::new(),
        ignored: Vec::new(),
        summary: PreviewSummary::default(),
    }
}

fn held(id: Uuid, quantity: Decimal) -> BookAsset {
    BookAsset {
        id,
        asset_class: AssetClass::BrStocks,
        symbol: Some("WEGE3".into()),
        display_name: "WEG".into(),
        listed_on_b3: true,
        treasury: false,
        quantity,
    }
}

fn entry(
    asset_id: Uuid,
    seq: i64,
    kind: Kind,
    day: Date,
    quantity: Decimal,
    price: Option<Decimal>,
) -> BookEntry {
    BookEntry {
        id: Uuid::new_v4(),
        asset_id,
        entry: Entry {
            kind,
            trade_date: day,
            seq,
            quantity,
            unit_price: price,
            fees: Decimal::ZERO,
            fx_rate: Decimal::ONE,
        },
        external_ref: None,
    }
}

/// Um livro vazio, com hoje fixo.
fn empty_book() -> Book {
    Book {
        assets: Vec::new(),
        entries: Vec::new(),
        transaction_refs: HashSet::new(),
        income: Vec::new(),
        treasury_symbols: Vec::new(),
        today: date!(2026 - 11 - 25),
    }
}

fn status(plan: &Plan, r: &str) -> (LineState, Option<&'static str>) {
    let line = &plan.lines[r];
    (line.status, line.reason_code)
}

fn only(plan: &Plan) -> &AssetPlan {
    &plan.assets[0]
}

fn decide(key: &str, decision: AssetDecision) -> Decisions {
    Decisions {
        assets: BTreeMap::from([(key.to_owned(), decision)]),
    }
}

#[test]
fn a_new_asset_gets_an_opening_that_covers_the_first_sale() {
    let mut g = group("WEGE3");
    g.class_basis = ClassBasis::Code;
    g.trades = vec![
        trade(
            "a",
            date!(2020 - 01 - 10),
            TradeSide::Buy,
            dec!(30),
            dec!(40),
        ),
        trade(
            "b",
            date!(2020 - 03 - 10),
            TradeSide::Sell,
            dec!(50),
            dec!(45),
        ),
        trade(
            "c",
            date!(2021 - 01 - 10),
            TradeSide::Buy,
            dec!(10),
            dec!(42),
        ),
    ];
    let p = plan(
        &preview(vec![g.clone()]),
        &empty_book(),
        &Decisions::default(),
    );
    let asset = only(&p);
    assert_eq!(asset.action, AssetAction::Create);
    assert_eq!(asset.symbol.as_deref(), Some("WEGE3"));
    let opening = asset.opening.as_ref().unwrap();
    assert_eq!(opening.quantity, dec!(20));
    assert_eq!(opening.basis, OpeningBasis::CoversSales);
    assert_eq!(opening.date, date!(2020 - 01 - 09));
    assert_eq!(opening.unit_price, None);
    assert_eq!(asset.final_quantity, Some(dec!(10)));
    assert_eq!(asset.writes.entries.len(), 4);
    assert_eq!(asset.writes.entries[0].kind, Kind::Opening);
    // Classe só pelo código: trava até a pessoa confirmar.
    assert!(asset.class_needs_confirmation);
    assert!(!p.ready);
    assert_eq!(status(&p, "b3:v1:trade:b"), (LineState::New, None));

    let confirmed = plan(
        &preview(vec![g.clone()]),
        &empty_book(),
        &decide(
            "WEGE3",
            AssetDecision {
                asset_class: Some(AssetClass::BrStocks),
                ..AssetDecision::default()
            },
        ),
    );
    assert!(confirmed.ready);
    assert_eq!(confirmed.summary.assets_created, 1);
    assert_eq!(confirmed.summary.transactions_new, 3);
    assert_eq!(confirmed.summary.openings, 1);
    assert_ne!(confirmed.digest, p.digest);
    // O mesmo pedido dá o mesmo resumo.
    let again = plan(
        &preview(vec![g]),
        &empty_book(),
        &decide(
            "WEGE3",
            AssetDecision {
                asset_class: Some(AssetClass::BrStocks),
                ..AssetDecision::default()
            },
        ),
    );
    assert_eq!(again.digest, confirmed.digest);
    assert_eq!(again.digest.len(), 32);
}

#[test]
fn an_opening_too_small_conflicts_and_skipping_unblocks() {
    let mut g = group("WEGE3");
    g.trades = vec![
        trade(
            "a",
            date!(2020 - 01 - 10),
            TradeSide::Buy,
            dec!(30),
            dec!(40),
        ),
        trade(
            "b",
            date!(2020 - 03 - 10),
            TradeSide::Sell,
            dec!(50),
            dec!(45),
        ),
    ];
    let small = decide(
        "WEGE3",
        AssetDecision {
            opening: Some(OpeningChoice {
                quantity: dec!(5),
                unit_price: Some(dec!(10)),
            }),
            ..AssetDecision::default()
        },
    );
    let p = plan(&preview(vec![g.clone()]), &empty_book(), &small);
    let asset = only(&p);
    assert_eq!(
        asset.conflict.as_ref().unwrap().reason_code,
        "exceeds_position"
    );
    assert_eq!(
        status(&p, "b3:v1:trade:b"),
        (LineState::Conflict, Some("exceeds_position"))
    );
    assert!(asset.opening.as_ref().unwrap().chosen);
    assert_eq!(asset.opening.as_ref().unwrap().suggested_quantity, dec!(20));
    assert!(!p.ready);
    assert_eq!(p.summary.conflicts, 1);

    // Fração numa ação: o saldo inicial não vale.
    let fraction = decide(
        "WEGE3",
        AssetDecision {
            opening: Some(OpeningChoice {
                quantity: dec!(20.5),
                unit_price: None,
            }),
            ..AssetDecision::default()
        },
    );
    let p = plan(&preview(vec![g.clone()]), &empty_book(), &fraction);
    assert_eq!(
        only(&p).conflict.as_ref().unwrap().reason_code,
        "invalid_opening"
    );

    // Preço médio escolhido entra no saldo inicial.
    let priced = decide(
        "WEGE3",
        AssetDecision {
            opening: Some(OpeningChoice {
                quantity: dec!(25),
                unit_price: Some(dec!(12.5)),
            }),
            ..AssetDecision::default()
        },
    );
    let p = plan(&preview(vec![g.clone()]), &empty_book(), &priced);
    assert!(p.ready);
    assert_eq!(only(&p).writes.entries[0].unit_price, Some(dec!(12.5)));
    assert_eq!(only(&p).final_quantity, Some(dec!(5)));

    let skipped = decide(
        "WEGE3",
        AssetDecision {
            skip: true,
            ..small.assets["WEGE3"].clone()
        },
    );
    let p = plan(&preview(vec![g]), &empty_book(), &skipped);
    assert!(p.ready);
    assert_eq!(only(&p).action, AssetAction::Skip);
    assert_eq!(
        status(&p, "b3:v1:trade:a"),
        (LineState::NotImported, Some("asset_skipped"))
    );
    assert_eq!(p.summary.assets_skipped, 1);
    assert_eq!(p.summary.transactions_new, 0);
}

#[test]
fn reimporting_finds_everything_and_writes_nothing() {
    let id = Uuid::new_v4();
    let mut g = group("WEGE3");
    g.trades = vec![trade(
        "a",
        date!(2020 - 01 - 10),
        TradeSide::Buy,
        dec!(30),
        dec!(40),
    )];
    g.income = vec![income(
        "i",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2020 - 05 - 10)),
        dec!(15),
    )];
    let mut imported = entry(
        id,
        7,
        Kind::Buy,
        date!(2020 - 01 - 10),
        dec!(30),
        Some(dec!(40)),
    );
    imported.external_ref = Some("b3:v1:trade:a".into());
    let book = Book {
        assets: vec![held(id, dec!(30))],
        entries: vec![imported],
        transaction_refs: HashSet::from(["b3:v1:trade:a".to_owned()]),
        income: vec![BookIncome {
            id: Uuid::new_v4(),
            symbol: Some("WEGE3".into()),
            kind: IncomeKind::Dividend,
            pay_date: date!(2020 - 05 - 10),
            amount_per_unit: dec!(0.15),
            net: dec!(15),
            imported: true,
            provisional: false,
            external_ref: Some("b3:v1:income:i".into()),
        }],
        treasury_symbols: Vec::new(),
        today: date!(2026 - 11 - 25),
    };
    let p = plan(&preview(vec![g]), &book, &Decisions::default());
    let asset = only(&p);
    assert_eq!(asset.action, AssetAction::Nothing);
    assert_eq!(asset.writes, AssetWrites::default());
    assert_eq!(
        status(&p, "b3:v1:trade:a"),
        (LineState::Existing, Some("same_ref"))
    );
    assert_eq!(
        status(&p, "b3:v1:income:i"),
        (LineState::Existing, Some("same_ref"))
    );
    assert_eq!(p.summary.transactions_existing, 1);
    assert_eq!(p.summary.income_existing, 1);
    assert!(p.ready);
}

#[test]
fn a_typed_purchase_with_a_close_price_is_the_same_trade() {
    let id = Uuid::new_v4();
    let mut g = group("WEGE3");
    g.trades = vec![
        trade(
            "a",
            date!(2026 - 03 - 10),
            TradeSide::Buy,
            dec!(10),
            dec!(40.30),
        ),
        trade(
            "b",
            date!(2026 - 03 - 10),
            TradeSide::Buy,
            dec!(10),
            dec!(48),
        ),
    ];
    // O "Já comprei" gravou 10 a R$ 40 (preço do plano); a B3 diz R$ 40,30.
    let plan_buy = entry(
        id,
        3,
        Kind::Buy,
        date!(2026 - 03 - 10),
        dec!(10),
        Some(dec!(40)),
    );
    let book = Book {
        assets: vec![held(id, dec!(10))],
        entries: vec![plan_buy.clone()],
        ..empty_book()
    };
    let p = plan(&preview(vec![g]), &book, &Decisions::default());
    let asset = only(&p);
    assert_eq!(
        status(&p, "b3:v1:trade:a"),
        (LineState::Existing, Some("same_entry"))
    );
    // A segunda compra do dia tem preço longe: é outro negócio.
    assert_eq!(status(&p, "b3:v1:trade:b"), (LineState::New, None));
    assert_eq!(
        asset.writes.adopt,
        vec![(plan_buy.id, "b3:v1:trade:a".to_owned())]
    );
    assert_eq!(asset.final_quantity, Some(dec!(20)));
    assert_eq!(asset.action, AssetAction::Update);
    assert!(asset.opening.is_none());
}

#[test]
fn the_typed_opening_gives_way_to_the_history() {
    let id = Uuid::new_v4();
    // A migração gravou 100 cotas digitadas em 2026-05 (custo desconhecido).
    let opening = entry(id, 1, Kind::Opening, date!(2026 - 05 - 01), dec!(100), None);
    let book = Book {
        assets: vec![held(id, dec!(110))],
        entries: vec![
            opening.clone(),
            // Depois a pessoa corrigiu para 110 (comprou fora do Prumo em julho).
            entry(
                id,
                2,
                Kind::Adjustment,
                date!(2026 - 08 - 01),
                dec!(10),
                None,
            ),
        ],
        ..empty_book()
    };
    // A B3 inteira: 60 + 40 antes de 2026-05 e as 10 de julho.
    let mut full = group("WEGE3");
    full.trades = vec![
        trade(
            "a",
            date!(2020 - 01 - 10),
            TradeSide::Buy,
            dec!(60),
            dec!(30),
        ),
        trade(
            "b",
            date!(2022 - 01 - 10),
            TradeSide::Buy,
            dec!(40),
            dec!(35),
        ),
        trade(
            "c",
            date!(2026 - 07 - 10),
            TradeSide::Buy,
            dec!(10),
            dec!(50),
        ),
    ];
    let p = plan(&preview(vec![full]), &book, &Decisions::default());
    let asset = only(&p);
    assert!(asset.opening.is_none(), "{:?}", asset.opening);
    assert_eq!(asset.writes.delete_entries.len(), 2);
    assert_eq!(asset.replaced.len(), 2);
    assert_eq!(asset.final_quantity, Some(dec!(110)));
    assert_eq!(asset.current_quantity, dec!(110));

    // Só o ano de 2026: o que a B3 não explica fica num saldo inicial.
    let mut recent = group("WEGE3");
    recent.trades = vec![trade(
        "c",
        date!(2026 - 07 - 10),
        TradeSide::Buy,
        dec!(10),
        dec!(50),
    )];
    let p = plan(&preview(vec![recent]), &book, &Decisions::default());
    let asset = only(&p);
    let opening_plan = asset.opening.as_ref().unwrap();
    assert_eq!(opening_plan.quantity, dec!(100));
    assert_eq!(opening_plan.basis, OpeningBasis::KeepsQuantity);
    assert_eq!(opening_plan.date, date!(2026 - 07 - 09));
    assert_eq!(asset.final_quantity, Some(dec!(110)));
}

#[test]
fn the_replaced_opening_lends_its_average_price() {
    let id = Uuid::new_v4();
    let book = Book {
        assets: vec![held(id, dec!(100))],
        entries: vec![entry(
            id,
            1,
            Kind::Opening,
            date!(2026 - 05 - 01),
            dec!(100),
            Some(dec!(20)),
        )],
        ..empty_book()
    };
    let mut g = group("WEGE3");
    g.trades = vec![trade(
        "c",
        date!(2026 - 07 - 10),
        TradeSide::Buy,
        dec!(10),
        dec!(50),
    )];
    let p = plan(&preview(vec![g]), &book, &Decisions::default());
    let opening = only(&p).opening.clone().unwrap();
    assert_eq!(opening.suggested_unit_price, Some(dec!(20)));
    assert_eq!(opening.unit_price, Some(dec!(20)));
}

#[test]
fn corporate_events_become_what_the_ledger_expects() {
    let mut g = group("WEGE3");
    g.trades = vec![trade(
        "a",
        date!(2020 - 01 - 10),
        TradeSide::Buy,
        dec!(100),
        dec!(20),
    )];
    let mut split = event(
        "split",
        date!(2020 - 06 - 01),
        EventKind::Split,
        Direction::Credit,
    );
    split.quantity = Some(dec!(100));
    let mut bonus = event(
        "bonus",
        date!(2020 - 07 - 01),
        EventKind::Bonus,
        Direction::Credit,
    );
    bonus.quantity = Some(dec!(20));
    bonus.unit_price = Some(dec!(5));
    // Grupamento 11:1 em que a B3 só credita as cotas novas (220 → 20).
    let mut reverse = event(
        "reverse",
        date!(2020 - 08 - 01),
        EventKind::ReverseSplit,
        Direction::Credit,
    );
    reverse.quantity = Some(dec!(20));
    let mut amortization = event(
        "amort",
        date!(2020 - 09 - 01),
        EventKind::Amortization,
        Direction::Credit,
    );
    amortization.value = Some(dec!(50));
    let fraction = {
        let mut e = event(
            "frac",
            date!(2020 - 07 - 01),
            EventKind::FractionInKind,
            Direction::Credit,
        );
        e.quantity = Some(dec!(0.5));
        e
    };
    g.events = vec![split, bonus, reverse, amortization, fraction];
    let p = plan(&preview(vec![g]), &empty_book(), &Decisions::default());
    let asset = only(&p);
    assert!(p.ready, "{:?}", asset.conflict);
    let kinds: Vec<(Kind, Decimal, Option<Decimal>)> = asset
        .writes
        .entries
        .iter()
        .map(|e| (e.kind, e.quantity, e.unit_price))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (Kind::Buy, dec!(100), Some(dec!(20))),
            (Kind::Split, dec!(100), None),
            (Kind::Bonus, dec!(20), Some(dec!(5))),
            (Kind::ReverseSplit, dec!(200), None),
            (Kind::Amortization, dec!(20), Some(dec!(2.5))),
        ]
    );
    assert_eq!(asset.final_quantity, Some(dec!(20)));
    assert_eq!(
        status(&p, "b3:v1:event:frac"),
        (LineState::NotImported, Some("fraction_in_kind"))
    );
}

#[test]
fn a_reverse_split_with_credit_and_debit_is_one_event() {
    let mut g = group("WEGE3");
    g.trades = vec![trade(
        "a",
        date!(2020 - 01 - 10),
        TradeSide::Buy,
        dec!(100),
        dec!(20),
    )];
    let mut out = event(
        "out",
        date!(2020 - 08 - 01),
        EventKind::ReverseSplit,
        Direction::Debit,
    );
    out.quantity = Some(dec!(100));
    let mut back = event(
        "in",
        date!(2020 - 08 - 01),
        EventKind::ReverseSplit,
        Direction::Credit,
    );
    back.quantity = Some(dec!(10));
    let mut debit_bonus = event(
        "odd",
        date!(2020 - 09 - 01),
        EventKind::Bonus,
        Direction::Debit,
    );
    debit_bonus.quantity = Some(dec!(1));
    g.events = vec![out, back, debit_bonus];
    let p = plan(&preview(vec![g]), &empty_book(), &Decisions::default());
    let asset = only(&p);
    let reverse = asset
        .writes
        .entries
        .iter()
        .find(|e| e.kind == Kind::ReverseSplit)
        .unwrap();
    assert_eq!(reverse.quantity, dec!(90));
    // Um lançamento para as duas linhas, com um `external_ref` próprio.
    let stored = reverse.external_ref.clone().unwrap();
    assert!(stored.starts_with("b3:v1:event:"));
    assert_ne!(stored, "b3:v1:event:out");
    assert_eq!(status(&p, "b3:v1:event:in").0, LineState::New);
    assert_eq!(
        status(&p, "b3:v1:event:odd"),
        (LineState::NotImported, Some("event_debit"))
    );
    assert_eq!(asset.final_quantity, Some(dec!(10)));

    // Reimportar: o lançamento junto já está.
    let id = Uuid::new_v4();
    let mut g2 = group("WEGE3");
    let mut out = event(
        "out",
        date!(2020 - 08 - 01),
        EventKind::ReverseSplit,
        Direction::Debit,
    );
    out.quantity = Some(dec!(100));
    let mut back = event(
        "in",
        date!(2020 - 08 - 01),
        EventKind::ReverseSplit,
        Direction::Credit,
    );
    back.quantity = Some(dec!(10));
    g2.events = vec![out, back];
    let book = Book {
        assets: vec![held(id, dec!(10))],
        transaction_refs: HashSet::from([stored]),
        ..empty_book()
    };
    let p = plan(&preview(vec![g2]), &book, &Decisions::default());
    assert_eq!(
        status(&p, "b3:v1:event:out"),
        (LineState::Existing, Some("same_ref"))
    );
    assert_eq!(
        status(&p, "b3:v1:event:in"),
        (LineState::Existing, Some("same_ref"))
    );
}

fn bond_group(key: &str, events: Vec<EventLine>) -> AssetGroup {
    AssetGroup {
        key: key.into(),
        name: Some(key.into()),
        suggested_class: AssetClass::FixedIncome,
        class_basis: ClassBasis::Treasury,
        listed_on_b3: false,
        treasury: true,
        totals: AssetTotals::default(),
        trades: Vec::new(),
        events,
        income: Vec::new(),
    }
}

fn bond_event(
    r: &str,
    day: Date,
    direction: Direction,
    description: &str,
    quantity: Option<Decimal>,
    value: Decimal,
) -> EventLine {
    EventLine {
        code: None,
        product: "Tesouro Selic 2029".into(),
        treasury: true,
        quantity,
        value: Some(value),
        description: description.into(),
        ..event(r, day, EventKind::TreasuryMovement, direction)
    }
}

#[test]
fn treasury_movements_go_to_the_bond_when_it_is_known() {
    let events = vec![
        bond_event(
            "t1",
            date!(2024 - 01 - 10),
            Direction::Credit,
            "COMPRA / VENDA",
            Some(dec!(1.5)),
            dec!(21000),
        ),
        bond_event(
            "t2",
            date!(2025 - 01 - 10),
            Direction::Debit,
            "Resgate",
            Some(dec!(0.5)),
            dec!(7500),
        ),
        bond_event(
            "t3",
            date!(2025 - 02 - 10),
            Direction::Credit,
            "Juros",
            None,
            dec!(100),
        ),
    ];
    let g = bond_group("TESOURO SELIC 2029", events);
    let book = Book {
        treasury_symbols: vec!["TD-SELIC-20290301".into(), "TD-IPCA-20350515".into()],
        ..empty_book()
    };
    let p = plan(&preview(vec![g.clone()]), &book, &Decisions::default());
    let asset = only(&p);
    assert_eq!(asset.action, AssetAction::Create);
    assert_eq!(asset.symbol.as_deref(), Some("TD-SELIC-20290301"));
    assert_eq!(asset.display_name, "Tesouro Selic 2029");
    assert!(asset.treasury && !asset.listed_on_b3);
    assert!(!asset.class_editable);
    let kinds: Vec<(Kind, Decimal, Option<Decimal>)> = asset
        .writes
        .entries
        .iter()
        .map(|e| (e.kind, e.quantity, e.unit_price))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (Kind::Buy, dec!(1.5), Some(dec!(14000))),
            (Kind::Redemption, dec!(0.5), Some(dec!(15000))),
        ]
    );
    assert_eq!(
        status(&p, "b3:v1:event:t3"),
        (LineState::NotImported, Some("treasury_interest"))
    );

    // Sem o título na tabela nem na carteira: tudo fica de fora, com o motivo.
    let p = plan(
        &preview(vec![g.clone()]),
        &empty_book(),
        &Decisions::default(),
    );
    assert_eq!(
        status(&p, "b3:v1:event:t1"),
        (LineState::NotImported, Some("treasury_unidentified"))
    );
    assert_eq!(only(&p).action, AssetAction::Nothing);
    assert!(p.ready);

    // O título que a pessoa já tem.
    let id = Uuid::new_v4();
    let book = Book {
        assets: vec![BookAsset {
            id,
            asset_class: AssetClass::FixedIncome,
            symbol: Some("TD-SELIC-20290301".into()),
            display_name: "Tesouro Selic 2029".into(),
            listed_on_b3: false,
            treasury: true,
            quantity: dec!(1),
        }],
        ..empty_book()
    };
    let p = plan(&preview(vec![g]), &book, &Decisions::default());
    assert_eq!(only(&p).asset_id, Some(id));
    assert_eq!(only(&p).action, AssetAction::Update);
    assert_eq!(
        treasury_kind_and_year("TESOURO IPCA+ COM JUROS SEMESTRAIS 2045"),
        Some((BondKind::IpcaPlusWithCoupons, 2045))
    );
    assert_eq!(treasury_kind_and_year("TESOURO SELIC"), None);
}

#[test]
fn the_received_payment_replaces_the_provisioned_one() {
    let provisional_id = Uuid::new_v4();
    let book = Book {
        income: vec![BookIncome {
            id: provisional_id,
            symbol: Some("WEGE3".into()),
            kind: IncomeKind::Jcp,
            pay_date: date!(2026 - 06 - 10),
            amount_per_unit: dec!(0.5),
            net: dec!(42.5),
            imported: true,
            provisional: true,
            external_ref: Some("b3:v1:income:old".into()),
        }],
        ..empty_book()
    };
    let mut g = group("WEGE3");
    // Pago cinco dias depois, com o mesmo valor por cota e outro líquido.
    let mut paid = income(
        "paid",
        B3IncomeKind::Jcp,
        B3IncomeStatus::Received,
        Some(date!(2026 - 06 - 15)),
        dec!(51),
    );
    paid.unit_price = Some(dec!(0.5));
    paid.quantity = Some(dec!(120));
    g.income = vec![paid];
    let p = plan(&preview(vec![g]), &book, &Decisions::default());
    let asset = only(&p);
    assert_eq!(
        status(&p, "b3:v1:income:paid"),
        (LineState::New, Some("replaces_provisioned"))
    );
    let write = &asset.writes.income[0];
    assert_eq!(write.replace, Some(provisional_id));
    assert!(!write.provisional);
    // JCP: bruto = líquido / 0,85; retenção = a diferença.
    assert_eq!(
        (write.gross, write.withholding, write.net),
        (dec!(60), dec!(9), dec!(51))
    );
    assert_eq!(
        asset.action,
        AssetAction::Nothing,
        "provento sem ativo na carteira"
    );
    assert_eq!(p.summary.income_replaced, 1);
    assert_eq!(p.summary.income_new, 0);
}

#[test]
fn provisioned_lines_are_matched_or_left_out() {
    let mut g = group("WEGE3");
    let received = income(
        "r",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2026 - 05 - 20)),
        dec!(150),
    );
    // O mesmo dividendo, ainda como "a receber" no mesmo envio, com outra data.
    let mut announced = income(
        "p",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Provisioned,
        Some(date!(2026 - 05 - 18)),
        dec!(149.9),
    );
    announced.unit_price = Some(dec!(1.5));
    let undated = income(
        "u",
        B3IncomeKind::Jcp,
        B3IncomeStatus::Provisioned,
        None,
        dec!(40),
    );
    let future = income(
        "f",
        B3IncomeKind::Yield,
        B3IncomeStatus::Provisioned,
        Some(date!(2026 - 12 - 15)),
        dec!(11),
    );
    g.income = vec![announced, undated, future, received];
    let p = plan(&preview(vec![g]), &empty_book(), &Decisions::default());
    assert_eq!(status(&p, "b3:v1:income:r"), (LineState::New, None));
    assert_eq!(
        status(&p, "b3:v1:income:p"),
        (LineState::NotImported, Some("superseded_by_received"))
    );
    assert_eq!(
        status(&p, "b3:v1:income:u"),
        (LineState::NotImported, Some("provisioned_without_date"))
    );
    assert_eq!(status(&p, "b3:v1:income:f"), (LineState::New, None));
    let writes = &only(&p).writes.income;
    assert_eq!(writes.len(), 2);
    let future = writes.iter().find(|w| w.provisional).unwrap();
    assert_eq!(future.kind, IncomeKind::ReitIncome);
    assert_eq!(future.withholding, Decimal::ZERO);

    // Um "a receber" velho de um provento já recebido: já existe.
    let book = Book {
        income: vec![BookIncome {
            id: Uuid::new_v4(),
            symbol: Some("WEGE3".into()),
            kind: IncomeKind::Dividend,
            pay_date: date!(2026 - 05 - 20),
            amount_per_unit: dec!(1.5),
            net: dec!(150),
            imported: false,
            provisional: false,
            external_ref: None,
        }],
        ..empty_book()
    };
    let mut g = group("WEGE3");
    let mut late = income(
        "late",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Provisioned,
        Some(date!(2026 - 05 - 18)),
        dec!(150),
    );
    late.unit_price = Some(dec!(1.5));
    let typed = income(
        "typed",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2026 - 05 - 20)),
        dec!(150),
    );
    g.income = vec![late, typed];
    let p = plan(&preview(vec![g]), &book, &Decisions::default());
    // O recebido bate com o digitado à mão; o provisionado fica sem par e
    // também é reconhecido pelo recebido do mesmo envio.
    assert_eq!(
        status(&p, "b3:v1:income:typed"),
        (LineState::Existing, Some("same_income"))
    );
    assert_eq!(
        status(&p, "b3:v1:income:late"),
        (LineState::NotImported, Some("superseded_by_received"))
    );
}

#[test]
fn income_kinds_and_values() {
    assert_eq!(income_kind(B3IncomeKind::Yield), IncomeKind::ReitIncome);
    assert_eq!(income_kind(B3IncomeKind::Reimbursement), IncomeKind::Other);
    assert_eq!(
        income_kind(B3IncomeKind::FractionAuction),
        IncomeKind::Other
    );
    let mut line = income(
        "x",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2026 - 01 - 01)),
        dec!(10.005),
    );
    line.unit_price = None;
    line.quantity = Some(dec!(3));
    let values = income_values(&line, Some(dec!(99)));
    assert_eq!(values.net, dec!(10.01));
    // A quantidade da B3 vale mais que a do livro.
    assert_eq!(values.quantity, dec!(3));
    assert_eq!(values.amount_per_unit, dec!(3.33666667));
    assert_eq!(values.gross, values.net);
    line.quantity = None;
    assert_eq!(income_values(&line, None).amount_per_unit, Decimal::ZERO);
    let held = income_values(&line, Some(dec!(7)));
    assert_eq!((held.quantity, held.amount_per_unit), (dec!(7), dec!(1.43)));
}

#[test]
fn income_without_quantity_takes_the_ledger_on_the_record_date() {
    let mut g = group("WEGE3");
    g.trades = vec![
        trade(
            "a",
            date!(2024 - 01 - 10),
            TradeSide::Buy,
            dec!(100),
            dec!(40),
        ),
        trade(
            "b",
            date!(2024 - 06 - 10),
            TradeSide::Sell,
            dec!(60),
            dec!(45),
        ),
    ];
    let mut early = income(
        "early",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2024 - 03 - 01)),
        dec!(50),
    );
    early.quantity = None;
    early.unit_price = None;
    let mut late = income(
        "late",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2024 - 09 - 01)),
        dec!(20),
    );
    late.quantity = None;
    late.unit_price = None;
    // A B3 trouxe a quantidade: vale ela, não a do livro.
    let given = income(
        "given",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Received,
        Some(date!(2024 - 10 - 01)),
        dec!(30),
    );
    // A receber, com a data no futuro: a data com fica em hoje.
    let mut ahead = income(
        "ahead",
        B3IncomeKind::Dividend,
        B3IncomeStatus::Provisioned,
        Some(date!(2027 - 01 - 10)),
        dec!(8),
    );
    ahead.quantity = None;
    ahead.unit_price = None;
    g.income = vec![early, late, given, ahead];
    let p = plan(&preview(vec![g]), &empty_book(), &Decisions::default());
    let writes = &only(&p).writes.income;
    let by_ref = |r: &str| {
        writes
            .iter()
            .find(|w| w.external_ref == format!("b3:v1:income:{r}"))
            .unwrap()
    };
    // Antes da venda o livro (já com os lançamentos desta importação) tinha 100.
    assert_eq!(
        (by_ref("early").quantity, by_ref("early").amount_per_unit),
        (dec!(100), dec!(0.5))
    );
    assert_eq!(
        (by_ref("late").quantity, by_ref("late").amount_per_unit),
        (dec!(40), dec!(0.5))
    );
    assert_eq!(by_ref("given").quantity, dec!(100));
    assert_eq!(by_ref("ahead").quantity, dec!(40));
}

#[test]
fn prices_close_follows_the_tolerance() {
    assert!(prices_close(None, None));
    assert!(!prices_close(Some(dec!(1)), None));
    assert!(prices_close(Some(dec!(100)), Some(dec!(101.9))));
    assert!(!prices_close(Some(dec!(100)), Some(dec!(103))));
    assert!(prices_close(Some(dec!(0.30)), Some(dec!(0.31))));
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]

        /// Qualquer sequência de compras e vendas da B3 sai pronta para gravar
        /// com o saldo inicial sugerido, e ele é o menor que fecha o livro.
        #[test]
        fn the_suggested_opening_always_closes_the_ledger(
            moves in prop::collection::vec((0u16..400, prop::bool::ANY, 1i64..500, 1i64..10_000), 1..30),
        ) {
            let mut g = group("WEGE3");
            g.trades = moves
                .iter()
                .enumerate()
                .map(|(i, (day, buy, quantity, cents))| {
                    trade(
                        &format!("{i}"),
                        date!(2020 - 01 - 01) + time::Duration::days(i64::from(*day)),
                        if *buy { TradeSide::Buy } else { TradeSide::Sell },
                        Decimal::from(*quantity),
                        Decimal::new(*cents, 2),
                    )
                })
                .collect();
            let p = plan(&preview(vec![g.clone()]), &empty_book(), &Decisions::default());
            let asset = only(&p);
            prop_assert!(p.ready, "{:?}", asset.conflict);
            let final_quantity = asset.final_quantity.unwrap_or_default();
            prop_assert!(final_quantity >= Decimal::ZERO);
            let suggested = asset.opening.as_ref().map_or(Decimal::ZERO, |o| o.quantity);
            if suggested > Decimal::ZERO {
                let less = decide("WEGE3", AssetDecision {
                    opening: Some(OpeningChoice { quantity: suggested - Decimal::ONE, unit_price: None }),
                    ..AssetDecision::default()
                });
                let tighter = plan(&preview(vec![g]), &empty_book(), &less);
                prop_assert!(!tighter.ready, "um a menos ainda fechava");
            }
        }
    }
}
