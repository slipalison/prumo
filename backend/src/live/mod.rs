//! Preço ao vivo.
//!
//! O streamer ([`yahoo::YahooStreamer`]) manda cada preço para o canal de
//! [`LiveQuotes::ticks`]; o hub junta o último preço de cada símbolo e grava o
//! lote no Postgres a cada `flush_every`; depois avisa, num `broadcast`, quais
//! símbolos mudaram de preço. Cada conexão de `GET /portfolio/stream` ouve o
//! aviso e recalcula a carteira da sua pessoa.
//!
//! O banco continua a fonte da verdade: a tela, o plano de aporte e as duas
//! réplicas leem o mesmo preço. O `chart`, a cada 15 minutos, segue cotando
//! tudo (moeda, nome e o que o streamer não cobre) e avisa pelo mesmo canal.

pub mod yahoo;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch, Notify};
use tokio::time::MissedTickBehavior;

use crate::quotes::fx_symbol;
use crate::store::{Store, StoreError};

/// Um preço que chegou pelo streamer, já com o símbolo do `chart`.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub symbol: String,
    pub price: Decimal,
    pub traded_at: OffsetDateTime,
}

/// Símbolos (do `chart`) com preço novo no banco.
pub type Changed = Arc<BTreeSet<String>>;

/// Quantos preços podem esperar o próximo lote. O streamer manda ~1 por
/// segundo por símbolo; o lote sai a cada segundo.
const TICK_BUFFER: usize = 1024;
/// Avisos guardados para quem está atrasado. Quem perder algum recalcula tudo.
const CHANGED_BUFFER: usize = 64;

pub struct LiveQuotes {
    store: Store,
    ticks_tx: mpsc::Sender<Tick>,
    ticks_rx: Mutex<Option<mpsc::Receiver<Tick>>>,
    changed: broadcast::Sender<Changed>,
    symbols: watch::Sender<BTreeSet<String>>,
    resync: Notify,
}

impl LiveQuotes {
    pub fn new(store: Store) -> Arc<Self> {
        let (ticks_tx, ticks_rx) = mpsc::channel(TICK_BUFFER);
        let (changed, _) = broadcast::channel(CHANGED_BUFFER);
        let (symbols, _) = watch::channel(BTreeSet::new());
        Arc::new(Self {
            store,
            ticks_tx,
            ticks_rx: Mutex::new(Some(ticks_rx)),
            changed,
            symbols,
            resync: Notify::new(),
        })
    }

    /// Por onde o streamer entrega os preços.
    pub fn ticks(&self) -> mpsc::Sender<Tick> {
        self.ticks_tx.clone()
    }

    /// O que o streamer deve assinar (símbolos do `chart`).
    pub fn symbols(&self) -> watch::Receiver<BTreeSet<String>> {
        self.symbols.subscribe()
    }

    /// Aviso de preço novo no banco.
    pub fn subscribe(&self) -> broadcast::Receiver<Changed> {
        self.changed.subscribe()
    }

    /// Diz a quem ouve que estes símbolos têm preço novo no banco.
    pub fn announce(&self, symbols: BTreeSet<String>) {
        if !symbols.is_empty() {
            // Sem ninguém ouvindo o envio falha, e tudo bem.
            let _ = self.changed.send(Arc::new(symbols));
        }
    }

    /// A lista de ativos mudou: refaz a assinatura já, sem esperar a rodada.
    pub fn resync(&self) {
        self.resync.notify_one();
    }

    /// Liga o gravador de lotes e o acompanhamento dos símbolos. Vale uma vez
    /// por processo; uma segunda chamada não faz nada.
    pub fn spawn(self: &Arc<Self>, flush_every: Duration, track_every: Duration) {
        let receiver = self.ticks_rx.lock().ok().and_then(|mut slot| slot.take());
        let Some(receiver) = receiver else {
            tracing::warn!("hub de precos ja estava ligado");
            return;
        };
        tokio::spawn(Arc::clone(self).flush_loop(receiver, flush_every));
        tokio::spawn(Arc::clone(self).track_loop(track_every));
    }

    async fn flush_loop(self: Arc<Self>, mut ticks: mpsc::Receiver<Tick>, every: Duration) {
        let mut pending: HashMap<String, Tick> = HashMap::new();
        let mut ticker = tokio::time::interval(every);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                tick = ticks.recv() => {
                    // O hub guarda um `Sender`: o canal nunca fecha.
                    let Some(tick) = tick else { return };
                    keep_latest(&mut pending, tick);
                }
                _ = ticker.tick() => {
                    if pending.is_empty() {
                        continue;
                    }
                    let batch: Vec<Tick> = pending.drain().map(|(_, tick)| tick).collect();
                    match self.store.apply_ticks(&batch, OffsetDateTime::now_utc()).await {
                        Ok(moved) => {
                            metrics::counter!("quote_ticks_total").increment(batch.len() as u64);
                            self.announce(moved);
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, ticks = batch.len(), "precos ao vivo nao gravados");
                        }
                    }
                }
            }
        }
    }

    async fn track_loop(self: Arc<Self>, every: Duration) {
        let mut ticker = tokio::time::interval(every);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                () = self.resync.notified() => {}
            }
            match self.wanted().await {
                Ok(wanted) => {
                    self.symbols.send_if_modified(|current| {
                        if *current == wanted {
                            return false;
                        }
                        *current = wanted;
                        true
                    });
                }
                Err(error) => tracing::warn!(error = %error, "simbolos ao vivo nao atualizados"),
            }
        }
    }

    /// O que assinar: todo símbolo de ativo que já tem cotação no banco, mais o
    /// câmbio das moedas que aparecem. Sem cotação o streamer não teria moeda
    /// para o preço — esse símbolo espera o `chart`. O título do Tesouro fica
    /// de fora: o preço dele vem da tabela diária, não de bolsa.
    async fn wanted(&self) -> Result<BTreeSet<String>, StoreError> {
        let symbols: Vec<String> = self
            .store
            .tracked_symbols()
            .await?
            .into_iter()
            .filter(|(market, _)| market.from_provider())
            .filter_map(|(market, symbol)| market.quote_symbol(&symbol))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let quotes = self.store.quotes(&symbols).await?;
        let mut wanted: BTreeSet<String> = quotes.keys().cloned().collect();
        wanted.extend(
            quotes
                .values()
                .filter(|q| q.currency != "BRL")
                .map(|q| fx_symbol(&q.currency)),
        );
        Ok(wanted)
    }
}

/// Fica o preço mais recente de cada símbolo.
fn keep_latest(pending: &mut HashMap<String, Tick>, tick: Tick) {
    match pending.get(&tick.symbol) {
        Some(current) if current.traded_at > tick.traded_at => {}
        _ => {
            pending.insert(tick.symbol.clone(), tick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::datetime;

    fn tick(symbol: &str, price: Decimal, at: OffsetDateTime) -> Tick {
        Tick {
            symbol: symbol.to_owned(),
            price,
            traded_at: at,
        }
    }

    #[test]
    fn a_batch_keeps_the_latest_price_of_each_symbol() {
        let mut pending = HashMap::new();
        keep_latest(
            &mut pending,
            tick("A", dec!(1), datetime!(2026-11-25 13:00:01 UTC)),
        );
        keep_latest(
            &mut pending,
            tick("A", dec!(2), datetime!(2026-11-25 13:00:03 UTC)),
        );
        // Chegou fora de ordem: mais velho que o que já está no lote.
        keep_latest(
            &mut pending,
            tick("A", dec!(9), datetime!(2026-11-25 13:00:02 UTC)),
        );
        keep_latest(
            &mut pending,
            tick("B", dec!(5), datetime!(2026-11-25 13:00:00 UTC)),
        );
        assert_eq!(pending.len(), 2);
        assert_eq!(pending["A"].price, dec!(2));
        assert_eq!(pending["B"].price, dec!(5));
    }
}
