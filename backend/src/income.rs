//! A estimativa de renda da carteira: histórico de dividendo por cota do
//! provedor, guardado no banco, vezes a quantidade que a pessoa tem.
//!
//! É **estimativa**: o provedor dá data ex e valor bruto (JCP misturado ao
//! dividendo, sem data de pagamento), e o passado não garante o futuro. Quem
//! mostra o número escreve "estimativa", e o que a pessoa de fato recebeu
//! (`income_events`) sempre vence.
//!
//! **Nenhuma requisição espera pela rede.** [`IncomeService::estimate`] lê só o
//! cache do banco; quem o enche é uma tarefa em segundo plano
//! ([`IncomeService::spawn_refresher`], no mesmo esquema do refresher de
//! cotações) e a busca disparada quando um ativo nasce
//! ([`IncomeService::prefetch`]). Gravar é idempotente, então duas réplicas
//! buscando o mesmo símbolo não brigam.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::dividends::{DisabledDividends, DividendError, DividendProvider};
use crate::domain::asset_class::Market;
use crate::domain::income::{monthly_estimate, trailing_per_unit, DividendPoint};
use crate::quotes::fx_symbol;
use crate::service::PortfolioView;
use crate::store::{CachedDividends, Store};

/// Quanto tempo o histórico guardado vale antes de buscar de novo. Dividendo
/// novo aparece de poucos em poucos dias; um dia basta e poupa o provedor.
pub const CACHE_TTL: time::Duration = time::Duration::days(1);

/// De quanto em quanto tempo a tarefa confere o que está vencido. A conferência
/// é barata (uma leitura); só o vencido vai ao provedor.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

/// Depois de uma busca que falhou, o símbolo descansa antes da próxima
/// tentativa: com o provedor fora do ar, a tarefa não o martela.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(10 * 60);

/// Quantos símbolos vão ao provedor ao mesmo tempo.
const FETCH_BATCH: usize = 8;

/// A estimativa de um ativo.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetIncome {
    pub asset_id: Uuid,
    /// O que uma cota pagou em 12 meses, em reais, bruto.
    pub per_unit_12m: Decimal,
    /// Renda mensal estimada com a quantidade atual, em reais.
    pub monthly: Decimal,
}

/// A estimativa da carteira inteira.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PortfolioIncome {
    pub assets: HashMap<Uuid, AssetIncome>,
    /// Soma das rendas mensais por ativo.
    pub monthly_total: Decimal,
    /// Quantos ativos da carteira pagam provento (B3 e EUA). Com zero, não há
    /// o que estimar; com algum e nenhuma estimativa, ela está indisponível.
    pub payers: usize,
    /// Símbolos que pagam dividendo mas ficaram sem estimativa (histórico ainda
    /// não buscado, provedor fora do ar ou sem câmbio).
    pub missing: Vec<String>,
    /// De quando é o dado mais velho que entrou na conta.
    pub updated_at: Option<OffsetDateTime>,
}

impl PortfolioIncome {
    pub fn asset(&self, id: Uuid) -> Option<&AssetIncome> {
        self.assets.get(&id)
    }
}

pub struct IncomeService {
    store: Store,
    provider: Arc<dyn DividendProvider>,
    /// Símbolos que falharam há pouco e quando.
    resting: Mutex<HashMap<String, Instant>>,
}

/// O símbolo no provedor de um ativo que paga provento, ou `None`.
fn payer_symbol(market: Market, symbol: Option<&str>) -> Option<String> {
    if !matches!(market, Market::B3 | Market::Us) {
        return None;
    }
    market.quote_symbol(symbol?)
}

impl IncomeService {
    pub fn new(store: Store, provider: Arc<dyn DividendProvider>) -> Self {
        Self {
            store,
            provider,
            resting: Mutex::default(),
        }
    }

    /// Sem provedor: a estimativa só usa o que já está no cache.
    pub fn disabled(store: Store) -> Self {
        Self::new(store, Arc::new(DisabledDividends))
    }

    /// Estima a renda mensal de cada ativo que paga dividendo, **só com o que
    /// está no cache**: não fala com o provedor. `today` é o dia de Brasília.
    pub async fn estimate(&self, view: &PortfolioView, today: Date) -> PortfolioIncome {
        let payers: Vec<(Uuid, Decimal, String)> = view
            .assets
            .iter()
            .filter_map(|a| {
                let symbol = payer_symbol(a.asset.market(), a.asset.symbol.as_deref())?;
                Some((a.asset.id, a.asset.quantity, symbol))
            })
            .collect();
        let symbols: Vec<String> = payers
            .iter()
            .map(|(_, _, s)| s.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let cache = self
            .store
            .dividend_cache(&symbols)
            .await
            .unwrap_or_else(|error| {
                tracing::error!(error = %error, "cache de dividendos ilegivel");
                HashMap::new()
            });
        let currencies: Vec<String> = cache
            .values()
            .filter(|c| c.currency != "BRL")
            .map(|c| fx_symbol(&c.currency))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let fx = self.store.quotes(&currencies).await.unwrap_or_default();

        let mut income = PortfolioIncome {
            payers: payers.len(),
            ..PortfolioIncome::default()
        };
        let mut missing = BTreeSet::new();
        for (asset_id, quantity, symbol) in payers {
            let Some(cached) = cache.get(&symbol) else {
                missing.insert(symbol);
                continue;
            };
            let rate = if cached.currency == "BRL" {
                Some(Decimal::ONE)
            } else {
                fx.get(&fx_symbol(&cached.currency)).map(|q| q.price)
            };
            let Some(rate) = rate else {
                missing.insert(symbol);
                continue;
            };
            let points: Vec<DividendPoint> = cached
                .events
                .iter()
                .map(|e| DividendPoint {
                    ex_date: e.ex_date,
                    amount: e.amount,
                })
                .collect();
            let per_unit_12m = (trailing_per_unit(&points, today) * rate).round_dp(8);
            let monthly = monthly_estimate(per_unit_12m, quantity);
            income.monthly_total += monthly;
            income.updated_at = Some(
                income
                    .updated_at
                    .map_or(cached.fetched_at, |at| at.min(cached.fetched_at)),
            );
            income.assets.insert(
                asset_id,
                AssetIncome {
                    asset_id,
                    per_unit_12m,
                    monthly,
                },
            );
        }
        income.missing = missing
            .into_iter()
            .map(|s| s.trim_end_matches(".SA").to_owned())
            .collect();
        income
    }

    /// Busca no provedor, e guarda, o histórico dos símbolos (do provedor,
    /// `KNRI11.SA`) que nunca foram buscados ou estão vencidos, fora os que
    /// falharam há pouco. Devolve quantos foram buscados. Fica fora do caminho
    /// de qualquer requisição: demora o que o provedor demorar.
    pub async fn fill(&self, symbols: &[String], now: OffsetDateTime) -> usize {
        let cache = self.store.dividend_cache(symbols).await.unwrap_or_default();
        let stale = self.stale(symbols, &cache, now);
        for batch in stale.chunks(FETCH_BATCH) {
            futures_util::future::join_all(batch.iter().map(|s| self.fetch_and_store(s, now)))
                .await;
        }
        stale.len()
    }

    /// Enche o cache de todos os ativos que pagam provento, de todas as
    /// carteiras.
    pub async fn refresh_all(&self, now: OffsetDateTime) -> usize {
        match self.store.tracked_symbols().await {
            Ok(tracked) => {
                let symbols: Vec<String> = tracked
                    .into_iter()
                    .filter_map(|(market, symbol)| payer_symbol(market, Some(&symbol)))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                self.fill(&symbols, now).await
            }
            Err(error) => {
                tracing::error!(error = %error, "nao foi possivel listar os simbolos");
                0
            }
        }
    }

    /// Busca em segundo plano o histórico de um ativo recém-criado (ou que
    /// mudou de mercado), para a primeira estimativa já o encontrar.
    pub fn prefetch(self: &Arc<Self>, market: Market, symbol: Option<&str>) {
        let Some(symbol) = payer_symbol(market, symbol) else {
            return;
        };
        let service = Arc::clone(self);
        tokio::spawn(async move {
            service.fill(&[symbol], OffsetDateTime::now_utc()).await;
        });
    }

    /// Rodada periódica, até o processo encerrar: a primeira na hora, as outras
    /// a cada `every`.
    pub fn spawn_refresher(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let fetched = self.refresh_all(OffsetDateTime::now_utc()).await;
                tracing::info!(fetched, "historico de dividendos conferido");
            }
        })
    }

    /// Os símbolos que precisam de busca: nunca buscados ou com o cache vencido,
    /// fora os que falharam há pouco.
    fn stale(
        &self,
        symbols: &[String],
        cache: &HashMap<String, CachedDividends>,
        now: OffsetDateTime,
    ) -> Vec<String> {
        let mut resting = self
            .resting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        resting.retain(|_, at| at.elapsed() < RETRY_AFTER_FAILURE);
        symbols
            .iter()
            .filter(|symbol| {
                let fresh = cache
                    .get(*symbol)
                    .is_some_and(|c| now - c.fetched_at < CACHE_TTL);
                !fresh && !resting.contains_key(*symbol)
            })
            .cloned()
            .collect()
    }

    async fn fetch_and_store(&self, symbol: &str, now: OffsetDateTime) {
        let outcome = match self.provider.history(symbol).await {
            Ok(history) => self
                .store
                .save_dividends(symbol, &history, now)
                .await
                .map_err(|e| e.to_string()),
            Err(DividendError::NotFound) => Err("sem historico no provedor".to_owned()),
            Err(DividendError::Unavailable(reason)) => Err(reason),
        };
        if let Err(reason) = outcome {
            tracing::warn!(symbol = %symbol, reason = %reason, "historico de dividendos nao atualizado");
            self.resting
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(symbol.to_owned(), Instant::now());
        }
    }
}
