use super::*;
use proptest::prelude::*;
use rust_decimal_macros::dec;
use time::macros::date;

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

fn stock(entries: Vec<Entry>) -> AutoAsset {
    AutoAsset {
        id: Uuid::from_u128(1),
        symbol: "ABCD4".into(),
        display_name: "Empresa Ficticia PN".into(),
        class: AssetClass::BrStocks,
        isin: Some("BRABCDACNPR1".into()),
        entries,
    }
}

fn reit(entries: Vec<Entry>) -> AutoAsset {
    AutoAsset {
        id: Uuid::from_u128(2),
        symbol: "FICT11".into(),
        display_name: "Fundo Ficticio".into(),
        class: AssetClass::BrReits,
        isin: Some("BRFICTCTF001".into()),
        entries,
    }
}

fn announcement(
    isin: &str,
    label: &str,
    record: Date,
    rate: Decimal,
    pay: Option<Date>,
) -> CashAnnouncement {
    CashAnnouncement {
        issuer: isin[2..6].to_owned(),
        isin: isin.into(),
        label: label.into(),
        record_date: record,
        rate,
        pay_date: pay,
        approved_on: None,
        related_to: None,
    }
}

fn book() -> Vec<Entry> {
    vec![
        entry(
            Kind::Buy,
            date!(2026 - 01 - 10),
            1,
            dec!(100),
            Some(dec!(10)),
        ),
        entry(
            Kind::Buy,
            date!(2026 - 03 - 10),
            2,
            dec!(50),
            Some(dec!(12)),
        ),
        entry(
            Kind::Sell,
            date!(2026 - 05 - 11),
            3,
            dec!(30),
            Some(dec!(14)),
        ),
    ]
}

fn known_from(auto: &AutoIncome, id: u128, source: IncomeSource) -> KnownIncome {
    KnownIncome {
        id: Uuid::from_u128(id),
        asset_id: Some(auto.asset_id),
        symbol: Some(auto.symbol.clone()),
        kind: auto.kind,
        ex_date: auto.ex_date,
        pay_date: auto.pay_date,
        amount_per_unit: auto.amount_per_unit,
        quantity: auto.quantity,
        gross: auto.amounts.gross,
        withholding: auto.amounts.withholding,
        net: auto.amounts.net,
        source,
        external_ref: (source == IncomeSource::B3Announcement).then(|| auto.external_ref.clone()),
        display_name: auto.display_name.clone(),
    }
}

/// Grava o plano numa lista em memória, como a store grava no banco.
fn apply(known: &mut Vec<KnownIncome>, plan: &AutoPlan, next_id: &mut u128) {
    known.retain(|k| !plan.delete.contains(&k.id));
    for (id, auto) in &plan.update {
        let slot = known.iter_mut().find(|k| k.id == *id).unwrap();
        *slot = known_from(auto, id.as_u128(), IncomeSource::B3Announcement);
    }
    for auto in &plan.insert {
        *next_id += 1;
        known.push(known_from(auto, *next_id, IncomeSource::B3Announcement));
    }
}

fn manual(
    asset: &AutoAsset,
    kind: IncomeKind,
    ex: Option<Date>,
    pay: Date,
    per_unit: Decimal,
    quantity: Decimal,
) -> KnownIncome {
    let values = amounts(kind, pay, per_unit, quantity, None).unwrap();
    KnownIncome {
        id: Uuid::from_u128(900),
        asset_id: Some(asset.id),
        symbol: Some(asset.symbol.clone()),
        kind,
        ex_date: ex,
        pay_date: pay,
        amount_per_unit: per_unit,
        quantity,
        gross: values.gross,
        withholding: values.withholding,
        net: values.net,
        source: IncomeSource::Manual,
        external_ref: None,
        display_name: asset.display_name.clone(),
    }
}

// ------------------------------------------------------------ texto da B3 --

#[test]
fn reads_brazilian_numbers_and_dates() {
    assert_eq!(parse_br_decimal("0,02424250000"), Some(dec!(0.02424250000)));
    assert_eq!(parse_br_decimal("1.234,5"), Some(dec!(1234.5)));
    assert_eq!(parse_br_decimal("100,00000000000"), Some(dec!(100)));
    assert_eq!(parse_br_decimal(""), None);
    assert_eq!(parse_br_decimal("abc"), None);
    assert_eq!(parse_br_decimal("-1,0"), None);
    assert_eq!(parse_br_date("31/08/2026"), Some(date!(2026 - 08 - 31)));
    assert_eq!(parse_br_date(" 01/10/2026 "), Some(date!(2026 - 10 - 01)));
    // O "sem data" da B3 e o que não é data.
    assert_eq!(parse_br_date("31/12/9999"), None);
    assert_eq!(parse_br_date(""), None);
    assert_eq!(parse_br_date("31/02/2026"), None);
    assert_eq!(parse_br_date("2026-08-31"), None);
    assert_eq!(parse_br_date("01/01/2026/1"), None);
}

#[test]
fn splits_the_ticker_into_issuer_and_number() {
    let itsa = parse_ticker("itsa4").unwrap();
    assert_eq!((itsa.issuer.as_str(), itsa.number), ("ITSA", 4));
    assert_eq!(parse_ticker("TAEE11").unwrap().number, 11);
    assert_eq!(parse_ticker("B3SA3").unwrap().issuer, "B3SA");
    assert_eq!(parse_ticker("ITSA4F").unwrap().code, "ITSA4");
    assert_eq!(parse_ticker("KNRI11.SA").unwrap().code, "KNRI11");
    assert_eq!(parse_ticker("AAPL"), None);
    assert_eq!(parse_ticker("1234"), None);
    assert_eq!(parse_ticker("12345"), None);
    assert_eq!(parse_ticker("TD-IPCA-2035"), None);
    assert_eq!(parse_ticker("ITSA444"), None);
}

#[test]
fn venue_follows_the_class_and_the_number() {
    let t = |s: &str| parse_ticker(s).unwrap();
    assert_eq!(Venue::of(AssetClass::BrReits, &t("KNRI11")), Venue::Fund);
    assert_eq!(
        Venue::of(AssetClass::BrStocks, &t("TAEE11")),
        Venue::Company
    );
    assert_eq!(Venue::of(AssetClass::IntlStocks, &t("IVVB11")), Venue::Fund);
    assert_eq!(
        Venue::of(AssetClass::IntlStocks, &t("AAPL34")),
        Venue::Company
    );
    assert_eq!(Venue::Fund.as_str(), "fund");
    assert_eq!(Venue::Company.as_str(), "company");
}

#[test]
fn the_isin_comes_from_the_listing_or_from_the_rule() {
    let t = |s: &str| parse_ticker(s).unwrap();
    let itsa = ["BRITSAACNOR0", "BRITSAACNPR7"];
    // O cadastro da B3 vence.
    assert_eq!(
        isin_for(&t("ITSA4"), Venue::Company, Some("BRITSAACNPR7"), &[]),
        Some("BRITSAACNPR7".into())
    );
    // Sem cadastro: a regra do tipo de papel (ACNOR = ON, ACNPR = PN).
    assert_eq!(
        isin_for(&t("ITSA3"), Venue::Company, None, &itsa),
        Some("BRITSAACNOR0".into())
    );
    assert_eq!(
        isin_for(&t("ITSA4"), Venue::Company, None, &itsa),
        Some("BRITSAACNPR7".into())
    );
    // FII: a cota (CTF), não o recibo de subscrição.
    assert_eq!(
        isin_for(
            &t("HGLG11"),
            Venue::Fund,
            None,
            &["BRHGLGR24M19", "BRHGLGCTF004", "BRHGLGCTF004"]
        ),
        Some("BRHGLGCTF004".into())
    );
    assert_eq!(
        isin_for(&t("TAEE11"), Venue::Company, None, &["BRTAEECDAM10"]),
        Some("BRTAEECDAM10".into())
    );
    assert_eq!(
        isin_for(&t("AAPL34"), Venue::Company, None, &["BRAAPLBDR004"]),
        Some("BRAAPLBDR004".into())
    );
    // Dois candidatos ou nenhum: dúvida, sem ISIN.
    assert_eq!(
        isin_for(
            &t("ABCD3"),
            Venue::Company,
            None,
            &["BRABCDACNOR1", "BRABCDACNOR9"]
        ),
        None
    );
    assert_eq!(isin_for(&t("ITSA9"), Venue::Company, None, &itsa), None);
    assert_eq!(isin_for(&t("ITSA5"), Venue::Company, None, &itsa), None);
}

#[test]
fn label_decides_the_kind() {
    assert_eq!(
        cash_kind("DIVIDENDO", AssetClass::BrStocks),
        IncomeKind::Dividend
    );
    assert_eq!(
        cash_kind("JRS CAP PROPRIO", AssetClass::BrStocks),
        IncomeKind::Jcp
    );
    assert_eq!(
        cash_kind("RENDIMENTO", AssetClass::BrReits),
        IncomeKind::ReitIncome
    );
    // Rendimento de ação (correção até o pagamento) ou de ETF: outro.
    assert_eq!(
        cash_kind("RENDIMENTO", AssetClass::BrStocks),
        IncomeKind::Other
    );
    assert_eq!(
        cash_kind("RENDIMENTO", AssetClass::IntlStocks),
        IncomeKind::Other
    );
    assert_eq!(
        cash_kind("AMORTIZACAO", AssetClass::BrReits),
        IncomeKind::Amortization
    );
    assert_eq!(
        cash_kind("RESTITUICAO DE CAPITAL", AssetClass::BrStocks),
        IncomeKind::Other
    );
    assert_eq!(
        StockEventKind::of_label("BONIFICACAO"),
        Some(StockEventKind::Bonus)
    );
    assert_eq!(
        StockEventKind::of_label("DESDOBRAMENTO"),
        Some(StockEventKind::Split)
    );
    assert_eq!(
        StockEventKind::of_label("GRUPAMENTO"),
        Some(StockEventKind::ReverseSplit)
    );
    assert_eq!(StockEventKind::of_label("CIS RED CAP"), None);
    assert_eq!(StockEventKind::Bonus.ledger_kind(), Kind::Bonus);
    assert_eq!(StockEventKind::Split.ledger_kind(), Kind::Split);
    assert_eq!(
        StockEventKind::ReverseSplit.ledger_kind(),
        Kind::ReverseSplit
    );
}

#[test]
fn ex_date_is_the_next_business_day() {
    // Quinta → sexta; sexta → segunda.
    assert_eq!(ex_date_after(date!(2026 - 08 - 27)), date!(2026 - 08 - 28));
    assert_eq!(ex_date_after(date!(2026 - 05 - 29)), date!(2026 - 06 - 01));
    // Quinta antes da Sexta-feira Santa de 2026 → segunda.
    assert_eq!(ex_date_after(date!(2026 - 04 - 02)), date!(2026 - 04 - 06));
    // Ano fora do calendário: só o fim de semana pula.
    assert_eq!(ex_date_after(date!(2031 - 01 - 03)), date!(2031 - 01 - 06));
}

#[test]
fn external_ref_is_stable_and_readable() {
    let a = announcement(
        "BRITSAACNPR7",
        "JRS CAP PROPRIO",
        date!(2026 - 08 - 31),
        dec!(0.02424250000),
        Some(date!(2026 - 10 - 01)),
    );
    assert_eq!(
        external_ref(&a, 0),
        "b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425"
    );
    let mut later = a.clone();
    later.pay_date = None;
    assert_eq!(external_ref(&a, 0), external_ref(&later, 0));
    // A segunda e a terceira parcela do mesmo anúncio.
    assert_eq!(
        external_ref(&a, 1),
        "b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425:2"
    );
    assert_eq!(
        external_ref(&a, 2),
        "b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425:3"
    );
}

// ------------------------------------------------------ mesmo provento --

#[test]
fn same_value_allows_one_percent_and_the_net_jcp() {
    let auto = Payment {
        kind: IncomeKind::Jcp,
        pay_date: date!(2026 - 10 - 01),
        per_unit: dec!(0.10),
        gross: dec!(10),
        net: dec!(8.25),
    };
    let mut existing = auto;
    assert!(same_value(&existing, &auto));
    existing.per_unit = dec!(0.1009);
    assert!(same_value(&existing, &auto));
    existing.per_unit = dec!(0.102);
    assert!(!same_value(&existing, &auto));
    // O JCP gravado pelo líquido (17,5% em 2026).
    existing.per_unit = dec!(0.0825);
    assert!(same_value(&existing, &auto));
    // Outro tipo não é o mesmo.
    existing.per_unit = dec!(0.10);
    existing.kind = IncomeKind::Dividend;
    assert!(!same_value(&existing, &auto));
    // Dividendo e rendimento contam como iguais.
    let dividend = Payment {
        kind: IncomeKind::ReitIncome,
        ..auto
    };
    assert!(same_value(&existing, &dividend));
    // Sem valor por cota no gravado: o líquido ou o bruto.
    let mut blank = Payment {
        per_unit: Decimal::ZERO,
        ..auto
    };
    assert!(same_value(&blank, &auto));
    blank.net = dec!(9);
    blank.gross = dec!(11);
    assert!(!same_value(&blank, &auto));
}

// --------------------------------------------------------------- plano --

#[test]
fn quantity_is_the_book_at_the_record_date() {
    let asset = stock(book());
    let list = [
        // Antes da primeira compra: nada.
        announcement(
            "BRABCDACNPR1",
            "DIVIDENDO",
            date!(2026 - 01 - 09),
            dec!(1),
            Some(date!(2026 - 01 - 20)),
        ),
        // Data com no dia da segunda compra: ela conta (150).
        announcement(
            "BRABCDACNPR1",
            "DIVIDENDO",
            date!(2026 - 03 - 10),
            dec!(0.5),
            Some(date!(2026 - 03 - 25)),
        ),
        // Data com um dia antes da compra: só as 100 de antes.
        announcement(
            "BRABCDACNPR1",
            "DIVIDENDO",
            date!(2026 - 03 - 09),
            dec!(0.25),
            Some(date!(2026 - 03 - 25)),
        ),
    ];
    let plan = plan(&[asset], &list, &[], &HashSet::new());
    let mut got: Vec<(Date, Decimal, Decimal)> = plan
        .insert
        .iter()
        .map(|a| (a.pay_date, a.quantity, a.amounts.gross))
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            (date!(2026 - 03 - 25), dec!(100), dec!(25)),
            (date!(2026 - 03 - 25), dec!(150), dec!(75)),
        ]
    );
    assert!(plan.update.is_empty() && plan.delete.is_empty());
}

#[test]
fn jcp_withholding_follows_the_pay_date() {
    let asset = stock(book());
    let list = [
        announcement(
            "BRABCDACNPR1",
            "JRS CAP PROPRIO",
            date!(2026 - 02 - 02),
            dec!(0.10),
            Some(date!(2026 - 03 - 02)),
        ),
        announcement(
            "BRABCDACNPR1",
            "JRS CAP PROPRIO",
            date!(2026 - 02 - 03),
            dec!(0.20),
            Some(date!(2026 - 02 - 27)),
        ),
    ];
    let plan = plan(std::slice::from_ref(&asset), &list, &[], &HashSet::new());
    let first = plan
        .insert
        .iter()
        .find(|a| a.amount_per_unit == dec!(0.1))
        .unwrap();
    assert_eq!(first.kind, IncomeKind::Jcp);
    assert_eq!(first.amounts.gross, dec!(10));
    assert_eq!(first.amounts.withholding, dec!(1.75));
    assert_eq!(first.amounts.net, dec!(8.25));
    assert_eq!(first.ex_date, Some(date!(2026 - 02 - 03)));

    // Pago em 2025 (anúncio antigo): 15%.
    let old_book = vec![entry(
        Kind::Buy,
        date!(2025 - 01 - 10),
        1,
        dec!(100),
        Some(dec!(10)),
    )];
    let old = plan_one(
        &stock(old_book),
        announcement(
            "BRABCDACNPR1",
            "JRS CAP PROPRIO",
            date!(2025 - 11 - 28),
            dec!(0.10),
            Some(date!(2025 - 12 - 30)),
        ),
    );
    assert_eq!(old.amounts.withholding, dec!(1.50));
    assert_eq!(old.amounts.net, dec!(8.50));
}

fn plan_one(asset: &AutoAsset, a: CashAnnouncement) -> AutoIncome {
    let plan = plan(std::slice::from_ref(asset), &[a], &[], &HashSet::new());
    assert_eq!(plan.insert.len(), 1, "{plan:?}");
    plan.insert[0].clone()
}

#[test]
fn reit_income_and_the_rate_with_eleven_places() {
    let asset = reit(vec![entry(
        Kind::Opening,
        date!(2026 - 11 - 20),
        1,
        dec!(37),
        None,
    )]);
    let auto = plan_one(
        &asset,
        announcement(
            "BRFICTCTF001",
            "RENDIMENTO",
            date!(2026 - 10 - 30),
            dec!(0.00281361809),
            Some(date!(2026 - 11 - 16)),
        ),
    );
    // O saldo inicial conta desde sempre.
    assert_eq!(auto.quantity, dec!(37));
    assert_eq!(auto.kind, IncomeKind::ReitIncome);
    // O bruto usa as 11 casas; o valor por cota gravado, 8.
    assert_eq!(auto.amount_per_unit, dec!(0.00281362));
    assert_eq!(auto.amounts.gross, dec!(0.10));
    assert_eq!(auto.amounts.withholding, Decimal::ZERO);
}

#[test]
fn without_a_pay_date_nothing_is_written_yet() {
    let asset = stock(book());
    let pending = announcement(
        "BRABCDACNPR1",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(1),
        None,
    );
    assert!(plan(
        std::slice::from_ref(&asset),
        std::slice::from_ref(&pending),
        &[],
        &HashSet::new()
    )
    .is_empty());
    // A data saiu: entra.
    let mut defined = pending;
    defined.pay_date = Some(date!(2026 - 04 - 20));
    let auto = plan_one(&asset, defined);
    assert_eq!(auto.quantity, dec!(150));
}

#[test]
fn asset_without_isin_or_other_isin_is_ignored() {
    let mut asset = stock(book());
    let a = announcement(
        "BRABCDACNOR3",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(1),
        Some(date!(2026 - 04 - 20)),
    );
    assert!(plan(
        std::slice::from_ref(&asset),
        std::slice::from_ref(&a),
        &[],
        &HashSet::new()
    )
    .is_empty());
    asset.isin = None;
    assert!(plan(&[asset], &[a], &[], &HashSet::new()).is_empty());
}

#[test]
fn a_manual_income_with_the_same_values_wins() {
    let asset = stock(book());
    let a = announcement(
        "BRABCDACNPR1",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(0.5),
        Some(date!(2026 - 04 - 20)),
    );
    // Lançado à mão com o valor de outra ferramenta (2 casas) e sem data ex.
    let typed = manual(
        &asset,
        IncomeKind::Dividend,
        None,
        date!(2026 - 04 - 20),
        dec!(0.50),
        dec!(150),
    );
    let p = plan(
        std::slice::from_ref(&asset),
        std::slice::from_ref(&a),
        std::slice::from_ref(&typed),
        &HashSet::new(),
    );
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(p.covered, 1);

    // Com a data com no campo de data ex (outra ferramenta) e outra data de pagamento.
    let mut by_record = typed.clone();
    by_record.ex_date = Some(date!(2026 - 04 - 01));
    by_record.pay_date = date!(2026 - 04 - 22);
    assert!(plan(
        std::slice::from_ref(&asset),
        std::slice::from_ref(&a),
        &[by_record],
        &HashSet::new()
    )
    .is_empty());

    // Com a data ex de verdade.
    let mut by_ex = typed.clone();
    by_ex.ex_date = Some(date!(2026 - 04 - 02));
    by_ex.pay_date = date!(2026 - 04 - 22);
    assert!(plan(
        std::slice::from_ref(&asset),
        std::slice::from_ref(&a),
        &[by_ex],
        &HashSet::new()
    )
    .is_empty());

    // Valor por cota diferente: é outro provento, e o automático entra.
    let mut other_value = typed.clone();
    other_value.amount_per_unit = dec!(0.55);
    let p = plan(
        std::slice::from_ref(&asset),
        std::slice::from_ref(&a),
        &[other_value],
        &HashSet::new(),
    );
    assert_eq!(p.insert.len(), 1);

    // Outra data (nem data com, nem pagamento): também entra.
    let mut other_day = typed;
    other_day.pay_date = date!(2026 - 05 - 20);
    other_day.ex_date = Some(date!(2026 - 05 - 01));
    assert_eq!(
        plan(&[asset], &[a], &[other_day], &HashSet::new())
            .insert
            .len(),
        1
    );
}

#[test]
fn one_manual_covers_one_announcement_only() {
    // Dois JCP com a mesma data com e o mesmo pagamento, valores diferentes.
    let asset = stock(book());
    let list = [
        announcement(
            "BRABCDACNPR1",
            "JRS CAP PROPRIO",
            date!(2026 - 09 - 01),
            dec!(0.10245643978),
            Some(date!(2026 - 09 - 11)),
        ),
        announcement(
            "BRABCDACNPR1",
            "JRS CAP PROPRIO",
            date!(2026 - 09 - 01),
            dec!(0.03450628312),
            Some(date!(2026 - 09 - 11)),
        ),
    ];
    let typed = manual(
        &asset,
        IncomeKind::Jcp,
        None,
        date!(2026 - 09 - 11),
        dec!(0.10),
        dec!(120),
    );
    // 0,10 está a mais de 1% de 0,10246: não cobre; com 0,1025 cobre só o primeiro.
    let p = plan(
        std::slice::from_ref(&asset),
        &list,
        std::slice::from_ref(&typed),
        &HashSet::new(),
    );
    assert_eq!(p.insert.len(), 2);
    let mut close = typed;
    close.amount_per_unit = dec!(0.1025);
    let p = plan(&[asset], &list, &[close], &HashSet::new());
    assert_eq!(p.insert.len(), 1);
    assert_eq!(p.insert[0].amount_per_unit, dec!(0.03450628));
    assert_eq!(p.covered, 1);
}

#[test]
fn each_installment_is_its_own_automatic() {
    // Um dividendo da mesma data com e do mesmo valor, pago em três anos.
    let asset = stock(book());
    let record = date!(2026 - 06 - 19);
    let pays = [
        date!(2028 - 08 - 16),
        date!(2026 - 08 - 12),
        date!(2027 - 08 - 11),
    ];
    let list: Vec<CashAnnouncement> = pays
        .iter()
        .map(|pay| announcement("BRABCDACNPR1", "DIVIDENDO", record, dec!(0.4), Some(*pay)))
        .collect();
    let p = plan(std::slice::from_ref(&asset), &list, &[], &HashSet::new());
    let mut got: Vec<(Date, String)> = p
        .insert
        .iter()
        .map(|a| (a.pay_date, a.external_ref.clone()))
        .collect();
    got.sort();
    let base = "b3a:BRABCDACNPR1:DIVIDENDO:2026-06-19:0.4";
    assert_eq!(
        got,
        [
            (date!(2026 - 08 - 12), base.to_owned()),
            (date!(2027 - 08 - 11), format!("{base}:2")),
            (date!(2028 - 08 - 16), format!("{base}:3")),
        ]
    );
    assert!(p.insert.iter().all(|a| a.amounts.gross == dec!(48.00)));

    // Gravado, a rodada seguinte não muda nada; a pessoa lançou a segunda
    // parcela à mão: só ela sai.
    let mut known = Vec::new();
    let mut next = 100u128;
    apply(&mut known, &p, &mut next);
    assert!(plan(std::slice::from_ref(&asset), &list, &known, &HashSet::new()).is_empty());
    known.push(manual(
        &asset,
        IncomeKind::Dividend,
        Some(record),
        date!(2027 - 08 - 11),
        dec!(0.4),
        dec!(120),
    ));
    let p = plan(std::slice::from_ref(&asset), &list, &known, &HashSet::new());
    assert_eq!(p.covered, 1);
    assert_eq!(p.delete.len(), 1);
    let gone = known.iter().find(|k| k.id == p.delete[0]).unwrap();
    assert_eq!(gone.pay_date, date!(2027 - 08 - 11));
}

#[test]
fn installments_typed_by_hand_cover_an_announcement_in_one_line() {
    // A B3 publica um JCP de 0,269 numa linha só ("pago até 31/12"); a pessoa
    // lançou as três parcelas que caíram de fato, cada uma com a data com.
    let asset = stock(book());
    let record = date!(2026 - 06 - 18);
    let list = [announcement(
        "BRABCDACNPR1",
        "JRS CAP PROPRIO",
        record,
        dec!(0.269),
        Some(date!(2026 - 12 - 31)),
    )];
    let part = |id: u128, pay: Date, per_unit: Decimal| {
        let mut k = manual(
            &asset,
            IncomeKind::Jcp,
            Some(record),
            pay,
            per_unit,
            dec!(120),
        );
        k.id = Uuid::from_u128(id);
        k
    };
    let parts = [
        part(901, date!(2026 - 07 - 06), dec!(0.075)),
        part(902, date!(2026 - 09 - 06), dec!(0.0755)),
        part(903, date!(2026 - 11 - 06), dec!(0.1185)),
    ];
    let p = plan(std::slice::from_ref(&asset), &list, &parts, &HashSet::new());
    assert!(p.insert.is_empty(), "{p:?}");
    assert_eq!(p.covered, 1);

    // Duas parcelas só (0,1505) não fecham o anúncio: o automático fica.
    let p = plan(
        std::slice::from_ref(&asset),
        &list,
        &parts[..2],
        &HashSet::new(),
    );
    assert_eq!(p.insert.len(), 1);
    // Parcela de outra data com não entra na soma.
    let mut other = parts.clone();
    other[2].ex_date = Some(date!(2026 - 03 - 02));
    let p = plan(std::slice::from_ref(&asset), &list, &other, &HashSet::new());
    assert_eq!(p.insert.len(), 1);
}

#[test]
fn a_group_never_takes_what_matches_one_to_one() {
    // Dois anúncios na mesma data com: 0,10 (casa com o lançado de 0,10) e
    // 0,30 (casa com a soma de 0,10 + 0,20 só se o de 0,10 sobrasse).
    let asset = stock(book());
    let record = date!(2026 - 06 - 18);
    let pay = date!(2026 - 07 - 20);
    let list = [
        announcement("BRABCDACNPR1", "DIVIDENDO", record, dec!(0.30), Some(pay)),
        announcement("BRABCDACNPR1", "DIVIDENDO", record, dec!(0.10), Some(pay)),
    ];
    let mut small = manual(
        &asset,
        IncomeKind::Dividend,
        Some(record),
        pay,
        dec!(0.10),
        dec!(120),
    );
    small.id = Uuid::from_u128(901);
    let mut other = manual(
        &asset,
        IncomeKind::Dividend,
        Some(record),
        date!(2026 - 08 - 20),
        dec!(0.20),
        dec!(120),
    );
    other.id = Uuid::from_u128(902);
    let p = plan(
        std::slice::from_ref(&asset),
        &list,
        &[small, other],
        &HashSet::new(),
    );
    // O de 0,10 casa um para um; o de 0,30 fica sem parcelas que fechem.
    assert_eq!(p.covered, 1);
    assert_eq!(p.insert.len(), 1);
    assert_eq!(p.insert[0].amount_per_unit, dec!(0.30));
}

#[test]
fn an_import_arriving_later_removes_the_automatic() {
    let asset = stock(book());
    let a = announcement(
        "BRABCDACNPR1",
        "JRS CAP PROPRIO",
        date!(2026 - 04 - 01),
        dec!(0.2),
        Some(date!(2026 - 04 - 20)),
    );
    let auto = plan_one(&asset, a.clone());
    let mut known = vec![known_from(&auto, 10, IncomeSource::B3Announcement)];
    // O extrato da B3 dá o líquido por cota.
    let mut imported = manual(
        &asset,
        IncomeKind::Jcp,
        None,
        date!(2026 - 04 - 20),
        dec!(0.165),
        dec!(150),
    );
    imported.source = IncomeSource::B3Import;
    imported.external_ref = Some("b3:v1:income:x".into());
    known.push(imported);
    let p = plan(&[asset], &[a], &known, &HashSet::new());
    assert!(p.insert.is_empty() && p.update.is_empty());
    assert_eq!(p.delete, vec![Uuid::from_u128(10)]);
}

#[test]
fn recalculates_when_the_book_changes_and_removes_when_it_is_empty() {
    let a = announcement(
        "BRABCDACNPR1",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(1),
        Some(date!(2026 - 04 - 20)),
    );
    let auto = plan_one(&stock(book()), a.clone());
    let known = vec![known_from(&auto, 10, IncomeSource::B3Announcement)];
    // Rodada repetida: nada muda.
    assert!(plan(
        &[stock(book())],
        std::slice::from_ref(&a),
        &known,
        &HashSet::new()
    )
    .is_empty());

    // Lançamento retroativo antes da data com: a quantidade muda.
    let mut more = book();
    more.push(entry(
        Kind::Buy,
        date!(2026 - 02 - 01),
        4,
        dec!(10),
        Some(dec!(11)),
    ));
    let p = plan(
        &[stock(more)],
        std::slice::from_ref(&a),
        &known,
        &HashSet::new(),
    );
    assert_eq!(p.update.len(), 1);
    assert_eq!(p.update[0].0, Uuid::from_u128(10));
    assert_eq!(p.update[0].1.quantity, dec!(160));
    assert_eq!(p.update[0].1.amounts.gross, dec!(160));

    // O livro zerou na data com: o automático some.
    let sold = vec![
        entry(
            Kind::Buy,
            date!(2026 - 01 - 10),
            1,
            dec!(100),
            Some(dec!(10)),
        ),
        entry(
            Kind::Sell,
            date!(2026 - 02 - 10),
            2,
            dec!(100),
            Some(dec!(11)),
        ),
    ];
    let p = plan(
        &[stock(sold)],
        std::slice::from_ref(&a),
        &known,
        &HashSet::new(),
    );
    assert_eq!(p.delete, vec![Uuid::from_u128(10)]);

    // O anúncio sumiu da B3 (corrigido): some também.
    let p = plan(&[stock(book())], &[], &known, &HashSet::new());
    assert_eq!(p.delete, vec![Uuid::from_u128(10)]);

    // A data de pagamento mudou: troca.
    let mut moved = a;
    moved.pay_date = Some(date!(2026 - 04 - 30));
    let p = plan(&[stock(book())], &[moved], &known, &HashSet::new());
    assert_eq!(p.update.len(), 1);
    assert_eq!(p.update[0].1.pay_date, date!(2026 - 04 - 30));
}

#[test]
fn manual_and_imported_are_never_in_the_plan() {
    let asset = stock(book());
    let typed = manual(
        &asset,
        IncomeKind::Dividend,
        None,
        date!(2026 - 07 - 01),
        dec!(3),
        dec!(120),
    );
    let mut imported = typed.clone();
    imported.id = Uuid::from_u128(901);
    imported.source = IncomeSource::B3Import;
    let p = plan(&[asset], &[], &[typed, imported], &HashSet::new());
    assert!(p.is_empty());
}

#[test]
fn a_dismissed_automatic_does_not_come_back() {
    let asset = stock(book());
    let a = announcement(
        "BRABCDACNPR1",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(1),
        Some(date!(2026 - 04 - 20)),
    );
    let dismissed: HashSet<String> = [external_ref(&a, 0)].into_iter().collect();
    assert!(plan(&[asset], &[a], &[], &dismissed).is_empty());
}

#[test]
fn a_broken_book_keeps_what_is_there() {
    let a = announcement(
        "BRABCDACNPR1",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(1),
        Some(date!(2026 - 04 - 20)),
    );
    let auto = plan_one(&stock(book()), a.clone());
    let known = vec![known_from(&auto, 10, IncomeSource::B3Announcement)];
    // Venda além da posição: o livro não fecha.
    let broken = vec![
        entry(
            Kind::Buy,
            date!(2026 - 01 - 10),
            1,
            dec!(10),
            Some(dec!(10)),
        ),
        entry(
            Kind::Sell,
            date!(2026 - 02 - 10),
            2,
            dec!(50),
            Some(dec!(11)),
        ),
    ];
    let p = plan(&[stock(broken)], &[a], &known, &HashSet::new());
    assert!(p.is_empty());
    assert_eq!(p.skipped, vec![Uuid::from_u128(1)]);
}

#[test]
fn two_assets_with_the_same_isin_write_once() {
    let a = announcement(
        "BRABCDACNPR1",
        "DIVIDENDO",
        date!(2026 - 04 - 01),
        dec!(1),
        Some(date!(2026 - 04 - 20)),
    );
    let first = stock(book());
    let mut second = stock(book());
    second.id = Uuid::from_u128(7);
    let p = plan(&[second, first], &[a], &[], &HashSet::new());
    assert_eq!(p.insert.len(), 1);
    assert_eq!(p.insert[0].asset_id, Uuid::from_u128(1));
}

// --------------------------------------------------------- pendências --

fn bonus(record: Date, factor: Decimal) -> StockAnnouncement {
    StockAnnouncement {
        issuer: "ABCD".into(),
        isin: "BRABCDACNPR1".into(),
        label: "BONIFICACAO".into(),
        record_date: record,
        factor,
        approved_on: None,
    }
}

#[test]
fn a_stock_event_is_pending_until_the_book_has_it() {
    let today = date!(2026 - 11 - 26);
    let asset = stock(book());
    let event = bonus(date!(2026 - 06 - 18), dec!(2.0));
    let pending = pending_stock_events(&asset, std::slice::from_ref(&event), today);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, StockEventKind::Bonus);
    assert_eq!(pending[0].quantity, dec!(120));
    assert_eq!(pending[0].factor, dec!(2));

    // Lançado no livro (na data de crédito): resolvido.
    let mut launched = book();
    launched.push(entry(Kind::Bonus, date!(2026 - 06 - 25), 9, dec!(2), None));
    assert!(pending_stock_events(&stock(launched), std::slice::from_ref(&event), today).is_empty());

    // Velho demais, no futuro, sem cota, de outro ISIN ou outro rótulo: nada.
    let mut other = event.clone();
    other.isin = "BRABCDACNOR3".into();
    let mut label = event.clone();
    label.label = "CIS RED CAP".into();
    for e in [
        bonus(date!(2024 - 06 - 18), dec!(10)),
        bonus(date!(2026 - 12 - 18), dec!(10)),
        bonus(date!(2025 - 12 - 18), dec!(10)),
        other,
        label,
    ] {
        assert!(pending_stock_events(&asset, &[e], today).is_empty());
    }
}

#[test]
fn the_opening_balance_already_has_older_events() {
    let today = date!(2026 - 11 - 26);
    let asset = stock(vec![entry(
        Kind::Opening,
        date!(2026 - 08 - 01),
        1,
        dec!(100),
        None,
    )]);
    let events = [
        bonus(date!(2026 - 06 - 18), dec!(10)),
        StockAnnouncement {
            label: "DESDOBRAMENTO".into(),
            ..bonus(date!(2026 - 09 - 15), dec!(100))
        },
    ];
    let pending = pending_stock_events(&asset, &events, today);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, StockEventKind::Split);
    let mut no_isin = asset;
    no_isin.isin = None;
    assert!(pending_stock_events(&no_isin, &events, today).is_empty());
}

// ------------------------------------------------------------ proptest --

fn arb_book() -> impl Strategy<Value = Vec<Entry>> {
    proptest::collection::vec((0i64..300, 1u32..50, any::<bool>()), 1..12).prop_map(|moves| {
        let start = date!(2026 - 01 - 01);
        let mut held = Decimal::ZERO;
        let mut entries = Vec::new();
        let mut sorted = moves;
        sorted.sort_by_key(|m| m.0);
        for (seq, (day, quantity, sell)) in sorted.into_iter().enumerate() {
            let quantity = Decimal::from(quantity);
            let sell = sell && held >= quantity;
            let kind = if sell { Kind::Sell } else { Kind::Buy };
            held += if sell { -quantity } else { quantity };
            entries.push(entry(
                kind,
                start + Duration::days(day),
                i64::try_from(seq).unwrap_or(0),
                quantity,
                Some(dec!(10)),
            ));
        }
        entries
    })
}

fn arb_announcements() -> impl Strategy<Value = Vec<CashAnnouncement>> {
    let labels = prop_oneof![
        Just("DIVIDENDO"),
        Just("JRS CAP PROPRIO"),
        Just("RENDIMENTO")
    ];
    proptest::collection::vec(
        (
            0i64..330,
            1u32..5000,
            labels,
            proptest::option::weighted(0.8, 1i64..60),
        ),
        0..10,
    )
    .prop_map(|items| {
        let start = date!(2026 - 01 - 01);
        items
            .into_iter()
            .map(|(day, cents, label, pay)| {
                let record = start + Duration::days(day);
                announcement(
                    "BRABCDACNPR1",
                    label,
                    record,
                    Decimal::new(i64::from(cents), 4),
                    pay.map(|p| record + Duration::days(p)),
                )
            })
            .collect()
    })
}

proptest! {
    #[test]
    fn a_round_applied_is_a_fixed_point(entries in arb_book(), list in arb_announcements()) {
        let asset = stock(entries.clone());
        let mut known = Vec::new();
        let mut next = 100u128;
        let first = plan(std::slice::from_ref(&asset), &list, &known, &HashSet::new());
        apply(&mut known, &first, &mut next);
        // Toda linha gravada tem a quantidade do livro na data com, positiva.
        for auto in &first.insert {
            let a = list
                .iter()
                .find(|a| (0..list.len()).any(|i| external_ref(a, i) == auto.external_ref))
                .unwrap();
            prop_assert_eq!(auto.quantity, quantity_on(&entries, a.record_date).unwrap());
            prop_assert!(auto.quantity > Decimal::ZERO);
            prop_assert!(a.pay_date.is_some());
            prop_assert_eq!(auto.amounts.net, auto.amounts.gross - auto.amounts.withholding);
        }
        // A segunda rodada não muda nada.
        let second = plan(std::slice::from_ref(&asset), &list, &known, &HashSet::new());
        prop_assert!(second.is_empty(), "{:?}", second);
    }

    #[test]
    fn a_manual_copy_of_each_automatic_replaces_it(entries in arb_book(), list in arb_announcements()) {
        let asset = stock(entries);
        let mut known = Vec::new();
        let mut next = 100u128;
        let first = plan(std::slice::from_ref(&asset), &list, &known, &HashSet::new());
        apply(&mut known, &first, &mut next);
        // A pessoa lança à mão cada um, igual: os automáticos saem todos.
        let typed: Vec<KnownIncome> = first
            .insert
            .iter()
            .enumerate()
            .map(|(i, auto)| {
                let mut k = known_from(auto, 10_000 + i as u128, IncomeSource::Manual);
                k.external_ref = None;
                k
            })
            .collect();
        known.extend(typed);
        let second = plan(std::slice::from_ref(&asset), &list, &known, &HashSet::new());
        prop_assert!(second.insert.is_empty());
        prop_assert_eq!(second.delete.len(), first.insert.len());
        prop_assert_eq!(second.covered, first.insert.len());
    }

    #[test]
    fn brazilian_decimals_round_trip(units in 0u64..10_000_000, places in 0u32..11) {
        let value = Decimal::new(i64::try_from(units).unwrap_or(0), places);
        let text = value.to_string().replace('.', ",");
        prop_assert_eq!(parse_br_decimal(&text), Some(value));
    }
}
