//! Alertas pelo Telegram: vincular o chat, ler o que a pessoa manda ao bot,
//! apurar o resumo e enviá-lo uma vez por dia. As regras do que vale um aviso e
//! o texto estão em [`crate::domain::alerts`]; o passo a passo para a pessoa,
//! em `docs/alertas.md`.
//!
//! Com várias réplicas, duas tarefas disputam advisory locks do Postgres
//! (como a tarefa diária de desempenho):
//!
//! - [`POLL_LOCK_KEY`]: só uma réplica por vez lê o `getUpdates` (duas leriam
//!   as mesmas mensagens, e o Telegram devolve 409 a quem chega por segundo);
//! - [`DIGEST_LOCK_KEY`]: só uma réplica por vez apura e envia o resumo. A
//!   apuração marca o dia da pessoa (`last_run_on`) antes de soltar o lock, e
//!   quem pega o lock depois não encontra mais ninguém devido.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::alerts::{
    self as rules, AlertConfig, AlertKind, CeilingAsset, ClassShare, Item, LINK_CODE_TTL_MINUTES,
};
use crate::domain::health::{self, Warning};
use crate::http::health::health_assets;
use crate::performance::{local_date, BRASILIA};
use crate::service;
use crate::store::{IncomeFilter, Store, StoreError};
use crate::telegram::{TelegramApi, TelegramError};

/// Advisory lock do resumo diário ("PRUMO" + migração 0017).
pub const DIGEST_LOCK_KEY: i64 = 0x50_52_55_4d_4f_17;
/// Advisory lock da leitura do Telegram.
pub const POLL_LOCK_KEY: i64 = 0x50_52_55_4d_4f_27;

/// Quanto o `getUpdates` espera por mensagem nova.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(25);
/// De quanto em quanto tempo se confere quem já passou do horário.
pub const DIGEST_EVERY: Duration = Duration::from_secs(60);
/// Intervalo mínimo entre dois "enviar teste" da mesma pessoa.
pub const TEST_COOLDOWN: Duration = Duration::from_secs(10);
/// O Telegram pode pedir para esperar; acima disto o envio fica para a próxima rodada.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);

const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum AlertError {
    #[error("os alertas estao desligados neste servidor")]
    NotConfigured,
    #[error("nenhum chat vinculado")]
    NotLinked,
    #[error("aguarde alguns segundos antes de pedir outro teste")]
    TooSoon,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Telegram(#[from] TelegramError),
}

/// Um código de vinculação recém-criado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCode {
    pub code: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollOutcome {
    /// Outra réplica está lendo o Telegram.
    NotLeader,
    Polled {
        updates: usize,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DigestReport {
    /// `false`: outra réplica estava com o lock.
    pub ran: bool,
    pub sent: usize,
    /// Apurados sem nada a dizer.
    pub silent: usize,
    pub failed: usize,
}

pub struct AlertService {
    store: Store,
    api: Option<Arc<dyn TelegramApi>>,
    username: Mutex<Option<String>>,
    tests: Mutex<HashMap<Uuid, Instant>>,
    poll_timeout: Duration,
}

impl AlertService {
    pub fn new(store: Store, api: Option<Arc<dyn TelegramApi>>) -> Self {
        Self {
            store,
            api,
            username: Mutex::new(None),
            tests: Mutex::default(),
            poll_timeout: POLL_TIMEOUT,
        }
    }

    /// Sem token: os alertas ficam desligados.
    pub fn disabled(store: Store) -> Self {
        Self::new(store, None)
    }

    /// Outro tempo de espera do `getUpdates` (os testes usam zero).
    pub fn with_poll_timeout(mut self, timeout: Duration) -> Self {
        self.poll_timeout = timeout;
        self
    }

    pub fn configured(&self) -> bool {
        self.api.is_some()
    }

    /// O `@usuario` do bot, buscado uma vez. `None` se o bot não está
    /// configurado ou o Telegram não respondeu (tenta de novo na próxima).
    pub async fn bot_username(&self) -> Option<String> {
        let api = self.api.as_ref()?;
        let mut cached = self.username.lock().await;
        if cached.is_none() {
            match api.username().await {
                Ok(name) => *cached = Some(name),
                Err(error) => tracing::warn!(error = %error, "usuario do bot nao obtido"),
            }
        }
        cached.clone()
    }

    fn api(&self) -> Result<&Arc<dyn TelegramApi>, AlertError> {
        self.api.as_ref().ok_or(AlertError::NotConfigured)
    }

    // ------------------------------------------------------- vinculação --

    /// Um código novo para a pessoa mandar ao bot (`/start <código>`). Vale
    /// [`LINK_CODE_TTL_MINUTES`] minutos e uma vez só; o anterior dela morre.
    pub async fn new_link_code(
        &self,
        user: Uuid,
        now: OffsetDateTime,
    ) -> Result<LinkCode, AlertError> {
        self.api()?;
        let expires_at = now + time::Duration::minutes(LINK_CODE_TTL_MINUTES);
        for _ in 0..5 {
            let code = random_code();
            match self.store.create_link_code(user, &code, expires_at).await {
                Ok(()) => return Ok(LinkCode { code, expires_at }),
                Err(StoreError::Duplicate) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(StoreError::Duplicate.into())
    }

    pub async fn unlink(&self, user: Uuid) -> Result<bool, AlertError> {
        Ok(self.store.unlink_alert_chat(user).await?)
    }

    /// "Enviar teste agora": uma mensagem curta, sem dado da carteira.
    pub async fn send_test(&self, user: Uuid) -> Result<(), AlertError> {
        let api = self.api()?;
        let settings = self.store.alert_settings(user).await?;
        let chat = settings.chat_id.ok_or(AlertError::NotLinked)?;
        {
            let mut marks = self.tests.lock().await;
            let now = Instant::now();
            if marks
                .get(&user)
                .is_some_and(|at| now.duration_since(*at) < TEST_COOLDOWN)
            {
                return Err(AlertError::TooSoon);
            }
            marks.insert(user, now);
        }
        self.deliver(api, chat, &rules::test_message(settings.send_at))
            .await
    }

    /// Envia; se o Telegram pede para esperar um tempo curto, espera e tenta
    /// mais uma vez. Chat que o Telegram recusa (bot bloqueado) é desvinculado.
    async fn deliver(
        &self,
        api: &Arc<dyn TelegramApi>,
        chat: i64,
        text: &str,
    ) -> Result<(), AlertError> {
        let mut result = api.send(chat, text).await;
        if let Err(TelegramError::RateLimited(wait)) = &result {
            if *wait <= MAX_RETRY_WAIT {
                tokio::time::sleep(*wait).await;
                result = api.send(chat, text).await;
            }
        }
        if let Err(TelegramError::Forbidden(reason)) = &result {
            tracing::warn!(reason = %reason, "telegram recusou o chat: desvinculando");
            self.store.unlink_alert_chat_by_id(chat).await?;
        }
        Ok(result?)
    }

    // ----------------------------------------------------------- polling --

    /// Uma rodada de leitura: pega o lock, lê o `getUpdates` a partir do
    /// `offset` guardado, trata cada mensagem e guarda o novo `offset`.
    pub async fn poll_cycle(&self) -> Result<PollOutcome, AlertError> {
        let api = self.api()?;
        let Some(lock) = self.store.try_job_lock(POLL_LOCK_KEY).await? else {
            return Ok(PollOutcome::NotLeader);
        };
        let outcome = self.poll_locked(api).await;
        lock.release().await;
        outcome
    }

    async fn poll_locked(&self, api: &Arc<dyn TelegramApi>) -> Result<PollOutcome, AlertError> {
        let offset = self.store.telegram_offset().await?;
        let updates = api.updates(offset, self.poll_timeout).await?;
        let count = updates.len();
        let mut next = offset;
        for update in updates {
            self.handle_message(api, &update).await?;
            next = next.max(update.update_id + 1);
        }
        if next > offset {
            self.store.save_telegram_offset(next).await?;
        }
        Ok(PollOutcome::Polled { updates: count })
    }

    /// O que a pessoa mandou ao bot: `/start <código>` vincula, `/stop`
    /// desvincula, o resto é ignorado. Só conversa privada.
    async fn handle_message(
        &self,
        api: &Arc<dyn TelegramApi>,
        update: &crate::telegram::Update,
    ) -> Result<(), AlertError> {
        let Some(message) = &update.message else {
            return Ok(());
        };
        if message.chat.kind != "private" {
            return Ok(());
        }
        let chat = message.chat.id;
        let text = message.text.as_deref().unwrap_or("").trim();
        let mut words = text.split_whitespace();
        let command = words.next().unwrap_or("");
        // `/start@NomeDoBot` vale como `/start` (é assim que o Telegram manda em grupo).
        let command = command.split('@').next().unwrap_or("");
        let reply = match command {
            "/start" => match words.next() {
                None => "Para vincular, abra a tela Alertas do Prumo, gere um código e mande aqui: /start CÓDIGO.".to_owned(),
                Some(code) => self.link(chat, code).await?,
            },
            "/stop" => {
                if self.store.unlink_alert_chat_by_id(chat).await? {
                    "Desvinculado: você não receberá mais o resumo do Prumo.".to_owned()
                } else {
                    "Este chat não estava vinculado a nenhum Prumo.".to_owned()
                }
            }
            _ => return Ok(()),
        };
        // A resposta é cortesia: se não sair, o vínculo (já gravado) vale.
        if let Err(error) = api.send(chat, &reply).await {
            tracing::warn!(error = %error, "resposta ao bot nao enviada");
        }
        Ok(())
    }

    async fn link(&self, chat: i64, code: &str) -> Result<String, AlertError> {
        let now = OffsetDateTime::now_utc();
        let code = code.to_ascii_uppercase();
        let Some(user) = self.store.consume_link_code(&code, now).await? else {
            return Ok(
                "Código inválido ou vencido. Gere um novo na tela Alertas do Prumo.".to_owned(),
            );
        };
        self.store.link_alert_chat(user, chat, now).await?;
        Ok("Pronto: este chat está vinculado ao seu Prumo. O resumo diário chega por aqui, só quando houver algo a dizer. Para parar, mande /stop.".to_owned())
    }

    /// Lê o Telegram até o processo encerrar. Sem token, nem começa.
    pub fn spawn_polling(self: Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        self.configured().then(|| {
            tokio::spawn(async move {
                loop {
                    let started = Instant::now();
                    let pause = match self.poll_cycle().await {
                        Ok(PollOutcome::Polled { .. }) => Duration::ZERO,
                        Ok(PollOutcome::NotLeader) => Duration::from_secs(30),
                        Err(AlertError::Telegram(TelegramError::RateLimited(wait))) => wait,
                        Err(error) => {
                            tracing::warn!(error = %error, "leitura do telegram falhou");
                            Duration::from_secs(15)
                        }
                    };
                    // Sem esta folga, um `getUpdates` que volta na hora viraria laço apertado.
                    let floor = Duration::from_secs(1).saturating_sub(started.elapsed());
                    tokio::time::sleep(pause.max(floor)).await;
                }
            })
        })
    }

    // ------------------------------------------------------------ resumo --

    /// Apura e envia o resumo de quem já passou do horário e ainda não o
    /// recebeu hoje. Só roda com o lock.
    pub async fn run_digest(&self, now: OffsetDateTime) -> Result<DigestReport, AlertError> {
        let api = self.api()?;
        let Some(lock) = self.store.try_job_lock(DIGEST_LOCK_KEY).await? else {
            return Ok(DigestReport::default());
        };
        let report = self.digest_locked(api, now).await;
        lock.release().await;
        report
    }

    async fn digest_locked(
        &self,
        api: &Arc<dyn TelegramApi>,
        now: OffsetDateTime,
    ) -> Result<DigestReport, AlertError> {
        let today = local_date(now);
        let clock = now.to_offset(BRASILIA).time();
        let mut report = DigestReport {
            ran: true,
            ..DigestReport::default()
        };
        for user in self.store.alert_users_due(today, clock).await? {
            match self.digest_for(api, user, today).await {
                Ok(true) => report.sent += 1,
                Ok(false) => report.silent += 1,
                Err(error) => {
                    report.failed += 1;
                    // Sem marcar o dia: a próxima rodada tenta de novo.
                    tracing::warn!(error = %error, "resumo diario nao enviado");
                }
            }
        }
        Ok(report)
    }

    /// O resumo de uma pessoa. `true` se enviou, `false` se não havia nada.
    async fn digest_for(
        &self,
        api: &Arc<dyn TelegramApi>,
        user: Uuid,
        today: Date,
    ) -> Result<bool, AlertError> {
        let settings = self.store.alert_settings(user).await?;
        let Some(chat) = settings.chat_id else {
            return Ok(false);
        };
        let (config, seeded) = self.store.alert_config(user).await?;
        let view = service::portfolio(&self.store, user).await?;

        let mut items: Vec<Item> = Vec::new();
        // Tipos de transição: o que vale agora, para comparar com o que já foi avisado.
        let mut replace: Vec<(AlertKind, Vec<String>)> = Vec::new();
        for kind in config.active() {
            let found = self.collect(user, kind, &config, &view, today).await?;
            if !kind.is_transition() {
                items.extend(found);
                continue;
            }
            let keys: Vec<String> = found.iter().map(Item::key).collect();
            if seeded.contains(&kind) {
                let seen = self.store.alert_seen(user, kind).await?;
                items.extend(rules::new_items(found, &seen));
                replace.push((kind, keys));
            } else {
                // Primeira apuração do tipo: guarda o que já vale, sem avisar.
                self.store
                    .replace_alert_seen(user, kind, config.get(kind), &keys)
                    .await?;
            }
        }

        let sent = match rules::render_digest(today, &items, settings.hide_values) {
            Some(text) => {
                self.deliver(api, chat, &text).await?;
                true
            }
            None => false,
        };
        for (kind, keys) in replace {
            self.store
                .replace_alert_seen(user, kind, config.get(kind), &keys)
                .await?;
        }
        self.store.mark_alert_run(user, today).await?;
        Ok(sent)
    }

    /// O que vale hoje para um tipo.
    async fn collect(
        &self,
        user: Uuid,
        kind: AlertKind,
        config: &AlertConfig,
        view: &service::PortfolioView,
        today: Date,
    ) -> Result<Vec<Item>, AlertError> {
        Ok(match kind {
            AlertKind::IncomeTomorrow => self.income_tomorrow(user, today).await?,
            AlertKind::ClassDrift => {
                let limit = config
                    .get(kind)
                    .threshold
                    .or_else(|| kind.default_threshold())
                    .unwrap_or(Decimal::ONE_HUNDRED);
                let shares: Vec<ClassShare> = view
                    .classes
                    .iter()
                    .map(|c| ClassShare {
                        class: c.asset_class,
                        target: c.target_percent,
                        current: c.current_percent,
                    })
                    .collect();
                rules::class_drifts(&shares, limit, view.total_value <= Decimal::ZERO)
            }
            AlertKind::PriceBelowCeiling => {
                let assets: Vec<CeilingAsset> = view
                    .assets
                    .iter()
                    .map(|a| CeilingAsset {
                        asset_id: a.asset.id,
                        label: a.label(),
                        price: a.unit_price,
                        ceiling: a.asset.max_price,
                    })
                    .collect();
                rules::below_ceiling(&assets)
            }
            AlertKind::ObjectiveAchieved => self
                .store
                .achieved_objectives(user)
                .await?
                .into_iter()
                .map(|o| Item::ObjectiveAchieved {
                    id: o.id,
                    kind: o.kind,
                    target: o.target,
                })
                .collect(),
            AlertKind::HealthWarning => {
                let health_config = self.store.health_config(user).await?;
                let income = self.store.income_history(user, today).await?;
                let evaluation =
                    health::evaluate(&health_config, &health_assets(view), view.targets, &income);
                evaluation
                    .warnings
                    .into_iter()
                    .map(|w: Warning| Item::Health(w))
                    .collect()
            }
            // O isentômetro ainda não é apurado (ver `AlertKind::is_available`).
            AlertKind::TaxExemption => Vec::new(),
        })
    }

    /// Os proventos que caem amanhã, somados por pagador.
    async fn income_tomorrow(&self, user: Uuid, today: Date) -> Result<Vec<Item>, AlertError> {
        let Some(tomorrow) = today.next_day() else {
            return Ok(Vec::new());
        };
        let events = self
            .store
            .income_events(
                user,
                &IncomeFilter {
                    from: Some(tomorrow),
                    to: Some(tomorrow),
                    asset: None,
                    status: None,
                    today,
                    limit: 200,
                },
            )
            .await?;
        let mut by_payer: Vec<(String, Decimal)> = Vec::new();
        for event in events {
            let label = event
                .symbol
                .clone()
                .unwrap_or_else(|| event.display_name.clone());
            match by_payer.iter_mut().find(|(name, _)| *name == label) {
                Some((_, net)) => *net += event.net,
                None => by_payer.push((label, event.net)),
            }
        }
        by_payer.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(by_payer
            .into_iter()
            .map(|(label, net)| Item::IncomeTomorrow { label, net })
            .collect())
    }

    /// Confere a cada minuto quem já passou do horário, até o processo encerrar.
    pub fn spawn_digest(self: Arc<Self>, every: Duration) -> Option<tokio::task::JoinHandle<()>> {
        self.configured().then(|| {
            tokio::spawn(async move {
                loop {
                    match self.run_digest(OffsetDateTime::now_utc()).await {
                        Ok(report) if report.sent + report.failed > 0 => tracing::info!(
                            sent = report.sent,
                            silent = report.silent,
                            failed = report.failed,
                            "resumo diario de alertas"
                        ),
                        Ok(_) => {}
                        Err(error) => tracing::warn!(error = %error, "resumo diario falhou"),
                    }
                    tokio::time::sleep(every).await;
                }
            })
        })
    }
}

/// Dez caracteres sem I, O, 0 e 1 (fáceis de confundir ao digitar): 50 bits.
fn random_code() -> String {
    let bytes = *Uuid::new_v4().as_bytes();
    bytes[..CODE_LEN]
        .iter()
        .map(|b| char::from(CODE_ALPHABET[usize::from(b & 31)]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_codes_use_the_unambiguous_alphabet() {
        for _ in 0..200 {
            let code = random_code();
            assert_eq!(code.len(), CODE_LEN);
            assert!(code
                .bytes()
                .all(|b| CODE_ALPHABET.contains(&b) && !b"IO01".contains(&b)));
        }
        assert_ne!(random_code(), random_code());
    }
}
