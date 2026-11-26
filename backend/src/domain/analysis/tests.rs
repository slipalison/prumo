use super::*;
use crate::domain::income::YearMonth;
use proptest::prelude::*;
use rust_decimal_macros::dec;
use time::macros::date;
use time::Date;

fn asset(label: &str, class: AssetClass, value: Decimal) -> AnalysisAsset {
    AnalysisAsset {
        id: Uuid::new_v4(),
        label: label.to_owned(),
        asset_class: class,
        symbol: None,
        value,
        sector: None,
        cost_basis: None,
    }
}

fn with_sector(mut asset: AnalysisAsset, sector: &str) -> AnalysisAsset {
    asset.sector = Some(sector.to_owned());
    asset
}

fn with_cost(mut asset: AnalysisAsset, cost: Decimal) -> AnalysisAsset {
    asset.cost_basis = Some(cost);
    asset
}

fn today() -> Date {
    date!(2026 - 11 - 25)
}

fn no_income() -> IncomeHistory {
    IncomeHistory {
        today: today(),
        entries: Vec::new(),
    }
}

/// `count` pagamentos mensais seguidos, o primeiro em `first` (ano, mês).
fn monthly(
    asset: &AnalysisAsset,
    first: (i32, u8),
    count: i32,
    net: Decimal,
) -> Vec<income::Entry> {
    let start = YearMonth {
        year: first.0,
        month: first.1,
    };
    (0..count)
        .map(|i| {
            let month = start.shift(i);
            income::Entry {
                pay_date: Date::from_calendar_date(
                    month.year,
                    time::Month::try_from(month.month).unwrap(),
                    10,
                )
                .unwrap(),
                net,
                payer_key: asset.id.to_string(),
                payer_label: asset.label.clone(),
            }
        })
        .collect()
}

fn history(entries: Vec<income::Entry>) -> IncomeHistory {
    IncomeHistory {
        today: today(),
        entries,
    }
}

/// Só `rule` ligada, com o limite dado (ou o de fábrica).
fn only(rule: RuleId, threshold: Option<Decimal>) -> AnalysisConfig {
    let mut config = AnalysisConfig::default();
    for other in RuleId::ALL {
        config.set(
            other,
            RuleSetting {
                enabled: other == rule,
                threshold: other.default_threshold(),
            },
        );
    }
    if let Some(value) = threshold {
        config.set(
            rule,
            RuleSetting {
                enabled: true,
                threshold: Some(value),
            },
        );
    }
    config
}

fn run(
    rule: RuleId,
    threshold: Option<Decimal>,
    assets: &[AnalysisAsset],
    income: &IncomeHistory,
    returns: &ReturnsFacts,
) -> RuleResult {
    let results = evaluate(
        &only(rule, threshold),
        &Input {
            assets,
            targets: Targets::default(),
            income,
            returns,
            quality: &[],
        },
    );
    results.into_iter().find(|r| r.rule == rule).unwrap()
}

fn run_simple(rule: RuleId, threshold: Option<Decimal>, assets: &[AnalysisAsset]) -> RuleResult {
    run(
        rule,
        threshold,
        assets,
        &no_income(),
        &ReturnsFacts::default(),
    )
}

// ------------------------------------------------------------ catálogo --

#[test]
fn every_rule_has_a_unique_id_a_dimension_and_round_trips() {
    let mut seen = BTreeSet::new();
    for rule in RuleId::ALL {
        assert!(seen.insert(rule.as_str()), "{rule:?} repetida");
        assert_eq!(RuleId::parse(rule.as_str()), Some(rule));
        // O nome na rede é o mesmo do JSON.
        assert_eq!(
            serde_json::to_value(rule).unwrap(),
            serde_json::Value::String(rule.as_str().to_owned())
        );
    }
    assert_eq!(RuleId::ALL.len(), RULE_COUNT);
    assert_eq!(RuleId::parse("nota_final"), None);
    let count = |d: Dimension| RuleId::ALL.iter().filter(|r| r.dimension() == d).count();
    assert_eq!(count(Dimension::Diversification), 10);
    assert_eq!(count(Dimension::Dividends), 6);
    assert_eq!(count(Dimension::Returns), 9);
    assert_eq!(count(Dimension::Quality), 8);
}

#[test]
fn the_seven_original_rules_map_both_ways_into_the_right_dimension() {
    let legacy: Vec<RuleId> = RuleId::ALL
        .into_iter()
        .filter(|r| r.legacy().is_some())
        .collect();
    assert_eq!(legacy.len(), health::RuleId::ALL.len());
    for original in health::RuleId::ALL {
        let rule = RuleId::from_legacy(original);
        assert_eq!(rule.legacy(), Some(original));
        // O nome e o limite de fábrica não mudam.
        assert_eq!(rule.as_str(), original.as_str());
        assert_eq!(rule.default_threshold(), original.default_threshold());
    }
    assert_eq!(RuleId::MaxAsset.dimension(), Dimension::Diversification);
    assert_eq!(RuleId::EmptyClass.dimension(), Dimension::Diversification);
    assert_eq!(RuleId::IncomeEveryMonth.dimension(), Dimension::Dividends);
    assert_eq!(RuleId::MaxPayer.dimension(), Dimension::Dividends);
}

#[test]
fn a_rule_is_editable_exactly_when_it_has_a_default_limit() {
    for rule in RuleId::ALL {
        assert_eq!(rule.editable(), rule.default_threshold().is_some());
        if rule.editable() {
            // O limite de fábrica passa na própria validação.
            assert_eq!(
                rule.validate_threshold(rule.default_threshold().unwrap()),
                Ok(rule.default_threshold().unwrap().normalize()),
                "{rule:?}"
            );
            assert!(rule.direction().is_some(), "{rule:?}");
        } else {
            assert!(rule.validate_threshold(dec!(5)).is_err());
        }
    }
}

#[test]
fn thresholds_are_validated_by_unit() {
    // Percentual: de 0 (exclusive) a 100, até duas casas.
    assert_eq!(
        RuleId::MinAbroad.validate_threshold(dec!(10.50)),
        Ok(dec!(10.5))
    );
    assert!(RuleId::MinAbroad.validate_threshold(dec!(0)).is_err());
    assert!(RuleId::MinAbroad.validate_threshold(dec!(100.5)).is_err());
    assert!(RuleId::MaxVolatility
        .validate_threshold(dec!(1.234))
        .is_err());
    // Contagem: inteiro de 1 até o máximo de cada regra.
    assert_eq!(RuleId::MinClasses.validate_threshold(dec!(7)), Ok(dec!(7)));
    assert!(RuleId::MinClasses.validate_threshold(dec!(8)).is_err());
    assert!(RuleId::MinSectors.validate_threshold(dec!(30)).is_ok());
    assert!(RuleId::MinSectors.validate_threshold(dec!(31)).is_err());
    assert!(RuleId::MinPortfolioAssets
        .validate_threshold(dec!(2.5))
        .is_err());
    assert!(RuleId::MinPortfolioAssets
        .validate_threshold(dec!(100))
        .is_ok());
    // As originais seguem a validação delas.
    assert!(RuleId::IncomeEveryMonth
        .validate_threshold(dec!(13))
        .is_err());
    assert!(RuleId::EmptyClass.validate_threshold(dec!(1)).is_err());
}

#[test]
fn the_original_health_config_is_taken_from_the_shared_settings() {
    let mut config = AnalysisConfig::default();
    config.set(
        RuleId::MaxAsset,
        RuleSetting {
            enabled: false,
            threshold: Some(dec!(12.5)),
        },
    );
    // Uma regra nova mexida não vaza para a configuração da Saúde.
    config.set(
        RuleId::MinAbroad,
        RuleSetting {
            enabled: false,
            threshold: Some(dec!(50)),
        },
    );
    let health = config.health();
    assert!(!health.get(health::RuleId::MaxAsset).enabled);
    assert_eq!(
        health.get(health::RuleId::MaxAsset).threshold,
        Some(dec!(12.5))
    );
    assert!(health.get(health::RuleId::TopFive).enabled);
}

// ------------------------------------------------------- diversificação --

#[test]
fn original_rules_report_the_current_number_even_when_ok() {
    let big = asset("AAAA3", AssetClass::BrStocks, dec!(600));
    let small = asset("BBBB3", AssetClass::BrStocks, dec!(400));
    let result = run_simple(
        RuleId::MaxAsset,
        Some(dec!(70)),
        &[big.clone(), small.clone()],
    );
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(dec!(60)));
    assert_eq!(result.limit, Some(dec!(70)));
    assert_eq!(result.detail, Detail::None);

    let result = run_simple(RuleId::MaxAsset, Some(dec!(50)), &[big, small]);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(60)));
    let Detail::Findings(found) = &result.detail else {
        panic!("{:?}", result.detail);
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].current, dec!(60));
}

#[test]
fn an_original_rule_without_anything_to_measure_is_no_data() {
    let result = run_simple(RuleId::MaxAsset, None, &[]);
    assert_eq!(result.status, Status::NoData);
    assert_eq!(result.current, None);
    assert_eq!(result.limit, None);
}

#[test]
fn the_minimum_of_assets_counts_assets_with_value() {
    let assets = [
        asset("A", AssetClass::BrStocks, dec!(10)),
        asset("B", AssetClass::BrStocks, dec!(10)),
        // Sem valor não diversifica nada.
        asset("C", AssetClass::BrStocks, dec!(0)),
    ];
    let result = run_simple(RuleId::MinPortfolioAssets, Some(dec!(3)), &assets);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(2)));
    let result = run_simple(RuleId::MinPortfolioAssets, Some(dec!(2)), &assets);
    assert_eq!(result.status, Status::Ok);
    assert_eq!(
        run_simple(RuleId::MinPortfolioAssets, None, &[]).status,
        Status::NoData
    );
}

#[test]
fn the_minimum_of_classes_counts_classes_with_money() {
    let assets = [
        asset("A", AssetClass::BrStocks, dec!(10)),
        asset("B", AssetClass::BrStocks, dec!(10)),
        asset("C", AssetClass::FixedIncome, dec!(10)),
    ];
    let result = run_simple(RuleId::MinClasses, Some(dec!(3)), &assets);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(2)));
    assert_eq!(
        result.detail,
        Detail::Classes(vec![AssetClass::BrStocks, AssetClass::FixedIncome])
    );
    assert_eq!(
        run_simple(RuleId::MinClasses, Some(dec!(2)), &assets).status,
        Status::Ok
    );
}

#[test]
fn the_minimum_of_sectors_looks_only_at_known_stock_sectors() {
    let assets = [
        with_sector(asset("A", AssetClass::BrStocks, dec!(10)), "Financeiro"),
        with_sector(asset("B", AssetClass::BrStocks, dec!(10)), "Financeiro"),
        with_sector(asset("C", AssetClass::BrStocks, dec!(10)), "Energia"),
        // FII e papel sem setor não contam.
        with_sector(asset("D", AssetClass::BrReits, dec!(10)), "Tijolo"),
        asset("E", AssetClass::BrStocks, dec!(10)),
    ];
    let result = run_simple(RuleId::MinSectors, Some(dec!(3)), &assets);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(2)));
    assert_eq!(
        result.detail,
        Detail::Sectors(vec!["Energia".to_owned(), "Financeiro".to_owned()])
    );
    // Sem ação com setor: sem dado, não falha.
    let only_reit = [with_sector(
        asset("D", AssetClass::BrReits, dec!(10)),
        "Tijolo",
    )];
    assert_eq!(
        run_simple(RuleId::MinSectors, Some(dec!(3)), &only_reit).status,
        Status::NoData
    );
    let unknown = [asset("E", AssetClass::BrStocks, dec!(10))];
    assert_eq!(
        run_simple(RuleId::MinSectors, Some(dec!(3)), &unknown).status,
        Status::NoData
    );
}

#[test]
fn abroad_share_includes_stocks_reits_and_bonds_from_abroad() {
    let assets = [
        asset("AAPL", AssetClass::IntlStocks, dec!(50)),
        asset("O", AssetClass::UsReits, dec!(30)),
        asset("TBOND", AssetClass::IntlFixedIncome, dec!(20)),
        asset("BTC", AssetClass::Crypto, dec!(100)),
        asset("WEGE3", AssetClass::BrStocks, dec!(800)),
    ];
    let result = run_simple(RuleId::MinAbroad, Some(dec!(10)), &assets);
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(dec!(10)));
    // Exatamente no limite está tudo bem.
    assert_eq!(result.needed_contribution, None);
    let result = run_simple(RuleId::MinAbroad, Some(dec!(20)), &assets);
    assert_eq!(result.status, Status::Warning);
    // (20·1000 − 100·100) / 80 = 125.
    assert_eq!(result.needed_contribution, Some(dec!(125)));
    assert_eq!(
        run_simple(RuleId::MinAbroad, None, &[]).status,
        Status::NoData
    );
}

#[test]
fn the_low_risk_share_is_the_fixed_income_share() {
    let assets = [
        asset("CDB", AssetClass::FixedIncome, dec!(50)),
        asset("WEGE3", AssetClass::BrStocks, dec!(950)),
    ];
    let result = run_simple(RuleId::MinLowRisk, Some(dec!(10)), &assets);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(5)));
    // (10·1000 − 100·50) / 90 = 55,5555… => 55,56 (para cima).
    assert_eq!(result.needed_contribution, Some(dec!(55.56)));
}

// ------------------------------------------------------------ dividendos --

/// Duas posições que pagam todo mês desde dez/2024 (24 meses até nov/2026).
fn payers() -> (AnalysisAsset, AnalysisAsset) {
    (
        asset("AAAA3", AssetClass::BrStocks, dec!(1000)),
        with_sector(asset("BBBB11", AssetClass::BrReits, dec!(1000)), "Tijolo"),
    )
}

#[test]
fn the_portfolio_yield_is_twelve_months_of_income_over_the_payers_value() {
    let (a, b) = payers();
    let mut entries = monthly(&a, (2025, 12), 12, dec!(10));
    entries.extend(monthly(&b, (2025, 12), 12, dec!(20)));
    let income = history(entries);
    let assets = [a, b, asset("CDB", AssetClass::FixedIncome, dec!(5000))];
    // 360 recebidos sobre 2.000 de ações e FIIs (a renda fixa fica fora) = 18%.
    let result = run(
        RuleId::MinYield,
        Some(dec!(5)),
        &assets,
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(dec!(18)));
    assert_eq!(
        result.detail,
        Detail::Yield {
            income_12m: dec!(360),
            base_value: dec!(2000)
        }
    );
    let result = run(
        RuleId::MinYield,
        Some(dec!(20)),
        &assets,
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Warning);
}

#[test]
fn less_than_a_year_of_income_is_no_data_for_the_yield_rules() {
    let (a, b) = payers();
    // Proventos de jan a out/2026: o histórico cobre 11 meses de calendário
    // (de janeiro até o mês de hoje), um a menos que a janela de 12 meses.
    let income = history(monthly(&a, (2026, 1), 10, dec!(10)));
    let assets = [a, b];
    for rule in [
        RuleId::MinYield,
        RuleId::MinPayingShare,
        RuleId::IncomeGrowth,
    ] {
        let result = run(rule, None, &assets, &income, &ReturnsFacts::default());
        assert_eq!(result.status, Status::NoData, "{rule:?}");
    }
    // Sem nenhum provento também.
    let result = run(
        RuleId::MinYield,
        None,
        &assets,
        &no_income(),
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::NoData);
}

#[test]
fn the_paying_share_names_who_paid_nothing() {
    let (a, b) = payers();
    let silent = asset("CCCC3", AssetClass::BrStocks, dec!(500));
    let income = history(monthly(&a, (2025, 12), 12, dec!(10)));
    let assets = [a, b, silent];
    // Só 1 de 3 pagou: 33,33%.
    let result = run(
        RuleId::MinPayingShare,
        Some(dec!(70)),
        &assets,
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(33.33)));
    assert_eq!(
        result.detail,
        Detail::Paying {
            paying: 1,
            total: 3,
            silent: vec!["BBBB11".to_owned(), "CCCC3".to_owned()]
        }
    );
    let result = run(
        RuleId::MinPayingShare,
        Some(dec!(33.33)),
        &assets,
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Ok);
}

#[test]
fn only_variable_income_that_pays_counts_in_the_paying_share() {
    let (a, _) = payers();
    let income = history(monthly(&a, (2025, 12), 12, dec!(10)));
    // Renda fixa e cripto não pagam provento: ficam de fora do denominador.
    let assets = [
        a,
        asset("CDB", AssetClass::FixedIncome, dec!(100)),
        asset("BTC", AssetClass::Crypto, dec!(100)),
    ];
    let result = run(
        RuleId::MinPayingShare,
        Some(dec!(100)),
        &assets,
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(dec!(100)));
}

#[test]
fn income_growth_compares_the_twelve_months_with_the_twelve_before() {
    let (a, b) = payers();
    // Dez/2024 a nov/2025: 5 por mês; dez/2025 a nov/2026: 10 por mês.
    let mut entries = monthly(&a, (2024, 12), 12, dec!(5));
    entries.extend(monthly(&a, (2025, 12), 12, dec!(10)));
    let income = history(entries);
    let assets = [a.clone(), b.clone()];
    let result = run(
        RuleId::IncomeGrowth,
        None,
        &assets,
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(dec!(100)));
    assert_eq!(
        result.detail,
        Detail::IncomeGrowth {
            current_12m: dec!(120),
            previous_12m: dec!(60)
        }
    );

    // Renda que caiu: aviso.
    let mut entries = monthly(&a, (2024, 12), 12, dec!(10));
    entries.extend(monthly(&a, (2025, 12), 12, dec!(5)));
    let result = run(
        RuleId::IncomeGrowth,
        None,
        &assets,
        &history(entries),
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(-50)));

    // Igual não é "maior".
    let mut entries = monthly(&a, (2024, 12), 12, dec!(10));
    entries.extend(monthly(&a, (2025, 12), 12, dec!(10)));
    let result = run(
        RuleId::IncomeGrowth,
        None,
        &assets,
        &history(entries),
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(Decimal::ZERO));
}

#[test]
fn income_growth_needs_two_years_of_history() {
    let (a, b) = payers();
    // Só os 12 meses de agora.
    let income = history(monthly(&a, (2025, 12), 12, dec!(10)));
    let result = run(
        RuleId::IncomeGrowth,
        None,
        &[a, b],
        &income,
        &ReturnsFacts::default(),
    );
    assert_eq!(result.status, Status::NoData);
}

#[test]
fn the_brick_reit_rule_needs_a_reit_with_a_known_type() {
    let brick = with_sector(asset("KNRI11", AssetClass::BrReits, dec!(100)), "Tijolo");
    let paper = with_sector(asset("KNCR11", AssetClass::BrReits, dec!(100)), "Papel");
    let unknown = asset("XXXX11", AssetClass::BrReits, dec!(100));

    let result = run_simple(RuleId::BrickReit, None, &[brick.clone(), paper.clone()]);
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(dec!(1)));
    assert_eq!(result.detail, Detail::Brick { reits: 2, brick: 1 });

    let result = run_simple(RuleId::BrickReit, None, &[paper.clone(), unknown.clone()]);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(Decimal::ZERO));

    // Sem FII, ou sem tipo conhecido: nada a medir.
    assert_eq!(
        run_simple(
            RuleId::BrickReit,
            None,
            &[asset("A", AssetClass::BrStocks, dec!(1))]
        )
        .status,
        Status::NoData
    );
    assert_eq!(
        run_simple(RuleId::BrickReit, None, &[unknown]).status,
        Status::NoData
    );
    // O texto bate com o que a fonte da CVM grava.
    assert_eq!(BRICK_SECTOR, crate::sectors::cvm::BRICK);
}

#[test]
fn the_income_rules_that_already_existed_live_in_the_dividends_dimension() {
    let (a, b) = payers();
    let income = history(monthly(&a, (2025, 5), 4, dec!(10)));
    let results = evaluate(
        &AnalysisConfig::default(),
        &Input {
            assets: &[a, b],
            targets: Targets::default(),
            income: &income,
            returns: &ReturnsFacts::default(),
            quality: &[],
        },
    );
    let every = results
        .iter()
        .find(|r| r.rule == RuleId::IncomeEveryMonth)
        .unwrap();
    assert_eq!(every.status, Status::Warning);
    assert_eq!(every.rule.dimension(), Dimension::Dividends);
    assert!(matches!(&every.detail, Detail::Findings(f) if f.len() == 1));
}

// ----------------------------------------------------------- rentabilidade --

fn comparison(portfolio: Decimal, benchmark: Decimal) -> Comparison {
    Comparison {
        from: date!(2025 - 11 - 25),
        to: today(),
        portfolio,
        benchmark,
    }
}

#[test]
fn beating_an_index_compares_the_rounded_returns() {
    let facts = ReturnsFacts {
        comparisons: vec![
            (
                Benchmark::Cdi,
                Horizon::OneYear,
                comparison(dec!(0.12), dec!(0.10)),
            ),
            (
                Benchmark::Ibov,
                Horizon::OneYear,
                comparison(dec!(0.05), dec!(0.10)),
            ),
            // Diferença abaixo de um centésimo de ponto: empata no arredondamento.
            (
                Benchmark::Ipca,
                Horizon::OneYear,
                comparison(dec!(0.04996), dec!(0.05)),
            ),
        ],
        ..ReturnsFacts::default()
    };
    let cdi = run(RuleId::BeatCdi12m, None, &[], &no_income(), &facts);
    assert_eq!(cdi.status, Status::Ok);
    assert_eq!(cdi.current, Some(dec!(12)));
    assert_eq!(cdi.limit, Some(dec!(10)));
    assert!(matches!(cdi.detail, Detail::Benchmark(_)));
    let ibov = run(RuleId::BeatIbov12m, None, &[], &no_income(), &facts);
    assert_eq!(ibov.status, Status::Warning);
    let ipca = run(RuleId::BeatIpca12m, None, &[], &no_income(), &facts);
    assert_eq!(ipca.status, Status::Ok);
}

#[test]
fn a_missing_comparison_is_no_data_never_a_warning() {
    let facts = ReturnsFacts::default();
    for rule in [
        RuleId::BeatCdi12m,
        RuleId::BeatCdi5y,
        RuleId::BeatIbov12m,
        RuleId::BeatIbov5y,
        RuleId::BeatIpca12m,
        RuleId::BeatIpca5y,
        RuleId::MinPositiveYears,
        RuleId::MaxVolatility,
    ] {
        let result = run(rule, None, &[], &no_income(), &facts);
        assert_eq!(result.status, Status::NoData, "{rule:?}");
        assert_eq!(result.current, None);
    }
}

#[test]
fn positive_years_and_volatility_follow_their_limits() {
    let facts = ReturnsFacts {
        positive_years: Some(YearsFact {
            positive: 2,
            total: 3,
        }),
        volatility_percent: Some(dec!(18.5)),
        ..ReturnsFacts::default()
    };
    let years = run(
        RuleId::MinPositiveYears,
        Some(dec!(70)),
        &[],
        &no_income(),
        &facts,
    );
    assert_eq!(years.status, Status::Warning);
    assert_eq!(years.current, Some(dec!(66.67)));
    assert_eq!(
        years.detail,
        Detail::Years(YearsFact {
            positive: 2,
            total: 3
        })
    );
    let years = run(
        RuleId::MinPositiveYears,
        Some(dec!(66.67)),
        &[],
        &no_income(),
        &facts,
    );
    assert_eq!(years.status, Status::Ok);

    let vol = run(
        RuleId::MaxVolatility,
        Some(dec!(20)),
        &[],
        &no_income(),
        &facts,
    );
    assert_eq!(vol.status, Status::Ok);
    let vol = run(
        RuleId::MaxVolatility,
        Some(dec!(18.5)),
        &[],
        &no_income(),
        &facts,
    );
    assert_eq!(vol.status, Status::Ok);
    let vol = run(
        RuleId::MaxVolatility,
        Some(dec!(18)),
        &[],
        &no_income(),
        &facts,
    );
    assert_eq!(vol.status, Status::Warning);
}

#[test]
fn the_loss_rule_compares_value_with_the_ledger_cost() {
    let assets = [
        with_cost(asset("PERDE", AssetClass::BrStocks, dec!(600)), dec!(1000)),
        with_cost(asset("GANHA", AssetClass::BrStocks, dec!(1500)), dec!(1000)),
        with_cost(asset("LEVE", AssetClass::BrStocks, dec!(900)), dec!(1000)),
        // Sem custo conhecido: fora.
        asset("SEMCUSTO", AssetClass::BrStocks, dec!(10)),
    ];
    let result = run_simple(RuleId::MaxLoss, Some(dec!(30)), &assets);
    assert_eq!(result.status, Status::Warning);
    assert_eq!(result.current, Some(dec!(40)));
    let Detail::Losses(losses) = &result.detail else {
        panic!("{:?}", result.detail);
    };
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].label, "PERDE");
    assert_eq!(losses[0].loss_percent, dec!(40));
    // No limite exato está tudo bem.
    assert_eq!(
        run_simple(RuleId::MaxLoss, Some(dec!(40)), &assets).status,
        Status::Ok
    );
    // Só ativo sem custo: sem dado.
    assert_eq!(
        run_simple(
            RuleId::MaxLoss,
            None,
            &[asset("X", AssetClass::BrStocks, dec!(10))]
        )
        .status,
        Status::NoData
    );
    // Ninguém com perda: a maior perda é zero.
    let rising = [with_cost(
        asset("G", AssetClass::BrStocks, dec!(11)),
        dec!(10),
    )];
    let result = run_simple(RuleId::MaxLoss, None, &rising);
    assert_eq!(result.status, Status::Ok);
    assert_eq!(result.current, Some(Decimal::ZERO));
}

// ---------------------------------------------------------------- pontuação --

fn result(status: Status) -> RuleResult {
    RuleResult {
        rule: RuleId::MaxAsset,
        setting: RuleSetting {
            enabled: status != Status::Off,
            threshold: None,
        },
        status,
        current: None,
        limit: None,
        needed_contribution: None,
        detail: Detail::None,
    }
}

#[test]
fn the_score_is_the_share_of_enabled_rules_with_data_that_are_ok() {
    let results = [
        result(Status::Ok),
        result(Status::Ok),
        result(Status::Warning),
        result(Status::NoData),
        result(Status::Off),
    ];
    let score = score(&results);
    assert_eq!(
        score,
        Score {
            ok: 2,
            measured: 3,
            no_data: 1,
            percent: Some(dec!(66.7))
        }
    );
    assert_eq!(attention_count(&results), 1);
}

#[test]
fn no_data_is_not_a_failure_in_the_score() {
    // Só regras sem dado: não há pontuação, e a nota não cai a zero.
    let results = [result(Status::NoData), result(Status::NoData)];
    let score = score(&results);
    assert_eq!(score.percent, None);
    assert_eq!(score.measured, 0);
    assert_eq!(attention_count(&results), 0);
    // Tudo ok é 100%.
    assert_eq!(super::score(&[result(Status::Ok)]).percent, Some(dec!(100)));
}

#[test]
fn the_four_dimensions_are_always_listed_and_quality_without_data_is_not_a_failure() {
    let (a, b) = payers();
    let results = evaluate(
        &AnalysisConfig::default(),
        &Input {
            assets: &[a, b],
            targets: Targets::default(),
            income: &no_income(),
            returns: &ReturnsFacts::default(),
            quality: &[],
        },
    );
    let report = dimensions(results);
    assert_eq!(
        report.iter().map(|d| d.dimension).collect::<Vec<_>>(),
        Dimension::ALL
    );
    let quality = report.last().unwrap();
    assert_eq!(quality.dimension, Dimension::Quality);
    assert_eq!(quality.rules.len(), 8);
    // Sem fundamentos todas ficam sem dado: fora da pontuação, nunca falha.
    assert!(quality.rules.iter().all(|r| r.status == Status::NoData));
    assert_eq!(quality.score.percent, None);
    assert_eq!(quality.score.no_data, 8);
    assert_eq!(report[0].rules.len(), 10);
    assert_eq!(report[1].rules.len(), 6);
    assert_eq!(report[2].rules.len(), 9);
}

// ---------------------------------------------------------------- contribuição --

#[test]
fn the_contribution_reaches_the_limit_and_never_overshoots_by_a_cent() {
    // 10 de 100 para chegar a 20%: (20·100 − 100·10) / 80 = 12,5.
    assert_eq!(
        contribution_to_grow(dec!(10), dec!(100), dec!(20)),
        Some(dec!(12.5))
    );
    // Já no limite ou acima: nada a aportar.
    assert_eq!(
        contribution_to_grow(dec!(30), dec!(100), dec!(20)),
        Some(Decimal::ZERO)
    );
    // 100% é inalcançável com aporte só na parte.
    assert_eq!(contribution_to_grow(dec!(10), dec!(100), dec!(100)), None);
}

proptest! {
    /// Depois do aporte indicado, a fatia alcança o limite.
    #[test]
    fn the_contribution_always_reaches_the_limit(
        part in 0i64..1_000_000,
        rest in 1i64..1_000_000,
        limit in 1u32..100,
    ) {
        let part = Decimal::new(part, 2);
        let whole = part + Decimal::new(rest, 2);
        let limit = Decimal::from(limit);
        let needed = contribution_to_grow(part, whole, limit).unwrap();
        prop_assert!(needed >= Decimal::ZERO);
        prop_assert!(percent(part + needed, whole + needed) >= limit);
    }

    /// Regra desligada nunca avisa, seja qual for a carteira.
    #[test]
    fn disabled_rules_are_always_off(values in proptest::collection::vec((0usize..7, 0i64..1_000_000), 0..10)) {
        let assets: Vec<AnalysisAsset> = values
            .iter()
            .enumerate()
            .map(|(i, (class, cents))| asset(&format!("A{i}"), AssetClass::ALL[*class], Decimal::new(*cents, 2)))
            .collect();
        let mut config = AnalysisConfig::default();
        for rule in RuleId::ALL {
            config.set(rule, RuleSetting { enabled: false, threshold: rule.default_threshold() });
        }
        let results = evaluate(&config, &Input {
            assets: &assets,
            targets: Targets::default(),
            income: &no_income(),
            returns: &ReturnsFacts::default(),
            quality: &[],
        });
        prop_assert_eq!(results.len(), RULE_COUNT);
        prop_assert!(results.iter().all(|r| r.status == Status::Off && r.current.is_none()));
    }

    /// Sem dado nenhum (carteira vazia, sem proventos, sem série), nada vira aviso.
    #[test]
    fn an_empty_world_never_warns(_n in 0u8..4) {
        let results = evaluate(&AnalysisConfig::default(), &Input {
            assets: &[],
            targets: Targets::default(),
            income: &no_income(),
            returns: &ReturnsFacts::default(),
            quality: &[],
        });
        prop_assert!(results.iter().all(|r| r.status == Status::NoData));
        prop_assert_eq!(attention_count(&results), 0);
        prop_assert_eq!(score(&results).percent, None);
    }

    /// A pontuação nunca passa de 100, `ok <= measured`, e quem está sem dado
    /// ou desligado fica fora da conta.
    #[test]
    fn the_score_is_bounded_and_ignores_unmeasured_rules(
        statuses in proptest::collection::vec(0u8..4, 0..30)
    ) {
        let results: Vec<RuleResult> = statuses
            .iter()
            .map(|s| result(match s { 0 => Status::Off, 1 => Status::NoData, 2 => Status::Ok, _ => Status::Warning }))
            .collect();
        let score = score(&results);
        let ok = statuses.iter().filter(|s| **s == 2).count();
        let warn = statuses.iter().filter(|s| **s == 3).count();
        prop_assert_eq!(usize::try_from(score.ok).unwrap(), ok);
        prop_assert_eq!(usize::try_from(score.measured).unwrap(), ok + warn);
        prop_assert!(score.ok <= score.measured);
        match score.percent {
            None => prop_assert_eq!(score.measured, 0),
            Some(p) => prop_assert!(p >= Decimal::ZERO && p <= Decimal::ONE_HUNDRED),
        }
        prop_assert_eq!(usize::try_from(attention_count(&results)).unwrap(), warn);
    }

    /// Os limites são "em ou acima/abaixo": o número exatamente no limite passa.
    #[test]
    fn a_number_exactly_at_the_limit_passes(limit in 1u32..=100) {
        let limit = Decimal::from(limit);
        let facts = ReturnsFacts {
            positive_years: None,
            volatility_percent: Some(limit),
            ..ReturnsFacts::default()
        };
        let result = run(RuleId::MaxVolatility, Some(limit), &[], &no_income(), &facts);
        prop_assert_eq!(result.status, Status::Ok);
    }

    /// A fatia no exterior que a regra reporta é a de `percent`.
    #[test]
    fn the_abroad_share_matches_a_direct_computation(
        values in proptest::collection::vec((0usize..7, 1i64..1_000_000), 1..10),
        limit in 1u32..=100,
    ) {
        let assets: Vec<AnalysisAsset> = values
            .iter()
            .enumerate()
            .map(|(i, (class, cents))| asset(&format!("A{i}"), AssetClass::ALL[*class], Decimal::new(*cents, 2)))
            .collect();
        let total: Decimal = assets.iter().map(|a| a.value).sum();
        let abroad: Decimal = assets.iter().filter(|a| is_abroad(a.asset_class)).map(|a| a.value).sum();
        let limit = Decimal::from(limit);
        let r = run_simple(RuleId::MinAbroad, Some(limit), &assets);
        let share = percent(abroad, total);
        prop_assert_eq!(r.current, Some(share));
        prop_assert_eq!(r.status == Status::Ok, share >= limit);
        if r.status == Status::Warning {
            // Aportar o indicado, só no exterior, alcança o limite (só 100% não alcança).
            match r.needed_contribution {
                Some(needed) => prop_assert!(percent(abroad + needed, total + needed) >= limit),
                None => prop_assert_eq!(limit, Decimal::ONE_HUNDRED),
            }
        }
    }
}
