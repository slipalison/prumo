//! Análise da carteira: junta o banco (carteira, proventos, série de cotas,
//! índices, histórico de preço e de dividendos) e as regras puras de
//! [`crate::domain::analysis`]. As regras, as fórmulas e os limiares estão em
//! `docs/analise.md`.
//!
//! Nada aqui espera pela rede: tudo vem do que já está guardado. Série curta,
//! índice que ainda não chegou ou histórico de preço que não alcança 12 meses
//! viram "sem dado" — nunca erro, nunca aviso.

use std::collections::{BTreeSet, HashMap};

use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::domain::allocation::Plan;
use crate::domain::analysis::highlights::{self, Mover, Ranking};
use crate::domain::analysis::profile::{self, Mix};
use crate::domain::analysis::quality::{self, Attention};
use crate::domain::analysis::references::{self, Bazin, PriceReferences};
use crate::domain::analysis::returns::{self, Benchmark, Horizon, ReturnsFacts};
use crate::domain::analysis::{
    self as rules, AnalysisAsset, AnalysisConfig, DimensionReport, Input, RuleResult,
};
use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::fundamentals::AssetFundamentals;
use crate::domain::income::{today_in_brazil, DividendPoint};
use crate::domain::performance::months_before;
use crate::domain::retro;
use crate::domain::sector::SectorSource;
use crate::history::IndexCode;
use crate::performance::{self as series, Indexes};
use crate::quotes::fx_symbol;
use crate::service::{self, AssetView, PortfolioView};
use crate::store::{Asset, Store, StoreError};

/// O fechamento de base dos 12 meses pode ser de até 10 dias antes da data
/// (fim de semana, feriado, papel sem negócio): mais velho que isso o preço
/// não representa a data.
const BASE_PRICE_SLACK_DAYS: i64 = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileReport {
    pub current: Option<Mix>,
    /// O que a pessoa quer: o perfil das metas por classe.
    pub target: Option<Mix>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HighlightsReport {
    /// Pela variação do preço nos últimos 12 meses.
    pub price_12m: Ranking,
    /// Pela rentabilidade desde a compra (preço de hoje contra o preço médio).
    pub since_purchase: Ranking,
}

/// Uma referência de preço de um ativo.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceItem {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    pub unit_price: Option<Decimal>,
    /// O teto de preço que a pessoa pôs no ativo.
    pub max_price: Option<Decimal>,
    pub bazin: Option<Bazin>,
    pub graham: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferencesReport {
    /// O rendimento-alvo do Bazin, em % ao ano.
    pub bazin_yield: Decimal,
    pub items: Vec<ReferenceItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisReport {
    pub today: Date,
    pub total_value: Decimal,
    pub dimensions: Vec<DimensionReport>,
    /// Quantas regras estão em aviso: "N pontos a ver".
    pub attention_count: u32,
    /// Os pontos de atenção dos ativos (fundamentos e queda da renda).
    pub asset_attention: Attention,
    pub profile: ProfileReport,
    pub highlights: HighlightsReport,
    pub references: ReferencesReport,
    /// Ativos com valor em Ações Nacionais e FIIs que a fonte de setor não
    /// classificou.
    pub unclassified_count: usize,
}

/// Quantos ativos com valor em Ações Nacionais e FIIs a fonte de setor não
/// classificou.
pub fn unclassified_count(view: &PortfolioView) -> usize {
    view.assets
        .iter()
        .filter(|a| {
            a.current_value > Decimal::ZERO
                && a.sector.is_none()
                && SectorSource::ALL
                    .iter()
                    .any(|s| s.class() == a.asset.asset_class)
        })
        .count()
}

/// Os ativos como as regras os enxergam.
pub fn analysis_assets(view: &PortfolioView) -> Vec<AnalysisAsset> {
    view.assets
        .iter()
        .map(|a| AnalysisAsset {
            id: a.asset.id,
            label: a.label(),
            asset_class: a.asset.asset_class,
            symbol: a.asset.symbol.clone(),
            value: a.current_value,
            sector: a.sector.as_ref().map(|s| s.sector.clone()),
            cost_basis: a.performance.cost_basis,
        })
        .collect()
}

fn index_code(benchmark: Benchmark) -> IndexCode {
    match benchmark {
        Benchmark::Cdi => IndexCode::Cdi,
        Benchmark::Ibov => IndexCode::Ibov,
        Benchmark::Ipca => IndexCode::Ipca,
    }
}

/// A rentabilidade da carteira e as comparações com os índices.
async fn returns_facts(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
    today: Date,
) -> Result<ReturnsFacts, StoreError> {
    let points = series::series(store, user, now).await?;
    let quotas = series::quota_points(&points);
    let codes = Benchmark::ALL.map(index_code);
    let indexes: Indexes = series::load_indexes(store, &codes, &points, today).await?;
    let mut comparisons = Vec::new();
    for benchmark in Benchmark::ALL {
        let code = index_code(benchmark);
        let values = indexes.get(&code).map_or(&[][..], Vec::as_slice);
        let index_start = values.first().map(|(date, _)| *date);
        for horizon in Horizon::ALL {
            if let Some(comparison) =
                returns::compare(&quotas, today, horizon, index_start, |from, to| {
                    series::accrue(code, values, from, to)
                })
            {
                comparisons.push((benchmark, horizon, comparison));
            }
        }
    }
    Ok(ReturnsFacts::from_series(&quotas, today, comparisons))
}

/// Destaques: o preço em 12 meses (ajustado por desdobramentos) e a
/// rentabilidade desde a compra.
async fn build_highlights(
    store: &Store,
    user: Uuid,
    view: &PortfolioView,
    today: Date,
) -> Result<HighlightsReport, StoreError> {
    let owned: Vec<&AssetView> = view
        .assets
        .iter()
        .filter(|a| a.current_value > Decimal::ZERO)
        .collect();

    let since_purchase = rank(owned.iter().filter_map(|a| {
        let change = a.performance.return_percent?;
        Some(mover(a, change.round_dp(2)))
    }));

    let base_day = months_before(today, 12);
    let mut symbols: BTreeSet<String> = BTreeSet::new();
    for asset in &owned {
        if let Some(symbol) = series::price_symbol(asset) {
            symbols.insert(symbol);
            let currency = series::price_currency(asset);
            if currency != "BRL" {
                symbols.insert(fx_symbol(&currency));
            }
        }
    }
    let symbols: Vec<String> = symbols.into_iter().collect();
    let closes = store.price_series(&symbols, base_day, today).await?;
    let entries = store.ledger_entries(user).await?;
    let empty = retro::Series::new();
    let price_12m = rank(owned.iter().filter_map(|a| {
        let symbol = series::price_symbol(a)?;
        let now = a.unit_price.filter(|p| *p > Decimal::ZERO)?;
        let currency = series::price_currency(a);
        let fx = (currency != "BRL").then(|| closes.get(&fx_symbol(&currency)).unwrap_or(&empty));
        let in_reais = retro::in_reais(closes.get(&symbol)?, fx);
        let (base_date, base) = in_reais.iter().rev().find(|(d, _)| *d <= base_day)?;
        if base_day - *base_date > Duration::days(BASE_PRICE_SLACK_DAYS) {
            return None;
        }
        let book = entries.get(&a.asset.id).map_or(&[][..], Vec::as_slice);
        let factor = highlights::share_factor_after(book, *base_date)?;
        Some(mover(a, highlights::price_change(now, *base, factor)?))
    }));

    Ok(HighlightsReport {
        price_12m,
        since_purchase,
    })
}

fn mover(asset: &AssetView, change_percent: Decimal) -> Mover {
    Mover {
        asset_id: asset.asset.id,
        label: asset.label(),
        asset_class: asset.asset.asset_class,
        change_percent,
    }
}

fn rank(movers: impl Iterator<Item = Mover>) -> Ranking {
    highlights::rank(movers.collect())
}

/// O símbolo da cotação (a chave do cache de dividendos) de um ativo que pode
/// ter referência de preço: Ação Nacional ou FII da B3, com símbolo.
fn reference_symbol(asset: &Asset) -> Option<String> {
    let listed = matches!(
        asset.asset_class,
        AssetClass::BrStocks | AssetClass::BrReits
    ) && asset.market() == Market::B3;
    if !listed {
        return None;
    }
    asset.market().quote_symbol(asset.symbol.as_deref()?)
}

/// As referências de Bazin e de Graham de cada ativo dado, só com o que já está
/// guardado: o histórico de dividendos por cota do cache do Yahoo (em reais) e
/// os `fundamentals` que o chamador leu. Não espera a rede. O rendimento-alvo é
/// o da pessoa. Ativo fora de Ações Nacionais e FIIs da B3, ou sem nenhuma das
/// duas referências, não entra no mapa.
///
/// É a conta única que a Análise (`build_references`) e o aviso do aporte
/// (`plan_references`) usam.
pub async fn price_references(
    store: &Store,
    user: Uuid,
    assets: &[&Asset],
    today: Date,
    fundamentals: &HashMap<String, AssetFundamentals>,
) -> Result<(Decimal, HashMap<Uuid, PriceReferences>), StoreError> {
    let bazin_yield = store.bazin_yield(user).await?;
    let payers: Vec<(&Asset, String)> = assets
        .iter()
        .filter_map(|asset| Some((*asset, reference_symbol(asset)?)))
        .collect();
    let symbols: Vec<String> = payers.iter().map(|(_, s)| s.clone()).collect();
    let cache = store.dividend_cache(&symbols).await?;
    let found = payers
        .into_iter()
        .filter_map(|(asset, quote_symbol)| {
            let points: Option<Vec<DividendPoint>> = cache
                .get(&quote_symbol)
                .filter(|c| c.currency == "BRL")
                .map(|cached| {
                    cached
                        .events
                        .iter()
                        .map(|e| DividendPoint {
                            ex_date: e.ex_date,
                            amount: e.amount,
                        })
                        .collect()
                });
            let references = references::price_references(
                points.as_deref(),
                asset
                    .symbol
                    .as_deref()
                    .and_then(|symbol| fundamentals.get(symbol)),
                today,
                bazin_yield,
            );
            references.any().then_some((asset.id, references))
        })
        .collect();
    Ok((bazin_yield, found))
}

/// Bazin (e Graham, quando houver fundamentos) dos ativos da carteira, para a
/// tela de Análise.
async fn build_references(
    store: &Store,
    user: Uuid,
    view: &PortfolioView,
    today: Date,
    fundamentals: &HashMap<String, AssetFundamentals>,
) -> Result<ReferencesReport, StoreError> {
    let assets: Vec<&Asset> = view.assets.iter().map(|a| &a.asset).collect();
    let (bazin_yield, found) = price_references(store, user, &assets, today, fundamentals).await?;
    let mut items: Vec<ReferenceItem> = view
        .assets
        .iter()
        .filter_map(|asset| {
            let references = found.get(&asset.asset.id)?;
            Some(ReferenceItem {
                asset_id: asset.asset.id,
                label: asset.label(),
                asset_class: asset.asset.asset_class,
                unit_price: asset.unit_price,
                max_price: asset.asset.max_price,
                bazin: references.bazin,
                graham: references.graham,
            })
        })
        .collect();
    items.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(ReferencesReport { bazin_yield, items })
}

/// As referências dos ativos que o plano manda comprar e que **não têm teto**:
/// é o aviso informativo do aporte (nada bloqueia, nada preenche o teto). Lê
/// os fundamentos só desses ativos e nada da rede. Ativo com teto, ou sem
/// nenhuma das duas referências, não entra.
pub async fn plan_references(
    store: &Store,
    user: Uuid,
    plan: &Plan,
    view: &PortfolioView,
    today: Date,
) -> Result<HashMap<Uuid, PriceReferences>, StoreError> {
    let without_ceiling: Vec<&Asset> = plan
        .items
        .iter()
        .filter_map(|item| view.asset(item.asset_id))
        .map(|view| &view.asset)
        .filter(|asset| asset.max_price.is_none() && reference_symbol(asset).is_some())
        .collect();
    if without_ceiling.is_empty() {
        return Ok(HashMap::new());
    }
    let fundamentals = store.fundamentals_for_assets(&without_ceiling).await?;
    let (_, found) = price_references(store, user, &without_ceiling, today, &fundamentals).await?;
    Ok(found)
}

/// A análise inteira da carteira da pessoa.
pub async fn report(
    store: &Store,
    user: Uuid,
    now: OffsetDateTime,
    fundamentals: &HashMap<String, AssetFundamentals>,
) -> Result<AnalysisReport, StoreError> {
    let today = today_in_brazil(now);
    let view = service::portfolio(store, user).await?;
    let config: AnalysisConfig = store.analysis_config(user).await?;
    let income = store.income_history(user, today).await?;
    let returns = returns_facts(store, user, now, today).await?;
    let assets = analysis_assets(&view);
    let quality = quality::quality_assets(&assets, fundamentals);
    let results: Vec<RuleResult> = rules::evaluate(
        &config,
        &Input {
            assets: &assets,
            targets: view.targets,
            income: &income,
            returns: &returns,
            quality: &quality,
        },
    );
    let attention_count = rules::attention_count(&results);
    Ok(AnalysisReport {
        today,
        total_value: view.total_value,
        dimensions: rules::dimensions(results),
        attention_count,
        asset_attention: quality::attention(&assets, &quality, &income),
        profile: ProfileReport {
            current: profile::current(
                view.assets
                    .iter()
                    .map(|a| (a.asset.asset_class, a.current_value)),
            ),
            target: profile::target(view.targets),
        },
        highlights: build_highlights(store, user, &view, today).await?,
        references: build_references(store, user, &view, today, fundamentals).await?,
        unclassified_count: unclassified_count(&view),
    })
}
