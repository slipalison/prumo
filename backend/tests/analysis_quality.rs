//! Qualidade dos ativos, pontos de atenção e Graham contra um Postgres de
//! verdade, com fundamentos da CVM gravados pelo banco (companhias e FIIs
//! inventados): regras ponderadas pelo valor, o "sem dado" que não é falha, a
//! recuperação judicial, o P/VP e a vacância, os pontos de atenção por ativo, o
//! Graham ao lado do teto e o dono.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use axum::http::StatusCode;
use common::*;
use prumo::domain::fundamentals::FundReport;
use prumo::domain::fundamentals::{
    Column, Figure, Filing, FilingKind, Line, Registration, Scope, Statement, Vacancy,
};
use prumo::fundamentals::b3::IssuerInfo;
use prumo::fundamentals::download::Validators;
use prumo::store::fundamentals::FundVacancy;
use prumo::store::Store;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::macros::{date, datetime};
use time::{Date, Month, OffsetDateTime};

const NOW: OffsetDateTime = datetime!(2026-11-26 15:00 UTC);

fn rule<'a>(analysis: &'a Value, id: &str) -> &'a Value {
    analysis["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["rules"].as_array().unwrap())
        .find(|r| r["rule"] == id)
        .unwrap_or_else(|| panic!("regra {id} ausente"))
}

fn close(value: &Value, expected: f64) {
    let got = value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} não é número"));
    assert!((got - expected).abs() < 0.011, "{got} != {expected}");
}

async fn analysis(app: &TestApp) -> Value {
    let reply = get(&app.router, "/api/v1/analysis").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply.body
}

async fn add_asset(app: &TestApp, symbol: &str, class: &str) -> String {
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": class, "symbol": symbol, "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    created.body["id"].as_str().unwrap().to_owned()
}

/// Os números (em reais) de um exercício de uma companhia fictícia.
#[derive(Clone, Copy)]
struct Year {
    revenue: i64,
    profit: i64,
    equity: i64,
    debt: i64,
    cash: i64,
}

fn flow(statement: Statement, year: i32, code: &str, description: &str, value: i64) -> Line {
    Line {
        statement,
        column: Column::Current,
        period_start: Some(Date::from_calendar_date(year, Month::January, 1).unwrap()),
        period_end: Date::from_calendar_date(year, Month::December, 31).unwrap(),
        code: code.to_owned(),
        description: description.to_owned(),
        value: Decimal::from(value),
    }
}

fn balance(statement: Statement, year: i32, code: &str, description: &str, value: i64) -> Line {
    Line {
        period_start: None,
        ..flow(statement, year, code, description, value)
    }
}

/// A DFP de um exercício: DRE, caixa operacional, depreciação, patrimônio,
/// dívida e caixa.
fn dfp(cvm_code: i32, name: &str, year: i32, y: Year) -> Filing {
    Filing {
        cvm_code,
        kind: FilingKind::Annual,
        reference_date: Date::from_calendar_date(year, Month::December, 31).unwrap(),
        version: 1,
        scope: Scope::Consolidated,
        company_name: name.to_owned(),
        lines: vec![
            flow(
                Statement::Income,
                year,
                "3.01",
                "Receita de Venda de Bens e/ou Serviços",
                y.revenue,
            ),
            flow(
                Statement::Income,
                year,
                "3.05",
                "Resultado Antes do Resultado Financeiro e dos Tributos",
                y.profit * 3 / 2,
            ),
            flow(
                Statement::Income,
                year,
                "3.11",
                "Lucro/Prejuízo Consolidado do Período",
                y.profit,
            ),
            flow(
                Statement::Income,
                year,
                "3.11.01",
                "Atribuído a Sócios da Empresa Controladora",
                y.profit,
            ),
            flow(
                Statement::ValueAdded,
                year,
                "7.04.01",
                "Depreciação, Amortização e Exaustão",
                -100,
            ),
            flow(
                Statement::CashFlow,
                year,
                "6.01",
                "Caixa Líquido Atividades Operacionais",
                y.profit + 50,
            ),
            balance(
                Statement::Liabilities,
                year,
                "2.03",
                "Patrimônio Líquido Consolidado",
                y.equity,
            ),
            balance(
                Statement::Liabilities,
                year,
                "2.03.09",
                "Participação dos Acionistas Não Controladores",
                0,
            ),
            balance(
                Statement::Liabilities,
                year,
                "2.01.04",
                "Empréstimos e Financiamentos",
                y.debt,
            ),
            balance(
                Statement::Assets,
                year,
                "1.01.01",
                "Caixa e Equivalentes de Caixa",
                y.cash,
            ),
            balance(
                Statement::Liabilities,
                year,
                "2.02.01",
                "Empréstimos e Financiamentos",
                0,
            ),
            balance(
                Statement::Assets,
                year,
                "1.01.02",
                "Aplicações Financeiras",
                0,
            ),
        ],
    }
}

fn registration(cvm_code: i32, name: &str, issuer_status: &str) -> Registration {
    Registration {
        cvm_code,
        cnpj: format!("00.000.000/{cvm_code:04}-00"),
        name: name.to_owned(),
        status: "ATIVO".to_owned(),
        issuer_status: Some(issuer_status.to_owned()),
        sector: None,
    }
}

/// Duas companhias (`FORT3`, forte e crescendo; `FRAC3`, fraca, endividada e
/// em recuperação judicial) e dois FIIs (`KNRI11` de tijolo, caro sobre o
/// patrimônio; `MXRF11` de papel, barato).
async fn seed_fundamentals(store: &Store) {
    let v = Validators::default();
    let scanned: BTreeSet<i32> = [91001, 91002].into_iter().collect();
    for (index, year) in (2021..=2025).enumerate() {
        let step = i64::try_from(index).unwrap();
        let strong = Year {
            revenue: 600 + 100 * step,
            profit: 100 + 20 * step,
            equity: 1000,
            debt: 300,
            cash: 200,
        };
        let weak = Year {
            revenue: 1000,
            profit: 100 - 20 * step,
            equity: 1000,
            debt: 3000,
            cash: 100,
        };
        store
            .save_company_filings(
                &format!("annual:{year}"),
                &[
                    dfp(91001, "FORTE SA", year, strong),
                    dfp(91002, "FRACA SA", year, weak),
                ],
                &v,
                &scanned,
                NOW,
            )
            .await
            .unwrap();
    }
    store
        .save_registry(
            "registry",
            &[
                registration(91001, "FORTE SA", "FASE OPERACIONAL"),
                registration(91002, "FRACA SA", "EM RECUPERAÇÃO JUDICIAL OU EQUIVALENTE"),
            ],
            &v,
            NOW,
        )
        .await
        .unwrap();
    for (issuer, code, shares) in [("FORT", 91001, 100), ("FRAC", 91002, 100)] {
        store
            .save_issuer_code(
                issuer,
                &IssuerInfo {
                    cvm_code: Some(code),
                    total_shares: Some(Decimal::from(shares)),
                },
                NOW,
            )
            .await
            .unwrap();
    }
    let report = |cnpj: &str, issuer: &str, isin: &str, shares: i64| FundReport {
        cnpj: cnpj.to_owned(),
        issuer: issuer.to_owned(),
        isin: isin.to_owned(),
        name: format!("FUNDO {issuer}"),
        reference_date: date!(2026 - 10 - 31),
        version: 1,
        net_assets: Some(dec!(1000000)),
        shares: Some(Decimal::from(shares)),
    };
    store
        .save_fund_reports(
            "fund_monthly:2026",
            &[
                // Valor patrimonial de 100 e 10 por cota.
                report("11.111.111/0001-11", "KNRI", "BRKNRICTF007", 10_000),
                report("22.222.222/0001-22", "MXRF", "BRMXRFCTF004", 100_000),
            ],
            &v,
            NOW,
        )
        .await
        .unwrap();
    // Só o fundo de tijolo declara imóvel: 5% de vacância.
    store
        .save_fund_vacancy(
            "fund_quarterly:2026",
            &[FundVacancy {
                cnpj: "11.111.111/0001-11".to_owned(),
                vacancy: Vacancy {
                    reference_date: date!(2026 - 09 - 30),
                    value: Figure::known(dec!(0.05)),
                    properties: 3,
                },
            }],
            &v,
            NOW,
        )
        .await
        .unwrap();
}

fn quotes() -> std::sync::Arc<FakeQuotes> {
    let quotes = FakeQuotes::with_market();
    {
        let mut prices = quotes.prices.lock().unwrap();
        prices.insert(
            "FORT3.SA".into(),
            ("40.00".parse().unwrap(), "BRL", "Forte"),
        );
        prices.insert(
            "FRAC3.SA".into(),
            ("20.00".parse().unwrap(), "BRL", "Fraca"),
        );
    }
    quotes
}

#[tokio::test]
async fn quality_attention_and_graham_contract() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    seed_fundamentals(&db.store).await;

    let app = app(&db.store, "qualidade@prumo.test", quotes(), &dir.path());
    let fort = add_asset(&app, "FORT3", "br_stocks").await;
    add_asset(&app, "FRAC3", "br_stocks").await;
    add_asset(&app, "KNRI11", "br_reits").await;
    add_asset(&app, "MXRF11", "br_reits").await;
    // Sem fundamentos: ETF em Ações Nacionais e ação dos EUA.
    add_asset(&app, "AAPL", "intl_stocks").await;

    let body = analysis(&app).await;

    // FORT3 vale 400 e FRAC3, 200: o ROE (18% e 2%) passa em 400 de 600.
    let roe = rule(&body, "quality_roe");
    assert_eq!(roe["status"], "warning");
    close(&roe["current"], 66.67);
    assert_eq!(roe["limit"], 70.0);
    assert_eq!(roe["unit"], "percent");
    assert_eq!(roe["direction"], "at_least");
    assert_eq!(
        roe["detail"],
        json!({ "kind": "quality", "covered": 2, "total": 2, "failing": ["FRAC3"] })
    );
    // As duas tiveram lucro nos 5 anos, caixa operacional e margem positivos.
    for id in ["quality_profit", "quality_cash", "quality_margin"] {
        let r = rule(&body, id);
        assert_eq!(r["status"], "ok", "{id}: {r}");
        assert_eq!(r["current"], 100.0, "{id}");
    }
    // Dívida líquida/EBITDA: 0,27 e 22,3 (limite 3x): só FORT3 passa.
    let leverage = rule(&body, "quality_leverage");
    assert_eq!(leverage["status"], "warning");
    close(&leverage["current"], 66.67);
    assert_eq!(leverage["detail"]["failing"], json!(["FRAC3"]));
    // Recuperação judicial: nomeia a empresa, sem limite editável.
    let recovery = rule(&body, "quality_recovery");
    assert_eq!(recovery["status"], "warning");
    assert_eq!(recovery["current"], 1.0);
    assert_eq!(recovery["unit"], "count");
    assert_eq!(recovery["threshold"], Value::Null);
    assert_eq!(recovery["detail"]["failing"], json!(["FRAC3"]));
    // FIIs: o de tijolo está a 1,42 do patrimônio (acima de 1,10), o de papel a
    // 0,84; pelo valor (1.420 e 83,70) só 5,6% passam. A vacância só existe no
    // de tijolo (5%, dentro dos 15%): o de papel fica fora da conta.
    let pvp = rule(&body, "quality_fii_pvp");
    assert_eq!(pvp["status"], "warning");
    close(&pvp["current"], 5.57);
    assert_eq!(pvp["detail"]["failing"], json!(["KNRI11"]));
    let vacancy = rule(&body, "quality_fii_vacancy");
    assert_eq!(vacancy["status"], "ok");
    assert_eq!(vacancy["current"], 100.0);
    assert_eq!(
        vacancy["detail"],
        json!({ "kind": "quality", "covered": 1, "total": 2, "failing": [] })
    );

    // A pontuação da Qualidade: 4 ok (lucro, caixa, margem, vacância) de 8.
    let quality = find(&body["dimensions"], "id", "quality");
    assert_eq!(quality["rules"].as_array().unwrap().len(), 8);
    assert_eq!(
        quality["score"],
        json!({ "ok": 4, "measured": 8, "no_data": 0, "percent": 50.0 })
    );

    // Pontos de atenção: só a FRAC3, factuais; a FORT3 está de fora da lista.
    let attention = &body["attention"];
    assert_eq!(attention["assessed"], 4);
    assert_eq!(attention["without_data"], 0);
    let items = attention["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{attention}");
    assert_eq!(items[0]["label"], "FRAC3");
    let kinds: Vec<&str> = items[0]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "judicial_recovery",
            "low_roe",
            "low_net_margin",
            "debt_above_equity",
            "low_revenue_growth",
            "low_earnings_growth"
        ]
    );
    let point = |kind: &str| {
        items[0]["points"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["kind"] == kind)
            .unwrap()
    };
    close(&point("low_roe")["percent"], 2.0);
    close(&point("low_net_margin")["percent"], 2.0);
    close(&point("debt_above_equity")["times"], 2.9);
    close(&point("low_revenue_growth")["percent"], 0.0);

    // Graham: lucro por ação 1,80 e patrimônio por ação 10 => √(22,5 × 1,8 × 10).
    let references = body["references"]["items"].as_array().unwrap();
    let fort_ref = references.iter().find(|r| r["label"] == "FORT3").unwrap();
    close(&fort_ref["graham"], 20.12);
    assert_eq!(fort_ref["bazin"], Value::Null);
    // Só o Graham: o teto pelas duas é ele.
    close(&fort_ref["ceiling"], 20.12);
    close(&fort_ref["unit_price"], 40.0);
    let frac_ref = references.iter().find(|r| r["label"] == "FRAC3").unwrap();
    close(&frac_ref["graham"], 6.71);
    // O botão de aplicar como teto é o PATCH do ativo, e o teto volta na análise.
    let applied = patch(
        &app.router,
        &format!("/api/v1/assets/{fort}"),
        json!({ "max_price": 20.12 }),
    )
    .await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text);
    let body = analysis(&app).await;
    let fort_ref = body["references"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["label"] == "FORT3")
        .unwrap()
        .clone();
    close(&fort_ref["max_price"], 20.12);

    // O limite é da pessoa: com 60%, o ROE (66,67%) passa; a regra sem limite não aceita um.
    let tuned = put(
        &app.router,
        "/api/v1/analysis/rules",
        json!({ "rules": { "quality_roe": { "threshold": 60 }, "quality_cash": { "enabled": false } } }),
    )
    .await;
    assert_eq!(tuned.status, StatusCode::OK, "{}", tuned.text);
    let body = analysis(&app).await;
    assert_eq!(rule(&body, "quality_roe")["status"], "ok");
    assert_eq!(rule(&body, "quality_cash")["status"], "off");
    for bad in [
        json!({ "rules": { "quality_recovery": { "threshold": 1 } } }),
        json!({ "rules": { "quality_roe": { "threshold": 0 } } }),
        json!({ "rules": { "quality_fii_pvp": { "threshold": 120 } } }),
    ] {
        let reply = put(&app.router, "/api/v1/analysis/rules", bad.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    // Quem não tem esses ativos não vê nada deles: nenhum fundamento, tudo sem dado.
    let other = app_for(&db, &dir, "qualidade-outra@prumo.test");
    let empty = analysis(&other).await;
    let quality = find(&empty["dimensions"], "id", "quality");
    assert!(quality["rules"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "no_data"));
    assert_eq!(
        empty["attention"],
        json!({ "assessed": 0, "without_data": 0, "items": [] })
    );
    assert_eq!(empty["references"]["items"], json!([]));
}

fn app_for(db: &Database, dir: &tempfile_dir::Dir, email: &str) -> TestApp {
    app(&db.store, email, quotes(), &dir.path())
}

/// Ação ou FII que ainda não tem fundamentos aparece só como contagem; nunca
/// como ponto de atenção nem como regra reprovada.
#[tokio::test]
async fn assets_without_fundamentals_are_counted_not_blamed() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let app = app_for(&db, &dir, "sem-fundamentos@prumo.test");
    add_asset(&app, "WEGE3", "br_stocks").await;
    add_asset(&app, "KNRI11", "br_reits").await;
    let body = analysis(&app).await;
    // Nenhuma busca da CVM aconteceu: os dois estão "sem dados ainda".
    assert_eq!(
        body["attention"],
        json!({ "assessed": 0, "without_data": 2, "items": [] })
    );
    let quality = find(&body["dimensions"], "id", "quality");
    assert_eq!(
        quality["score"],
        json!({ "ok": 0, "measured": 0, "no_data": 8, "percent": null })
    );
    assert!(quality["rules"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "no_data"));
    assert_eq!(body["attention_count"], rule_warnings(&body));
}

fn rule_warnings(body: &Value) -> usize {
    body["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["rules"].as_array().unwrap())
        .filter(|r| r["status"] == "warning")
        .count()
}
