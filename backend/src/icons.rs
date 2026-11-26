//! Logos dos ativos: busca na brapi, saneamento, cache no banco e a tarefa em
//! segundo plano que mantém tudo em dia. O que as telas veem sai de
//! `http/icons.rs`; o método, os limites e o porquê de cada escolha estão em
//! `docs/icones.md`.
//!
//! **Nenhuma requisição espera pela rede.** A rota só lê o cache. Quem o enche é
//! a rodada periódica ([`IconService::spawn_refresher`]) e a busca disparada
//! quando um ativo nasce ([`IconService::prefetch`]).
//!
//! Cada símbolo é buscado **uma vez** e revalidado só depois de 30 dias, porque
//! o plano gratuito da brapi tem 15 mil pedidos por mês, um ticker por pedido e
//! um pedido por vez. Por isso:
//!
//! - as buscas de uma réplica passam por uma fila de um ([`IconService::gate`]);
//! - duas réplicas não buscam o mesmo símbolo: [`Store::claim_icons`] reserva os
//!   símbolos numa transação curta sob um advisory lock, e a reserva fica no
//!   banco. **Nenhuma conexão fica presa durante a ida à rede**: o pool tem 4;
//! - resposta 429, token recusado ou plano sem o recurso paralisam a tarefa por
//!   um tempo, em vez de insistir; falha de rede só vale para o símbolo, e três
//!   seguidas encerram a rodada.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use time::OffsetDateTime;

use crate::brapi::{LogoError, LogoSource};
use crate::domain::asset_class::Market;
use crate::domain::icon::{icon_symbol, sanitize_svg};
use crate::store::icons::{IconOutcome, IconState};
use crate::store::Store;

/// Número do advisory lock da reserva ("PRUMO" + migração 0019).
pub const LOCK_KEY: i64 = 0x50_52_55_4d_4f_19;
/// De quanto em quanto tempo a tarefa confere o que está vencido. A conferência
/// é uma leitura; só o vencido vai à brapi.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// Logo achado, ou "sem logo" confirmado, vale por isto antes de ser conferido
/// de novo: marca de empresa muda de ano em ano, e a cota do plano é curta.
pub const REVALIDATE_AFTER: time::Duration = time::Duration::days(30);
/// Depois de uma falha, o símbolo descansa isto.
pub const RETRY_FAILED_AFTER: time::Duration = time::Duration::hours(6);
/// Uma reserva que não terminou em tanto tempo é de um processo que caiu.
pub const CLAIM_TTL: time::Duration = time::Duration::minutes(15);
/// Pausa depois de o token ser recusado ou o plano não ter o recurso: tentar de
/// novo antes não adianta.
const LONG_PAUSE: time::Duration = time::Duration::hours(6);
/// Falhas de rede seguidas que encerram a rodada (a brapi está fora do ar).
const MAX_CONSECUTIVE_FAILURES: usize = 3;

/// Se o símbolo deve ser buscado agora, dado o que a última tentativa deixou.
pub fn is_due(state: Option<&IconState>, now: OffsetDateTime) -> bool {
    let Some(state) = state else {
        return true;
    };
    if state.claimed_at.is_some_and(|at| now - at < CLAIM_TTL) {
        return false;
    }
    let waited = |wait: time::Duration| state.attempted_at.is_none_or(|at| now - at >= wait);
    match state.outcome {
        // Reservado e nunca terminado: a reserva venceu, busca.
        IconOutcome::Pending => true,
        IconOutcome::Found | IconOutcome::Missing => waited(REVALIDATE_AFTER),
        IconOutcome::Failed => waited(RETRY_FAILED_AFTER),
    }
}

/// O que uma rodada fez.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoundReport {
    /// Símbolos que esta réplica reservou e foi buscar.
    pub claimed: usize,
    pub found: usize,
    pub missing: usize,
    pub failed: usize,
    /// Por que a rodada parou antes do fim, se parou.
    pub stopped: Option<String>,
}

pub struct IconService {
    store: Store,
    source: Option<Arc<dyn LogoSource>>,
    /// Um pedido por vez: é o limite do plano gratuito da brapi.
    gate: tokio::sync::Mutex<()>,
    /// Até quando a tarefa fica parada (limite, token recusado).
    paused_until: Mutex<Option<OffsetDateTime>>,
    /// As buscas disparadas por [`IconService::prefetch`] que ainda podem estar
    /// rodando: [`IconService::settle`] espera por elas.
    inflight: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl IconService {
    pub fn new(store: Store, source: Option<Arc<dyn LogoSource>>) -> Self {
        Self {
            store,
            source,
            gate: tokio::sync::Mutex::new(()),
            paused_until: Mutex::new(None),
            inflight: Mutex::new(Vec::new()),
        }
    }

    /// Sem fonte (sem `BRAPI_TOKEN`): só serve o que já está no cache.
    pub fn disabled(store: Store) -> Self {
        Self::new(store, None)
    }

    pub fn enabled(&self) -> bool {
        self.source.is_some()
    }

    fn paused(&self, now: OffsetDateTime) -> bool {
        self.paused_until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|until| now < until)
    }

    fn pause_for(&self, now: OffsetDateTime, wait: time::Duration) {
        *self
            .paused_until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(now + wait);
    }

    /// Busca os logos dos símbolos que estão vencidos (ou nunca foram buscados).
    pub async fn fill(&self, symbols: &[String], now: OffsetDateTime) -> RoundReport {
        let mut report = RoundReport::default();
        let Some(source) = self.source.as_deref() else {
            return report;
        };
        if symbols.is_empty() || self.paused(now) {
            return report;
        }
        let claimed = match self
            .store
            .claim_icons(symbols, LOCK_KEY, now, |state| is_due(state, now))
            .await
        {
            Ok(claimed) => claimed,
            Err(error) => {
                tracing::error!(error = %error, "nao foi possivel reservar os logos");
                return report;
            }
        };
        report.claimed = claimed.len();
        let mut consecutive_failures = 0;
        for (done, symbol) in claimed.iter().enumerate() {
            let step = {
                let _turn = self.gate.lock().await;
                self.fetch_one(source, symbol, now).await
            };
            match step {
                Step::Found => {
                    report.found += 1;
                    consecutive_failures = 0;
                }
                Step::Missing => {
                    report.missing += 1;
                    consecutive_failures = 0;
                }
                Step::Failed => {
                    report.failed += 1;
                    consecutive_failures += 1;
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        report.stopped = Some("a brapi nao respondeu".to_owned());
                    }
                }
                Step::Stop(reason) => {
                    report.failed += 1;
                    report.stopped = Some(reason);
                }
            }
            if report.stopped.is_some() {
                // O que sobrou volta a ficar livre para a próxima rodada.
                let rest = &claimed[done + 1..];
                if !rest.is_empty() {
                    if let Err(error) = self.store.release_icon_claims(rest).await {
                        tracing::error!(error = %error, "reserva de logo nao devolvida");
                    }
                }
                break;
            }
        }
        report
    }

    async fn fetch_one(&self, source: &dyn LogoSource, symbol: &str, now: OffsetDateTime) -> Step {
        let result = match source.fetch(symbol).await {
            Ok(logo) => match sanitize_svg(&logo.bytes) {
                Ok(svg) => self
                    .store
                    .save_icon(symbol, &svg, &logo.url, now)
                    .await
                    .map(|()| Step::Found),
                Err(reason) => {
                    tracing::warn!(symbol = %symbol, reason = %reason, "logo recusado");
                    self.store
                        .save_icon_missing(symbol, &format!("svg recusado: {reason}"), now)
                        .await
                        .map(|()| Step::Missing)
                }
            },
            Err(LogoError::NotFound) => self
                .store
                .save_icon_missing(symbol, "a brapi nao tem logo", now)
                .await
                .map(|()| Step::Missing),
            Err(LogoError::Rejected(reason)) => {
                tracing::warn!(symbol = %symbol, reason = %reason, "logo recusado");
                self.store
                    .save_icon_missing(symbol, &reason, now)
                    .await
                    .map(|()| Step::Missing)
            }
            Err(LogoError::Unavailable(reason)) => {
                tracing::warn!(symbol = %symbol, reason = %reason, "logo nao buscado");
                self.store
                    .save_icon_failure(symbol, &reason, now)
                    .await
                    .map(|()| Step::Failed)
            }
            Err(error @ LogoError::RateLimited(wait)) => {
                tracing::warn!(wait_secs = wait.as_secs(), "brapi pediu para esperar");
                let wait = time::Duration::try_from(wait)
                    .unwrap_or(LONG_PAUSE)
                    .min(LONG_PAUSE);
                self.pause_for(now, wait);
                self.store
                    .save_icon_failure(symbol, &error.to_string(), now)
                    .await
                    .map(|()| Step::Stop(error.to_string()))
            }
            Err(error @ (LogoError::Unauthorized | LogoError::Forbidden)) => {
                tracing::error!(reason = %error, "logos parados: confira o BRAPI_TOKEN e o plano");
                self.pause_for(now, LONG_PAUSE);
                self.store
                    .save_icon_failure(symbol, &error.to_string(), now)
                    .await
                    .map(|()| Step::Stop(error.to_string()))
            }
        };
        result.unwrap_or_else(|error| {
            tracing::error!(symbol = %symbol, error = %error, "logo nao gravado");
            Step::Failed
        })
    }

    /// Os logos de todas as carteiras: um por símbolo da B3.
    pub async fn refresh_all(&self, now: OffsetDateTime) -> RoundReport {
        match self.store.tracked_symbols().await {
            Ok(tracked) => {
                let symbols: Vec<String> = tracked
                    .into_iter()
                    .filter_map(|(market, symbol)| icon_target(market, Some(&symbol)))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                self.fill(&symbols, now).await
            }
            Err(error) => {
                tracing::error!(error = %error, "nao foi possivel listar os simbolos");
                RoundReport::default()
            }
        }
    }

    /// Busca em segundo plano o logo de um ativo recém-criado.
    pub fn prefetch(self: &Arc<Self>, market: Market, symbol: Option<&str>) {
        if !self.enabled() {
            return;
        }
        let Some(symbol) = icon_target(market, symbol) else {
            return;
        };
        let service = Arc::clone(self);
        let handle = tokio::spawn(async move {
            service.fill(&[symbol], OffsetDateTime::now_utc()).await;
        });
        if let Ok(mut inflight) = self.inflight.lock() {
            inflight.retain(|task| !task.is_finished());
            inflight.push(handle);
        }
    }

    /// Espera as buscas disparadas por [`IconService::prefetch`] terminarem,
    /// inclusive as que forem disparadas enquanto espera. Para quem precisa do
    /// resultado sem adivinhar quanto a busca demora (teste, encerramento).
    pub async fn settle(&self) {
        loop {
            let pending = match self.inflight.lock() {
                Ok(mut inflight) => std::mem::take(&mut *inflight),
                Err(_) => return,
            };
            if pending.is_empty() {
                return;
            }
            for task in pending {
                // Uma busca que entrou em pânico não segura quem espera.
                let _ = task.await;
            }
        }
    }

    /// Rodada periódica, até o processo encerrar: a primeira na hora.
    pub fn spawn_refresher(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let report = self.refresh_all(OffsetDateTime::now_utc()).await;
                if report.claimed > 0 {
                    tracing::info!(
                        claimed = report.claimed,
                        found = report.found,
                        missing = report.missing,
                        failed = report.failed,
                        stopped = report.stopped.as_deref().unwrap_or(""),
                        "logos conferidos"
                    );
                }
            }
        })
    }
}

enum Step {
    Found,
    Missing,
    Failed,
    /// A rodada para: limite, token recusado ou plano sem o recurso.
    Stop(String),
}

/// O símbolo cujo logo se busca: só ativo da B3 (a brapi só cobre a B3). Cripto
/// de verdade, título do Tesouro, ação dos EUA e renda fixa por valor ficam de
/// fora.
fn icon_target(market: Market, symbol: Option<&str>) -> Option<String> {
    if market != Market::B3 {
        return None;
    }
    icon_symbol(symbol?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-11-26 12:00 UTC);

    fn state(
        outcome: IconOutcome,
        attempted_ago: Option<time::Duration>,
        claimed_ago: Option<time::Duration>,
    ) -> IconState {
        IconState {
            outcome,
            attempted_at: attempted_ago.map(|ago| NOW - ago),
            claimed_at: claimed_ago.map(|ago| NOW - ago),
        }
    }

    #[test]
    fn a_new_symbol_is_due() {
        assert!(is_due(None, NOW));
    }

    #[test]
    fn found_and_missing_wait_the_month() {
        for outcome in [IconOutcome::Found, IconOutcome::Missing] {
            let fresh = state(outcome, Some(time::Duration::days(29)), None);
            assert!(!is_due(Some(&fresh), NOW), "{outcome:?}");
            let old = state(outcome, Some(time::Duration::days(30)), None);
            assert!(is_due(Some(&old), NOW), "{outcome:?}");
        }
    }

    #[test]
    fn a_failure_rests_a_few_hours() {
        let fresh = state(IconOutcome::Failed, Some(time::Duration::hours(5)), None);
        assert!(!is_due(Some(&fresh), NOW));
        let old = state(IconOutcome::Failed, Some(time::Duration::hours(6)), None);
        assert!(is_due(Some(&old), NOW));
    }

    #[test]
    fn a_recent_claim_blocks_and_an_old_one_does_not() {
        let busy = state(
            IconOutcome::Pending,
            None,
            Some(time::Duration::minutes(14)),
        );
        assert!(!is_due(Some(&busy), NOW));
        let crashed = state(
            IconOutcome::Pending,
            None,
            Some(time::Duration::minutes(16)),
        );
        assert!(is_due(Some(&crashed), NOW));
        // Reserva recente vence até um logo velho: outra réplica já o busca.
        let revalidating = state(
            IconOutcome::Found,
            Some(time::Duration::days(60)),
            Some(time::Duration::minutes(1)),
        );
        assert!(!is_due(Some(&revalidating), NOW));
    }

    #[test]
    fn only_b3_symbols_get_a_logo_lookup() {
        assert_eq!(
            icon_target(Market::B3, Some("itsa4")).as_deref(),
            Some("ITSA4")
        );
        assert_eq!(icon_target(Market::B3, Some("BRK.B")), None);
        assert_eq!(icon_target(Market::B3, None), None);
        for market in [Market::Us, Market::Crypto, Market::Treasury, Market::Manual] {
            assert_eq!(icon_target(market, Some("AAPL")), None);
        }
    }
}
