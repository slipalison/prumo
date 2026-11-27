use super::*;
use crate::domain::income::Entry;
use proptest::prelude::*;
use rust_decimal_macros::dec;
use time::macros::date;
use time::Date;

fn blank() -> CompanyFacts {
    CompanyFacts {
        profile: Profile::Operating,
        roe: None,
        profit_years: None,
        net_debt_to_ebitda: None,
        ebitda_margin: None,
        net_margin: None,
        net_debt_to_equity: None,
        operating_cash_average: None,
        revenue_cagr: None,
        earnings_cagr: None,
        judicial_recovery: None,
        per_share: None,
    }
}

fn analysis_asset(label: &str, class: AssetClass, value: Decimal) -> AnalysisAsset {
    AnalysisAsset {
        id: Uuid::new_v4(),
        label: label.to_owned(),
        asset_class: class,
        symbol: Some(label.to_owned()),
        value,
        sector: None,
        cost_basis: None,
    }
}

fn company(label: &str, value: Decimal, facts: CompanyFacts) -> QualityAsset {
    QualityAsset {
        id: Uuid::new_v4(),
        label: label.to_owned(),
        asset_class: AssetClass::BrStocks,
        value,
        facts: Facts::Company(facts),
    }
}

fn fund(
    label: &str,
    value: Decimal,
    pvp: Option<Decimal>,
    vacancy: Option<Decimal>,
) -> QualityAsset {
    QualityAsset {
        id: Uuid::new_v4(),
        label: label.to_owned(),
        asset_class: AssetClass::BrReits,
        value,
        facts: Facts::Fund(FundFacts {
            price_to_book: pvp,
            vacancy,
        }),
    }
}

#[test]
fn leverage_does_not_apply_to_a_holding_and_says_who_is_out() {
    let assets = [
        company(
            "IND3",
            dec!(100),
            CompanyFacts {
                net_debt_to_ebitda: Some(dec!(1)),
                operating_cash_average: Some(dec!(5)),
                ..blank()
            },
        ),
        company(
            "BANC4",
            dec!(300),
            CompanyFacts {
                profile: Profile::Bank,
                ..blank()
            },
        ),
        // A holding tem número (o EBITDA leva a equivalência), mas não entra.
        company(
            "HOLD4",
            dec!(500),
            CompanyFacts {
                profile: Profile::Holding,
                net_debt_to_ebitda: Some(dec!(9)),
                ebitda_margin: Some(dec!(2)),
                operating_cash_average: Some(dec!(3)),
                roe: Some(dec!(0.2)),
                ..blank()
            },
        ),
        company(
            "SEGU3",
            dec!(50),
            CompanyFacts {
                profile: Profile::Insurer,
                ..blank()
            },
        ),
    ];
    let found = weighted(&assets, Criterion::Leverage).unwrap();
    assert_eq!(found.covered, 1);
    assert_eq!(found.total, 4);
    assert_eq!(found.share, dec!(100));
    // Do maior valor para o menor.
    assert_eq!(
        found.not_applicable,
        vec![
            ("HOLD4".to_owned(), Profile::Holding),
            ("BANC4".to_owned(), Profile::Bank),
            ("SEGU3".to_owned(), Profile::Insurer),
        ]
    );
    assert!(weighted(&assets, Criterion::EbitdaMargin).is_none());
    // O caixa operacional vale para a holding; banco e seguradora não têm.
    let cash = weighted(&assets, Criterion::OperatingCash).unwrap();
    assert_eq!(cash.covered, 2);
    assert_eq!(
        cash.not_applicable,
        vec![
            ("BANC4".to_owned(), Profile::Bank),
            ("SEGU3".to_owned(), Profile::Insurer)
        ]
    );
    // ROE vale para todos.
    let roe = weighted(&assets, Criterion::Roe).unwrap();
    assert!(roe.not_applicable.is_empty());
    assert_eq!(roe.covered, 1);
    assert!(Profile::Bank.financial());
    assert!(!Profile::Holding.financial());
}

#[test]
fn roe_is_weighted_by_value_and_only_counts_who_has_the_number() {
    let assets = [
        company(
            "BOA3",
            dec!(600),
            CompanyFacts {
                roe: Some(dec!(0.15)),
                ..blank()
            },
        ),
        // Exatamente no corte passa.
        company(
            "LIMITE3",
            dec!(100),
            CompanyFacts {
                roe: Some(dec!(0.10)),
                ..blank()
            },
        ),
        company(
            "FRACA3",
            dec!(300),
            CompanyFacts {
                roe: Some(dec!(0.04)),
                ..blank()
            },
        ),
        // Sem número: fora da conta, não é reprovada.
        company("SEMDADO3", dec!(5000), blank()),
        // Fundo não é empresa.
        fund("FII11", dec!(9000), Some(dec!(1)), Some(dec!(0.05))),
    ];
    let found = weighted(&assets, Criterion::Roe).unwrap();
    // 700 de 1.000 passam.
    assert_eq!(found.share, dec!(70));
    assert_eq!((found.covered, found.total), (3, 4));
    assert_eq!(found.failing, ["FRACA3"]);
}

#[test]
fn profit_years_need_four_known_years_and_four_positive() {
    let years = |positive, known| CompanyFacts {
        profit_years: Some((positive, known)),
        ..blank()
    };
    let verdict_of = |facts| verdict(Criterion::ProfitYears, &Facts::Company(facts));
    assert_eq!(verdict_of(years(5, 5)), Some(true));
    assert_eq!(verdict_of(years(4, 5)), Some(true));
    assert_eq!(verdict_of(years(3, 5)), Some(false));
    assert_eq!(verdict_of(years(4, 4)), Some(true));
    // Três anos conhecidos não dizem "4 dos 5": sem dado.
    assert_eq!(verdict_of(years(3, 3)), None);
    assert_eq!(verdict_of(blank()), None);
}

#[test]
fn leverage_and_margin_do_not_apply_to_banks() {
    let bank = CompanyFacts {
        profile: Profile::Bank,
        net_debt_to_ebitda: Some(dec!(9)),
        ebitda_margin: Some(dec!(-1)),
        roe: Some(dec!(0.2)),
        ..blank()
    };
    assert_eq!(
        verdict(Criterion::Leverage, &Facts::Company(bank.clone())),
        None
    );
    assert_eq!(
        verdict(Criterion::EbitdaMargin, &Facts::Company(bank.clone())),
        None
    );
    // ROE vale para o banco.
    assert_eq!(verdict(Criterion::Roe, &Facts::Company(bank)), Some(true));

    let industrial = CompanyFacts {
        net_debt_to_ebitda: Some(dec!(3)),
        ebitda_margin: Some(dec!(0.0001)),
        ..blank()
    };
    // No limite passa; caixa líquido (razão negativa) também.
    assert_eq!(
        verdict(Criterion::Leverage, &Facts::Company(industrial.clone())),
        Some(true)
    );
    assert_eq!(
        verdict(Criterion::EbitdaMargin, &Facts::Company(industrial)),
        Some(true)
    );
    let heavy = CompanyFacts {
        net_debt_to_ebitda: Some(dec!(3.01)),
        ebitda_margin: Some(dec!(0)),
        ..blank()
    };
    assert_eq!(
        verdict(Criterion::Leverage, &Facts::Company(heavy.clone())),
        Some(false)
    );
    assert_eq!(
        verdict(Criterion::EbitdaMargin, &Facts::Company(heavy)),
        Some(false)
    );
    let net_cash = CompanyFacts {
        net_debt_to_ebitda: Some(dec!(-2)),
        ..blank()
    };
    assert_eq!(
        verdict(Criterion::Leverage, &Facts::Company(net_cash)),
        Some(true)
    );
}

#[test]
fn operating_cash_must_be_positive_on_average() {
    let cash = |v| CompanyFacts {
        operating_cash_average: v,
        ..blank()
    };
    assert_eq!(
        verdict(
            Criterion::OperatingCash,
            &Facts::Company(cash(Some(dec!(10))))
        ),
        Some(true)
    );
    assert_eq!(
        verdict(
            Criterion::OperatingCash,
            &Facts::Company(cash(Some(dec!(0))))
        ),
        Some(false)
    );
    assert_eq!(
        verdict(Criterion::OperatingCash, &Facts::Company(cash(None))),
        None
    );
}

#[test]
fn fund_rules_look_only_at_funds_and_vacancy_skips_paper_funds() {
    let assets = [
        fund("TIJOLO11", dec!(800), Some(dec!(1.05)), Some(dec!(0.10))),
        fund("CARO11", dec!(200), Some(dec!(1.30)), Some(dec!(0.40))),
        // Papel: sem imóvel, a vacância não se aplica; o P/VP vale.
        fund("PAPEL11", dec!(1000), Some(dec!(0.95)), None),
        company(
            "EMP3",
            dec!(7000),
            CompanyFacts {
                roe: Some(dec!(0.2)),
                ..blank()
            },
        ),
    ];
    let pvp = weighted(&assets, Criterion::FundPriceToBook).unwrap();
    // 1.800 de 2.000 com P/VP até 1,10.
    assert_eq!(pvp.share, dec!(90));
    assert_eq!((pvp.covered, pvp.total), (3, 3));
    assert_eq!(pvp.failing, ["CARO11"]);
    let vacancy = weighted(&assets, Criterion::FundVacancy).unwrap();
    assert_eq!(vacancy.share, dec!(80));
    assert_eq!((vacancy.covered, vacancy.total), (2, 3));
    // Só fundo de papel: sem dado.
    assert_eq!(
        weighted(
            &[fund("P11", dec!(5), Some(dec!(1)), None)],
            Criterion::FundVacancy
        ),
        None
    );
    // Sem empresa: as regras de empresa não têm dado.
    assert_eq!(weighted(&assets[..3], Criterion::Roe), None);
}

#[test]
fn judicial_recovery_names_the_companies_and_needs_a_registry() {
    let assets = [
        company(
            "OK3",
            dec!(1),
            CompanyFacts {
                judicial_recovery: Some(false),
                ..blank()
            },
        ),
        company(
            "RJ3",
            dec!(1),
            CompanyFacts {
                judicial_recovery: Some(true),
                ..blank()
            },
        ),
        company("SEMCAD3", dec!(1), blank()),
    ];
    let found = judicial_recovery(&assets).unwrap();
    assert_eq!(found.failing, ["RJ3"]);
    assert_eq!((found.covered, found.total), (2, 3));
    assert_eq!(judicial_recovery(&assets[2..]), None);
    assert_eq!(judicial_recovery(&[]), None);
}

fn paid(asset: &AnalysisAsset, year: i32, month: u8, net: Decimal) -> Entry {
    Entry {
        pay_date: Date::from_calendar_date(year, time::Month::try_from(month).unwrap(), 10)
            .unwrap(),
        net,
        payer_key: asset.id.to_string(),
        payer_label: asset.label.clone(),
    }
}

#[test]
fn attention_points_are_short_factual_and_only_for_who_has_them() {
    let weak = analysis_asset("FRACA3", AssetClass::BrStocks, dec!(500));
    let solid = analysis_asset("SOLIDA3", AssetClass::BrStocks, dec!(900));
    let new = analysis_asset("NOVA3", AssetClass::BrStocks, dec!(100));
    let quality = [
        QualityAsset {
            id: weak.id,
            label: weak.label.clone(),
            asset_class: weak.asset_class,
            value: weak.value,
            facts: Facts::Company(CompanyFacts {
                roe: Some(dec!(0.0412)),
                net_margin: Some(dec!(0.0833)),
                net_debt_to_equity: Some(dec!(1.5)),
                revenue_cagr: Some(dec!(0.0234)),
                earnings_cagr: Some(dec!(-0.03)),
                judicial_recovery: Some(true),
                ..blank()
            }),
        },
        QualityAsset {
            id: solid.id,
            label: solid.label.clone(),
            asset_class: solid.asset_class,
            value: solid.value,
            facts: Facts::Company(CompanyFacts {
                roe: Some(dec!(0.10)),
                net_margin: Some(dec!(0.10)),
                net_debt_to_equity: Some(dec!(1)),
                revenue_cagr: Some(dec!(0.05)),
                earnings_cagr: Some(dec!(0.12)),
                judicial_recovery: Some(false),
                ..blank()
            }),
        },
    ];
    let income = IncomeHistory {
        today: date!(2026 - 11 - 25),
        entries: Vec::new(),
    };
    let attention = attention(&[weak.clone(), solid, new], &quality, &income);
    // Só a FRACA3 tem ponto; os limites exatos (ROE 10%, margem 10%, dívida 1x, 5%) não são ponto.
    assert_eq!(attention.items.len(), 1);
    let item = &attention.items[0];
    assert_eq!(item.id, weak.id);
    assert_eq!(
        item.points,
        [
            Point::JudicialRecovery,
            Point::LowRoe {
                percent: dec!(4.12)
            },
            Point::LowNetMargin {
                percent: dec!(8.33)
            },
            Point::DebtAboveEquity { times: dec!(1.5) },
            Point::LowRevenueGrowth {
                percent: dec!(2.34)
            },
            Point::LowEarningsGrowth { percent: dec!(-3) },
        ]
    );
    assert_eq!((attention.assessed, attention.without_data), (2, 1));
}

#[test]
fn an_income_drop_needs_two_years_of_history_and_a_real_drop() {
    let payer = analysis_asset("PAGA11", AssetClass::BrReits, dec!(1000));
    let steady = analysis_asset("FIRME11", AssetClass::BrReits, dec!(900));
    let mut entries = Vec::new();
    // dez/2024 a nov/2025: 10 por mês; dez/2025 a nov/2026: 6 por mês (-40%).
    for index in 0..24 {
        let (year, month) = (
            2024 + (11 + index) / 12,
            u8::try_from((11 + index) % 12 + 1).unwrap(),
        );
        let net = if index < 12 { dec!(10) } else { dec!(6) };
        entries.push(paid(&payer, year, month, net));
        entries.push(paid(&steady, year, month, dec!(5)));
    }
    let income = IncomeHistory {
        today: date!(2026 - 11 - 25),
        entries,
    };
    let found = attention(&[payer.clone(), steady], &[], &income);
    assert_eq!(found.items.len(), 1);
    assert_eq!(
        found.items[0].points,
        [Point::IncomeDrop { percent: dec!(40) }]
    );
    assert_eq!((found.assessed, found.without_data), (0, 2));

    // Só 12 meses de histórico: sem janela anterior, sem ponto.
    let short = IncomeHistory {
        today: date!(2026 - 11 - 25),
        entries: income
            .entries
            .iter()
            .filter(|e| e.pay_date >= date!(2025 - 12 - 01))
            .cloned()
            .collect(),
    };
    assert!(attention(&[payer], &[], &short).items.is_empty());
}

#[test]
fn a_small_income_drop_is_not_a_point() {
    let payer = analysis_asset("PAGA11", AssetClass::BrReits, dec!(1000));
    let mut entries = Vec::new();
    for index in 0..24 {
        let (year, month) = (
            2024 + (11 + index) / 12,
            u8::try_from((11 + index) % 12 + 1).unwrap(),
        );
        entries.push(paid(
            &payer,
            year,
            month,
            if index < 12 { dec!(10) } else { dec!(9.5) },
        ));
    }
    let income = IncomeHistory {
        today: date!(2026 - 11 - 25),
        entries,
    };
    assert!(attention(&[payer], &[], &income).items.is_empty());
}

#[test]
fn unsupported_and_valueless_assets_stay_out_of_the_quality_list() {
    use crate::domain::fundamentals::Unsupported;
    use std::collections::HashMap;
    let listed = analysis_asset("ETF11", AssetClass::BrStocks, dec!(100));
    let empty = analysis_asset("ZERO3", AssetClass::BrStocks, dec!(0));
    let map = HashMap::from([
        (
            "ETF11".to_owned(),
            AssetFundamentals::Unsupported(Unsupported::ExchangeTradedProduct),
        ),
        (
            "ZERO3".to_owned(),
            AssetFundamentals::Unsupported(Unsupported::Pending),
        ),
    ]);
    assert!(quality_assets(&[listed, empty], &map).is_empty());
}

proptest! {
    /// A fatia é sempre de 0 a 100, e reprovados + aprovados cobrem o que tem dado.
    #[test]
    fn the_weighted_share_is_bounded_and_names_exactly_who_failed(
        rows in proptest::collection::vec((1i64..1_000_000, proptest::option::of(-20i64..40)), 1..12)
    ) {
        let assets: Vec<QualityAsset> = rows
            .iter()
            .enumerate()
            .map(|(i, (cents, roe))| company(
                &format!("A{i}"),
                Decimal::new(*cents, 2),
                CompanyFacts { roe: roe.map(|r| Decimal::new(r, 2)), ..blank() },
            ))
            .collect();
        let expected_failing = rows.iter().filter(|(_, r)| r.is_some_and(|r| r < 10)).count();
        match weighted(&assets, Criterion::Roe) {
            None => prop_assert!(rows.iter().all(|(_, r)| r.is_none())),
            Some(found) => {
                prop_assert!(found.share >= Decimal::ZERO && found.share <= Decimal::ONE_HUNDRED);
                prop_assert_eq!(found.failing.len(), expected_failing);
                prop_assert_eq!(usize::try_from(found.covered).unwrap(), rows.iter().filter(|(_, r)| r.is_some()).count());
                prop_assert_eq!(found.share == Decimal::ONE_HUNDRED, expected_failing == 0);
            }
        }
    }

    /// Quem não tem dado nunca muda o resultado, qualquer que seja o valor dele.
    #[test]
    fn assets_without_data_do_not_move_the_share(extra in 1i64..10_000_000) {
        let base = [
            company("A3", dec!(100), CompanyFacts { roe: Some(dec!(0.2)), ..blank() }),
            company("B3", dec!(100), CompanyFacts { roe: Some(dec!(0.01)), ..blank() }),
        ];
        let mut with = base.to_vec();
        with.push(company("X3", Decimal::new(extra, 2), blank()));
        prop_assert_eq!(
            weighted(&base, Criterion::Roe).map(|w| w.share),
            weighted(&with, Criterion::Roe).map(|w| w.share)
        );
    }
}
