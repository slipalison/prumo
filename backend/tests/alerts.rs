//! Alertas pelo Telegram contra um Postgres de verdade: a vinculação por
//! código, a leitura do `getUpdates`, o resumo diário (o que entra, o que não
//! se repete, "sem valores"), o lock entre réplicas e a API. O Telegram é um
//! falso em memória (e, num teste, o cliente real contra um wiremock).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::http::StatusCode;
use common::*;
use prumo::alerts::{
    AlertError, AlertService, PollOutcome, DIGEST_LOCK_KEY, POLL_LOCK_KEY, TEST_COOLDOWN,
};
use prumo::config::Secret;
use prumo::performance::{local_date, BRASILIA};
use prumo::store::Store;
use prumo::telegram::{BotApi, Chat, Message, TelegramApi, TelegramError, Update};
use serde_json::{json, Value};
use time::macros::time;
use time::{Date, OffsetDateTime};
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------- Telegram falso --

#[derive(Default)]
struct FakeTelegram {
    sent: Mutex<Vec<(i64, String)>>,
    inbox: Mutex<VecDeque<Result<Vec<Update>, TelegramError>>>,
    offsets: Mutex<Vec<i64>>,
    /// O que os próximos `send` devolvem (vazio: tudo certo).
    outcomes: Mutex<VecDeque<Result<(), TelegramError>>>,
    send_calls: AtomicUsize,
    username_calls: AtomicUsize,
}

impl FakeTelegram {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn deliver(&self, updates: Vec<Update>) {
        self.inbox.lock().unwrap().push_back(Ok(updates));
    }

    fn fail_next_send(&self, error: TelegramError) {
        self.outcomes.lock().unwrap().push_back(Err(error));
    }

    fn messages(&self) -> Vec<(i64, String)> {
        self.sent.lock().unwrap().clone()
    }

    fn texts_to(&self, chat: i64) -> Vec<String> {
        self.messages()
            .into_iter()
            .filter(|(c, _)| *c == chat)
            .map(|(_, t)| t)
            .collect()
    }
}

#[async_trait]
impl TelegramApi for FakeTelegram {
    async fn username(&self) -> Result<String, TelegramError> {
        self.username_calls.fetch_add(1, Ordering::SeqCst);
        Ok("prumo_exemplo_bot".to_owned())
    }

    async fn updates(&self, offset: i64, _timeout: Duration) -> Result<Vec<Update>, TelegramError> {
        self.offsets.lock().unwrap().push(offset);
        self.inbox
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn send(&self, chat_id: i64, text: &str) -> Result<(), TelegramError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(outcome) = self.outcomes.lock().unwrap().pop_front() {
            outcome?;
        }
        self.sent.lock().unwrap().push((chat_id, text.to_owned()));
        Ok(())
    }
}

fn message(update_id: i64, chat: i64, kind: &str, text: &str) -> Update {
    Update {
        update_id,
        message: Some(Message {
            chat: Chat {
                id: chat,
                kind: kind.to_owned(),
            },
            text: Some(text.to_owned()),
        }),
    }
}

fn service(store: &Store, fake: &Arc<FakeTelegram>) -> Arc<AlertService> {
    Arc::new(
        AlertService::new(
            store.clone(),
            Some(Arc::clone(fake) as Arc<dyn TelegramApi>),
        )
        .with_poll_timeout(Duration::ZERO),
    )
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// Meio-dia em Brasília, `days` dias depois de hoje.
fn noon(days: i64) -> OffsetDateTime {
    local_date(now())
        .with_time(time!(12:00))
        .assume_offset(BRASILIA)
        + time::Duration::days(days)
}

fn day_after(at: OffsetDateTime) -> Date {
    local_date(at).next_day().unwrap()
}

// ------------------------------------------------------------- vinculação --

#[tokio::test]
async fn linking_by_code_and_reading_updates() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let alerts = service(&db.store, &fake);
    let ana = db.store.ensure_user("ana@prumo.test").await.unwrap();
    let beto = db.store.ensure_user("beto@prumo.test").await.unwrap();

    // Sem token: tudo recusa, sem pânico.
    let off = AlertService::disabled(db.store.clone());
    assert!(!off.configured());
    assert!(matches!(
        off.new_link_code(ana, now()).await,
        Err(AlertError::NotConfigured)
    ));
    assert!(matches!(
        off.poll_cycle().await,
        Err(AlertError::NotConfigured)
    ));
    assert!(matches!(
        off.send_test(ana).await,
        Err(AlertError::NotConfigured)
    ));
    assert_eq!(off.bot_username().await, None);

    // O usuário do bot vem do Telegram e fica guardado.
    assert_eq!(
        alerts.bot_username().await.as_deref(),
        Some("prumo_exemplo_bot")
    );
    assert_eq!(
        alerts.bot_username().await.as_deref(),
        Some("prumo_exemplo_bot")
    );
    assert_eq!(fake.username_calls.load(Ordering::SeqCst), 1);

    // /start <código> liga o chat à pessoa; o código é de uso único.
    let code = alerts.new_link_code(ana, now()).await.unwrap();
    assert_eq!(code.code.len(), 10);
    fake.deliver(vec![message(
        10,
        777,
        "private",
        &format!("/start {}", code.code),
    )]);
    assert_eq!(
        alerts.poll_cycle().await.unwrap(),
        PollOutcome::Polled { updates: 1 }
    );
    let settings = db.store.alert_settings(ana).await.unwrap();
    assert_eq!(settings.chat_id, Some(777));
    assert!(settings.linked_at.is_some());
    let replies = fake.texts_to(777);
    assert_eq!(replies.len(), 1);
    assert!(replies[0].starts_with("Pronto: este chat esta vinculado"));
    assert!(replies[0].is_ascii());
    // O offset foi guardado: a próxima leitura continua de 11.
    assert_eq!(db.store.telegram_offset().await.unwrap(), 11);

    fake.deliver(vec![message(
        11,
        888,
        "private",
        &format!("/start {}", code.code),
    )]);
    alerts.poll_cycle().await.unwrap();
    assert_eq!(*fake.offsets.lock().unwrap(), [0, 11]);
    assert!(fake.texts_to(888)[0].starts_with("Codigo invalido ou vencido"));
    assert_eq!(
        db.store.alert_settings(ana).await.unwrap().chat_id,
        Some(777),
        "o código gasto não liga outro chat"
    );

    // Minúsculas valem (quem digita no celular não escolhe).
    let again = alerts.new_link_code(ana, now()).await.unwrap();
    fake.deliver(vec![message(
        12,
        779,
        "private",
        &format!("/start {}", again.code.to_lowercase()),
    )]);
    alerts.poll_cycle().await.unwrap();
    assert_eq!(
        db.store.alert_settings(ana).await.unwrap().chat_id,
        Some(779)
    );

    // Código vencido não vale, e um código novo mata o anterior da pessoa.
    let stale = alerts
        .new_link_code(beto, now() - time::Duration::minutes(11))
        .await
        .unwrap();
    assert!(stale.expires_at < now());
    let first = alerts.new_link_code(beto, now()).await.unwrap();
    let second = alerts.new_link_code(beto, now()).await.unwrap();
    assert_ne!(first.code, second.code);
    fake.deliver(vec![
        message(13, 901, "private", &format!("/start {}", stale.code)),
        message(14, 902, "private", &format!("/start {}", first.code)),
    ]);
    alerts.poll_cycle().await.unwrap();
    assert!(fake.texts_to(901)[0].starts_with("Codigo invalido"));
    assert!(fake.texts_to(902)[0].starts_with("Codigo invalido"));
    assert_eq!(db.store.alert_settings(beto).await.unwrap().chat_id, None);

    // Grupo é ignorado (nem resposta, nem gasto do código); mensagem solta também.
    fake.deliver(vec![
        message(15, -500, "group", &format!("/start {}", second.code)),
        message(16, 903, "private", "bom dia"),
        Update {
            update_id: 17,
            message: None,
        },
    ]);
    alerts.poll_cycle().await.unwrap();
    assert!(fake.texts_to(-500).is_empty());
    assert!(fake.texts_to(903).is_empty());
    assert_eq!(db.store.telegram_offset().await.unwrap(), 18);

    // /start sem código explica; o `@NomeDoBot` do grupo vale como /start.
    fake.deliver(vec![message(18, 904, "private", "/start")]);
    alerts.poll_cycle().await.unwrap();
    assert!(fake.texts_to(904)[0].contains("gere um codigo"));
    fake.deliver(vec![message(
        19,
        905,
        "private",
        &format!("/start@prumo_exemplo_bot {}", second.code),
    )]);
    alerts.poll_cycle().await.unwrap();
    assert_eq!(
        db.store.alert_settings(beto).await.unwrap().chat_id,
        Some(905)
    );

    // Um chat é de uma pessoa só: Ana liga o 905 e Beto o perde.
    let third = alerts.new_link_code(ana, now()).await.unwrap();
    fake.deliver(vec![message(
        20,
        905,
        "private",
        &format!("/start {}", third.code),
    )]);
    alerts.poll_cycle().await.unwrap();
    assert_eq!(
        db.store.alert_settings(ana).await.unwrap().chat_id,
        Some(905)
    );
    assert_eq!(db.store.alert_settings(beto).await.unwrap().chat_id, None);

    // /stop desvincula; de novo, avisa que não havia vínculo.
    fake.deliver(vec![
        message(21, 905, "private", "/stop"),
        message(22, 905, "private", "/stop"),
    ]);
    alerts.poll_cycle().await.unwrap();
    assert_eq!(db.store.alert_settings(ana).await.unwrap().chat_id, None);
    let texts = fake.texts_to(905);
    assert!(texts[texts.len() - 2].starts_with("Desvinculado"));
    assert!(texts[texts.len() - 1].starts_with("Este chat nao estava vinculado"));

    // Desvincular pela tela.
    db.store.link_alert_chat(ana, 1234, now()).await.unwrap();
    assert!(alerts.unlink(ana).await.unwrap());
    assert!(!alerts.unlink(ana).await.unwrap());
    assert_eq!(db.store.alert_settings(ana).await.unwrap().chat_id, None);
}

#[tokio::test]
async fn a_failing_read_does_not_advance_the_offset() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let alerts = service(&db.store, &fake);
    fake.inbox
        .lock()
        .unwrap()
        .push_back(Err(TelegramError::Conflict));
    assert!(matches!(
        alerts.poll_cycle().await,
        Err(AlertError::Telegram(TelegramError::Conflict))
    ));
    assert_eq!(db.store.telegram_offset().await.unwrap(), 0);
    // O lock foi solto mesmo com o erro: a próxima rodada entra.
    assert_eq!(
        alerts.poll_cycle().await.unwrap(),
        PollOutcome::Polled { updates: 0 }
    );
    // O offset só sobe, nunca desce (duas réplicas que chegam fora de ordem).
    db.store.save_telegram_offset(50).await.unwrap();
    db.store.save_telegram_offset(40).await.unwrap();
    assert_eq!(db.store.telegram_offset().await.unwrap(), 50);
}

#[tokio::test]
async fn the_real_client_polls_with_the_saved_offset_against_a_mock_server() {
    let db = database().await;
    let server = MockServer::start().await;
    let token = "999:token-de-mentira";
    let api = BotApi::with_base(reqwest::Client::new(), Secret::new(token), &server.uri());
    let alerts =
        AlertService::new(db.store.clone(), Some(Arc::new(api))).with_poll_timeout(Duration::ZERO);
    let ana = db.store.ensure_user("ana@prumo.test").await.unwrap();
    let code = alerts.new_link_code(ana, now()).await.unwrap();

    Mock::given(method("POST"))
        .and(path(format!("/bot{token}/getUpdates")))
        .and(body_partial_json(json!({ "offset": 0 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "result": [{
                "update_id": 5,
                "message": {
                    "message_id": 1,
                    "chat": { "id": 321, "type": "private" },
                    "date": 1_767_000_000,
                    "text": format!("/start {}", code.code)
                }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{token}/getUpdates")))
        .and(body_partial_json(json!({ "offset": 6 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "result": [] })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/bot{token}/sendMessage")))
        .and(body_partial_json(json!({ "chat_id": 321 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "result": {} })))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        alerts.poll_cycle().await.unwrap(),
        PollOutcome::Polled { updates: 1 }
    );
    assert_eq!(
        alerts.poll_cycle().await.unwrap(),
        PollOutcome::Polled { updates: 0 }
    );
    assert_eq!(
        db.store.alert_settings(ana).await.unwrap().chat_id,
        Some(321)
    );
}

// ----------------------------------------------------------------- resumo --

struct Person {
    app: TestApp,
    user: Uuid,
    chat: i64,
    _dir: tempfile_dir::Dir,
}

async fn person(db: &Database, alerts: &Arc<AlertService>, email: &str, chat: i64) -> Person {
    let dir = tempfile_dir::Dir::new();
    let base = app(&db.store, email, FakeQuotes::with_market(), &dir.path());
    let state = base.state.clone().with_alerts_service(Arc::clone(alerts));
    let router = prumo::http::public_router(state.clone(), &dir.path());
    let user = db.store.ensure_user(email).await.unwrap();
    db.store.link_alert_chat(user, chat, now()).await.unwrap();
    Person {
        app: TestApp {
            router,
            ops: base.ops,
            state,
        },
        user,
        chat,
        _dir: dir,
    }
}

async fn build_portfolio(p: &Person) -> (String, String) {
    let reit = post(
        &p.app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_reits", "symbol": "KNRI11", "quantity": 10 }),
    )
    .await;
    assert_eq!(reit.status, StatusCode::CREATED, "{}", reit.text);
    let stock = post(
        &p.app.router,
        "/api/v1/assets",
        json!({ "asset_class": "br_stocks", "symbol": "WEGE3", "quantity": 20 }),
    )
    .await;
    assert_eq!(stock.status, StatusCode::CREATED, "{}", stock.text);
    let goals = put(
        &p.app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 50, "br_reits": 50 } }),
    )
    .await;
    assert_eq!(goals.status, StatusCode::OK);
    (
        reit.body["id"].as_str().unwrap().to_owned(),
        stock.body["id"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn the_daily_digest_only_says_what_is_new() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let alerts = service(&db.store, &fake);
    let ana = person(&db, &alerts, "ana@prumo.test", 4242).await;
    let (knri, wege) = build_portfolio(&ana).await;
    let pool = sqlx::PgPool::connect(&db.url).await.unwrap();

    // Antes de ligar nada: sem limite de saúde, para o primeiro resumo ser
    // só o provento (KNRI11 vale R$ 1.420 de R$ 2.420: 58,7%).
    let rules = put(
        &ana.app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "max_asset": { "threshold": 90 }, "min_assets": { "enabled": false } } }),
    )
    .await;
    assert_eq!(rules.status, StatusCode::OK, "{}", rules.text);

    // Dia 0: um provento cai amanhã. As classes (58,7% e 41,3% contra 50/50)
    // já estão fora da meta, mas é a primeira apuração: guarda sem avisar.
    let day0 = noon(0);
    let income = post(
        &ana.app.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day_after(day0).to_string(), "amount_per_unit": 1.1 }),
    )
    .await;
    assert_eq!(income.status, StatusCode::CREATED, "{}", income.text);
    // Outro pagador amanhã, de outro dia, não entra.
    let later = post(
        &ana.app.router,
        "/api/v1/income",
        json!({ "asset_id": wege, "kind": "dividend", "pay_date": day_after(noon(1)).to_string(), "amount_per_unit": 0.5 }),
    )
    .await;
    assert_eq!(later.status, StatusCode::CREATED, "{}", later.text);

    let report = alerts.run_digest(day0).await.unwrap();
    assert_eq!(
        (report.ran, report.sent, report.silent, report.failed),
        (true, 1, 0, 0)
    );
    let texts = fake.texts_to(ana.chat);
    assert_eq!(texts.len(), 1);
    assert!(texts[0].starts_with("Prumo - resumo de "), "{}", texts[0]);
    assert!(
        texts[0].contains("KNRI11 paga amanha: R$ 11,00 liquidos."),
        "{}",
        texts[0]
    );
    assert!(!texts[0].contains("WEGE3"), "{}", texts[0]);
    assert!(!texts[0].contains("Classes fora da meta"), "{}", texts[0]);
    assert!(texts[0].is_ascii());
    assert_eq!(
        db.store.alert_settings(ana.user).await.unwrap().last_run_on,
        Some(local_date(day0))
    );

    // Duas rodadas no mesmo dia: o resumo sai uma vez só.
    let again = alerts
        .run_digest(day0 + time::Duration::hours(3))
        .await
        .unwrap();
    assert_eq!((again.sent, again.silent, again.failed), (0, 0, 0));
    assert_eq!(fake.texts_to(ana.chat).len(), 1);

    // Dia 1: o provento de WEGE3 cai no dia 2, e a classe fora da meta já foi
    // guardada: nada novo para as classes. Resumo só do provento de amanhã.
    let report = alerts.run_digest(noon(1)).await.unwrap();
    assert_eq!(report.sent, 1);
    let texts = fake.texts_to(ana.chat);
    assert!(
        texts[1].contains("WEGE3 paga amanha: R$ 10,00 liquidos."),
        "{}",
        texts[1]
    );
    assert!(!texts[1].contains("Classes fora da meta"), "{}", texts[1]);

    // Dia 2: nada novo, nada enviado — e o dia fica marcado.
    let report = alerts.run_digest(noon(2)).await.unwrap();
    assert_eq!((report.sent, report.silent), (0, 1));
    assert_eq!(fake.texts_to(ana.chat).len(), 2);

    // Dia 3: WEGE3 ganha um teto acima do preço (R$ 50): voltou a ser comprável.
    // O objetivo é atingido. A regra de saúde passa a avisar de KNRI11.
    let ceiling = patch(
        &ana.app.router,
        &format!("/api/v1/assets/{wege}"),
        json!({ "max_price": 60 }),
    )
    .await;
    assert_eq!(ceiling.status, StatusCode::OK, "{}", ceiling.text);
    let goal = put(
        &ana.app.router,
        "/api/v1/income/goal",
        json!({ "target": 100 }),
    )
    .await;
    assert!(goal.status.is_success(), "{}", goal.text);
    sqlx::query("update objectives set achieved_at = now() where user_id = $1")
        .bind(ana.user)
        .execute(&pool)
        .await
        .unwrap();
    let rules = put(
        &ana.app.router,
        "/api/v1/health/rules",
        json!({ "rules": { "max_asset": { "threshold": 50 } } }),
    )
    .await;
    assert_eq!(rules.status, StatusCode::OK);

    let report = alerts.run_digest(noon(3)).await.unwrap();
    assert_eq!(report.sent, 1);
    let texts = fake.texts_to(ana.chat);
    let digest = &texts[2];
    assert!(
        digest.contains("WEGE3 voltou a ficar no seu teto de preco: R$ 50,00 (teto R$ 60,00)."),
        "{digest}"
    );
    assert!(
        digest.contains("Objetivo de renda mensal atingido: R$ 100,00."),
        "{digest}"
    );
    assert!(
        digest.contains("KNRI11 esta com 58,68% da carteira (limite 50%)."),
        "{digest}"
    );
    assert!(!digest.contains("Classes fora da meta"), "{digest}");
    assert!(!digest.contains("paga amanha"), "{digest}");

    // Dia 4: a mesma situação não se repete.
    let report = alerts.run_digest(noon(4)).await.unwrap();
    assert_eq!((report.sent, report.silent), (0, 1));
    assert_eq!(fake.texts_to(ana.chat).len(), 3);

    // Dia 5: as classes voltam para perto da meta (60/40): nada a dizer.
    put(
        &ana.app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 40, "br_reits": 60 } }),
    )
    .await;
    let report = alerts.run_digest(noon(5)).await.unwrap();
    assert_eq!((report.sent, report.silent), (0, 1));

    // Dia 6: saem da meta de novo (50/50 e limite 3 p.p.): agora avisa.
    put(
        &ana.app.router,
        "/api/v1/goals",
        json!({ "targets": { "br_stocks": 50, "br_reits": 50 } }),
    )
    .await;
    let report = alerts.run_digest(noon(6)).await.unwrap();
    assert_eq!(report.sent, 1);
    let digest = fake.texts_to(ana.chat).pop().unwrap();
    assert!(digest.contains("Classes fora da meta"), "{digest}");
    assert!(
        digest.contains(
            "Fundos Imobiliarios esta com 58,68% da carteira (meta 50%), 8,68 p.p. acima."
        ),
        "{digest}"
    );
    assert!(
        digest
            .contains("Acoes Nacionais esta com 41,32% da carteira (meta 50%), 8,68 p.p. abaixo."),
        "{digest}"
    );
    // O preço e o objetivo não voltam: seguem valendo, já avisados.
    assert!(!digest.contains("teto de preco"), "{digest}");
    assert!(!digest.contains("Objetivo"), "{digest}");
}

#[tokio::test]
async fn types_hours_and_hidden_values_are_respected() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let alerts = service(&db.store, &fake);
    let bia = person(&db, &alerts, "bia@prumo.test", 5001).await;
    let caio = person(&db, &alerts, "caio@prumo.test", 5002).await;
    let (knri, _) = build_portfolio(&bia).await;
    let (caio_knri, _) = build_portfolio(&caio).await;
    let day0 = noon(0);
    let tomorrow = day_after(day0).to_string();
    for (p, asset) in [(&bia, &knri), (&caio, &caio_knri)] {
        let created = post(
            &p.app.router,
            "/api/v1/income",
            json!({ "asset_id": asset, "kind": "reit_income", "pay_date": tomorrow, "amount_per_unit": 2 }),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    }

    // Bia: horário às 13:00 — meio-dia ainda não chegou; Caio: sem valores.
    let saved = put(
        &bia.app.router,
        "/api/v1/alerts",
        json!({ "send_at": "13:00" }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);
    let saved = put(
        &caio.app.router,
        "/api/v1/alerts",
        json!({ "hide_values": true }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);

    let report = alerts.run_digest(day0).await.unwrap();
    assert_eq!(report.sent, 1, "só Caio estava na hora");
    assert!(fake.texts_to(bia.chat).is_empty());
    let sent = fake.texts_to(caio.chat);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].contains("KNRI11 paga amanha."), "{}", sent[0]);
    assert!(!sent[0].contains("R$"), "{}", sent[0]);

    // 13:00 chegou: agora é a vez da Bia, com valores. O resumo de cada um é
    // só dele (mesmo ativo, outra carteira).
    let report = alerts
        .run_digest(day0 + time::Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    let sent = fake.texts_to(bia.chat);
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].contains("KNRI11 paga amanha: R$ 20,00 liquidos."),
        "{}",
        sent[0]
    );
    assert_eq!(fake.texts_to(caio.chat).len(), 1);

    // Desligar o tipo tira o provento do resumo (e ligar de novo traz de volta).
    let off = put(
        &bia.app.router,
        "/api/v1/alerts",
        json!({ "types": { "income_tomorrow": { "enabled": false } } }),
    )
    .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text);
    let report = alerts.run_digest(noon(1)).await.unwrap();
    assert_eq!(report.sent, 0);
    assert_eq!(fake.texts_to(bia.chat).len(), 1);
}

#[tokio::test]
async fn a_delivery_that_fails_is_retried_and_a_blocked_bot_unlinks() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let alerts = service(&db.store, &fake);
    let dani = person(&db, &alerts, "dani@prumo.test", 6001).await;
    let (knri, _) = build_portfolio(&dani).await;
    let day0 = noon(0);
    let created = post(
        &dani.app.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day_after(day0).to_string(), "amount_per_unit": 1 }),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);

    // Telegram fora do ar: não marca o dia, tenta de novo na próxima rodada.
    fake.fail_next_send(TelegramError::Network("fora do ar".into()));
    let report = alerts.run_digest(day0).await.unwrap();
    assert_eq!((report.sent, report.failed), (0, 1));
    assert_eq!(
        db.store
            .alert_settings(dani.user)
            .await
            .unwrap()
            .last_run_on,
        None
    );
    let report = alerts
        .run_digest(day0 + time::Duration::minutes(1))
        .await
        .unwrap();
    assert_eq!((report.sent, report.failed), (1, 0));
    assert_eq!(fake.texts_to(dani.chat).len(), 1);

    // 429 com retry_after curto: espera e reenvia na mesma rodada.
    let day1 = noon(1);
    post(
        &dani.app.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day_after(day1).to_string(), "amount_per_unit": 1 }),
    )
    .await;
    let before = fake.send_calls.load(Ordering::SeqCst);
    fake.fail_next_send(TelegramError::RateLimited(Duration::ZERO));
    let report = alerts.run_digest(day1).await.unwrap();
    assert_eq!((report.sent, report.failed), (1, 0));
    assert_eq!(fake.send_calls.load(Ordering::SeqCst) - before, 2);
    assert_eq!(fake.texts_to(dani.chat).len(), 2);

    // Bot bloqueado pela pessoa: o chat é desvinculado em vez de falhar para sempre.
    let day2 = noon(2);
    post(
        &dani.app.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day_after(day2).to_string(), "amount_per_unit": 1 }),
    )
    .await;
    fake.fail_next_send(TelegramError::Forbidden(
        "bot was blocked by the user".into(),
    ));
    let report = alerts.run_digest(day2).await.unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(
        db.store.alert_settings(dani.user).await.unwrap().chat_id,
        None
    );
    let report = alerts
        .run_digest(day2 + time::Duration::minutes(1))
        .await
        .unwrap();
    assert_eq!((report.sent, report.silent, report.failed), (0, 0, 0));
}

// ----------------------------------------------------------------- réplicas --

#[tokio::test]
async fn only_one_replica_polls_and_only_one_sends_the_digest() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let first = service(&db.store, &fake);
    let second = service(&db.store, &fake);
    let eva = person(&db, &first, "eva@prumo.test", 7001).await;
    let (knri, _) = build_portfolio(&eva).await;
    let day0 = noon(0);
    post(
        &eva.app.router,
        "/api/v1/income",
        json!({ "asset_id": knri, "kind": "reit_income", "pay_date": day_after(day0).to_string(), "amount_per_unit": 1 }),
    )
    .await;

    // Leitura: com a outra réplica no lock, esta nem chama o Telegram.
    let polling = db.store.try_job_lock(POLL_LOCK_KEY).await.unwrap().unwrap();
    assert_eq!(second.poll_cycle().await.unwrap(), PollOutcome::NotLeader);
    assert!(fake.offsets.lock().unwrap().is_empty());
    polling.release().await;
    assert_eq!(
        second.poll_cycle().await.unwrap(),
        PollOutcome::Polled { updates: 0 }
    );

    // Resumo: com o lock ocupado, nada é apurado nem enviado.
    let digest = db
        .store
        .try_job_lock(DIGEST_LOCK_KEY)
        .await
        .unwrap()
        .unwrap();
    let skipped = second.run_digest(day0).await.unwrap();
    assert!(!skipped.ran);
    assert!(fake.messages().is_empty());
    digest.release().await;

    // Duas réplicas na mesma hora: um resumo só.
    let (a, b) = tokio::join!(first.run_digest(day0), second.run_digest(day0));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.sent + b.sent, 1);
    assert_eq!(fake.texts_to(eva.chat).len(), 1);
}

// --------------------------------------------------------------------- API --

#[tokio::test]
async fn the_api_shows_state_validates_changes_and_drives_linking() {
    let db = database().await;
    let fake = FakeTelegram::new();
    let alerts = service(&db.store, &fake);
    let fabio = person(&db, &alerts, "fabio@prumo.test", 8001).await;
    // Uma pessoa sem Telegram vinculado e com o servidor sem token.
    let dir = tempfile_dir::Dir::new();
    let off = app(
        &db.store,
        "gabi@prumo.test",
        FakeQuotes::with_market(),
        &dir.path(),
    );

    // Servidor sem token: a tela diz, sem erro.
    let state = get(&off.router, "/api/v1/alerts").await;
    assert_eq!(state.status, StatusCode::OK, "{}", state.text);
    assert_eq!(state.body["telegram"]["configured"], false);
    assert_eq!(state.body["telegram"]["bot_username"], Value::Null);
    assert_eq!(state.body["telegram"]["linked"], false);
    let link = post(&off.router, "/api/v1/alerts/telegram", json!({})).await;
    assert_eq!(link.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(link.body["detail"].as_str().unwrap().contains("token"));
    let test = post(&off.router, "/api/v1/alerts/test", json!({})).await;
    assert_eq!(test.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Estado de fábrica de quem já está vinculado.
    let state = get(&fabio.app.router, "/api/v1/alerts").await;
    assert_eq!(state.status, StatusCode::OK, "{}", state.text);
    assert_eq!(state.body["telegram"]["configured"], true);
    assert_eq!(state.body["telegram"]["bot_username"], "prumo_exemplo_bot");
    assert_eq!(state.body["telegram"]["linked"], true);
    assert!(state.body["telegram"]["linked_at"].is_string());
    assert_eq!(state.body["send_at"], "08:00");
    assert_eq!(state.body["hide_values"], false);
    let types = &state.body["types"];
    assert_eq!(types.as_array().unwrap().len(), 6);
    assert_eq!(find(types, "kind", "income_tomorrow")["enabled"], true);
    let drift = find(types, "kind", "class_drift");
    assert_eq!(
        (drift["threshold"].clone(), drift["unit"].clone()),
        (json!(5.0), json!("percentage_points"))
    );
    let tax = find(types, "kind", "tax_exemption");
    assert_eq!(
        (tax["enabled"].clone(), tax["available"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(tax["default_threshold"], 80.0);

    // Mudanças: horário, sem valores, tipos e limite.
    let saved = put(
        &fabio.app.router,
        "/api/v1/alerts",
        json!({
            "send_at": "07:30",
            "hide_values": true,
            "types": {
                "class_drift": { "threshold": 7.5 },
                "health_warning": { "enabled": false }
            }
        }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);
    assert_eq!(saved.body["send_at"], "07:30");
    assert_eq!(saved.body["hide_values"], true);
    assert_eq!(
        find(&saved.body["types"], "kind", "class_drift")["threshold"],
        7.5
    );
    assert_eq!(
        find(&saved.body["types"], "kind", "health_warning")["enabled"],
        false
    );
    assert_eq!(
        find(&saved.body["types"], "kind", "income_tomorrow")["enabled"],
        true
    );
    let reread = get(&fabio.app.router, "/api/v1/alerts").await;
    assert_eq!(reread.body, saved.body);

    // O que é recusado (422) e não muda nada.
    for (bad, why) in [
        (json!({}), "corpo vazio"),
        (json!({ "send_at": "24:00" }), "hora fora do relógio"),
        (json!({ "send_at": "8:00" }), "hora sem zero"),
        (
            json!({ "types": { "outro": { "enabled": true } } }),
            "tipo desconhecido",
        ),
        (
            json!({ "types": { "class_drift": {} } }),
            "tipo sem mudança",
        ),
        (
            json!({ "types": { "class_drift": { "threshold": 0 } } }),
            "limite zero",
        ),
        (
            json!({ "types": { "class_drift": { "threshold": 101 } } }),
            "limite acima de 100",
        ),
        (
            json!({ "types": { "class_drift": { "threshold": 1.234 } } }),
            "três casas",
        ),
        (
            json!({ "types": { "income_tomorrow": { "threshold": 5 } } }),
            "tipo sem limite",
        ),
        (
            json!({ "types": { "tax_exemption": { "enabled": true } } }),
            "ainda indisponível",
        ),
        (json!({ "x": 1 }), "campo desconhecido"),
    ] {
        let refused = put(&fabio.app.router, "/api/v1/alerts", bad).await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{why}: {}",
            refused.text
        );
    }
    assert_eq!(
        get(&fabio.app.router, "/api/v1/alerts").await.body,
        saved.body
    );

    // Desligar o indisponível é inofensivo; o limite do isentômetro se guarda.
    let tax = put(
        &fabio.app.router,
        "/api/v1/alerts",
        json!({ "types": { "tax_exemption": { "enabled": false, "threshold": 90 } } }),
    )
    .await;
    assert_eq!(tax.status, StatusCode::OK, "{}", tax.text);
    assert_eq!(
        find(&tax.body["types"], "kind", "tax_exemption")["threshold"],
        90.0
    );

    // "Enviar teste agora": sem dado da carteira, no horário configurado; o
    // segundo pedido em seguida espera.
    let sent = post(&fabio.app.router, "/api/v1/alerts/test", json!({})).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text);
    let texts = fake.texts_to(fabio.chat);
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("07:30"), "{}", texts[0]);
    assert!(!texts[0].contains("R$"));
    let soon = post(&fabio.app.router, "/api/v1/alerts/test", json!({})).await;
    assert_eq!(soon.status, StatusCode::TOO_MANY_REQUESTS, "{}", soon.text);
    assert_eq!(soon.body["code"], "too_many_requests");
    assert_eq!(TEST_COOLDOWN, Duration::from_secs(10));

    // Vincular pela API: código, depois o /start do chat, depois desvincular.
    let gabi = person(&db, &alerts, "helena@prumo.test", 8002).await;
    assert!(db.store.unlink_alert_chat(gabi.user).await.unwrap());
    let unlinked_test = post(&gabi.app.router, "/api/v1/alerts/test", json!({})).await;
    assert_eq!(unlinked_test.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(unlinked_test.body["detail"]
        .as_str()
        .unwrap()
        .contains("Vincule"));
    let link = post(&gabi.app.router, "/api/v1/alerts/telegram", json!({})).await;
    assert_eq!(link.status, StatusCode::CREATED, "{}", link.text);
    let code = link.body["code"].as_str().unwrap().to_owned();
    assert_eq!(link.body["command"], format!("/start {code}"));
    assert_eq!(link.body["bot_username"], "prumo_exemplo_bot");
    assert!(link.body["expires_at"].is_string());
    fake.deliver(vec![message(
        900,
        8003,
        "private",
        &format!("/start {code}"),
    )]);
    alerts.poll_cycle().await.unwrap();
    let after = get(&gabi.app.router, "/api/v1/alerts").await;
    assert_eq!(after.body["telegram"]["linked"], true);
    let gone = delete(&gabi.app.router, "/api/v1/alerts/telegram").await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let after = get(&gabi.app.router, "/api/v1/alerts").await;
    assert_eq!(after.body["telegram"]["linked"], false);

    // O Telegram que recusa o teste vira 502 com texto da pessoa, sem vazar o motivo cru.
    db.store
        .link_alert_chat(gabi.user, 8004, now())
        .await
        .unwrap();
    fake.fail_next_send(TelegramError::Api {
        status: 500,
        description: "detalhe interno do telegram".into(),
    });
    let broken = post(&gabi.app.router, "/api/v1/alerts/test", json!({})).await;
    assert_eq!(broken.status, StatusCode::BAD_GATEWAY, "{}", broken.text);
    assert_eq!(broken.body["code"], "telegram_unavailable");
    assert!(!broken.text.contains("detalhe interno"), "{}", broken.text);
}
