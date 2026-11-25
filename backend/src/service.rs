//! Junta banco, cotações e regras: a carteira como a tela a vê, o plano de
//! aporte e a atualização de cotações.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::domain::allocation::{self, class_index, percent, round_cents, Holding, Plan, Targets};
use crate::domain::asset_class::{AssetClass, UnitKind};
use crate::domain::scoring::{self, ScoreSource};
use crate::quotes::{fx_symbol, Quote, QuoteError, QuoteProvider};
use crate::store::{unit_value, Asset, Question, Store, StoreError, StoredQuote};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    Quote,
    Manual,
    Missing,
}

impl PriceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PriceSource::Quote => "quote",
            PriceSource::Manual => "manual",
            PriceSource::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetView {
    pub asset: Asset,
    pub unit_price: Option<Decimal>,
    pub currency: String,
    pub price_source: PriceSource,
    pub quote_updated_at: Option<DateTime<Utc>>,
    pub current_value: Decimal,
    pub class_percent: Decimal,
    pub score: u8,
    pub score_source: ScoreSource,
    pub answers: HashMap<Uuid, bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassView {
    pub asset_class: AssetClass,
    pub target_percent: Decimal,
    pub current_value: Decimal,
    pub current_percent: Decimal,
    pub asset_count: usize,
    pub eligible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioView {
    pub total_value: Decimal,
    pub quotes_updated_at: Option<DateTime<Utc>>,
    pub targets: Targets,
    pub classes: Vec<ClassView>,
    pub assets: Vec<AssetView>,
    pub questions: Vec<Question>,
}

impl PortfolioView {
    pub fn asset(&self, id: Uuid) -> Option<&AssetView> {
        self.assets.iter().find(|a| a.asset.id == id)
    }
}

/// Preço em reais de um ativo e de onde ele veio.
fn resolve_price(
    asset: &Asset,
    quotes: &HashMap<String, StoredQuote>,
) -> (Option<Decimal>, String, PriceSource, Option<DateTime<Utc>>) {
    if asset.asset_class.unit_kind() == UnitKind::Value {
        return (Some(Decimal::ONE), "BRL".into(), PriceSource::Manual, None);
    }
    if let Some(price) = asset.manual_price {
        return (Some(price), "BRL".into(), PriceSource::Manual, None);
    }
    let quote = asset
        .symbol
        .as_deref()
        .and_then(|s| asset.asset_class.quote_symbol(s))
        .and_then(|s| quotes.get(&s));
    let Some(quote) = quote else {
        return (None, "BRL".into(), PriceSource::Missing, None);
    };
    if quote.currency == "BRL" {
        return (
            Some(quote.price),
            quote.currency.clone(),
            PriceSource::Quote,
            Some(quote.fetched_at),
        );
    }
    match quotes.get(&fx_symbol(&quote.currency)) {
        Some(fx) => (
            Some((quote.price * fx.price).round_dp(8)),
            quote.currency.clone(),
            PriceSource::Quote,
            Some(quote.fetched_at.min(fx.fetched_at)),
        ),
        None => (
            None,
            quote.currency.clone(),
            PriceSource::Missing,
            Some(quote.fetched_at),
        ),
    }
}

/// A carteira inteira, pronta para a tela.
pub async fn portfolio(store: &Store, user: Uuid) -> Result<PortfolioView, StoreError> {
    let targets = store.goals(user).await?;
    let assets = store.assets(user).await?;
    let questions = store.questions(user).await?;
    let answers = store.answers(user).await?;

    let symbols: Vec<String> = assets
        .iter()
        .filter_map(|a| {
            a.symbol
                .as_deref()
                .and_then(|s| a.asset_class.quote_symbol(s))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut quotes = store.quotes(&symbols).await?;
    let fx: Vec<String> = quotes
        .values()
        .filter(|q| q.currency != "BRL")
        .map(|q| fx_symbol(&q.currency))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    quotes.extend(store.quotes(&fx).await?);

    let mut by_asset: HashMap<Uuid, HashMap<Uuid, bool>> = HashMap::new();
    for answer in answers {
        by_asset
            .entry(answer.asset_id)
            .or_default()
            .insert(answer.question_id, answer.answer);
    }

    let mut views: Vec<AssetView> = assets
        .into_iter()
        .map(|asset| {
            let (unit_price, currency, price_source, quote_updated_at) =
                resolve_price(&asset, &quotes);
            let answers = by_asset.remove(&asset.id).unwrap_or_default();
            let checklist = asset.asset_class.checklist();
            let relevant = answers.iter().filter(|(question, _)| {
                questions
                    .iter()
                    .any(|q| q.id == **question && Some(q.checklist) == checklist)
            });
            let (score, score_source) = scoring::score(
                asset.score_override,
                checklist,
                relevant.map(|(_, answer)| *answer),
            );
            let current_value = round_cents(
                unit_price
                    .map(|p| unit_value(asset.asset_class, asset.quantity, p))
                    .unwrap_or(Decimal::ZERO),
            );
            AssetView {
                asset,
                unit_price,
                currency,
                price_source,
                quote_updated_at,
                current_value,
                class_percent: Decimal::ZERO,
                score,
                score_source,
                answers,
            }
        })
        .collect();

    let total_value: Decimal = views.iter().map(|v| v.current_value).sum();
    let mut class_values = [Decimal::ZERO; 7];
    let mut class_counts = [0usize; 7];
    let mut class_eligible = [false; 7];
    for view in &views {
        let i = class_index(view.asset.asset_class);
        class_values[i] += view.current_value;
        class_counts[i] += 1;
        class_eligible[i] |= view.score > 0 && view.unit_price.is_some_and(|p| p > Decimal::ZERO);
    }
    for view in &mut views {
        view.class_percent = percent(
            view.current_value,
            class_values[class_index(view.asset.asset_class)],
        );
    }
    let bp_sum = targets.sum().max(1);
    let classes = AssetClass::ALL
        .into_iter()
        .map(|class| {
            let i = class_index(class);
            ClassView {
                asset_class: class,
                target_percent: percent(Decimal::from(targets.get(class)), Decimal::from(bp_sum)),
                current_value: class_values[i],
                current_percent: percent(class_values[i], total_value),
                asset_count: class_counts[i],
                eligible: class_eligible[i],
            }
        })
        .collect();
    let quotes_updated_at = views.iter().filter_map(|v| v.quote_updated_at).min();

    Ok(PortfolioView {
        total_value,
        quotes_updated_at,
        targets,
        classes,
        assets: views,
        questions,
    })
}

/// O plano para `amount` e a carteira de onde ele saiu.
pub async fn plan(
    store: &Store,
    user: Uuid,
    amount: Decimal,
) -> Result<(Plan, PortfolioView), StoreError> {
    let view = portfolio(store, user).await?;
    let holdings: Vec<Holding> = view
        .assets
        .iter()
        .map(|a| Holding {
            asset_id: a.asset.id,
            asset_class: a.asset.asset_class,
            label: a
                .asset
                .symbol
                .clone()
                .unwrap_or_else(|| a.asset.display_name.clone()),
            value: a.current_value,
            unit_price: a.unit_price,
            score: a.score,
        })
        .collect();
    let plan = allocation::plan(amount, view.targets, &holdings);
    Ok((plan, view))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub refreshed: usize,
    pub failed: Vec<String>,
}

/// Busca cotações no provedor e guarda no banco.
pub struct QuoteService {
    store: Store,
    provider: Arc<dyn QuoteProvider>,
}

impl QuoteService {
    pub fn new(store: Store, provider: Arc<dyn QuoteProvider>) -> Self {
        Self { store, provider }
    }

    pub fn provider(&self) -> &dyn QuoteProvider {
        self.provider.as_ref()
    }

    /// Cota um ativo agora, para ele já aparecer com preço. Falha não é erro
    /// para quem chama: o ativo fica sem preço até a próxima rodada.
    pub async fn quote_now(&self, asset_class: AssetClass, symbol: &str) -> Option<Quote> {
        let report = self.refresh(&[(asset_class, symbol.to_owned())]).await;
        if report.refreshed == 0 {
            return None;
        }
        let provider_symbol = asset_class.quote_symbol(symbol)?;
        let stored = self.store.quotes(&[provider_symbol]).await.ok()?;
        stored.into_values().next().map(|q| Quote {
            symbol: q.symbol,
            price: q.price,
            currency: q.currency,
            name: q.name,
        })
    }

    /// Atualiza os símbolos dados e o câmbio das moedas que aparecerem.
    pub async fn refresh(&self, targets: &[(AssetClass, String)]) -> RefreshReport {
        let mut report = RefreshReport::default();
        let mut currencies = BTreeSet::new();
        for (asset_class, symbol) in targets {
            let Some(provider_symbol) = asset_class.quote_symbol(symbol) else {
                continue;
            };
            match self.fetch_and_store(&provider_symbol).await {
                Ok(quote) => {
                    report.refreshed += 1;
                    if quote.currency != "BRL" {
                        currencies.insert(quote.currency);
                    }
                }
                Err(error) => {
                    tracing::warn!(symbol = %provider_symbol, error = %error, "cotacao nao atualizada");
                    report.failed.push(symbol.clone());
                }
            }
        }
        for currency in currencies {
            let symbol = fx_symbol(&currency);
            if let Err(error) = self.fetch_and_store(&symbol).await {
                tracing::warn!(symbol = %symbol, error = %error, "cambio nao atualizado");
            }
        }
        report
    }

    async fn fetch_and_store(&self, provider_symbol: &str) -> Result<Quote, QuoteError> {
        let mut quote = self.provider.quote(provider_symbol).await?;
        quote.symbol = provider_symbol.to_owned();
        self.store
            .upsert_quote(&quote, Utc::now())
            .await
            .map_err(|e| QuoteError::Unavailable(e.to_string()))?;
        Ok(quote)
    }

    /// Todas as carteiras.
    pub async fn refresh_all(&self) -> RefreshReport {
        match self.store.tracked_symbols().await {
            Ok(symbols) => self.refresh(&symbols).await,
            Err(error) => {
                tracing::error!(error = %error, "nao foi possivel listar os simbolos");
                RefreshReport::default()
            }
        }
    }

    /// Rodada periódica, até o processo encerrar.
    pub fn spawn_refresher(self: Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let report = self.refresh_all().await;
                tracing::info!(
                    refreshed = report.refreshed,
                    failed = report.failed.len(),
                    "cotacoes atualizadas"
                );
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn asset(class: AssetClass, symbol: Option<&str>, manual: Option<Decimal>) -> Asset {
        Asset {
            id: Uuid::new_v4(),
            asset_class: class,
            symbol: symbol.map(str::to_owned),
            display_name: "x".into(),
            quantity: dec!(1),
            manual_price: manual,
            score_override: None,
        }
    }

    fn stored(symbol: &str, price: Decimal, currency: &str) -> (String, StoredQuote) {
        (
            symbol.to_owned(),
            StoredQuote {
                symbol: symbol.to_owned(),
                price,
                currency: currency.to_owned(),
                name: None,
                fetched_at: Utc::now(),
            },
        )
    }

    #[test]
    fn price_comes_from_the_right_place() {
        let quotes: HashMap<String, StoredQuote> = [
            stored("KNRI11.SA", dec!(142), "BRL"),
            stored("AAPL", dec!(200), "USD"),
            stored("USDBRL=X", dec!(5.5), "BRL"),
            stored("ETH-USD", dec!(3000), "USD"),
            stored("VOD.L", dec!(70), "GBP"),
        ]
        .into_iter()
        .collect();

        let (price, _, source, _) =
            resolve_price(&asset(AssetClass::FixedIncome, None, None), &quotes);
        assert_eq!((price, source), (Some(Decimal::ONE), PriceSource::Manual));

        let (price, _, source, _) = resolve_price(
            &asset(AssetClass::BrReits, Some("KNRI11"), Some(dec!(150))),
            &quotes,
        );
        assert_eq!((price, source), (Some(dec!(150)), PriceSource::Manual));

        let (price, currency, source, at) =
            resolve_price(&asset(AssetClass::BrReits, Some("KNRI11"), None), &quotes);
        assert_eq!(
            (price, currency.as_str(), source),
            (Some(dec!(142)), "BRL", PriceSource::Quote)
        );
        assert!(at.is_some());

        let (price, currency, source, _) =
            resolve_price(&asset(AssetClass::IntlStocks, Some("AAPL"), None), &quotes);
        assert_eq!(
            (price, currency.as_str(), source),
            (Some(dec!(1100)), "USD", PriceSource::Quote)
        );

        let (price, _, source, _) =
            resolve_price(&asset(AssetClass::Crypto, Some("ETH"), None), &quotes);
        assert_eq!((price, source), (Some(dec!(16500)), PriceSource::Quote));

        let (price, _, source, _) =
            resolve_price(&asset(AssetClass::IntlStocks, Some("VOD.L"), None), &quotes);
        assert_eq!((price, source), (None, PriceSource::Missing));

        let (price, _, source, _) =
            resolve_price(&asset(AssetClass::BrStocks, Some("NADA3"), None), &quotes);
        assert_eq!((price, source), (None, PriceSource::Missing));
        assert_eq!(PriceSource::Missing.as_str(), "missing");
        assert_eq!(PriceSource::Quote.as_str(), "quote");
        assert_eq!(PriceSource::Manual.as_str(), "manual");
    }
}
