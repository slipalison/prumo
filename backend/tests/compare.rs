//! Comparador contra um Postgres de verdade: a escolha na URL, a tabela com os
//! fundamentos que já estão guardados (e o "sem fundamentos" do ativo de fora),
//! o destaque neutro, o preço histórico buscado na hora e guardado (com o
//! desdobramento desfeito), os proventos reinvestidos, o início igualado e os
//! índices. Carteira, companhias e fundos fictícios.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::dividends::{DividendError, DividendEvent, DividendHistory, DividendProvider};
use prumo::domain::fundamentals::{
    Column, Figure, Filing, FilingKind, FundReport, Line, Registration, Scope, Statement, Vacancy,
};
use prumo::domain::performance::months_before;
use prumo::fundamentals::b3::IssuerInfo;
use prumo::fundamentals::download::Validators;
use prumo::history::{Chart, HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::local_date;
use prumo::store::fundamentals::FundVacancy;
use prumo::store::Store;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::macros::{date, datetime};
use time::{Date, Duration, Month, OffsetDateTime};

const NOW: OffsetDateTime = datetime!(2026-11-26 15:00 UTC);

// ------------------------------------------------------------ fontes falsas --

/// Séries em memória; guarda cada pedido para o teste conferir o cache.
#[derive(Default)]
struct FakeHistory {
    closes: Mutex<HashMap<String, Series>>,
    splits: Mutex<HashMap<String, Vec<(Date, Decimal)>>>,
    /// O volume financeiro de cada pregão, por símbolo.
    traded: Mutex<HashMap<String, Series>>,
    indexes: Mutex<HashMap<IndexCode, Series>>,
    requests: Mutex<Vec<String>>,
}

impl FakeHistory {
    fn set(&self, symbol: &str, series: Series) {
        self.closes
            .lock()
            .unwrap()
            .insert(symbol.to_owned(), series);
    }

    fn calls(&self, symbol: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.as_str() == symbol)
            .count()
    }
}

fn window(series: Option<&Series>, from: Date, to: Date) -> Series {
    series
        .map(|s| {
            s.iter()
                .filter(|(d, _)| *d >= from && *d <= to)
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait]
impl HistoryProvider for FakeHistory {
    async fn index(&self, code: IndexCode, from: Date, to: Date) -> Result<Series, HistoryError> {
        self.requests.lock().unwrap().push(code.as_str().to_owned());
        Ok(window(self.indexes.lock().unwrap().get(&code), from, to))
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        self.requests.lock().unwrap().push(symbol.to_owned());
        Ok(window(self.closes.lock().unwrap().get(symbol), from, to))
    }

    async fn closes_and_splits(
        &self,
        symbol: &str,
        from: Date,
        to: Date,
    ) -> Result<(Series, Vec<(Date, Decimal)>), HistoryError> {
        let closes = self.closes(symbol, from, to).await?;
        let splits = self
            .splits
            .lock()
            .unwrap()
            .get(symbol)
            .cloned()
            .unwrap_or_default();
        Ok((closes, splits))
    }

    async fn chart(&self, symbol: &str, from: Date, to: Date) -> Result<Chart, HistoryError> {
        let (closes, splits) = self.closes_and_splits(symbol, from, to).await?;
        let traded = window(self.traded.lock().unwrap().get(symbol), from, to);
        Ok(Chart {
            closes,
            splits,
            traded,
        })
    }
}

/// O histórico de proventos do Yahoo por símbolo.
#[derive(Default)]
struct FakeDividends {
    histories: Mutex<HashMap<String, DividendHistory>>,
}

impl FakeDividends {
    fn set(&self, symbol: &str, currency: &str, events: Vec<(Date, Decimal)>) {
        self.histories.lock().unwrap().insert(
            symbol.to_owned(),
            DividendHistory {
                currency: currency.to_owned(),
                events: events
                    .into_iter()
                    .map(|(ex_date, amount)| DividendEvent { ex_date, amount })
                    .collect(),
            },
        );
    }
}

#[async_trait]
impl DividendProvider for FakeDividends {
    async fn history(&self, provider_symbol: &str) -> Result<DividendHistory, DividendError> {
        self.histories
            .lock()
            .unwrap()
            .get(provider_symbol)
            .cloned()
            .ok_or(DividendError::NotFound)
    }
}

// ----------------------------------------------------- fundamentos da CVM --

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

fn registration(cvm_code: i32, name: &str) -> Registration {
    Registration {
        cvm_code,
        cnpj: format!("00.000.000/{cvm_code:04}-00"),
        name: name.to_owned(),
        status: "ATIVO".to_owned(),
        issuer_status: Some("FASE OPERACIONAL".to_owned()),
        sector: None,
    }
}

/// Duas companhias (`FORT3`: lucro de 180 e patrimônio de 1.000 em 100 ações;
/// `FRAC3`: lucro de 20) e dois FIIs (`KNRI11`, valor patrimonial de 100 por
/// cota e 5% de vacância; `MXRF11`, de 10 por cota, sem imóvel).
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
                registration(91001, "FORTE SA"),
                registration(91002, "FRACA SA"),
            ],
            &v,
            NOW,
        )
        .await
        .unwrap();
    for (issuer, code) in [("FORT", 91001), ("FRAC", 91002)] {
        store
            .save_issuer_code(
                issuer,
                &IssuerInfo {
                    cvm_code: Some(code),
                    total_shares: Some(dec!(100)),
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
        // Caixa de 10% do ativo total (fictício).
        total_assets: Some(dec!(1250000)),
        liquid_assets: Some(dec!(125000)),
    };
    store
        .save_fund_reports(
            "fund_monthly:2026",
            &[
                report("11.111.111/0001-11", "KNRI", "BRKNRICTF007", 10_000),
                report("22.222.222/0001-22", "MXRF", "BRMXRFCTF004", 100_000),
            ],
            &v,
            NOW,
        )
        .await
        .unwrap();
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

fn quotes() -> Arc<FakeQuotes> {
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

// ----------------------------------------------------------------- ajudas --

struct Env {
    router: axum::Router,
    history: Arc<FakeHistory>,
    dividends: Arc<FakeDividends>,
}

fn env(db: &Database, email: &str, dir: &tempfile_dir::Dir) -> Env {
    let history = Arc::new(FakeHistory::default());
    let dividends = Arc::new(FakeDividends::default());
    let base = app(&db.store, email, quotes(), &dir.path());
    let state = base
        .state
        .clone()
        .with_history_provider(history.clone())
        .with_dividend_provider(dividends.clone());
    Env {
        router: prumo::http::public_router(state, &dir.path()),
        history,
        dividends,
    }
}

async fn table(router: &axum::Router, assets: &str) -> Value {
    let reply = get(router, &format!("/api/v1/compare?assets={assets}")).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply.body
}

async fn returns(router: &axum::Router, query: &str) -> Value {
    let reply = get(router, &format!("/api/v1/compare/returns?{query}")).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    reply.body
}

fn row<'a>(body: &'a Value, id: &str) -> &'a Value {
    find(&body["rows"], "id", id)
}

fn has_row(body: &Value, id: &str) -> bool {
    body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == id)
}

fn cell<'a>(body: &'a Value, id: &str, index: usize) -> &'a Value {
    &row(body, id)["cells"][index]
}

fn close(value: &Value, expected: f64) {
    let got = value
        .as_f64()
        .unwrap_or_else(|| panic!("{value} não é número"));
    assert!((got - expected).abs() < 0.0051, "{got} != {expected}");
}

fn marks(body: &Value, id: &str) -> Vec<Value> {
    row(body, id)["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["mark"].clone())
        .collect()
}

fn series<'a>(body: &'a Value, key: &str) -> &'a Value {
    find(&body["series"], "key", key)
}

async fn add_asset(router: &axum::Router, symbol: &str, class: &str) {
    let created = post(
        router,
        "/api/v1/assets",
        json!({ "asset_class": class, "symbol": symbol, "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
}

// ------------------------------------------------------------------ teste --

#[tokio::test]
async fn comparator_end_to_end() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    seed_fundamentals(&db.store).await;

    rejects_what_cannot_be_compared(&db, &dir).await;
    side_by_side_table_with_neutral_marks(&db, &dir).await;
    fund_rows_show_cash_liquidity_and_growth(&db, &dir).await;
    a_holding_indicator_does_not_apply_and_gets_no_mark(&db, &dir).await;
    returns_undo_splits_reinvest_dividends_and_align_the_start(&db, &dir).await;
    each_person_has_their_own_portfolio(&db, &dir).await;
}

async fn rejects_what_cannot_be_compared(db: &Database, dir: &tempfile_dir::Dir) {
    let env = env(db, "valida@prumo.test", dir);
    for (query, why) in [
        ("/api/v1/compare", "sem ativo"),
        ("/api/v1/compare?assets=", "lista vazia"),
        ("/api/v1/compare?assets=A1,B2,C3,D4,E5,F6", "seis ativos"),
        ("/api/v1/compare?assets=AAPL:nada", "classe desconhecida"),
        ("/api/v1/compare?assets=AAPL:fixed_income", "renda fixa"),
        ("/api/v1/compare?assets=a%20b", "símbolo inválido"),
        (
            "/api/v1/compare/returns?assets=AAPL&period=2y",
            "período desconhecido",
        ),
        (
            "/api/v1/compare/returns?assets=AAPL&benchmarks=ipca",
            "índice fora do comparador",
        ),
    ] {
        let reply = get(&env.router, query).await;
        assert_eq!(
            reply.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            reply.text
        );
        assert_eq!(reply.body["code"], "validation_failed", "{why}");
    }
}

async fn side_by_side_table_with_neutral_marks(db: &Database, dir: &tempfile_dir::Dir) {
    let env = env(db, "tabela@prumo.test", dir);
    let today = local_date(OffsetDateTime::now_utc());

    // Proventos brutos por cota: FORT3 2,5%, FRAC3 5%, KNRI11 1,4%, MXRF11 14,3%.
    env.dividends.set(
        "FORT3.SA",
        "BRL",
        vec![(today - Duration::days(100), dec!(1))],
    );
    env.dividends.set(
        "FRAC3.SA",
        "BRL",
        vec![(today - Duration::days(30), dec!(1))],
    );
    env.dividends.set(
        "KNRI11.SA",
        "BRL",
        vec![
            (today - Duration::days(60), dec!(1)),
            (today - Duration::days(30), dec!(1)),
            (today - Duration::days(500), dec!(9)),
        ],
    );
    env.dividends.set(
        "MXRF11.SA",
        "BRL",
        (1..=12)
            .map(|n| (today - Duration::days(25 * n), dec!(0.1)))
            .collect(),
    );
    add_asset(&env.router, "KNRI11", "br_reits").await;
    // KNRI11 subiu de 100 para 120 em 12 meses.
    env.history.set(
        "KNRI11.SA",
        vec![
            (today - Duration::days(380), dec!(100)),
            (today - Duration::days(2), dec!(120)),
        ],
    );

    let body = table(
        &env.router,
        "FORT3:br_stocks,FRAC3:br_stocks,KNRI11,MXRF11:br_reits,AAPL:intl_stocks",
    )
    .await;

    let assets = body["assets"].as_array().unwrap();
    let flags: Vec<(&str, bool, &str)> = assets
        .iter()
        .map(|a| {
            (
                a["symbol"].as_str().unwrap(),
                a["in_portfolio"].as_bool().unwrap(),
                a["family"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        flags,
        vec![
            ("FORT3", false, "company"),
            ("FRAC3", false, "company"),
            ("KNRI11", true, "fund"),
            ("MXRF11", false, "fund"),
            ("AAPL", false, "company"),
        ]
    );
    // O ativo de fora ganha o nome e o preço do provedor; o dólar fica em dólar.
    assert_eq!(assets[0]["name"], "Forte");
    close(&assets[0]["price"], 40.0);
    assert_eq!(assets[4]["currency"], "USD");
    close(&assets[4]["price"], 200.0);

    // Múltiplos: preço sobre o lucro (1,80) e sobre o patrimônio (10) por ação.
    close(&cell(&body, "price_to_earnings", 0)["value"], 22.2222);
    close(&cell(&body, "price_to_earnings", 1)["value"], 100.0);
    close(&cell(&body, "price_to_book", 0)["value"], 4.0);
    close(&cell(&body, "price_to_book", 1)["value"], 2.0);
    // FII: preço sobre o valor patrimonial por cota (100 e 10).
    close(&cell(&body, "price_to_book", 2)["value"], 1.42);
    close(&cell(&body, "price_to_book", 3)["value"], 0.837);
    close(&cell(&body, "nav_per_share", 2)["value"], 100.0);
    close(&cell(&body, "vacancy", 2)["value"], 5.0);
    close(&cell(&body, "roe", 0)["value"], 18.0);
    close(&cell(&body, "roe", 1)["value"], 2.0);
    close(&cell(&body, "market_cap", 0)["value"], 4000.0);

    // Destaque neutro: o menor múltiplo e o maior ROE, só entre empresas, e o
    // menor P/VP só entre fundos. Linha sem direção não marca.
    let none = Value::Null;
    assert_eq!(
        marks(&body, "price_to_earnings"),
        vec![
            json!("min"),
            none.clone(),
            none.clone(),
            none.clone(),
            none.clone()
        ]
    );
    assert_eq!(
        marks(&body, "price_to_book"),
        vec![
            none.clone(),
            json!("min"),
            none.clone(),
            json!("min"),
            none.clone()
        ]
    );
    assert_eq!(
        marks(&body, "roe"),
        vec![
            json!("max"),
            none.clone(),
            none.clone(),
            none.clone(),
            none.clone()
        ]
    );
    assert_eq!(row(&body, "roe")["direction"], "higher");
    assert_eq!(row(&body, "price_to_earnings")["direction"], "lower");
    assert_eq!(row(&body, "price")["direction"], "none");
    assert_eq!(marks(&body, "price"), vec![none.clone(); 5]);
    // Rendimento de 12 meses: FRAC3 (5%) entre as empresas; MXRF11 entre os
    // fundos; o provento de 500 dias atrás não conta.
    close(&cell(&body, "dividend_yield_12m", 0)["value"], 2.5);
    close(&cell(&body, "dividend_yield_12m", 1)["value"], 5.0);
    close(&cell(&body, "dividend_yield_12m", 2)["value"], 1.4085);
    close(&cell(&body, "dividend_yield_12m", 3)["value"], 14.3369);
    assert_eq!(
        marks(&body, "dividend_yield_12m"),
        vec![
            none.clone(),
            json!("max"),
            none.clone(),
            json!("max"),
            none.clone()
        ]
    );
    // AAPL não tem histórico de proventos no provedor: o motivo, não um zero.
    assert_eq!(
        cell(&body, "dividend_yield_12m", 4)["reason"],
        "no_dividend_history"
    );
    assert!(cell(&body, "dividend_yield_12m", 4)["value"].is_null());

    // Retorno de 12 meses só do preço (guardado na hora para o ativo de fora).
    close(&cell(&body, "return_12m", 2)["value"], 20.0);
    assert_eq!(cell(&body, "return_12m", 0)["reason"], "no_price_history");

    // O que não existe para o tipo diz "não se aplica"; ativo de fora sem
    // balanço guardado diz por quê, sem disparar a busca.
    assert_eq!(
        cell(&body, "price_to_earnings", 2)["reason"],
        "not_applicable"
    );
    assert_eq!(cell(&body, "vacancy", 0)["reason"], "not_applicable");
    assert_eq!(cell(&body, "roe", 4)["reason"], "not_listed_on_b3");
    assert_eq!(cell(&body, "vacancy", 3)["reason"], "no_quarterly_report");
    assert_eq!(cell(&body, "segment", 4)["reason"], "no_sector_source");

    // O ativo de fora que a CVM nunca viu: "não acompanhado", e a linha de
    // empresa continua para as outras.
    let lone = table(&env.router, "ZZZZ3:br_stocks,FORT3:br_stocks").await;
    assert_eq!(cell(&lone, "price_to_earnings", 0)["reason"], "not_tracked");
    close(&cell(&lone, "price_to_earnings", 1)["value"], 22.2222);
    // Uma empresa sozinha não é extremo de nada.
    assert_eq!(
        marks(&lone, "price_to_earnings"),
        vec![Value::Null, Value::Null]
    );

    // Sem nenhum fundo na escolha, as linhas de fundo nem aparecem.
    let only = table(&env.router, "FORT3:br_stocks").await;
    assert!(has_row(&only, "price_to_earnings"));
    assert!(!has_row(&only, "vacancy"));
    assert!(!has_row(&only, "nav_per_share"));
}

/// Os dias úteis de `from` a `to`.
fn weekdays(from: Date, to: Date) -> Vec<Date> {
    let mut out = Vec::new();
    let mut day = from;
    while day <= to {
        if !matches!(
            day.weekday(),
            time::Weekday::Saturday | time::Weekday::Sunday
        ) {
            out.push(day);
        }
        day = day.next_day().unwrap();
    }
    out
}

/// FII completo no comparador: o caixa do informe, a liquidez dos últimos
/// pregões e os CAGRs de proventos e da cota (dois FIIs fictícios de fora da
/// carteira). O caixa não tem destaque; a liquidez e os CAGRs marcam o maior,
/// só com dois valores conhecidos.
async fn fund_rows_show_cash_liquidity_and_growth(db: &Database, dir: &tempfile_dir::Dir) {
    let env = env(db, "fii-completo@prumo.test", dir);
    let today = local_date(OffsetDateTime::now_utc());
    // Dois fundos fictícios no informe mensal, com 10% do ativo em caixa.
    let report = |cnpj: &str, issuer: &str, isin: &str| FundReport {
        cnpj: cnpj.to_owned(),
        issuer: issuer.to_owned(),
        isin: isin.to_owned(),
        name: format!("FUNDO {issuer}"),
        reference_date: date!(2026 - 10 - 31),
        version: 1,
        net_assets: Some(dec!(1000000)),
        shares: Some(dec!(10000)),
        total_assets: Some(dec!(1250000)),
        liquid_assets: Some(dec!(125000)),
    };
    db.store
        .save_fund_reports(
            "fund_monthly:2026",
            &[
                report("33.333.333/0001-33", "FUNA", "BRFUNACTF001"),
                report("44.444.444/0001-44", "FUNB", "BRFUNBCTF002"),
            ],
            &Validators::default(),
            NOW,
        )
        .await
        .unwrap();
    let yesterday = today - Duration::days(1);
    let base5 = months_before(today, 60) - Duration::days(3);
    let base3 = months_before(today, 36) - Duration::days(3);
    // FUNA11: 100 há cinco anos, 110 há três, 133,10 nos últimos 40 pregões
    // (com R$ 1 milhão por pregão); FUNB11: 100 pregões a 10, R$ 4 milhões.
    let recent = weekdays(today - Duration::days(70), yesterday);
    let knri: Vec<Date> = recent[recent.len() - 40..].to_vec();
    let mut closes = vec![(base5, dec!(100)), (base3, dec!(110))];
    closes.extend(knri.iter().map(|d| (*d, dec!(133.1))));
    env.history.set("FUNA11.SA", closes);
    env.history.traded.lock().unwrap().insert(
        "FUNA11.SA".into(),
        knri.iter().map(|d| (*d, dec!(1000000))).collect(),
    );
    let long = weekdays(today - Duration::days(170), yesterday);
    let mxrf: Vec<Date> = long[long.len() - 100..].to_vec();
    env.history
        .set("FUNB11.SA", mxrf.iter().map(|d| (*d, dec!(10))).collect());
    env.history.traded.lock().unwrap().insert(
        "FUNB11.SA".into(),
        mxrf.iter().map(|d| (*d, dec!(4000000))).collect(),
    );
    // FUNA11 paga todo mês há sete anos: R$ 0,80 até três anos atrás, R$ 1
    // depois. FUNB11, sem provento conhecido.
    let paid: Vec<(Date, Decimal)> = (0..84)
        .map(|k| {
            let day = months_before(today, k) - Duration::days(5);
            (day, if k >= 36 { dec!(0.8) } else { dec!(1) })
        })
        .collect();
    env.dividends.set("FUNA11.SA", "BRL", paid);

    let body = table(&env.router, "FUNA11:br_reits,FUNB11:br_reits").await;
    let none = Value::Null;
    // Caixa de 10% do ativo nos dois (informe fictício), sem destaque.
    close(&cell(&body, "cash_share", 0)["value"], 10.0);
    close(&cell(&body, "cash_share", 1)["value"], 10.0);
    assert_eq!(row(&body, "cash_share")["direction"], "none");
    assert_eq!(marks(&body, "cash_share"), vec![none.clone(), none.clone()]);
    // Liquidez: 30 pregões nos dois; 90 só no FUNB11 (o FUNA11 tem 40).
    close(&cell(&body, "liquidity_30d", 0)["value"], 1_000_000.0);
    close(&cell(&body, "liquidity_30d", 1)["value"], 4_000_000.0);
    assert_eq!(row(&body, "liquidity_30d")["unit"], "money_compact");
    assert_eq!(
        marks(&body, "liquidity_30d"),
        vec![none.clone(), json!("max")]
    );
    assert_eq!(
        cell(&body, "liquidity_90d", 0)["reason"],
        "insufficient_history"
    );
    close(&cell(&body, "liquidity_90d", 1)["value"], 4_000_000.0);
    assert_eq!(
        marks(&body, "liquidity_90d"),
        vec![none.clone(), none.clone()]
    );
    // CAGR da cota: (133,1 / 110)^(1/3) − 1 e (133,1 / 100)^(1/5) − 1; o FUNB11
    // não tem cinco anos de preço.
    close(
        &cell(&body, "price_cagr_3y", 0)["value"],
        ((133.1_f64 / 110.0).powf(1.0 / 3.0) - 1.0) * 100.0,
    );
    close(
        &cell(&body, "price_cagr_5y", 0)["value"],
        ((133.1_f64 / 100.0).powf(1.0 / 5.0) - 1.0) * 100.0,
    );
    assert_eq!(
        cell(&body, "price_cagr_5y", 1)["reason"],
        "insufficient_history"
    );
    // CAGR dos proventos: 12 meses de R$ 1 sobre 12 de R$ 0,80.
    close(
        &cell(&body, "dividend_cagr_3y", 0)["value"],
        ((12.0_f64 / 9.6).powf(1.0 / 3.0) - 1.0) * 100.0,
    );
    close(
        &cell(&body, "dividend_cagr_5y", 0)["value"],
        ((12.0_f64 / 9.6).powf(1.0 / 5.0) - 1.0) * 100.0,
    );
    assert_eq!(
        cell(&body, "dividend_cagr_3y", 1)["reason"],
        "insufficient_history"
    );
    // As taxas de mercado vêm junto (só contexto); sem CDI guardado, nulas.
    assert!(body["rates"].is_object());
    assert!(body["rates"]["cdi_12m"].is_null());
    // Ação não tem linha de fundo: "não se aplica".
    let mixed = table(&env.router, "FORT3:br_stocks,FUNA11:br_reits").await;
    assert_eq!(cell(&mixed, "liquidity_30d", 0)["reason"], "not_applicable");
    close(&cell(&mixed, "liquidity_30d", 1)["value"], 1_000_000.0);
}

/// Uma holding de participações (setor "Emp. Adm. Part." no cadastro e a
/// equivalência patrimonial maior que o lucro) ao lado de uma indústria: a
/// margem e a dívida sobre EBITDA dela vêm como "não se aplica" (`holding`),
/// com a mesma regra dos fundamentos, e o destaque da linha não a considera.
async fn a_holding_indicator_does_not_apply_and_gets_no_mark(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let v = Validators::default();
    let scanned: BTreeSet<i32> = [91001, 91002, 91003].into_iter().collect();
    for year in 2021..=2025 {
        let mut holding = dfp(
            91003,
            "HOLDING SA",
            year,
            Year {
                revenue: 100,
                profit: 300,
                equity: 2000,
                debt: 100,
                cash: 50,
            },
        );
        holding.lines.push(flow(
            Statement::Income,
            year,
            "3.04.06",
            "Resultado de Equivalência Patrimonial",
            360,
        ));
        db.store
            .save_company_filings(&format!("annual:{year}"), &[holding], &v, &scanned, NOW)
            .await
            .unwrap();
    }
    let mut holding = registration(91003, "HOLDING SA");
    holding.sector = Some("Emp. Adm. Part. - Sem Setor Principal".to_owned());
    db.store
        .save_registry("registry", &[holding], &v, NOW)
        .await
        .unwrap();
    db.store
        .save_issuer_code(
            "HOLD",
            &IssuerInfo {
                cvm_code: Some(91003),
                total_shares: Some(dec!(100)),
            },
            NOW,
        )
        .await
        .unwrap();
    let env = env(db, "holding@prumo.test", dir);
    let body = table(
        &env.router,
        "HOLD3:br_stocks,FORT3:br_stocks,FRAC3:br_stocks",
    )
    .await;
    for id in [
        "net_margin",
        "ebitda_margin",
        "net_debt_to_ebitda",
        "revenue_cagr",
    ] {
        let held = cell(&body, id, 0);
        assert!(held["value"].is_null(), "{id}: {held}");
        assert_eq!(held["reason"], "holding", "{id}");
        assert!(held["mark"].is_null(), "{id}");
    }
    // A margem EBITDA da holding (EBIT de 450 sobre receita de 100) passaria
    // de 500% e seria o "maior da linha"; fora dela, o destaque é entre as duas.
    let ebitda = marks(&body, "ebitda_margin");
    assert!(ebitda[0].is_null());
    assert_eq!(ebitda.iter().filter(|m| !m.is_null()).count(), 1);
    // O que vale para holding continua com número e entra no destaque.
    assert!(cell(&body, "roe", 0)["value"].is_number());
    assert!(cell(&body, "net_debt_to_equity", 0)["value"].is_number());
}

async fn returns_undo_splits_reinvest_dividends_and_align_the_start(
    db: &Database,
    dir: &tempfile_dir::Dir,
) {
    let env = env(db, "curvas@prumo.test", dir);
    let today = local_date(OffsetDateTime::now_utc());
    let at = |days: i64| today - Duration::days(days);
    env.dividends.set("NVDA", "USD", vec![(at(60), dec!(0.5))]);
    add_asset(&env.router, "HGLG11", "br_reits").await;

    // NVDA desdobrou 2 para 1 há 40 dias: o fechamento real de antes é o
    // dobro. O dólar fica em R$ 5 o tempo todo.
    env.history.set(
        "NVDA",
        vec![
            (at(80), dec!(100)),
            (at(60), dec!(110)),
            (at(40), dec!(57.75)),
        ],
    );
    env.history
        .splits
        .lock()
        .unwrap()
        .insert("NVDA".to_owned(), vec![(at(40), dec!(2))]);
    env.history.set("USDBRL=X", vec![(at(85), dec!(5))]);
    env.history
        .set("HGLG11.SA", vec![(at(70), dec!(100)), (at(10), dec!(110))]);
    env.history.indexes.lock().unwrap().insert(
        IndexCode::Cdi,
        (0..=120).map(|n| (at(n), dec!(0.05))).collect(),
    );

    // Só o preço: 50 → 57,75 na base de hoje, +15,5% (sem o desdobramento
    // desfeito seria −42%).
    let price = returns(
        &env.router,
        "assets=NVDA:intl_stocks&period=3m&benchmarks=cdi&portfolio=false",
    )
    .await;
    assert_eq!(price["period"], "3m");
    assert_eq!(price["total_return"], false);
    let nvda = series(&price, "NVDA:intl_stocks");
    assert_eq!(nvda["kind"], "asset");
    close(&nvda["return_percent"], 15.5);
    assert_eq!(nvda["starts_on"], at(80).to_string());
    assert_eq!(nvda["converted_to_brl"], true);
    assert_eq!(nvda["includes_income"], false);
    assert!(nvda["reason"].is_null());
    // A curva abre em 0% onde começa e fica nulo antes.
    let dates = price["dates"].as_array().unwrap();
    let values = nvda["values"].as_array().unwrap();
    assert_eq!(dates.len(), values.len());
    let open = dates
        .iter()
        .position(|d| *d == json!(at(80).to_string()))
        .unwrap();
    assert!(values[..open].iter().all(Value::is_null));
    close(&values[open], 0.0);
    // O CDI do período está lá e tem retorno positivo.
    let cdi = series(&price, "cdi");
    assert_eq!(cdi["kind"], "index");
    assert!(cdi["return_percent"].as_f64().unwrap() > 3.0);
    assert!(price["series"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["kind"] != "portfolio"));

    // O preço foi guardado (e os desdobramentos): o segundo pedido não vai à fonte.
    let before = env.history.calls("NVDA");
    assert_eq!(before, 1);
    let again = returns(
        &env.router,
        "assets=NVDA:intl_stocks&period=3m&benchmarks=cdi&portfolio=false",
    )
    .await;
    assert_eq!(env.history.calls("NVDA"), before);
    close(&series(&again, "NVDA:intl_stocks")["return_percent"], 15.5);
    // Um período mais longo pede o que falta para trás, uma vez.
    returns(
        &env.router,
        "assets=NVDA:intl_stocks&period=5y&benchmarks=cdi&portfolio=false",
    )
    .await;
    assert_eq!(env.history.calls("NVDA"), before + 1);

    // Com proventos: US$ 0,50 com data ex há 60 dias, reinvestido no preço do dia.
    let total = returns(
        &env.router,
        "assets=NVDA:intl_stocks&period=3m&benchmarks=cdi&portfolio=false&total_return=true",
    )
    .await;
    let nvda = series(&total, "NVDA:intl_stocks");
    close(&nvda["return_percent"], 16.55);
    assert_eq!(nvda["includes_income"], true);
    assert_eq!(total["total_return"], true);

    // Igualar o início: todas partem da data em que o último começa (NVDA, há
    // 80 dias, depois do preço de base do HGLG11).
    let aligned = returns(
        &env.router,
        "assets=NVDA:intl_stocks,HGLG11&period=3m&benchmarks=cdi&portfolio=true&align_start=true",
    )
    .await;
    assert_eq!(aligned["align_start"], true);
    let starts = aligned["from"].as_str().unwrap().to_owned();
    assert!(starts >= at(80).to_string());
    for s in aligned["series"].as_array().unwrap() {
        if s["reason"].is_null() {
            assert!(s["starts_on"].as_str().unwrap() >= starts.as_str(), "{s}");
        }
    }
    let first_axis = aligned["dates"][0].as_str().unwrap();
    assert_eq!(first_axis, starts);
    let knri = series(&aligned, "HGLG11");
    close(&knri["values"][0], 0.0);
    // Sem igualar, o ativo que começa antes mantém o próprio começo.
    let loose = returns(
        &env.router,
        "assets=NVDA:intl_stocks,HGLG11&period=3m&benchmarks=cdi&portfolio=false",
    )
    .await;
    assert!(loose["from"].as_str().unwrap() < starts.as_str());

    // A carteira entra como mais uma curva, com os proventos dentro.
    let wallet = series(&aligned, "portfolio");
    assert_eq!(wallet["kind"], "portfolio");
    assert_eq!(wallet["includes_income"], true);

    // Sem preço no provedor: a curva diz por quê, as outras seguem.
    let missing = returns(
        &env.router,
        "assets=NADA3:br_stocks,NVDA:intl_stocks&period=3m&benchmarks=&portfolio=false",
    )
    .await;
    assert_eq!(
        series(&missing, "NADA3:br_stocks")["reason"],
        "no_price_history"
    );
    assert!(series(&missing, "NADA3:br_stocks")["return_percent"].is_null());
    assert!(series(&missing, "NVDA:intl_stocks")["reason"].is_null());
    assert_eq!(missing["series"].as_array().unwrap().len(), 2);
}

async fn each_person_has_their_own_portfolio(db: &Database, dir: &tempfile_dir::Dir) {
    let mine = env(db, "dono@prumo.test", dir);
    let other = env(db, "outra@prumo.test", dir);
    add_asset(&mine.router, "KNRI11", "br_reits").await;

    let owner = table(&mine.router, "KNRI11").await;
    assert_eq!(owner["assets"][0]["in_portfolio"], true);
    let stranger = table(&other.router, "KNRI11").await;
    assert_eq!(stranger["assets"][0]["in_portfolio"], false);
    // O palpite do endereço digitado: ticker terminado em 11 é fundo.
    assert_eq!(stranger["assets"][0]["asset_class"], "br_reits");
}
