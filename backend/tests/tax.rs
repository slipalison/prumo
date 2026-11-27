//! IR contra um Postgres de verdade: o instrumento fiscal padrão que a
//! migração 0015 grava, um livro fictício pela API batendo com o domínio
//! (`domain::tax`) mês a mês e na declaração, o DARF pago, o saldo de
//! prejuízo informado, o IR retido informado, a multa e os juros do DARF em
//! atraso com a Selic guardada, o CNPJ da fonte pagadora vindo da CVM, a
//! tabela de regras e o dono.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use prumo::domain::asset_class::AssetClass;
use prumo::domain::tax::irpf::{annual_report, Income, IncomeKind};
use prumo::domain::tax::monthly::{assess, Carryforward, MonthReport, TaxInput};
use prumo::domain::tax::positions::{Action, OpeningPosition, Operation};
use prumo::domain::tax::rules::Pool;
use prumo::domain::tax::Instrument;
use prumo::domain::tax_book::TaxInstrument;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use time::macros::date;
use time::Date;
use uuid::Uuid;

/// Postgres 17 vazio, sem migração nenhuma.
async fn bare_postgres() -> (ContainerAsync<Postgres>, PgPool) {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .expect("postgres no docker");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let host = container.get_host().await.unwrap();
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();
    (container, pool)
}

fn f(value: Decimal) -> f64 {
    value.to_f64().unwrap()
}

// ---------------------------------------------------------------- migração --

/// Um ativo de antes da 0015: classe, na B3, Tesouro, símbolo, nome e o
/// instrumento esperado.
type Case = (
    AssetClass,
    bool,
    bool,
    Option<&'static str>,
    &'static str,
    TaxInstrument,
);

/// A carteira de antes da 0015 ganha o instrumento padrão, e o SQL da
/// migração dá o mesmo que `TaxInstrument::default_for` para cada caso.
#[tokio::test]
async fn migration_fills_the_default_instrument() {
    let (_container, pool) = bare_postgres().await;
    let mut before = sqlx::migrate!("./migrations");
    before.migrations = before
        .migrations
        .iter()
        .filter(|m| m.version < 15)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    before.run(&pool).await.unwrap();

    let user = Uuid::new_v4();
    sqlx::query("insert into users (id, email) values ($1, 'migra@prumo.test')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    use AssetClass::*;
    use TaxInstrument as T;
    // (classe, na B3, Tesouro, símbolo, nome, o esperado)
    let cases: Vec<Case> = vec![
        (BrStocks, true, false, Some("WEGE3"), "WEG S.A.", T::Stock),
        (
            BrStocks,
            true,
            false,
            Some("PETF3"),
            "Petfood S.A.",
            T::Stock,
        ),
        (
            BrStocks,
            true,
            false,
            Some("BOVA11"),
            "iShares Ibovespa Fundo de Índice",
            T::Etf,
        ),
        (
            BrStocks,
            true,
            false,
            Some("SMAL11"),
            "Small Caps ETF",
            T::Etf,
        ),
        (BrStocks, true, false, Some("AAPL34"), "Apple", T::Bdr),
        (
            BrStocks,
            true,
            false,
            Some("ROXO34"),
            "Nu Holdings DRN",
            T::Bdr,
        ),
        (
            BrStocks,
            true,
            false,
            Some("IMAB11"),
            "It Now IMA-B Fundo de Índice",
            T::Unsupported,
        ),
        (
            BrStocks,
            true,
            false,
            Some("RFIX11"),
            "Fundo de indice de renda fixa",
            T::Unsupported,
        ),
        (
            BrReits,
            true,
            false,
            Some("KNRI11"),
            "Kinea Renda Imobiliária",
            T::RealEstateFund,
        ),
        (
            BrReits,
            true,
            false,
            Some("AGRX11"),
            "Fiagro Exemplo Agro",
            T::AgribusinessFund,
        ),
        (
            BrReits,
            true,
            false,
            Some("XFIX11"),
            "Trend IFIX Fundo de Indice",
            T::Etf,
        ),
        (Crypto, true, false, Some("QBTC11"), "QR Bitcoin", T::Etf),
        (Crypto, false, false, Some("BTC"), "Bitcoin", T::Crypto),
        (
            IntlStocks,
            false,
            false,
            Some("AAPL"),
            "Apple Inc.",
            T::Unsupported,
        ),
        (
            IntlStocks,
            true,
            false,
            Some("MSFT34"),
            "Microsoft DRN",
            T::Bdr,
        ),
        (
            IntlStocks,
            true,
            false,
            Some("IVVB11"),
            "iShares S&P 500 Fundo de Índice",
            T::Etf,
        ),
        (
            IntlStocks,
            true,
            false,
            Some("B5P211"),
            "It Now Tesouro IPCA Fundo de Índice",
            T::Unsupported,
        ),
        (
            UsReits,
            true,
            false,
            Some("REIT11"),
            "Realty BDR DR3",
            T::Bdr,
        ),
        (
            UsReits,
            false,
            false,
            Some("O"),
            "Realty Income",
            T::Unsupported,
        ),
        (
            FixedIncome,
            false,
            false,
            None,
            "CDB Banco Exemplo",
            T::Unsupported,
        ),
        (
            FixedIncome,
            false,
            true,
            Some("TD-IPCA-20350515"),
            "Tesouro IPCA+ 2035",
            T::Unsupported,
        ),
        (
            FixedIncome,
            true,
            false,
            Some("IMAB11"),
            "It Now IMA-B",
            T::Unsupported,
        ),
        (
            IntlFixedIncome,
            false,
            false,
            None,
            "Treasury ficticio",
            T::Unsupported,
        ),
    ];
    let mut ids = Vec::new();
    for (class, listed, treasury, symbol, name, _) in &cases {
        let id = Uuid::new_v4();
        sqlx::query(
            "insert into assets (id, user_id, asset_class, symbol, display_name, quantity, listed_on_b3, treasury)
             values ($1, $2, $3, $4, $5, 0, $6, $7)",
        )
        .bind(id)
        .bind(user)
        .bind(class.as_str())
        .bind(symbol)
        .bind(name)
        .bind(listed)
        .bind(treasury)
        .execute(&pool)
        .await
        .unwrap();
        ids.push(id);
    }

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    for ((class, listed, _, symbol, name, expected), id) in cases.iter().zip(&ids) {
        let (stored, custom): (String, bool) = sqlx::query_as(
            "select tax_instrument, tax_instrument_custom from assets where id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let rust = TaxInstrument::default_for(*class, *listed, *symbol, name);
        assert_eq!(rust, *expected, "codigo para {name}");
        assert_eq!(stored, expected.as_str(), "migracao para {name}");
        assert!(!custom);
    }
    // A coluna não aceita valor desconhecido nem nulo.
    let bad = sqlx::query("update assets set tax_instrument = 'option' where id = $1")
        .bind(ids[0])
        .execute(&pool)
        .await;
    assert!(bad.is_err());
    let null = sqlx::query("update assets set tax_instrument = null where id = $1")
        .bind(ids[0])
        .execute(&pool)
        .await;
    assert!(null.is_err());
}

// ------------------------------------------------------------- o livro --

/// O mesmo livro, escrito direto na linguagem do domínio: o que a API devolve
/// tem de ser o que o domínio calcula com ele.
fn expected_book() -> (Vec<OpeningPosition>, Vec<Operation>) {
    let op = |date: Date, symbol: &str, instrument, action| Operation {
        date,
        symbol: symbol.to_owned(),
        instrument,
        broker: None,
        action,
    };
    let buy = |quantity, unit_price, fees| Action::Buy {
        quantity,
        unit_price,
        fees,
    };
    let sell = |quantity, unit_price, fees| Action::Sell {
        quantity,
        unit_price,
        fees,
    };
    let stock = Instrument::Stock;
    let openings = vec![OpeningPosition {
        symbol: "WEGE3".into(),
        instrument: stock,
        quantity: dec!(100),
        total_cost: dec!(2000),
    }];
    let operations = vec![
        op(
            date!(2025 - 02 - 03),
            "WEGE3",
            stock,
            buy(dec!(100), dec!(30), dec!(10)),
        ),
        op(
            date!(2025 - 03 - 03),
            "WEGE3",
            stock,
            Action::Split { factor: dec!(2) },
        ),
        op(
            date!(2025 - 04 - 07),
            "WEGE3",
            stock,
            sell(dec!(300), dec!(25), dec!(5)),
        ),
        op(
            date!(2025 - 05 - 05),
            "WEGE3",
            stock,
            buy(dec!(300), dec!(30), dec!(0)),
        ),
        op(
            date!(2025 - 05 - 20),
            "WEGE3",
            stock,
            sell(dec!(300), dec!(80), dec!(0)),
        ),
        op(
            date!(2025 - 07 - 01),
            "WEGE3",
            stock,
            buy(dec!(50), dec!(70), dec!(0)),
        ),
        op(
            date!(2025 - 07 - 01),
            "WEGE3",
            stock,
            sell(dec!(50), dec!(72), dec!(0)),
        ),
        op(
            date!(2025 - 01 - 15),
            "KNRI11",
            Instrument::RealEstateFund,
            buy(dec!(10), dec!(150), dec!(0)),
        ),
        op(
            date!(2025 - 08 - 05),
            "KNRI11",
            Instrument::RealEstateFund,
            sell(dec!(10), dec!(140), dec!(0)),
        ),
        // Em dólar no livro: 60.000 x 5,5 e 70.000 x 5,0.
        op(
            date!(2025 - 01 - 20),
            "BTC",
            Instrument::Crypto,
            buy(dec!(0.5), dec!(330000), dec!(0)),
        ),
        op(
            date!(2025 - 09 - 10),
            "BTC",
            Instrument::Crypto,
            sell(dec!(0.5), dec!(350000), dec!(0)),
        ),
    ];
    (openings, operations)
}

fn expected_incomes() -> Vec<Income> {
    vec![
        Income {
            pay_date: date!(2025 - 02 - 14),
            symbol: "KNRI11".into(),
            kind: IncomeKind::FundIncome,
            gross: dec!(10),
            withheld: dec!(0),
            payer: None,
        },
        Income {
            pay_date: date!(2025 - 04 - 15),
            symbol: "WEGE3".into(),
            kind: IncomeKind::Dividend,
            gross: dec!(50),
            withheld: dec!(0),
            payer: None,
        },
        Income {
            pay_date: date!(2025 - 06 - 16),
            symbol: "WEGE3".into(),
            kind: IncomeKind::InterestOnEquity,
            gross: dec!(100),
            withheld: dec!(15),
            payer: None,
        },
    ]
}

fn expected_carryforward() -> Carryforward {
    let mut carry = Carryforward::default();
    carry.losses.insert(Pool::Common, dec!(1000));
    carry
}

/// Um mês da API contra o mesmo mês do domínio.
fn assert_month(api: &Value, domain: &MonthReport) {
    let label = format!("{}/{}", domain.period.month, domain.period.year);
    assert_eq!(api["month"], u64::from(domain.period.month), "{label}");
    for (got, want) in api["exemptions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&domain.exemptions)
    {
        assert_eq!(got["sales"], f(want.sales), "vendas {label}");
        assert_eq!(got["exempt"], want.exempt, "isento {label}");
        assert_eq!(
            got["exempt_gain"],
            f(want.exempt_gain),
            "lucro isento {label}"
        );
        assert_eq!(got["remaining"], f(want.remaining), "falta {label}");
    }
    for (got, want) in api["pools"].as_array().unwrap().iter().zip(&domain.pools) {
        assert_eq!(got["result"], f(want.result), "resultado {label}");
        assert_eq!(got["base"], f(want.base), "base {label}");
        assert_eq!(got["tax"], f(want.tax), "imposto {label}");
        assert_eq!(got["loss_after"], f(want.loss_after), "prejuizo {label}");
    }
    assert_eq!(
        api["capital_gain"]["tax"],
        f(domain.capital_gain.tax),
        "cripto {label}"
    );
    for (got, want) in api["darfs"].as_array().unwrap().iter().zip(&domain.darfs) {
        assert_eq!(got["code"], want.code);
        assert_eq!(got["payable"], want.payable, "DARF {label}");
        assert_eq!(got["amount"], f(want.amount), "DARF {label}");
        assert_eq!(
            got["due_date"],
            want.due_date.to_string(),
            "vencimento {label}"
        );
    }
}

async fn create(router: &axum::Router, body: Value) -> String {
    let reply = post(router, "/api/v1/assets", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    reply.body["id"].as_str().unwrap().to_owned()
}

async fn launch(router: &axum::Router, body: Value) {
    let reply = post(router, "/api/v1/transactions", body).await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
}

#[tokio::test]
async fn ledger_feeds_the_months_and_the_irpf() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let me = app(
        &db.store,
        "ir@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let other = app(
        &db.store,
        "outra@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &me.router;

    let wege = create(
        r,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 0 }),
    )
    .await;
    let knri = create(
        r,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0 }),
    )
    .await;
    let btc = create(
        r,
        json!({ "asset_class": "crypto", "symbol": "BTC", "quantity": 0 }),
    )
    .await;
    create(
        r,
        json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 1000 }),
    )
    .await;

    let tx = |asset: &str, kind: &str, day: &str, quantity: Value, price: Option<Value>| {
        let mut body =
            json!({ "asset_id": asset, "kind": kind, "trade_date": day, "quantity": quantity });
        if let Some(price) = price {
            body["unit_price"] = price;
        }
        body
    };
    launch(
        r,
        tx(&wege, "opening", "2024-06-03", json!(100), Some(json!(20))),
    )
    .await;
    launch(r, json!({ "asset_id": wege, "kind": "buy", "trade_date": "2025-02-03", "quantity": 100, "unit_price": 30, "fees": 10 })).await;
    // A B3 informa o desdobramento em cotas recebidas.
    launch(r, tx(&wege, "split", "2025-03-03", json!(200), None)).await;
    launch(r, json!({ "asset_id": wege, "kind": "sell", "trade_date": "2025-04-07", "quantity": 300, "unit_price": 25, "fees": 5 })).await;
    launch(
        r,
        tx(&wege, "buy", "2025-05-05", json!(300), Some(json!(30))),
    )
    .await;
    launch(
        r,
        tx(&wege, "sell", "2025-05-20", json!(300), Some(json!(80))),
    )
    .await;
    launch(
        r,
        tx(&wege, "buy", "2025-07-01", json!(50), Some(json!(70))),
    )
    .await;
    launch(
        r,
        tx(&wege, "sell", "2025-07-01", json!(50), Some(json!(72))),
    )
    .await;
    launch(
        r,
        tx(&knri, "buy", "2025-01-15", json!(10), Some(json!(150))),
    )
    .await;
    launch(
        r,
        tx(
            &knri,
            "amortization",
            "2025-03-10",
            json!(10),
            Some(json!(1)),
        ),
    )
    .await;
    launch(
        r,
        tx(&knri, "sell", "2025-08-05", json!(10), Some(json!(140))),
    )
    .await;
    launch(r, json!({ "asset_id": btc, "kind": "buy", "trade_date": "2025-01-20", "quantity": 0.5, "unit_price": 60000, "currency": "USD", "fx_rate": 5.5 })).await;
    launch(r, json!({ "asset_id": btc, "kind": "sell", "trade_date": "2025-09-10", "quantity": 0.5, "unit_price": 70000, "currency": "USD", "fx_rate": 5.0 })).await;

    for body in [
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": "2025-02-14", "amount_per_unit": 1, "quantity": 10 }),
        json!({ "asset_id": knri, "kind": "amortization", "pay_date": "2025-03-10", "amount_per_unit": 1, "quantity": 10 }),
        json!({ "asset_id": wege, "kind": "dividend", "pay_date": "2025-04-15", "amount_per_unit": 0.5, "quantity": 100 }),
        json!({ "asset_id": wege, "kind": "jcp", "pay_date": "2025-06-16", "amount_per_unit": 1, "quantity": 100 }),
    ] {
        let reply = post(r, "/api/v1/income", body).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    }

    // Sem saldo informado: o primeiro ano com regra, tudo zero.
    let carry = get(r, "/api/v1/tax/carryforward").await;
    assert_eq!(carry.status, StatusCode::OK);
    assert_eq!(carry.body["year"], 2025);
    assert_eq!(carry.body["informed"], false);
    assert_eq!(carry.body["covered_years"], json!([2025, 2026]));
    let saved = put(
        r,
        "/api/v1/tax/carryforward",
        json!({ "year": 2025, "common_loss": 1000 }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);
    assert_eq!(saved.body["informed"], true);
    assert_eq!(saved.body["common_loss"], 1000.0);
    for bad in [
        json!({ "year": 2025, "common_loss": -1 }),
        json!({ "year": 2025, "day_trade_loss": 0.001 }),
        json!({ "year": 2024 }),
        json!({ "year": 2025, "extra": 1 }),
    ] {
        let reply = put(r, "/api/v1/tax/carryforward", bad.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    // ---- meses
    let (openings, operations) = expected_book();
    let input = TaxInput {
        openings: &openings,
        operations: &operations,
        since_year: 2025,
        carryforward: expected_carryforward(),
        informed_withholding: Default::default(),
    };
    let domain = assess(&input, 2025).unwrap();
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    assert_eq!(months.status, StatusCode::OK, "{}", months.text);
    assert_eq!(months.body["year"], 2025);
    assert_eq!(months.body["since_year"], 2025);
    let list = months.body["months"].as_array().unwrap();
    assert_eq!(list.len(), 12);
    for (api, report) in list.iter().zip(&domain.months) {
        assert_month(api, report);
    }
    // Conferência à mão dos números que importam.
    let april = &list[3];
    assert_eq!(april["exemptions"][0]["sales"], 7500.0);
    assert_eq!(april["exemptions"][0]["exempt"], true);
    assert_eq!(april["exemptions"][0]["exempt_gain"], 3737.5);
    let may = &list[4];
    assert_eq!(may["pools"][0]["loss_before"], 1000.0);
    assert_eq!(may["pools"][0]["tax"], 2296.59);
    // A corretora retém 0,005% dos R$ 24 mil vendidos (R$ 1,20), que abatem
    // o DARF; sem o valor da nota, a conta usa a regra e avisa.
    assert_eq!(may["withholding"]["common_sales"], 24000.0);
    assert_eq!(may["withholding"]["common_rule"], 1.2);
    assert_eq!(may["withholding"]["informed"], Value::Null);
    assert_eq!(may["notices"], json!(["withholding_by_rule"]));
    assert_eq!(may["darfs"][0]["amount"], 2295.39);
    assert_eq!(may["darfs"][0]["due_date"], "2025-06-30");
    let july = &list[6];
    assert_eq!(july["pools"][1]["pool"], "day_trade");
    // 20% de R$ 100 de day trade, menos o 1% retido (R$ 1).
    assert_eq!(july["withholding"]["day_trade_rule"], 1.0);
    assert_eq!(july["darfs"][0]["amount"], 19.0);
    assert_eq!(july["doubts"], json!(["day_trade_in_stock_exemption"]));
    let august = &list[7];
    assert_eq!(august["pools"][2]["loss_after"], 100.0);
    let september = &list[8];
    assert_eq!(september["darfs"][1]["code"], "4600");
    assert_eq!(september["darfs"][1]["amount"], 1500.0);
    assert_eq!(september["darfs"][1]["due_date"], "2025-10-31");
    assert_eq!(months.body["closing"]["losses"]["real_estate_fund"], 100.0);
    // O que ficou fora: a renda fixa e a amortização.
    let skipped = months.body["skipped"].as_array().unwrap();
    let reasons: Vec<(&str, &str)> = skipped
        .iter()
        .map(|s| {
            (
                s["subject"].as_str().unwrap(),
                s["reason"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        reasons,
        vec![
            ("CDB ficticio", "unsupported_instrument"),
            ("KNRI11", "amortization")
        ]
    );
    assert_eq!(skipped[1]["date"], "2025-03-10");
    assert_eq!(skipped[1]["amount"], 10.0);

    // ---- IRPF
    let report = annual_report(&input, &expected_incomes(), 2025).unwrap();
    let irpf = get(r, "/api/v1/tax/irpf/2025").await;
    assert_eq!(irpf.status, StatusCode::OK, "{}", irpf.text);
    let assets = irpf.body["assets"].as_array().unwrap();
    assert_eq!(assets.len(), report.assets.len());
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["symbol"], "WEGE3");
    assert_eq!(assets[0]["quantity"], 100.0);
    assert_eq!(assets[0]["total_cost"], f(report.assets[0].total_cost));
    assert_eq!(assets[0]["total_cost"], 2563.13);
    assert_eq!(assets[0]["group"], "03");
    assert_eq!(assets[0]["code"], "01");
    for (got, want) in irpf.body["exempt_income"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&report.exempt_income)
    {
        assert_eq!(got["code"], want.code.code);
        assert_eq!(got["gross"], f(want.gross));
        assert_eq!(got["net"], f(want.net));
    }
    let dividends = &irpf.body["exempt_income"][0];
    assert_eq!(dividends["code"], "09");
    assert_eq!(dividends["gross"], 50.0);
    assert_eq!(irpf.body["exempt_income"][1]["gross"], 10.0);
    assert_eq!(irpf.body["exempt_income"][2]["gross"], 3737.5);
    let jcp = &irpf.body["exclusive_income"][0];
    assert_eq!(jcp["code"], "10");
    assert_eq!(jcp["gross"], 100.0);
    assert_eq!(jcp["withheld"], 15.0);
    assert_eq!(jcp["net"], 85.0);
    assert_eq!(irpf.body["darf_total"], f(report.darf_total));
    assert_eq!(irpf.body["year_end"]["losses"]["real_estate_fund"], 100.0);
    assert_eq!(
        irpf.body["doubts"],
        json!(["irpf_codes", "interest_on_equity_net"])
    );
    let skipped_reasons: Vec<&str> = irpf.body["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["reason"].as_str().unwrap())
        .collect();
    assert!(skipped_reasons.contains(&"income_amortization"));

    // ---- ano sem regra: erro claro, nunca a regra de outro ano
    for uri in ["/api/v1/tax/months?year=2024", "/api/v1/tax/irpf/2027"] {
        let reply = get(r, uri).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        assert_eq!(reply.body["code"], "tax_year_not_covered");
        assert!(reply.body["detail"]
            .as_str()
            .unwrap()
            .contains("2025 e 2026"));
    }
    // 2026 apura desde 2025 e leva o prejuízo de FII para frente.
    let next = get(r, "/api/v1/tax/months?year=2026").await;
    assert_eq!(next.status, StatusCode::OK);
    assert_eq!(next.body["months"][0]["pools"][2]["loss_before"], 100.0);

    // ---- DARF pago
    let paid = post(
        r,
        "/api/v1/tax/darfs/2025/5/paid",
        json!({ "code": "6015" }),
    )
    .await;
    assert_eq!(paid.status, StatusCode::OK, "{}", paid.text);
    assert_eq!(paid.body["paid"]["amount"], 2295.39);
    assert_eq!(paid.body["paid"]["same_amount"], true);
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    assert_eq!(
        months.body["months"][4]["darfs"][0]["paid"]["amount"],
        2295.39
    );
    assert_eq!(months.body["months"][3]["darfs"][0]["paid"], Value::Null);
    let nothing = post(
        r,
        "/api/v1/tax/darfs/2025/4/paid",
        json!({ "code": "6015" }),
    )
    .await;
    assert_eq!(nothing.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(nothing.body["code"], "darf_not_due");
    let bad_code = post(
        r,
        "/api/v1/tax/darfs/2025/5/paid",
        json!({ "code": "0190" }),
    )
    .await;
    assert_eq!(bad_code.status, StatusCode::UNPROCESSABLE_ENTITY);
    let bad_month = post(
        r,
        "/api/v1/tax/darfs/2025/13/paid",
        json!({ "code": "6015" }),
    )
    .await;
    assert_eq!(bad_month.status, StatusCode::NOT_FOUND);
    let no_rule = post(
        r,
        "/api/v1/tax/darfs/2027/1/paid",
        json!({ "code": "6015" }),
    )
    .await;
    assert_eq!(no_rule.body["code"], "tax_year_not_covered");

    // O livro muda depois de marcar: a marca fica e diz que o valor é outro.
    launch(r, json!({ "asset_id": wege, "kind": "sell", "trade_date": "2025-05-21", "quantity": 10, "unit_price": 80 })).await;
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    let may = &months.body["months"][4]["darfs"][0];
    assert_ne!(may["amount"], 2295.39);
    assert_eq!(may["paid"]["same_amount"], false);

    // ---- dono: a outra pessoa não vê nem mexe
    let theirs = get(&other.router, "/api/v1/tax/months?year=2025").await;
    assert_eq!(theirs.status, StatusCode::OK);
    assert!(theirs.body["months"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["darfs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["paid"].is_null() && d["amount"] == 0.0)));
    assert_eq!(theirs.body["skipped"], json!([]));
    let their_carry = get(&other.router, "/api/v1/tax/carryforward").await;
    assert_eq!(their_carry.body["informed"], false);
    let their_paid = post(
        &other.router,
        "/api/v1/tax/darfs/2025/5/paid",
        json!({ "code": "6015" }),
    )
    .await;
    assert_eq!(their_paid.body["code"], "darf_not_due");
    let their_delete = delete(&other.router, "/api/v1/tax/darfs/2025/5/paid?code=6015").await;
    assert_eq!(their_delete.status, StatusCode::NOT_FOUND);
    let their_irpf = get(&other.router, "/api/v1/tax/irpf/2025").await;
    assert_eq!(their_irpf.body["assets"], json!([]));

    let unmarked = delete(r, "/api/v1/tax/darfs/2025/5/paid?code=6015").await;
    assert_eq!(unmarked.status, StatusCode::NO_CONTENT);
    let again = delete(r, "/api/v1/tax/darfs/2025/5/paid?code=6015").await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tax_instrument_is_chosen_on_the_asset_sheet() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let me = app(
        &db.store,
        "folha@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &me.router;

    let qbtc = post(
        r,
        "/api/v1/assets",
        json!({ "asset_class": "crypto", "symbol": "QBTC11", "listed_on_b3": true, "quantity": 0 }),
    )
    .await;
    assert_eq!(qbtc.status, StatusCode::CREATED, "{}", qbtc.text);
    // ETF de bitcoin da B3: ETF no IR.
    assert_eq!(qbtc.body["tax_instrument"], "etf");
    assert_eq!(qbtc.body["tax_instrument_default"], "etf");
    let id = qbtc.body["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/v1/assets/{id}");

    // Fora da B3 o padrão vira cripto, e o instrumento acompanha.
    let moved = patch(r, &uri, json!({ "listed_on_b3": false })).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text);
    assert_eq!(moved.body["tax_instrument"], "crypto");

    // A escolha da pessoa fica, mesmo quando a marca muda.
    let chosen = patch(r, &uri, json!({ "tax_instrument": "unsupported" })).await;
    assert_eq!(chosen.body["tax_instrument"], "unsupported");
    assert_eq!(chosen.body["tax_instrument_default"], "crypto");
    let back_on_b3 = patch(r, &uri, json!({ "listed_on_b3": true })).await;
    assert_eq!(back_on_b3.body["tax_instrument"], "unsupported");
    assert_eq!(back_on_b3.body["tax_instrument_default"], "etf");

    // `null` volta ao padrão.
    let reset = patch(r, &uri, json!({ "tax_instrument": null })).await;
    assert_eq!(reset.body["tax_instrument"], "etf");
    let portfolio = get(r, "/api/v1/portfolio").await;
    assert_eq!(portfolio.body["assets"][0]["tax_instrument"], "etf");

    let invalid = patch(r, &uri, json!({ "tax_instrument": "option" })).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);

    // O instrumento entra na conta: com o QBTC11 fora do IR, a venda some da
    // apuração e aparece em "não calculado".
    launch(r, json!({ "asset_id": id, "kind": "buy", "trade_date": "2025-03-03", "quantity": 10, "unit_price": 50 })).await;
    patch(r, &uri, json!({ "tax_instrument": "unsupported" })).await;
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    assert_eq!(months.body["skipped"][0]["subject"], "QBTC11");
    assert_eq!(
        months.body["skipped"][0]["reason"],
        "unsupported_instrument"
    );
}

/// Um livro curto: lucro de R$ 3.000 em ações em maio de 2025 (vendas de
/// R$ 30 mil, DARF 6015 que vence em 30/06/2025).
async fn may_2025_book(router: &axum::Router) -> String {
    let id = create(
        router,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 0 }),
    )
    .await;
    launch(router, json!({ "asset_id": id, "kind": "buy", "trade_date": "2025-02-03", "quantity": 1000, "unit_price": 27 })).await;
    launch(router, json!({ "asset_id": id, "kind": "sell", "trade_date": "2025-05-20", "quantity": 1000, "unit_price": 30 })).await;
    id
}

#[tokio::test]
async fn informed_withholding_replaces_the_rule() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let me = app(
        &db.store,
        "retido@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let other = app(
        &db.store,
        "outra-retido@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &me.router;
    may_2025_book(r).await;

    // Pela regra: 0,005% de R$ 30 mil = R$ 1,50; DARF de 450 − 1,50.
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    let may = &months.body["months"][4];
    assert_eq!(may["withholding"]["common_rule"], 1.5);
    assert_eq!(may["withholding"]["withheld"], 1.5);
    assert_eq!(may["darfs"][0]["amount"], 448.5);

    // A nota diz R$ 2,00: vale a nota, e o aviso some.
    let saved = put(r, "/api/v1/tax/withholding/2025/5", json!({ "amount": 2 })).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);
    assert_eq!(
        saved.body,
        json!({ "year": 2025, "month": 5, "amount": 2.0 })
    );
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    let may = &months.body["months"][4];
    assert_eq!(may["withholding"]["informed"], 2.0);
    assert_eq!(may["withholding"]["withheld"], 2.0);
    assert_eq!(may["withholding"]["common_rule"], 1.5);
    assert_eq!(may["notices"], json!([]));
    assert_eq!(may["darfs"][0]["amount"], 448.0);
    // A outra pessoa continua pela regra.
    let theirs = get(&other.router, "/api/v1/tax/months?year=2025").await;
    assert_eq!(
        theirs.body["months"][4]["withholding"]["informed"],
        Value::Null
    );

    for (uri, body, status) in [
        (
            "/api/v1/tax/withholding/2025/5",
            json!({ "amount": -1 }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/v1/tax/withholding/2025/5",
            json!({ "amount": 0.001 }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/v1/tax/withholding/2025/5",
            json!({ "value": 1 }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/v1/tax/withholding/2031/5",
            json!({ "amount": 1 }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/v1/tax/withholding/2025/13",
            json!({ "amount": 1 }),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let reply = put(r, uri, body.clone()).await;
        assert_eq!(reply.status, status, "{uri} {body}");
    }

    // Apagar volta para a regra; de novo, nada a apagar.
    let their_delete = delete(&other.router, "/api/v1/tax/withholding/2025/5").await;
    assert_eq!(their_delete.status, StatusCode::NOT_FOUND);
    let cleared = delete(r, "/api/v1/tax/withholding/2025/5").await;
    assert_eq!(cleared.status, StatusCode::NO_CONTENT);
    let again = delete(r, "/api/v1/tax/withholding/2025/5").await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    let months = get(r, "/api/v1/tax/months?year=2025").await;
    assert_eq!(months.body["months"][4]["darfs"][0]["amount"], 448.5);
}

#[tokio::test]
async fn late_darf_uses_the_stored_selic() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let me = app(
        &db.store,
        "atraso@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &me.router;
    may_2025_book(r).await;
    // Selic de junho a setembro de 2025 (SGS 4390).
    db.store
        .upsert_tax_selic(&[
            (date!(2025 - 06 - 01), dec!(1.10)),
            (date!(2025 - 07 - 01), dec!(1.28)),
            (date!(2025 - 08 - 01), dec!(1.16)),
            (date!(2025 - 09 - 01), dec!(1.22)),
        ])
        .await
        .unwrap();

    // DARF de R$ 448,50 vencido em 30/06/2025 e pago em 15/09/2025: 77 dias
    // (multa no teto de 20%) e juros de julho e agosto mais 1%.
    let late = get(
        r,
        "/api/v1/tax/darfs/2025/5/late?code=6015&pay_on=2025-09-15",
    )
    .await;
    assert_eq!(late.status, StatusCode::OK, "{}", late.text);
    let body = &late.body;
    assert_eq!(body["principal"], 448.5);
    assert_eq!(body["due_date"], "2025-06-30");
    assert_eq!(body["pay_date"], "2025-09-15");
    assert_eq!(body["days_late"], 77);
    assert_eq!(body["penalty_rate"], 0.2);
    assert_eq!(body["penalty"], 89.7);
    assert_eq!(
        body["selic_months"],
        json!([{ "year": 2025, "month": 7, "rate": 1.28 }, { "year": 2025, "month": 8, "rate": 1.16 }])
    );
    assert_eq!(body["payment_month_rate"], 0.01);
    assert_eq!(body["interest_rate"], 0.0344);
    assert_eq!(body["interest"], 15.43);
    assert_eq!(body["total"], 553.63);
    assert_eq!(body["missing_selic"], json!([]));
    assert_eq!(body["rule"]["selic_series"]["value"], "4390");

    // Pago em janeiro de 2026: falta a Selic de outubro a dezembro.
    let unknown = get(
        r,
        "/api/v1/tax/darfs/2025/5/late?code=6015&pay_on=2026-01-10",
    )
    .await;
    assert_eq!(unknown.status, StatusCode::OK);
    assert_eq!(unknown.body["interest"], Value::Null);
    assert_eq!(unknown.body["total"], Value::Null);
    assert_eq!(unknown.body["missing_selic"].as_array().unwrap().len(), 3);

    // Em dia: nada a mais.
    let on_time = get(
        r,
        "/api/v1/tax/darfs/2025/5/late?code=6015&pay_on=2025-06-30",
    )
    .await;
    assert_eq!(on_time.body["days_late"], 0);
    assert_eq!(on_time.body["total"], 448.5);

    for (uri, status, code) in [
        (
            "/api/v1/tax/darfs/2025/4/late?code=6015",
            StatusCode::UNPROCESSABLE_ENTITY,
            "darf_not_due",
        ),
        (
            "/api/v1/tax/darfs/2025/5/late?code=4600",
            StatusCode::UNPROCESSABLE_ENTITY,
            "darf_not_due",
        ),
        (
            "/api/v1/tax/darfs/2025/5/late?code=0190",
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
        ),
        (
            "/api/v1/tax/darfs/2025/5/late?code=6015&pay_on=15/09/2025",
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
        ),
        (
            "/api/v1/tax/darfs/2027/5/late?code=6015",
            StatusCode::UNPROCESSABLE_ENTITY,
            "tax_year_not_covered",
        ),
    ] {
        let reply = get(r, uri).await;
        assert_eq!(reply.status, status, "{uri}: {}", reply.text);
        assert_eq!(reply.body["code"], code, "{uri}");
    }
    let bad_month = get(r, "/api/v1/tax/darfs/2025/13/late?code=6015").await;
    assert_eq!(bad_month.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn payers_come_from_the_cvm_registry() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let me = app(
        &db.store,
        "cnpj@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let r = &me.router;
    let pool = PgPool::connect(&db.url).await.unwrap();
    // Cadastro fictício: o emissor WEGE com o código CVM 99901, e o fundo
    // com a raiz KNRI no informe mensal (dois informes; vale o mais novo).
    sqlx::query(
        "insert into b3_issuer_cvm (issuer, cvm_code, fetched_at) values ('WEGE', 99901, now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into cvm_companies (cvm_code, cnpj, name, status, updated_at)
         values (99901, '00.000.000/0001-91', 'EMPRESA EXEMPLO S.A.', 'ATIVO', now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (cnpj, name, reference) in [
        ("11111111000111", "FUNDO ANTIGO FII", "2024-12-01"),
        ("22.222.222/0001-22", "FUNDO EXEMPLO FII", "2025-10-01"),
    ] {
        sqlx::query(
            "insert into cvm_funds (cnpj, issuer, isin, name, reference_date, version, updated_at)
             values ($1, 'KNRI', 'BRKNRICTF007', $2, $3::date, 1, now())",
        )
        .bind(cnpj)
        .bind(name)
        .bind(reference)
        .execute(&pool)
        .await
        .unwrap();
    }

    let wege = create(
        r,
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 0 }),
    )
    .await;
    let knri = create(
        r,
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 0 }),
    )
    .await;
    let other = create(
        r,
        json!({ "asset_class": "br_stocks", "symbol": "SEMC3", "quantity": 0 }),
    )
    .await;
    for (asset, kind) in [(&wege, "buy"), (&knri, "buy"), (&other, "buy")] {
        launch(r, json!({ "asset_id": asset, "kind": kind, "trade_date": "2025-01-10", "quantity": 100, "unit_price": 10 })).await;
    }
    for body in [
        json!({ "asset_id": wege, "kind": "dividend", "pay_date": "2025-04-15", "amount_per_unit": 0.5, "quantity": 100 }),
        json!({ "asset_id": wege, "kind": "jcp", "pay_date": "2025-06-16", "amount_per_unit": 1, "quantity": 100 }),
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": "2025-02-14", "amount_per_unit": 1, "quantity": 100 }),
        json!({ "asset_id": other, "kind": "dividend", "pay_date": "2025-04-15", "amount_per_unit": 0.1, "quantity": 100 }),
    ] {
        let reply = post(r, "/api/v1/income", body).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text);
    }

    let irpf = get(r, "/api/v1/tax/irpf/2025").await;
    assert_eq!(irpf.status, StatusCode::OK, "{}", irpf.text);
    let dividends = &irpf.body["exempt_income"][0]["by_payer"];
    assert_eq!(dividends[0]["symbols"], json!(["SEMC3"]));
    assert_eq!(dividends[0]["cnpj"], Value::Null);
    assert_eq!(dividends[1]["symbols"], json!(["WEGE3"]));
    assert_eq!(dividends[1]["cnpj"], "00.000.000/0001-91");
    assert_eq!(dividends[1]["name"], "EMPRESA EXEMPLO S.A.");
    assert_eq!(dividends[1]["payer_source"], "cvm_company");
    assert_eq!(dividends[1]["net"], 50.0);
    let fund = &irpf.body["exempt_income"][1]["by_payer"][0];
    assert_eq!(fund["cnpj"], "22.222.222/0001-22");
    assert_eq!(fund["name"], "FUNDO EXEMPLO FII");
    assert_eq!(fund["payer_source"], "cvm_fund");
    let jcp = &irpf.body["exclusive_income"][0]["by_payer"][0];
    assert_eq!(jcp["cnpj"], "00.000.000/0001-91");
    assert_eq!(jcp["withheld"], 15.0);
    assert_eq!(jcp["net"], 85.0);
}

#[tokio::test]
async fn rules_list_every_number_with_its_source() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let me = app(
        &db.store,
        "regras@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let rules = get(&me.router, "/api/v1/tax/rules/2026").await;
    assert_eq!(rules.status, StatusCode::OK, "{}", rules.text);
    let body = &rules.body;
    assert_eq!(body["year"], 2026);
    assert_eq!(body["covered_years"], json!([2025, 2026]));
    let rate = |pool: &str| find(&body["pools"], "pool", pool)["rate"].clone();
    assert_eq!(rate("common")["value"], 0.15);
    assert_eq!(rate("day_trade")["value"], 0.2);
    assert!(rate("day_trade")["source"]
        .as_str()
        .unwrap()
        .contains("Lei 11.033/2004"));
    let etf = find(&body["instruments"], "instrument", "etf");
    assert_eq!(etf["exemption"], Value::Null);
    assert_eq!(etf["common_pool"], "common");
    assert_eq!(body["withholding"]["common_rate"]["value"], 0.00005);
    assert_eq!(body["withholding"]["day_trade_rate"]["value"], 0.01);
    assert_eq!(body["late_payment"]["daily_penalty"]["value"], 0.0033);
    assert_eq!(body["late_payment"]["penalty_cap"]["value"], 0.2);
    assert_eq!(body["interest_on_equity_rate"]["value"], 0.175);
    assert_eq!(body["dividend_withholding"]["monthly_threshold"], 50000.0);
    assert_eq!(body["exchange_darf_code"]["value"], "6015");
    let earlier = get(&me.router, "/api/v1/tax/rules/2025").await;
    assert_eq!(earlier.body["dividend_withholding"], Value::Null);
    assert_eq!(earlier.body["interest_on_equity_rate"]["value"], 0.15);
    let missing = get(&me.router, "/api/v1/tax/rules/2027").await;
    assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(missing.body["code"], "tax_year_not_covered");
}

#[tokio::test]
async fn selic_sync_keeps_only_closed_months_and_skips_when_fresh() {
    use prumo::tax_selic::TaxSelicSync;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // A série 4390 como o SGS respondeu em 27/11/2026: novembro vem
    // acumulado até a véspera (0,87) e não pode entrar.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dados/serie/bcdata.sgs.4390/dados"))
        .and(query_param("dataInicial", "01/01/2025"))
        .and(query_param("dataFinal", "27/11/2026"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("fixtures/history/bcb_4390_selic.json"),
            "application/json",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let db = database().await;
    let sync = TaxSelicSync::with_base(db.store.clone(), reqwest::Client::new(), &server.uri());
    let today = date!(2026 - 11 - 27);
    assert_eq!(sync.run_once(today).await.unwrap(), 22);
    let selic = db.store.tax_selic().await.unwrap();
    assert_eq!(selic.len(), 22);
    let october = prumo::domain::tax::YearMonth {
        year: 2026,
        month: 10,
    };
    assert_eq!(selic[&october], dec!(1.07));
    assert!(!selic.contains_key(&prumo::domain::tax::YearMonth {
        year: 2026,
        month: 11
    }));
    // Outubro já está: a segunda rodada nem vai à rede (o mock espera uma).
    assert_eq!(sync.run_once(today).await.unwrap(), 0);

    // Fonte fora do ar: erro na rodada, e o que estava guardado continua.
    let down = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&down)
        .await;
    let failing = TaxSelicSync::with_base(db.store.clone(), reqwest::Client::new(), &down.uri());
    assert!(failing.run_once(date!(2026 - 12 - 10)).await.is_err());
    assert_eq!(db.store.tax_selic().await.unwrap().len(), 22);
}
