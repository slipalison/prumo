//! Risco contra um Postgres de verdade: a rota com séries fictícias gravadas
//! no banco (desdobro, provento, ativo em dólar, renda fixa por valor, título
//! com cupom), a sincronia que guarda os desdobramentos (em `price_splits`, a
//! tabela do comparador) e o histórico longo, e a migração 0032 sobre um banco
//! com série. Carteira fictícia.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::dividends::{DividendEvent, DividendHistory};
use prumo::domain::performance::Quota;
use prumo::domain::price_check::revise_from;
use prumo::domain::risk::{history_start, RiskWindow};
use prumo::history::{HistoryError, HistoryProvider, IndexCode, Series};
use prumo::performance::{local_date, Performance};
use prumo::quotes::Quote;
use prumo::store::{Snapshot, Store};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use time::{Date, Duration, OffsetDateTime, Weekday};
use uuid::Uuid;

/// Gerador fixo (congruencial): a mesma carteira fictícia em toda rodada.
struct Noise(u64);

impl Noise {
    /// Um número em [−amp, amp].
    fn next(&mut self, amp: f64) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = (self.0 >> 11) as f64 / (1u64 << 53) as f64;
        (unit * 2.0 - 1.0) * amp
    }
}

fn dec(value: f64) -> Decimal {
    Decimal::from_f64(value).unwrap().round_dp(8)
}

fn weekdays(from: Date, to: Date) -> Vec<Date> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        if !matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday) {
            out.push(d);
        }
        d = d.next_day().unwrap();
    }
    out
}

fn volatility(levels: &[f64]) -> f64 {
    let r: Vec<f64> = levels.windows(2).map(|w| w[1] / w[0] - 1.0).collect();
    let n = r.len() as f64;
    let mean = r.iter().sum::<f64>() / n;
    let var = r.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (var * 252.0).sqrt()
}

fn max_drawdown(levels: &[f64]) -> f64 {
    let mut peak = levels[0];
    let mut worst = 0.0_f64;
    for v in levels {
        peak = peak.max(*v);
        worst = worst.min(v / peak - 1.0);
    }
    worst
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or_else(|| panic!("numero: {value}"))
}

fn near(actual: &Value, expected_fraction: f64) {
    let actual = number(actual);
    let expected = expected_fraction * 100.0;
    assert!(
        (actual - expected).abs() < 0.011,
        "esperado {expected:.4}, veio {actual}"
    );
}

fn asset<'a>(body: &'a Value, label: &str) -> &'a Value {
    find(&body["assets"], "label", label)
}

#[tokio::test]
async fn risk_route_end_to_end() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let email = "risco@exemplo.com";
    let bond = "TD-IPCAJS-20350515";
    db.store
        .upsert_quote(
            &Quote {
                symbol: bond.to_owned(),
                price: dec!(4000),
                currency: "BRL".to_owned(),
                name: Some("Tesouro IPCA+ com Juros Semestrais 2035".to_owned()),
                traded_at: None,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    for body in [
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 20 }),
        json!({ "asset_class": "intl_stocks", "symbol": "AAPL", "quantity": 5 }),
        json!({ "asset_class": "fixed_income", "display_name": "CDB ficticio", "amount": 1000 }),
        json!({ "asset_class": "fixed_income", "symbol": bond, "treasury": true, "quantity": 1, "score_override": 5 }),
    ] {
        let created = post(&app.router, "/api/v1/assets", body).await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    }
    let user = db.store.ensure_user(email).await.unwrap();

    // Pregões fictícios: os dias úteis de ~14 meses até ontem.
    let today = local_date(OffsetDateTime::now_utc());
    let calendar = weekdays(today - Duration::days(420), today - Duration::days(1));
    let start = RiskWindow::TwelveMonths.start(today);
    let base = calendar.iter().rposition(|d| *d <= start).unwrap();
    let end = calendar.len() - 1;
    let mut noise = Noise(42);

    // Ibovespa e os ativos: WEGE3 segue o índice e desdobra 1 para 2 no meio
    // da janela; KNRI11 segue o índice de leve e paga 0,80 por cota a cada 21
    // pregões; AAPL anda sozinho em dólar, e o câmbio também.
    let split_at = base + 120;
    let (mut ibov, mut wege, mut knri_tr, mut knri, mut aapl, mut fx) = (
        vec![120_000.0],
        vec![40.0],
        vec![100.0],
        vec![100.0],
        vec![150.0],
        vec![5.0],
    );
    let mut knri_dividends = Vec::new();
    for i in 1..calendar.len() {
        let market = noise.next(0.015);
        ibov.push(ibov[i - 1] * (1.0 + market));
        wege.push(wege[i - 1] * (1.0 + 1.2 * market + noise.next(0.01)));
        let r = 0.3 * market + noise.next(0.004);
        knri_tr.push(knri_tr[i - 1] * (1.0 + r));
        let mut price = knri[i - 1] * (1.0 + r);
        if i % 21 == 0 {
            price -= 0.8;
            knri_dividends.push(DividendEvent {
                ex_date: calendar[i],
                amount: dec!(0.8),
            });
        }
        knri.push(price);
        aapl.push(aapl[i - 1] * (1.0 + noise.next(0.02)));
        fx.push(fx[i - 1] * (1.0 + noise.next(0.008)));
    }
    let series = |levels: &[f64], skip: &dyn Fn(usize) -> bool| -> Series {
        calendar
            .iter()
            .zip(levels)
            .enumerate()
            .filter(|(i, _)| !skip(*i))
            .map(|(_, (d, v))| (*d, dec(*v)))
            .collect()
    };
    let keep = |_: usize| false;
    db.store
        .upsert_index(IndexCode::Ibov, &series(&ibov, &keep))
        .await
        .unwrap();
    // O fechamento real de WEGE3 antes do desdobro é o dobro do ajustado.
    let wege_real: Vec<f64> = wege
        .iter()
        .enumerate()
        .map(|(i, v)| if i < split_at { v * 2.0 } else { *v })
        .collect();
    db.store
        .insert_prices("WEGE3.SA", &series(&wege_real, &keep))
        .await
        .unwrap();
    db.store
        .save_price_splits(
            "WEGE3.SA",
            calendar[0],
            &[(calendar[split_at], dec!(2))],
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    db.store
        .insert_prices("KNRI11.SA", &series(&knri, &keep))
        .await
        .unwrap();
    db.store
        .save_dividends(
            "KNRI11.SA",
            &DividendHistory {
                currency: "BRL".to_owned(),
                events: knri_dividends,
            },
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();
    // O exterior não negocia em alguns pregões da B3: vale o último fechamento.
    let us_holiday = |i: usize| i % 37 == 5 && i != base && i != end;
    db.store
        .insert_prices("AAPL", &series(&aapl, &us_holiday))
        .await
        .unwrap();
    db.store
        .insert_prices("USDBRL=X", &series(&fx, &keep))
        .await
        .unwrap();
    // O título com cupom tem preço, mas fica fora das contas por ativo.
    db.store
        .insert_prices(bond, &series(&vec![4000.0; calendar.len()], &keep))
        .await
        .unwrap();

    // A cota da carteira: uma foto por dia corrido desde ~10 meses atrás.
    let first_photo = today - Duration::days(300);
    let mut quota = 1.0;
    let mut photos = Vec::new();
    let mut date = first_photo;
    let mut quota_on: HashMap<Date, f64> = HashMap::new();
    while date < today {
        if let Some(i) = calendar.iter().position(|d| *d == date) {
            quota *= ibov[i] / ibov[i - 1] * 0.5 + 0.5;
        }
        quota_on.insert(date, quota);
        photos.push(Snapshot {
            date,
            market_value: dec!(10000),
            net_flow: if date == first_photo {
                dec!(10000)
            } else {
                dec!(0)
            },
            income: dec!(0),
            quota: Quota {
                quotas: dec!(10000),
                value: dec(quota),
            },
            positions: Vec::new(),
            taken_at: OffsetDateTime::now_utc(),
        });
        date = date.next_day().unwrap();
    }
    db.store
        .replace_snapshots(user, Some(first_photo), first_photo, &photos)
        .await
        .unwrap();

    let reply = get(&app.router, "/api/v1/risk").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let body = &reply.body;
    assert_eq!(body["window"], "12m");
    assert_eq!(body["from"], calendar[base].to_string());
    assert_eq!(body["to"], calendar[end].to_string());
    assert_eq!(body["trading_days"], end - base);
    assert_eq!(body["limits"]["min_common"], 60);

    // Ibovespa: a conta feita à mão nos mesmos pregões.
    let window = |levels: &[f64]| levels[base..=end].to_vec();
    let benchmark = &body["benchmark"];
    assert_eq!(benchmark["code"], "ibov");
    assert_eq!(benchmark["full_window"], true);
    near(&benchmark["volatility_percent"], volatility(&window(&ibov)));
    near(&benchmark["return_percent"], ibov[end] / ibov[base] - 1.0);
    near(
        &benchmark["drawdown"]["depth_percent"],
        max_drawdown(&window(&ibov)),
    );

    // WEGE3: o desdobro não vira queda de 50%; os números são os do ajustado.
    let wege3 = asset(body, "WEGE3");
    assert_eq!(wege3["status"], "ok");
    let m = &wege3["metrics"];
    near(&m["volatility_percent"], volatility(&window(&wege)));
    near(&m["return_percent"], wege[end] / wege[base] - 1.0);
    near(
        &m["drawdown"]["depth_percent"],
        max_drawdown(&window(&wege)),
    );
    assert!(number(&m["drawdown"]["depth_percent"]) > -45.0);

    // KNRI11: o provento volta para o retorno (retorno total).
    let knri11 = &asset(body, "KNRI11")["metrics"];
    near(
        &knri11["return_percent"],
        knri_tr[end] / knri_tr[base] - 1.0,
    );
    near(&knri11["volatility_percent"], volatility(&window(&knri_tr)));
    let price_only = knri[end] / knri[base] - 1.0;
    assert!(number(&knri11["return_percent"]) > price_only * 100.0 + 5.0);

    // AAPL em reais: o preço em dólar vezes o câmbio, com o feriado de lá
    // valendo o fechamento anterior.
    let aapl_brl = &asset(body, "AAPL")["metrics"];
    near(
        &aapl_brl["return_percent"],
        aapl[end] * fx[end] / (aapl[base] * fx[base]) - 1.0,
    );
    assert_eq!(aapl_brl["returns"], end - base);

    // Sem preço de mercado e título com cupom: listados, sem número.
    let cdb = asset(body, "CDB ficticio");
    assert_eq!(cdb["status"], "no_series");
    assert!(cdb["metrics"].is_null() && cdb["risk_share_percent"].is_null());
    assert!(number(&cdb["weight_percent"]) > 0.0);
    let coupon = find(&body["assets"], "status", "pays_along_the_way");
    assert!(coupon["metrics"].is_null());
    assert_eq!(coupon["in_correlation"], false);

    // A carteira: pelas cotas, começa depois do fechamento de base.
    let portfolio = &body["portfolio"];
    assert_eq!(portfolio["full_window"], false);
    let first_quota = calendar.iter().find(|d| **d >= first_photo).unwrap();
    assert_eq!(portfolio["from"], first_quota.to_string());
    let quotas: Vec<f64> = calendar[base..=end]
        .iter()
        .filter_map(|d| quota_on.get(d).copied())
        .collect();
    near(&portfolio["volatility_percent"], volatility(&quotas));

    // Correlação: os três com série, simétrica, 1 na diagonal; o par que mais
    // anda junto é o dos dois que seguem o índice.
    let ids: Vec<String> = ["WEGE3", "KNRI11", "AAPL"]
        .iter()
        .map(|l| asset(body, l)["asset_id"].as_str().unwrap().to_owned())
        .collect();
    let order: Vec<&str> = body["correlation"]["asset_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(order.len(), 3);
    let values = &body["correlation"]["values"];
    for i in 0..3 {
        assert_eq!(number(&values[i][i]), 1.0);
        for j in 0..3 {
            assert_eq!(values[i][j], values[j][i]);
            assert!((-1.0..=1.0).contains(&number(&values[i][j])));
        }
    }
    let pairs = body["correlation"]["top_pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 3);
    let top = [
        pairs[0]["a"].as_str().unwrap(),
        pairs[0]["b"].as_str().unwrap(),
    ];
    assert!(top.contains(&ids[0].as_str()) && top.contains(&ids[1].as_str()));
    assert!(number(&pairs[0]["value"]) >= number(&pairs[1]["value"]));

    // Contribuição: só os três, somando 100%; o peso coberto é o deles.
    let model = &body["model"];
    assert_eq!(model["days"], end - base);
    let shares: f64 = ["WEGE3", "KNRI11", "AAPL"]
        .iter()
        .map(|l| number(&asset(body, l)["risk_share_percent"]))
        .sum();
    assert!((shares - 100.0).abs() < 0.05, "{shares}");
    let covered: f64 = ["WEGE3", "KNRI11", "AAPL"]
        .iter()
        .map(|l| number(&asset(body, l)["weight_percent"]))
        .sum();
    assert!((number(&model["covered_weight_percent"]) - covered).abs() < 0.05);
    assert!(number(&model["volatility_percent"]) > 0.0);

    // Janela de 5 anos: a série só tem ~14 meses, nada cobre a janela inteira.
    let long = get(&app.router, "/api/v1/risk?window=60m").await;
    assert_eq!(long.status, StatusCode::OK, "{}", long.text);
    assert_eq!(asset(&long.body, "WEGE3")["metrics"]["full_window"], false);
    assert!(long.body["model"].is_null());
    assert_eq!(long.body["from"], calendar[0].to_string());

    let bad = get(&app.router, "/api/v1/risk?window=5y").await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text);

    // Outra pessoa não vê nada desta carteira.
    let other = common::app(
        &db.store,
        "outra@exemplo.com",
        FakeQuotes::with_market(),
        &dir.path(),
    );
    let empty = get(&other.router, "/api/v1/risk").await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text);
    assert!(empty.body["assets"].as_array().unwrap().is_empty());
    assert!(empty.body["portfolio"].is_null());
    assert!(empty.body["correlation"]["asset_ids"]
        .as_array()
        .unwrap()
        .is_empty());
}

/// Fechamentos e desdobramentos em memória; guarda cada pedido.
#[derive(Default)]
struct SplitHistory {
    closes: Mutex<HashMap<String, Series>>,
    splits: Mutex<HashMap<String, Series>>,
    requests: Mutex<Vec<(String, Date, Date)>>,
}

fn within(series: Option<&Series>, from: Date, to: Date) -> Series {
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
impl HistoryProvider for SplitHistory {
    async fn index(&self, _: IndexCode, _: Date, _: Date) -> Result<Series, HistoryError> {
        Ok(Series::new())
    }

    async fn closes(&self, symbol: &str, from: Date, to: Date) -> Result<Series, HistoryError> {
        Ok(self.closes_and_splits(symbol, from, to).await?.0)
    }

    async fn closes_and_splits(
        &self,
        symbol: &str,
        from: Date,
        to: Date,
    ) -> Result<(Series, Series), HistoryError> {
        self.requests
            .lock()
            .unwrap()
            .push((symbol.to_owned(), from, to));
        let closes = within(self.closes.lock().unwrap().get(symbol), from, to);
        // Como o Yahoo: os eventos de `from` até hoje, além da janela.
        let splits = within(self.splits.lock().unwrap().get(symbol), from, Date::MAX);
        Ok((closes, splits))
    }
}

/// A sincronia guarda os desdobramentos que vêm com os fechamentos, busca o
/// histórico longo do risco e, para o símbolo guardado antes da 0032 (sem a
/// lista de desdobramentos), pede o trecho inteiro uma vez, sem sujar a série
/// com o que já estava guardado.
#[tokio::test]
async fn price_sync_keeps_splits_and_the_long_history() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let email = "sincronia-risco@exemplo.com";
    let app = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    for symbol in ["WEGE3", "KNRI11"] {
        let created = post(
            &app.router,
            "/api/v1/assets",
            json!({ "asset_class": "br_stocks", "symbol": symbol, "quantity": 10 }),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    }
    let now = OffsetDateTime::now_utc();
    let today = local_date(now);
    let from = history_start(today);
    let history = Arc::new(SplitHistory::default());
    let split_day = today - Duration::days(400);
    history.closes.lock().unwrap().insert(
        "WEGE3.SA".to_owned(),
        vec![(from, dec!(80)), (split_day, dec!(40)), (today, dec!(41))],
    );
    history
        .splits
        .lock()
        .unwrap()
        .insert("WEGE3.SA".to_owned(), vec![(split_day, dec!(2))]);
    // KNRI11 já estava guardado (de antes da 0032): preço e cobertura, sem a
    // lista de desdobramentos.
    let old = vec![(from, dec!(100)), (today, dec!(101))];
    history
        .closes
        .lock()
        .unwrap()
        .insert("KNRI11.SA".to_owned(), old.clone());
    db.store.insert_prices("KNRI11.SA", &old).await.unwrap();
    db.store
        .set_price_coverage("KNRI11.SA", from)
        .await
        .unwrap();
    let job = Performance::new(db.store.clone(), history.clone());
    job.run_daily(now).await.unwrap();

    let requests = history.requests.lock().unwrap().clone();
    let wege: Vec<_> = requests.iter().filter(|r| r.0 == "WEGE3.SA").collect();
    assert_eq!(wege.len(), 1, "{wege:?}");
    assert_eq!((wege[0].1, wege[0].2), (from, today));
    let knri: Vec<_> = requests.iter().filter(|r| r.0 == "KNRI11.SA").collect();
    assert_eq!(knri.len(), 1, "{knri:?}");
    assert_eq!((knri[0].1, knri[0].2), (from, today));

    let symbols = ["WEGE3.SA".to_owned(), "KNRI11.SA".to_owned()];
    let splits = db.store.price_splits(&symbols).await.unwrap();
    assert_eq!(splits["WEGE3.SA"], vec![(split_day, dec!(2))]);
    assert!(!splits.contains_key("KNRI11.SA"));
    // A conferência fica marcada (a mesma do comparador): ele não precisa
    // pedir de novo o ativo da carteira.
    for symbol in &symbols {
        let (checked_from, _) = db.store.split_check(symbol).await.unwrap().unwrap();
        assert_eq!(checked_from, from);
    }
    // Preço novo vem com a data (é ela que marca a série de quem tem o
    // ativo); o que já estava guardado não volta, nem suja nada.
    let wege_saved = db
        .store
        .insert_prices("WEGE3.SA", &vec![(from - Duration::days(1), dec!(79))])
        .await
        .unwrap();
    assert_eq!(wege_saved, vec![from - Duration::days(1)]);
    assert!(db
        .store
        .insert_prices("KNRI11.SA", &old)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        db.store.closes_on(&symbols[1..], today).await.unwrap()["KNRI11.SA"],
        dec!(101)
    );

    // Rodar de novo no mesmo dia não refaz a conferência longa (as duas listas
    // estão completas): pede o que falta e, no mínimo, os últimos dias de novo
    // (a revisão da fonte).
    let again = now + Duration::minutes(1);
    job.run_daily(again).await.unwrap();
    let all = history.requests.lock().unwrap().clone();
    let later = &all[requests.len()..];
    assert_eq!(later.len(), 2, "{later:?}");
    assert!(later
        .iter()
        .all(|r| r.1 > from && r.1 <= revise_from(again) && r.2 == today));
}

/// A barra de um pregão aberto não vira fechamento: a tarefa das 15h (de um
/// dia útil) não grava o dia; a das 19h grava. E a fonte que corrige um
/// fechamento recente é ouvida: a rodada seguinte pede de novo os últimos dias
/// e corrige, com a série de quem tem o ativo marcada desde a data corrigida
/// (fora dos últimos dias, o guardado não muda). Fictício: um FII genérico.
#[tokio::test]
async fn an_open_session_waits_for_the_close_and_recent_revisions_land() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let email = "pregao-aberto@exemplo.com";
    let quotes = FakeQuotes::with_market();
    quotes
        .prices
        .lock()
        .unwrap()
        .insert("FICT11.SA".to_owned(), (dec!(80), "BRL", "Fundo ficticio"));
    let app = app(&db.store, email, quotes, &dir.path());
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "FICT11", "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let user = db.store.ensure_user(email).await.unwrap();
    let symbol = "FICT11.SA";
    let symbols = [symbol.to_owned()];
    // Quarta, 25/11/2026, e os dois pregões antes dela.
    let day = Date::from_calendar_date(2026, time::Month::November, 25).unwrap();
    let history = Arc::new(SplitHistory::default());
    history.closes.lock().unwrap().insert(
        symbol.to_owned(),
        vec![
            (day - Duration::days(2), dec!(80)),
            (day - Duration::days(1), dec!(81)),
            (day, dec!(82)),
        ],
    );
    let job = Performance::new(db.store.clone(), history.clone());
    let at = |date: Date, hour: u8, minute: u8| {
        date.with_time(time::Time::from_hms(hour, minute, 0).unwrap())
            .assume_offset(prumo::performance::BRASILIA)
    };

    // 15h: o pregão ainda corre; o de ontem entra, o de hoje não.
    job.run_daily(at(day, 15, 0)).await.unwrap();
    assert_eq!(
        db.store
            .closes_on(&symbols, day - Duration::days(1))
            .await
            .unwrap()[symbol],
        dec!(81)
    );
    assert!(db.store.closes_on(&symbols, day).await.unwrap().is_empty());
    // 19h05: fechou; o de hoje entra.
    job.run_daily(at(day, 19, 5)).await.unwrap();
    assert_eq!(
        db.store.closes_on(&symbols, day).await.unwrap()[symbol],
        dec!(82)
    );

    // A fonte corrige o fechamento de ontem: a tarefa do dia seguinte acerta.
    history.closes.lock().unwrap().insert(
        symbol.to_owned(),
        vec![
            (day - Duration::days(2), dec!(80)),
            (day - Duration::days(1), dec!(81.5)),
            (day, dec!(82)),
        ],
    );
    job.run_daily(at(day + Duration::days(1), 19, 5))
        .await
        .unwrap();
    assert_eq!(
        db.store
            .closes_on(&symbols, day - Duration::days(1))
            .await
            .unwrap()[symbol],
        dec!(81.5)
    );

    // A correção marca a série de quem tem o ativo desde a data corrigida; a
    // de um dia fora da janela de revisão não muda nada.
    if let Some(dirty) = db.store.dirty_since(user).await.unwrap() {
        db.store.clear_dirty(&dirty).await.unwrap();
    }
    let evening = at(day + Duration::days(1), 20, 0);
    db.store
        .record_prices(symbol, &vec![(day, dec!(82.3))], evening)
        .await
        .unwrap();
    assert_eq!(
        db.store.closes_on(&symbols, day).await.unwrap()[symbol],
        dec!(82.3)
    );
    assert_eq!(
        db.store.dirty_since(user).await.unwrap().unwrap().since,
        day
    );
    let old = day - Duration::days(20);
    db.store
        .insert_prices(symbol, &vec![(old, dec!(70))])
        .await
        .unwrap();
    db.store
        .record_prices(symbol, &vec![(old, dec!(71))], evening)
        .await
        .unwrap();
    assert_eq!(
        db.store.closes_on(&symbols, old).await.unwrap()[symbol],
        dec!(70)
    );
    // Câmbio e cripto: o dia corrente nunca entra, nem às 20h.
    let fx = db
        .store
        .record_prices(
            "USDBRL=X",
            &vec![(day + Duration::days(1), dec!(5.4))],
            evening,
        )
        .await
        .unwrap();
    assert!(fx.is_empty());
}

/// A 0032 sobre um banco com série (as de antes dela aplicadas, carteira
/// fictícia): quem tem foto ganha a marca de "sujo" desde a última, e a marca
/// mais antiga que já existia continua valendo.
#[tokio::test]
async fn migration_marks_existing_series_for_the_price_sync() {
    let container = Postgres::default()
        .with_tag("17-alpine")
        .start()
        .await
        .unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    let mut before = sqlx::migrate!("./migrations");
    before.migrations = before
        .migrations
        .iter()
        .filter(|m| m.version < 32)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    before.run(&pool).await.unwrap();

    let (fresh, marked, none) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (id, email) in [
        (fresh, "serie@prumo.test"),
        (marked, "marcada@prumo.test"),
        (none, "sem-serie@prumo.test"),
    ] {
        sqlx::query("insert into users (id, email) values ($1, $2)")
            .bind(id)
            .bind(email)
            .execute(&pool)
            .await
            .unwrap();
    }
    let d1 = Date::from_calendar_date(2026, time::Month::November, 20).unwrap();
    let d2 = d1 + Duration::days(1);
    for user in [fresh, marked] {
        for date in [d1, d2] {
            sqlx::query(
                "insert into portfolio_snapshots (user_id, date, market_value, net_flow, quota_value, quotas, taken_at)
                 values ($1, $2, 1000, 0, 1, 1000, now())",
            )
            .bind(user)
            .bind(date)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    let older = d1 - Duration::days(30);
    sqlx::query(
        "insert into performance_rebuilds (user_id, dirty_since, marked_at) values ($1, $2, now())",
    )
    .bind(marked)
    .bind(older)
    .execute(&pool)
    .await
    .unwrap();

    let store = Store::from_pool(pool.clone());
    store.migrate().await.unwrap();
    assert_eq!(store.dirty_since(fresh).await.unwrap().unwrap().since, d2);
    assert_eq!(
        store.dirty_since(marked).await.unwrap().unwrap().since,
        older
    );
    assert!(store.dirty_since(none).await.unwrap().is_none());
}

/// Fechamento ruim da fonte (fictício: um FII genérico com a escala errada em
/// 100× por dois pregões): não entra quando a volta já está guardada; entra se
/// chega antes da volta e sai quando ela chega, com a série de quem tem o ativo
/// marcada desde o primeiro dia ruim. Desdobramento de verdade (5 para 1, que
/// não volta) e queda forte de verdade (−36%, que não volta) passam. A limpeza
/// do que já estava gravado acha o mesmo trecho e é idempotente.
#[tokio::test]
async fn bad_closes_from_the_source_are_dropped_and_cleaned() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let email = "fechamento-ruim@exemplo.com";
    let quotes = FakeQuotes::with_market();
    quotes
        .prices
        .lock()
        .unwrap()
        .insert("FICT11.SA".to_owned(), (dec!(80), "BRL", "Fundo ficticio"));
    let app = app(&db.store, email, quotes, &dir.path());
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "FICT11", "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let user = db.store.ensure_user(email).await.unwrap();
    if let Some(dirty) = db.store.dirty_since(user).await.unwrap() {
        db.store.clear_dirty(&dirty).await.unwrap();
    }
    let d = |day: u8| Date::from_calendar_date(2026, time::Month::January, day).unwrap();
    let symbol = "FICT11.SA";

    // O trecho ruim chega antes da volta: ainda não dá para saber, entra.
    let first = db
        .store
        .insert_prices(
            symbol,
            &vec![
                (d(14), dec!(80.00)),
                (d(15), dec!(0.79)),
                (d(16), dec!(0.81)),
            ],
        )
        .await
        .unwrap();
    assert_eq!(first, vec![d(14), d(15), d(16)]);
    assert!(db.store.dirty_since(user).await.unwrap().is_none());
    // A volta chega: os dois dias ruins saem e a série é refeita desde o 15.
    let back = db
        .store
        .insert_prices(symbol, &vec![(d(19), dec!(80.50))])
        .await
        .unwrap();
    assert_eq!(back, vec![d(19)]);
    let symbols = [symbol.to_owned()];
    assert!(db
        .store
        .closes_on(&symbols, d(15))
        .await
        .unwrap()
        .is_empty());
    assert!(db
        .store
        .closes_on(&symbols, d(16))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        db.store.dirty_since(user).await.unwrap().unwrap().since,
        d(15)
    );
    // Pedir de novo o trecho inteiro (como a conferência dos desdobramentos
    // faz) não traz o erro de volta.
    let again = db
        .store
        .insert_prices(
            symbol,
            &vec![
                (d(14), dec!(80.00)),
                (d(15), dec!(0.79)),
                (d(16), dec!(0.81)),
                (d(19), dec!(80.50)),
            ],
        )
        .await
        .unwrap();
    assert!(again.is_empty(), "{again:?}");

    // Desdobro de 5 para 1 sem evento guardado e queda de 36%: não voltam,
    // entram inteiros.
    let split: Series = vec![
        (d(5), dec!(100)),
        (d(6), dec!(20)),
        (d(7), dec!(20.4)),
        (d(8), dec!(19.9)),
    ];
    assert_eq!(
        db.store
            .insert_prices("DESD3.SA", &split)
            .await
            .unwrap()
            .len(),
        4
    );
    let crash: Series = vec![(d(5), dec!(100)), (d(6), dec!(64)), (d(7), dec!(63))];
    assert_eq!(
        db.store
            .insert_prices("QUED3.SA", &crash)
            .await
            .unwrap()
            .len(),
        3
    );

    // O que foi gravado antes do filtro: a limpeza acha e apaga, uma vez.
    let pool = PgPoolOptions::new().connect(&db.url).await.unwrap();
    for (date, close) in [(d(5), dec!(50)), (d(6), dec!(0.5)), (d(7), dec!(50.2))] {
        sqlx::query("insert into price_history (symbol, date, close) values ('VELH11.SA', $1, $2)")
            .bind(date)
            .bind(close)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(db.store.clean_bad_closes().await.unwrap(), 1);
    let old = ["VELH11.SA".to_owned()];
    assert!(db.store.closes_on(&old, d(6)).await.unwrap().is_empty());
    assert_eq!(db.store.clean_bad_closes().await.unwrap(), 0);
}

/// O fim da janela nunca passa de hoje no fuso de Brasília, nem perto da
/// meia-noite UTC (quando o UTC já é o dia seguinte), e pregão é dia útil: um
/// fechamento datado num sábado (fonte que date pelo UTC) não vira pregão.
#[tokio::test]
async fn the_window_ends_today_in_brasilia() {
    let db = database().await;
    let dir = tempfile_dir::Dir::new();
    let email = "fim-da-janela@exemplo.com";
    let quotes = FakeQuotes::with_market();
    quotes
        .prices
        .lock()
        .unwrap()
        .insert("FICT11.SA".to_owned(), (dec!(80), "BRL", "Fundo ficticio"));
    let app = app(&db.store, email, quotes, &dir.path());
    let created = post(
        &app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "FICT11", "quantity": 10 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let user = db.store.ensure_user(email).await.unwrap();
    // Sexta, 27/11/2026: Ibovespa até ela; o ativo com um ponto no sábado 28.
    let friday = Date::from_calendar_date(2026, time::Month::November, 27).unwrap();
    let days = weekdays(friday - Duration::days(400), friday);
    let ibov: Series = days
        .iter()
        .enumerate()
        .map(|(i, d)| (*d, Decimal::from(100_000 + i as i64 * 10)))
        .collect();
    db.store.upsert_index(IndexCode::Ibov, &ibov).await.unwrap();
    let mut closes: Series = days
        .iter()
        .enumerate()
        .map(|(i, d)| (*d, Decimal::from(80 + (i % 3) as i64)))
        .collect();
    closes.push((friday + Duration::days(1), dec!(81)));
    db.store.insert_prices("FICT11.SA", &closes).await.unwrap();

    // 22h30 de sexta em Brasília = 01h30 de sábado em UTC.
    let late = OffsetDateTime::new_utc(
        friday + Duration::days(1),
        time::Time::from_hms(1, 30, 0).unwrap(),
    );
    let report = prumo::risk::report(&db.store, user, RiskWindow::TwelveMonths, late)
        .await
        .unwrap();
    assert_eq!(report.to, Some(friday));
    // Domingo: o sábado com fechamento não é pregão; o fim é a sexta.
    let sunday = OffsetDateTime::new_utc(
        friday + Duration::days(2),
        time::Time::from_hms(15, 0, 0).unwrap(),
    );
    let report = prumo::risk::report(&db.store, user, RiskWindow::TwelveMonths, sunday)
        .await
        .unwrap();
    assert_eq!(report.to, Some(friday));
    // 23h30 de quinta em Brasília = 02h30 de sexta em UTC: termina na quinta.
    let thursday_night = OffsetDateTime::new_utc(friday, time::Time::from_hms(2, 30, 0).unwrap());
    let report = prumo::risk::report(&db.store, user, RiskWindow::TwelveMonths, thursday_night)
        .await
        .unwrap();
    assert_eq!(report.to, Some(friday - Duration::days(1)));
    assert_eq!(
        report.from,
        days.iter()
            .rev()
            .find(|d| **d <= Date::from_calendar_date(2025, time::Month::November, 26).unwrap())
            .copied()
    );
}
