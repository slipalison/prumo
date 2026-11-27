//! FII completo contra um Postgres de verdade: a sincronia de preços grava o
//! volume financeiro pelo caminho de sempre (`record_prices`) e, para o
//! símbolo guardado antes do volume, pede a janela da liquidez uma vez, no
//! pedido de sempre; a folha (fundamentos), a Análise e a escolha do teto
//! mostram o caixa, a liquidez e os CAGRs, com as taxas ao lado. Fundo e
//! carteira fictícios.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::domain::fund_market::TRADED_LOOKBACK_DAYS;
use prumo::domain::fundamentals::FundReport;
use prumo::fundamentals::download::Validators;
use prumo::history::{Chart, HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::{local_date, Performance};
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use time::{Date, Duration, OffsetDateTime};

const SYMBOL: &str = "FUNA11.SA";

/// O Yahoo em memória: fechamento e volume por símbolo, e cada pedido.
#[derive(Default)]
struct FakeHistory {
    closes: Mutex<HashMap<String, Series>>,
    traded: Mutex<HashMap<String, Series>>,
    requests: Mutex<Vec<(String, Date, Date)>>,
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
    async fn index(&self, _: IndexCode, _: Date, _: Date) -> Result<Series, HistoryError> {
        Ok(Vec::new())
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        Ok(window(self.closes.lock().unwrap().get(symbol), from, to))
    }

    async fn chart(&self, symbol: &str, from: Date, to: Date) -> Result<Chart, HistoryError> {
        self.requests
            .lock()
            .unwrap()
            .push((symbol.to_owned(), from, to));
        Ok(Chart {
            closes: window(self.closes.lock().unwrap().get(symbol), from, to),
            splits: Vec::new(),
            traded: window(self.traded.lock().unwrap().get(symbol), from, to),
        })
    }
}

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

fn requests_for(history: &FakeHistory) -> Vec<(Date, Date)> {
    history
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(s, _, _)| s == SYMBOL)
        .map(|(_, from, to)| (*from, *to))
        .collect()
}

#[tokio::test]
async fn the_volume_rides_on_the_price_sync_and_the_fund_shows_its_indicators() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let quotes = FakeQuotes::with_market();
    quotes
        .prices
        .lock()
        .unwrap()
        .insert(SYMBOL.into(), ("10.00".parse().unwrap(), "BRL", "Fundo A"));
    let history = Arc::new(FakeHistory::default());
    let base = app(&db.store, "fii@prumo.test", quotes, &dir.path());
    let state = base.state.clone().with_history_provider(history.clone());
    let router = prumo::http::public_router(state, &dir.path());
    let now = OffsetDateTime::now_utc();
    let today = local_date(now);
    let yesterday = today - Duration::days(1);

    let created = post(
        &router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "FUNA11", "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let asset_id = created.body["id"].as_str().unwrap().to_owned();
    // O informe mensal do fundo: 10% do ativo em caixa.
    db.store
        .save_fund_reports(
            "fund_monthly:2026",
            &[FundReport {
                cnpj: "33.333.333/0001-33".into(),
                issuer: "FUNA".into(),
                isin: "BRFUNACTF001".into(),
                name: "FUNDO A FICTICIO FII".into(),
                reference_date: today - Duration::days(40),
                version: 1,
                net_assets: Some(dec!(1000000)),
                shares: Some(dec!(100000)),
                total_assets: Some(dec!(1250000)),
                liquid_assets: Some(dec!(125000)),
            }],
            &Validators::default(),
            now,
        )
        .await
        .unwrap();

    // 95 pregões a R$ 10, com R$ 2 milhões negociados em cada um. O cache já
    // tem os fechamentos, sem o volume (gravados por um código de antes dele),
    // com o histórico e os desdobramentos conferidos.
    let days = weekdays(today - Duration::days(140), yesterday);
    let days: Vec<Date> = days[days.len() - 95..].to_vec();
    let closes: Series = days.iter().map(|d| (*d, dec!(10))).collect();
    history
        .closes
        .lock()
        .unwrap()
        .insert(SYMBOL.into(), closes.clone());
    history.traded.lock().unwrap().insert(
        SYMBOL.into(),
        days.iter().map(|d| (*d, dec!(2000000))).collect(),
    );
    db.store
        .record_prices(SYMBOL, &closes, &[], now)
        .await
        .unwrap();
    let start = time::macros::date!(2000 - 01 - 01);
    db.store.set_price_coverage(SYMBOL, start).await.unwrap();
    db.store
        .save_price_splits(SYMBOL, start, &[], now)
        .await
        .unwrap();
    assert_eq!(db.store.traded_covered_from(SYMBOL).await.unwrap(), None);

    // A sincronia: o pedido que vai até hoje começa na janela da liquidez.
    let performance = Performance::new(db.store.clone(), history.clone());
    performance.run_daily(now).await.unwrap();
    let window_start = today - Duration::days(TRADED_LOOKBACK_DAYS);
    let asked = requests_for(&history);
    assert_eq!(asked, vec![(window_start, today)], "{asked:?}");
    let stored = db
        .store
        .traded_values(&[SYMBOL.to_owned()], window_start, today)
        .await
        .unwrap();
    assert_eq!(stored[SYMBOL].len(), 95);
    assert!(stored[SYMBOL].iter().all(|(_, v)| *v == dec!(2000000)));
    assert_eq!(
        db.store.traded_covered_from(SYMBOL).await.unwrap(),
        Some(window_start)
    );
    // De novo: só o pedido de sempre (os últimos dias), sem a janela.
    performance.run_daily(now).await.unwrap();
    let asked = requests_for(&history);
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(asked[1].0 > window_start, "{asked:?}");

    // O volume guardado de um pregão antigo não muda; o dos últimos dias, a
    // fonte pode corrigir. Pregão sem fechamento não ganha volume.
    let (old_day, recent_day) = (days[0], days[days.len() - 1]);
    let ghost = today + Duration::days(3);
    db.store
        .record_prices(
            SYMBOL,
            &vec![(old_day, dec!(10)), (recent_day, dec!(10))],
            &[
                (old_day, dec!(1)),
                (recent_day, dec!(2500000)),
                (ghost, dec!(9)),
            ],
            now,
        )
        .await
        .unwrap();
    let stored = db
        .store
        .traded_values(&[SYMBOL.to_owned()], old_day, ghost)
        .await
        .unwrap();
    let value = |day: Date| {
        stored[SYMBOL]
            .iter()
            .find(|(d, _)| *d == day)
            .map(|(_, v)| *v)
    };
    assert_eq!(value(old_day), Some(dec!(2000000)));
    assert_eq!(value(recent_day), Some(dec!(2500000)));
    assert_eq!(value(ghost), None);
    db.store
        .record_prices(
            SYMBOL,
            &vec![(recent_day, dec!(10))],
            &[(recent_day, dec!(2000000))],
            now,
        )
        .await
        .unwrap();

    // A folha: o caixa do informe, a liquidez e os CAGRs (sem cinco anos de
    // preço nem de proventos, com o motivo), e as taxas ao lado.
    let reply = get(&router, "/api/v1/fundamentals").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let entry = find(&reply.body["assets"], "symbol", "FUNA11");
    let fund = &entry["fund"];
    assert_eq!(fund["cash_share"]["value"], 0.1);
    assert_eq!(fund["total_assets"]["value"], 1250000.0);
    let market = &fund["market"];
    assert_eq!(market["liquidity_30d"]["value"], 2000000.0);
    assert_eq!(market["liquidity_90d"]["value"], 2000000.0);
    assert_eq!(market["price_cagr_3y"]["gap"], "insufficient_history");
    assert_eq!(market["dividend_cagr_3y"]["gap"], "insufficient_history");
    assert!(reply.body["rates"].is_object());
    assert!(reply.body["rates"]["real_rate"].is_null());

    // A Análise e a escolha do teto: os mesmos números na linha do FII.
    let reply = get(&router, "/api/v1/analysis").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let fund = find(&reply.body["references"]["funds"], "label", "FUNA11");
    let indicators: &Value = &fund["indicators"];
    assert_eq!(indicators["cash_share"]["value"], 0.1);
    assert_eq!(indicators["liquidity_30d"]["value"], 2000000.0);
    let reply = get(&router, &format!("/api/v1/assets/{asset_id}/references")).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    assert_eq!(
        reply.body["funds"][0]["indicators"]["liquidity_90d"]["value"],
        2000000.0
    );
}
