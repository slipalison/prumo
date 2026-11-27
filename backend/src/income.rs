//! A estimativa de renda da carteira: o que cada cota pagou em 12 meses,
//! líquido do IR do JCP, vezes a quantidade que a pessoa tem. A regra está em
//! `domain::income::estimate`.
//!
//! **Fontes.** Ações e FIIs da B3: os anúncios da própria B3 que os proventos
//! automáticos já guardam (`b3_cash_announcements`, com o JCP separado). O
//! histórico de dividendo por cota do Yahoo, guardado no banco, é a reserva
//! (a B3 sem o ativo ou fora do ar), a fonte de EUA, BDR e ETF, e o histórico
//! longo da média do habitual.
//!
//! É **estimativa**: o passado não garante o futuro. Quem mostra o número
//! escreve "estimativa", e o que a pessoa de fato recebeu (`income_events`)
//! sempre vence.
//!
//! **Nenhuma requisição espera pela rede.** [`IncomeService::estimate`] lê só o
//! banco; quem o enche é uma tarefa em segundo plano
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
use crate::domain::allocation::round_cents;
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::entitlement::trailing_income;
use crate::domain::income::estimate::{
    asset_estimate, b3_trailing_income, b3_unit_payments, b3_window_start,
    foreign_withholding_rate, in_window, pick_source, yahoo_unit_payments, AssetEstimate,
    EstimateSource, UnitPayment,
};
use crate::domain::income::DividendPoint;
use crate::quotes::fx_symbol;
use crate::service::{AssetView, PortfolioView};
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
    /// A fonte, os 12 meses por cota e por mês (líquido e bruto), a média do
    /// habitual e a marca "acima do habitual".
    pub estimate: AssetEstimate,
    /// O que a posição teria rendido nos últimos 12 meses, em reais, com as
    /// cotas que o livro tinha na data com de cada provento (e não as de hoje):
    /// líquido pela B3; pelo Yahoo, líquido dos 30% retidos nos EUA e, numa
    /// ação da B3, bruto (ele não separa o JCP). `None` quando a estimativa foi
    /// pedida sem o livro, ou o livro não fecha.
    pub trailing_12m: Option<Decimal>,
    /// Os pagamentos por cota que entraram nos 12 meses, da fonte escolhida:
    /// a projeção de renda (`domain::projection`) tira deles o calendário.
    pub payments: Vec<UnitPayment>,
}

/// A estimativa da carteira inteira.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PortfolioIncome {
    pub assets: HashMap<Uuid, AssetIncome>,
    /// Soma das rendas mensais líquidas por ativo.
    pub monthly_total: Decimal,
    /// A mesma soma, bruta.
    pub monthly_total_gross: Decimal,
    /// A parte líquida de `monthly_total` que vem de ativos acima do habitual.
    pub above_usual_monthly: Decimal,
    /// Quantos ativos da carteira pagam provento (B3 e EUA). Com zero, não há
    /// o que estimar; com algum e nenhuma estimativa, ela está indisponível.
    pub payers: usize,
    /// Símbolos que pagam dividendo mas ficaram sem estimativa (nem a B3 nem o
    /// histórico do Yahoo, ou sem câmbio).
    pub missing: Vec<String>,
    /// De quando é o dado mais velho que entrou na conta.
    pub updated_at: Option<OffsetDateTime>,
}

impl PortfolioIncome {
    pub fn asset(&self, id: Uuid) -> Option<&AssetIncome> {
        self.assets.get(&id)
    }
}

/// Um ativo que paga provento, como a estimativa o lê da carteira.
struct Payer {
    asset_id: Uuid,
    class: AssetClass,
    market: Market,
    quantity: Decimal,
    /// O símbolo no Yahoo (`KNRI11.SA`).
    provider_symbol: String,
}

/// As ações e FIIs da B3: a B3 é a fonte deles. ETF e BDR (outras classes
/// negociadas na B3) ficam com o Yahoo.
fn b3_sourced(market: Market, class: AssetClass) -> bool {
    market == Market::B3 && matches!(class, AssetClass::BrStocks | AssetClass::BrReits)
}

pub struct IncomeService {
    store: Store,
    provider: Arc<dyn DividendProvider>,
    /// Símbolos que falharam há pouco e quando.
    resting: Mutex<HashMap<String, Instant>>,
    /// As buscas disparadas por [`IncomeService::prefetch`] que ainda podem
    /// estar rodando: [`IncomeService::settle`] espera por elas.
    inflight: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

/// O símbolo no provedor de um ativo que paga provento, ou `None`.
pub(crate) fn payer_symbol(market: Market, symbol: Option<&str>) -> Option<String> {
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
            inflight: Mutex::default(),
        }
    }

    /// Sem provedor: a estimativa só usa o que já está no cache.
    pub fn disabled(store: Store) -> Self {
        Self::new(store, Arc::new(DisabledDividends))
    }

    /// Estima a renda mensal de cada ativo que paga dividendo, **só com o que
    /// está no banco** (os anúncios da B3 e o cache do Yahoo): não fala com
    /// nenhum provedor. `today` é o dia de Brasília.
    /// Sem o livro: `trailing_12m` fica vazio.
    pub async fn estimate(&self, view: &PortfolioView, today: Date) -> PortfolioIncome {
        self.estimate_with(None, view, today).await
    }

    /// Como [`IncomeService::estimate`], e ainda calcula `trailing_12m` de cada
    /// ativo com a quantidade que o livro de `user` tinha na data com.
    pub async fn estimate_with_ledger(
        &self,
        user: Uuid,
        view: &PortfolioView,
        today: Date,
    ) -> PortfolioIncome {
        self.estimate_with(Some(user), view, today).await
    }

    async fn estimate_with(
        &self,
        user: Option<Uuid>,
        view: &PortfolioView,
        today: Date,
    ) -> PortfolioIncome {
        let assets: Vec<&AssetView> = view.assets.iter().collect();
        estimate_assets(&self.store, user, &assets, today).await
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
        let handle = tokio::spawn(async move {
            service.fill(&[symbol], OffsetDateTime::now_utc()).await;
        });
        if let Ok(mut inflight) = self.inflight.lock() {
            inflight.retain(|task| !task.is_finished());
            inflight.push(handle);
        }
    }

    /// Espera as buscas disparadas por [`IncomeService::prefetch`] terminarem,
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

/// A estimativa de `assets`, só com o banco (os anúncios da B3 e o cache do
/// Yahoo). É a conta de [`IncomeService::estimate`]; as referências de preço dos
/// FIIs (`crate::ceiling`) leem daqui os proventos por cota de 12 meses, para a
/// renda e o Rendimento contarem o mesmo ano. Com `user`, calcula também o
/// `trailing_12m` pelo livro.
pub async fn estimate_assets(
    store: &Store,
    user: Option<Uuid>,
    assets: &[&AssetView],
    today: Date,
) -> PortfolioIncome {
    let now = OffsetDateTime::now_utc();
    let ledgers = match user {
        Some(user) => store.entries_by_asset(user).await.unwrap_or_else(|error| {
            tracing::error!(error = %error, "livro ilegivel para a estimativa");
            HashMap::new()
        }),
        None => HashMap::new(),
    };
    let payers: Vec<Payer> = assets
        .iter()
        .filter_map(|a| {
            let provider_symbol = payer_symbol(a.asset.market(), a.asset.symbol.as_deref())?;
            Some(Payer {
                asset_id: a.asset.id,
                class: a.asset.asset_class,
                market: a.asset.market(),
                quantity: a.asset.quantity,
                provider_symbol,
            })
        })
        .collect();
    let b3_candidates: Vec<(Uuid, AssetClass, String)> = assets
        .iter()
        .filter(|a| b3_sourced(a.asset.market(), a.asset.asset_class))
        .filter_map(|a| Some((a.asset.id, a.asset.asset_class, a.asset.symbol.clone()?)))
        .collect();
    let b3 = store
        .b3_income_sources(&b3_candidates, b3_window_start(today))
        .await
        .unwrap_or_else(|error| {
            tracing::error!(error = %error, "anuncios da B3 ilegiveis para a estimativa");
            HashMap::new()
        });
    let symbols: Vec<String> = payers
        .iter()
        .map(|p| p.provider_symbol.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let cache = store
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
    let fx = store.quotes(&currencies).await.unwrap_or_default();

    let mut income = PortfolioIncome {
        payers: payers.len(),
        ..PortfolioIncome::default()
    };
    let mut missing = BTreeSet::new();
    for payer in payers {
        // O histórico do Yahoo, em reais: a reserva da B3 e o histórico longo
        // da média do habitual.
        let yahoo = cache.get(&payer.provider_symbol).and_then(|cached| {
            let rate = if cached.currency == "BRL" {
                Some(Decimal::ONE)
            } else {
                fx.get(&fx_symbol(&cached.currency)).map(|q| q.price)
            }?;
            let points: Vec<DividendPoint> = cached
                .events
                .iter()
                .map(|e| DividendPoint {
                    ex_date: e.ex_date,
                    amount: e.amount,
                })
                .collect();
            Some((points, rate, cached.fetched_at))
        });
        let announced = b3.get(&payer.asset_id);
        let source = pick_source(announced.map(|a| a.fetched_at), yahoo.is_some(), now);
        // O imposto retido lá fora (30% nos EUA) sai do líquido do Yahoo.
        let foreign = foreign_withholding_rate(payer.market);
        let history = yahoo
            .as_ref()
            .map(|(points, rate, _)| yahoo_unit_payments(points, *rate, foreign))
            .unwrap_or_default();
        let entries = ledgers.get(&payer.asset_id).map_or(&[][..], Vec::as_slice);
        let (source, payments, fetched_at, trailing) = match (source, announced, &yahoo) {
            (Some(EstimateSource::B3), Some(announced), _) => (
                EstimateSource::B3,
                b3_unit_payments(&announced.cash, &announced.stock, payer.class, today),
                announced.fetched_at,
                user.map(|_| {
                    b3_trailing_income(&announced.cash, payer.class, entries, today)
                        .map(|total| total.net)
                }),
            ),
            (Some(_), _, Some((points, rate, fetched_at))) => (
                EstimateSource::Yahoo,
                history.clone(),
                *fetched_at,
                user.map(|_| {
                    trailing_income(points, today, entries)
                        .map(|gross| round_cents(gross * rate * (Decimal::ONE - foreign)))
                }),
            ),
            // Nem a B3 nem o Yahoo têm o ativo (ou falta o câmbio).
            _ => {
                missing.insert(payer.provider_symbol);
                continue;
            }
        };
        let trailing_12m = trailing.and_then(|result| match result {
            Ok(total) => Some(total),
            Err(error) => {
                tracing::error!(error = %error, "livro nao fecha na estimativa");
                None
            }
        });
        let estimate = asset_estimate(source, &payments, &history, payer.quantity, today);
        income.monthly_total += estimate.monthly.net;
        income.monthly_total_gross += estimate.monthly.gross;
        if estimate.above_usual {
            income.above_usual_monthly += estimate.monthly.net;
        }
        income.updated_at = Some(
            income
                .updated_at
                .map_or(fetched_at, |at| at.min(fetched_at)),
        );
        income.assets.insert(
            payer.asset_id,
            AssetIncome {
                asset_id: payer.asset_id,
                estimate,
                trailing_12m,
                payments: payments
                    .into_iter()
                    .filter(|p| in_window(p.ex_date, today))
                    .collect(),
            },
        );
    }
    income.missing = missing
        .into_iter()
        .map(|s| s.trim_end_matches(".SA").to_owned())
        .collect();
    income
}
