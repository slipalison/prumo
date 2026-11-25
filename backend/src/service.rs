//! Junta banco, cotações e regras: a carteira como a tela a vê, o plano de
//! aporte e a atualização de cotações.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::allocation::{self, class_index, percent, round_cents, Holding, Plan, Targets};
use crate::domain::asset_class::{AssetClass, Market, UnitKind};
use crate::domain::scoring::{self, ScoreSource};
use crate::domain::sector::{reference_key, SectorInfo, SectorSource};
use crate::live::LiveQuotes;
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
    pub quote_updated_at: Option<OffsetDateTime>,
    pub current_value: Decimal,
    pub class_percent: Decimal,
    /// A fatia que o motor mira para o ativo dentro da classe (`nota / Σ notas`
    /// dos ativos elegíveis). Zero para quem não recebe aporte.
    pub ideal_class_percent: Decimal,
    pub score: u8,
    pub score_source: ScoreSource,
    pub answers: HashMap<Uuid, bool>,
    /// Setor e segmento da fonte pública; `None` fora de Ações Nacionais e FIIs,
    /// ou enquanto a fonte não respondeu.
    pub sector: Option<SectorInfo>,
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
    pub quotes_updated_at: Option<OffsetDateTime>,
    pub targets: Targets,
    pub classes: Vec<ClassView>,
    pub assets: Vec<AssetView>,
    pub questions: Vec<Question>,
}

impl AssetView {
    /// Como os avisos citam o ativo: o nome que a pessoa lê. O símbolo do
    /// título do Tesouro é uma chave, não um ticker.
    pub fn label(&self) -> String {
        match &self.asset.symbol {
            Some(symbol) if !self.asset.treasury => symbol.clone(),
            _ => self.asset.display_name.clone(),
        }
    }

    /// O ativo como o motor do aporte o enxerga.
    fn holding(&self) -> Holding {
        Holding {
            asset_id: self.asset.id,
            asset_class: self.asset.asset_class,
            label: self.label(),
            value: self.current_value,
            unit_price: self.unit_price,
            unit: self.asset.market().unit_kind(),
            score: self.score,
            max_price: self.asset.max_price,
        }
    }
}

impl PortfolioView {
    pub fn asset(&self, id: Uuid) -> Option<&AssetView> {
        self.assets.iter().find(|a| a.asset.id == id)
    }

    /// Os símbolos do `chart` de que o valor desta carteira depende: o de cada
    /// ativo cotável e o câmbio das moedas que aparecem.
    pub fn quote_symbols(&self) -> BTreeSet<String> {
        let mut symbols: BTreeSet<String> = self
            .assets
            .iter()
            .filter_map(|a| {
                let symbol = a.asset.symbol.as_deref()?;
                a.asset.market().quote_symbol(symbol)
            })
            .collect();
        symbols.extend(
            self.assets
                .iter()
                .filter(|a| a.currency != "BRL")
                .map(|a| fx_symbol(&a.currency)),
        );
        symbols
    }
}

/// Preço em reais de um ativo e de onde ele veio.
fn resolve_price(
    asset: &Asset,
    quotes: &HashMap<String, StoredQuote>,
) -> (Option<Decimal>, String, PriceSource, Option<OffsetDateTime>) {
    if asset.market().unit_kind() == UnitKind::Value {
        return (Some(Decimal::ONE), "BRL".into(), PriceSource::Manual, None);
    }
    if let Some(price) = asset.manual_price {
        return (Some(price), "BRL".into(), PriceSource::Manual, None);
    }
    let quote = asset
        .symbol
        .as_deref()
        .and_then(|s| asset.market().quote_symbol(s))
        .and_then(|s| quotes.get(&s));
    let Some(quote) = quote else {
        return (None, "BRL".into(), PriceSource::Missing, None);
    };
    if quote.currency == "BRL" {
        return (
            Some(quote.price),
            quote.currency.clone(),
            PriceSource::Quote,
            Some(quote.as_of()),
        );
    }
    match quotes.get(&fx_symbol(&quote.currency)) {
        Some(fx) => (
            Some((quote.price * fx.price).round_dp(8)),
            quote.currency.clone(),
            PriceSource::Quote,
            Some(quote.as_of().min(fx.as_of())),
        ),
        None => (
            None,
            quote.currency.clone(),
            PriceSource::Missing,
            Some(quote.as_of()),
        ),
    }
}

/// Onde procurar o setor de um ativo. O título do Tesouro é renda fixa sem
/// setor B3/CVM: nem é procurado, e não conta como "sem setor" de ações ou FIIs.
fn sector_key(asset: &Asset) -> Option<(SectorSource, String)> {
    if asset.treasury {
        return None;
    }
    reference_key(asset.asset_class, asset.symbol.as_deref()?)
}

/// A carteira inteira, pronta para a tela.
pub async fn portfolio(store: &Store, user: Uuid) -> Result<PortfolioView, StoreError> {
    let targets = store.goals(user).await?;
    let assets = store.assets(user).await?;
    let questions = store.questions(user).await?;
    let answers = store.answers(user).await?;

    let symbols: Vec<String> = assets
        .iter()
        .filter_map(|a| a.symbol.as_deref().and_then(|s| a.market().quote_symbol(s)))
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

    let keys: Vec<_> = assets
        .iter()
        .filter_map(sector_key)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let sectors = store.sector_reference(&keys).await?;

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
                    .map(|p| unit_value(asset.market().unit_kind(), asset.quantity, p))
                    .unwrap_or(Decimal::ZERO),
            );
            let sector = sector_key(&asset).and_then(|key| sectors.get(&key).cloned());
            AssetView {
                asset,
                unit_price,
                currency,
                price_source,
                quote_updated_at,
                current_value,
                class_percent: Decimal::ZERO,
                ideal_class_percent: Decimal::ZERO,
                score,
                score_source,
                answers,
                sector,
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
        // Como o motor vê: nota, preço e preço até o teto da pessoa.
        class_eligible[i] |= view.score > 0
            && view.unit_price.is_some_and(|p| {
                p > Decimal::ZERO && view.asset.max_price.is_none_or(|max| p <= max)
            });
    }
    let holdings: Vec<Holding> = views.iter().map(AssetView::holding).collect();
    let ideal = allocation::ideal_class_percents(&holdings);
    for (view, ideal) in views.iter_mut().zip(ideal) {
        view.class_percent = percent(
            view.current_value,
            class_values[class_index(view.asset.asset_class)],
        );
        view.ideal_class_percent = round_cents(ideal);
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
    let holdings: Vec<Holding> = view.assets.iter().map(AssetView::holding).collect();
    let limits = store.allocation_limits(user).await?;
    let plan = allocation::plan(amount, view.targets, &holdings, limits);
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
    live: Arc<LiveQuotes>,
}

impl QuoteService {
    pub fn new(store: Store, provider: Arc<dyn QuoteProvider>, live: Arc<LiveQuotes>) -> Self {
        Self {
            store,
            provider,
            live,
        }
    }

    pub fn provider(&self) -> &dyn QuoteProvider {
        self.provider.as_ref()
    }

    /// O preço ao vivo: avisos de preço novo e a assinatura do streamer.
    pub fn live(&self) -> &Arc<LiveQuotes> {
        &self.live
    }

    /// Cota um ativo agora, para ele já aparecer com preço. Falha não é erro
    /// para quem chama: o ativo fica sem preço até a próxima rodada. O título
    /// do Tesouro não tem o que buscar: o preço do dia já está no banco, posto
    /// pela rodada diária.
    pub async fn quote_now(&self, market: Market, symbol: &str) -> Option<Quote> {
        if !market.from_provider() {
            return self.stored_quote(market, symbol).await;
        }
        let report = self.refresh(&[(market, symbol.to_owned())]).await;
        if report.refreshed == 0 {
            return None;
        }
        // Símbolo novo no banco: o streamer passa a acompanhar já.
        self.live.resync();
        self.stored_quote(market, symbol).await
    }

    /// A cotação que o banco guarda do ativo, sem ir à rede.
    async fn stored_quote(&self, market: Market, symbol: &str) -> Option<Quote> {
        let provider_symbol = market.quote_symbol(symbol)?;
        let stored = self.store.quotes(&[provider_symbol]).await.ok()?;
        stored.into_values().next().map(|q| Quote {
            symbol: q.symbol,
            price: q.price,
            currency: q.currency,
            name: q.name,
            traded_at: q.traded_at,
        })
    }

    /// Atualiza os símbolos dados e o câmbio das moedas que aparecerem, e
    /// avisa quem acompanha a carteira ao vivo. O título do Tesouro fica de
    /// fora: quem o atualiza é a rodada diária de [`crate::treasury`].
    pub async fn refresh(&self, targets: &[(Market, String)]) -> RefreshReport {
        let mut report = RefreshReport::default();
        let mut currencies = BTreeSet::new();
        let mut stored = BTreeSet::new();
        for (market, symbol) in targets {
            if !market.from_provider() {
                continue;
            }
            let Some(provider_symbol) = market.quote_symbol(symbol) else {
                continue;
            };
            match self.fetch_and_store(&provider_symbol).await {
                Ok(quote) => {
                    report.refreshed += 1;
                    if quote.currency != "BRL" {
                        currencies.insert(quote.currency);
                    }
                    stored.insert(provider_symbol);
                }
                Err(error) => {
                    tracing::warn!(symbol = %provider_symbol, error = %error, "cotacao nao atualizada");
                    report.failed.push(symbol.clone());
                }
            }
        }
        for currency in currencies {
            let symbol = fx_symbol(&currency);
            match self.fetch_and_store(&symbol).await {
                Ok(_) => {
                    stored.insert(symbol);
                }
                Err(error) => {
                    tracing::warn!(symbol = %symbol, error = %error, "cambio nao atualizado");
                }
            }
        }
        self.live.announce(stored);
        report
    }

    async fn fetch_and_store(&self, provider_symbol: &str) -> Result<Quote, QuoteError> {
        let mut quote = self.provider.quote(provider_symbol).await?;
        quote.symbol = provider_symbol.to_owned();
        self.store
            .upsert_quote(&quote, OffsetDateTime::now_utc())
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
            listed_on_b3: class.native_market() == Market::B3,
            treasury: false,
            max_price: None,
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
                fetched_at: OffsetDateTime::now_utc(),
                traded_at: None,
            },
        )
    }

    #[test]
    fn treasury_bonds_and_other_classes_have_no_sector_key() {
        let key = sector_key;
        let stock = asset(AssetClass::BrStocks, Some("WEGE3"), None);
        assert_eq!(
            key(&stock),
            Some((SectorSource::B3Stocks, "WEGE".to_owned()))
        );
        // Título do Tesouro: renda fixa sem setor B3/CVM, nem chega a procurar.
        let bond = Asset {
            treasury: true,
            listed_on_b3: false,
            ..asset(AssetClass::FixedIncome, Some("TD-IPCA-20350515"), None)
        };
        assert_eq!(key(&bond), None);
        // Mesmo que a marca faltasse, a classe e o formato já não têm fonte.
        let unmarked = asset(AssetClass::FixedIncome, Some("TD-IPCA-20350515"), None);
        assert_eq!(key(&unmarked), None);
        assert_eq!(key(&asset(AssetClass::FixedIncome, None, None)), None);
    }

    #[test]
    fn price_comes_from_the_right_place() {
        let quotes: HashMap<String, StoredQuote> = [
            stored("KNRI11.SA", dec!(142), "BRL"),
            stored("QBTC11.SA", dec!(52), "BRL"),
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

        // ETF de bitcoin da B3 dentro de Criptomoedas: cota em reais, sem câmbio.
        // Sem a marca, o mesmo símbolo vira `QBTC11-USD` e fica sem preço.
        let etf = Asset {
            listed_on_b3: true,
            ..asset(AssetClass::Crypto, Some("QBTC11"), None)
        };
        let (price, currency, source, _) = resolve_price(&etf, &quotes);
        assert_eq!(
            (price, currency.as_str(), source),
            (Some(dec!(52)), "BRL", PriceSource::Quote)
        );
        let (price, _, source, _) =
            resolve_price(&asset(AssetClass::Crypto, Some("QBTC11"), None), &quotes);
        assert_eq!((price, source), (None, PriceSource::Missing));

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
