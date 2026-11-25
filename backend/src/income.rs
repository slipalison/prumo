//! A estimativa de renda da carteira: histórico de dividendo por cota do
//! provedor, guardado no banco, vezes a quantidade que a pessoa tem.
//!
//! É **estimativa**: o provedor dá data ex e valor bruto (JCP misturado ao
//! dividendo, sem data de pagamento), e o passado não garante o futuro. Quem
//! mostra o número escreve "estimativa", e o que a pessoa de fato recebeu
//! (`income_events`) sempre vence.

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

/// Depois de uma busca que falhou, o símbolo descansa antes da próxima
/// tentativa: com o provedor fora do ar, cada tela não pode refazer a espera.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(10 * 60);

/// Quanto uma tela espera pelo provedor. O que não chegar nesse prazo termina
/// em segundo plano e entra no cache para a próxima vez.
const FETCH_BUDGET: Duration = Duration::from_secs(6);

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
    /// Símbolos que pagam dividendo mas ficaram sem estimativa (provedor fora
    /// do ar ou sem câmbio).
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

    /// Estima a renda mensal de cada ativo que paga dividendo. `today` é o dia
    /// de Brasília; `now`, o instante, para a idade do cache.
    pub async fn estimate(
        self: &Arc<Self>,
        view: &PortfolioView,
        today: Date,
        now: OffsetDateTime,
    ) -> PortfolioIncome {
        // Só papel cotado paga provento: renda fixa por valor e cripto ficam de fora.
        let payers: Vec<(Uuid, Decimal, String)> = view
            .assets
            .iter()
            .filter_map(|a| {
                let market = a.asset.market();
                if !matches!(market, Market::B3 | Market::Us) {
                    return None;
                }
                let symbol = market.quote_symbol(a.asset.symbol.as_deref()?)?;
                Some((a.asset.id, a.asset.quantity, symbol))
            })
            .collect();
        let symbols: Vec<String> = payers
            .iter()
            .map(|(_, _, s)| s.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut cache = self
            .store
            .dividend_cache(&symbols)
            .await
            .unwrap_or_else(|error| {
                tracing::error!(error = %error, "cache de dividendos ilegivel");
                HashMap::new()
            });

        let stale = self.stale(&symbols, &cache, now);
        if !stale.is_empty() {
            self.refresh(&stale, now).await;
            if let Ok(fresh) = self.store.dividend_cache(&stale).await {
                cache.extend(fresh);
            }
        }

        let currencies: Vec<String> = cache
            .values()
            .filter(|c| c.currency != "BRL")
            .map(|c| fx_symbol(&c.currency))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let fx = self.store.quotes(&currencies).await.unwrap_or_default();

        let mut income = PortfolioIncome::default();
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

    /// Busca os símbolos no provedor, todos ao mesmo tempo, e guarda. A tela
    /// espera até [`FETCH_BUDGET`]; o que passar disso continua em segundo plano.
    async fn refresh(self: &Arc<Self>, symbols: &[String], now: OffsetDateTime) {
        let tasks: Vec<_> = symbols
            .iter()
            .map(|symbol| {
                let service = Arc::clone(self);
                let symbol = symbol.clone();
                tokio::spawn(async move { service.fetch_and_store(&symbol, now).await })
            })
            .collect();
        let waiting = futures_util::future::join_all(tasks);
        if tokio::time::timeout(FETCH_BUDGET, waiting).await.is_err() {
            tracing::warn!(
                symbols = symbols.len(),
                "historico de dividendos demorou; segue em segundo plano"
            );
        }
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
