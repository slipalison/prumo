//! O teto de preço de cada ativo e as referências que o calculam, só com o que
//! já está guardado: o histórico de dividendos (Bazin), os fundamentos da CVM
//! (Graham e VP), a estimativa de renda (proventos de 12 meses do Rendimento),
//! o tipo e a composição dos FIIs e as taxas de mercado. Nada espera a rede.
//!
//! A regra é pura e mora em [`crate::domain::ceiling`] e em
//! `domain::analysis::{references, fund_references}`; aqui só se lê o banco. É
//! a **mesma conta** para o plano de aporte (o motor), a Análise, os chips do
//! plano, a carteira e os alertas.

use std::collections::{BTreeSet, HashMap};

use rust_decimal::Decimal;
use time::Date;
use uuid::Uuid;

use crate::domain::allocation::Plan;
use crate::domain::analysis::fund_references::{self, MarketRates, Premiums};
use crate::domain::analysis::references;
use crate::domain::ceiling::{self, AssetReferences, Ceiling, CeilingChoice, ReferenceFamily};
use crate::domain::fundamentals::AssetFundamentals;
use crate::domain::income::DividendPoint;
use crate::domain::sector::{reference_key, FundType};
use crate::income::estimate_assets;
use crate::service::{AssetView, PortfolioView};
use crate::store::{Asset, Store, StoreError};

/// O que as referências pedem além do ativo: o rendimento-alvo do Bazin e os
/// prêmios da pessoa, e as taxas de mercado de hoje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceContext {
    /// Em % ao ano.
    pub bazin_yield: Decimal,
    pub premiums: Premiums,
    pub rates: MarketRates,
}

/// O contexto das referências de `user` em `today`.
pub async fn context(
    store: &Store,
    user: Uuid,
    today: Date,
) -> Result<ReferenceContext, StoreError> {
    let settings = store.reference_settings(user).await?;
    Ok(ReferenceContext {
        bazin_yield: settings.bazin_yield,
        premiums: settings.premiums,
        rates: MarketRates {
            real_rate: store.real_rate().await?,
            cdi_12m: store.cdi_12m(today).await?,
        },
    })
}

/// Que referências o ativo tem: Ações Nacionais e FIIs da B3, com símbolo.
pub fn family(asset: &Asset) -> Option<ReferenceFamily> {
    asset.symbol.as_ref()?;
    ReferenceFamily::of(asset.asset_class, asset.market())
}

/// O símbolo da cotação (a chave do cache de dividendos) de um ativo da B3.
fn quote_symbol(asset: &Asset) -> Option<String> {
    asset.market().quote_symbol(asset.symbol.as_deref()?)
}

/// As referências de cada ativo dado que tem família ([`family`]): Bazin e
/// Graham nas ações, VP e Rendimento nos FIIs. `fundamentals` é o que o
/// chamador já leu (a Análise lê os da carteira inteira de uma vez).
pub async fn references(
    store: &Store,
    ctx: &ReferenceContext,
    assets: &[&AssetView],
    today: Date,
    fundamentals: &HashMap<String, AssetFundamentals>,
) -> Result<HashMap<Uuid, AssetReferences>, StoreError> {
    let of = |wanted: ReferenceFamily| -> Vec<&AssetView> {
        assets
            .iter()
            .copied()
            .filter(|a| family(&a.asset) == Some(wanted))
            .collect()
    };
    let stocks = of(ReferenceFamily::Stock);
    let funds = of(ReferenceFamily::Fund);
    let fundamentals_of = |asset: &Asset| {
        asset
            .symbol
            .as_deref()
            .and_then(|symbol| fundamentals.get(symbol))
    };
    let mut found = HashMap::new();

    if !stocks.is_empty() {
        let symbols: Vec<String> = stocks
            .iter()
            .filter_map(|a| quote_symbol(&a.asset))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let cache = store.dividend_cache(&symbols).await?;
        for asset in stocks {
            // O Bazin é em reais: histórico em outra moeda não entra.
            let points: Option<Vec<DividendPoint>> = quote_symbol(&asset.asset)
                .and_then(|symbol| cache.get(&symbol))
                .filter(|cached| cached.currency == "BRL")
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
            let refs = references::price_references(
                points.as_deref(),
                fundamentals_of(&asset.asset),
                today,
                ctx.bazin_yield,
            );
            found.insert(asset.asset.id, AssetReferences::Stock(refs));
        }
    }

    if !funds.is_empty() {
        // Os proventos de 12 meses por cota: a mesma conta (fonte, janela e
        // base de cotas) da estimativa de renda.
        let income = estimate_assets(store, None, &funds, today).await;
        let code = |asset: &Asset| {
            reference_key(asset.asset_class, asset.symbol.as_deref()?).map(|(_, code)| code)
        };
        let codes: Vec<String> = funds
            .iter()
            .filter_map(|a| code(&a.asset))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let compositions = store.fund_compositions(&codes).await?;
        for asset in funds {
            let dividends = income
                .asset(asset.asset.id)
                .map(|found| found.estimate.per_unit_12m.net);
            let fund_type = asset
                .sector
                .as_ref()
                .and_then(|s| FundType::from_label(&s.sector));
            let composition = code(&asset.asset).and_then(|c| compositions.get(&c).copied());
            let refs = fund_references::fund_references(
                fundamentals_of(&asset.asset),
                dividends,
                fund_type,
                composition,
                &ctx.rates,
                ctx.premiums,
            );
            found.insert(asset.asset.id, AssetReferences::Fund(refs));
        }
    }
    Ok(found)
}

/// O teto de hoje de cada ativo da carteira, pela escolha e pelas referências
/// já calculadas. Ativo de método sem referência carregada fica sem valor,
/// nunca "sem teto".
pub fn ceilings(
    view: &PortfolioView,
    found: &HashMap<Uuid, AssetReferences>,
) -> HashMap<Uuid, Ceiling> {
    view.assets
        .iter()
        .map(|a| {
            (
                a.asset.id,
                ceiling::effective(a.asset.ceiling, found.get(&a.asset.id)),
            )
        })
        .collect()
}

/// As referências de `assets` com os fundamentos lidos só deles.
async fn references_reading_fundamentals(
    store: &Store,
    ctx: &ReferenceContext,
    assets: &[&AssetView],
    today: Date,
) -> Result<HashMap<Uuid, AssetReferences>, StoreError> {
    let rows: Vec<&Asset> = assets.iter().map(|a| &a.asset).collect();
    let fundamentals = store.fundamentals_for_assets(&rows).await?;
    references(store, ctx, assets, today, &fundamentals).await
}

/// O teto de hoje de cada ativo da carteira. Só lê referência de quem escolheu
/// um método: sem nenhum, não há consulta além da carteira.
pub async fn resolve_ceilings(
    store: &Store,
    user: Uuid,
    view: &PortfolioView,
    today: Date,
) -> Result<HashMap<Uuid, Ceiling>, StoreError> {
    let by_method: Vec<&AssetView> = view
        .assets
        .iter()
        .filter(|a| a.asset.ceiling.method().is_some())
        .collect();
    let found = if by_method.is_empty() {
        HashMap::new()
    } else {
        let ctx = context(store, user, today).await?;
        references_reading_fundamentals(store, &ctx, &by_method, today).await?
    };
    Ok(ceilings(view, &found))
}

/// Um ativo com referências, como a Análise e a escolha do teto o mostram.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceItem {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: crate::domain::asset_class::AssetClass,
    pub unit_price: Option<Decimal>,
    /// O teto de hoje pela escolha da pessoa.
    pub ceiling: Ceiling,
    pub references: AssetReferences,
}

/// As referências de um conjunto de ativos: as ações e os FIIs, cada lista em
/// ordem de rótulo, e o contexto (rendimento-alvo, prêmios, taxas).
#[derive(Debug, Clone, PartialEq)]
pub struct ReferencesReport {
    pub context: ReferenceContext,
    pub stocks: Vec<ReferenceItem>,
    pub funds: Vec<ReferenceItem>,
}

/// As referências e o teto de cada ativo de `assets` que tem família.
pub async fn report(
    store: &Store,
    user: Uuid,
    assets: &[&AssetView],
    today: Date,
    fundamentals: Option<&HashMap<String, AssetFundamentals>>,
) -> Result<ReferencesReport, StoreError> {
    let ctx = context(store, user, today).await?;
    let found = match fundamentals {
        Some(fundamentals) => references(store, &ctx, assets, today, fundamentals).await?,
        None => references_reading_fundamentals(store, &ctx, assets, today).await?,
    };
    let mut stocks = Vec::new();
    let mut funds = Vec::new();
    for asset in assets {
        let Some(refs) = found.get(&asset.asset.id) else {
            continue;
        };
        let item = ReferenceItem {
            asset_id: asset.asset.id,
            label: asset.label(),
            asset_class: asset.asset.asset_class,
            unit_price: asset.unit_price,
            ceiling: ceiling::effective(asset.asset.ceiling, Some(refs)),
            references: *refs,
        };
        match refs.family() {
            ReferenceFamily::Stock => stocks.push(item),
            ReferenceFamily::Fund => funds.push(item),
        }
    }
    stocks.sort_by(|a, b| a.label.cmp(&b.label));
    funds.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(ReferencesReport {
        context: ctx,
        stocks,
        funds,
    })
}

/// As referências dos ativos que o plano manda comprar e que **não têm teto**:
/// o aviso informativo do aporte (nada bloqueia, nada preenche o teto). Ativo
/// com teto, ou sem família, não entra.
pub async fn plan_references(
    store: &Store,
    user: Uuid,
    plan: &Plan,
    view: &PortfolioView,
    today: Date,
) -> Result<HashMap<Uuid, AssetReferences>, StoreError> {
    let without_ceiling: Vec<&AssetView> = plan
        .items
        .iter()
        .filter_map(|item| view.asset(item.asset_id))
        .filter(|a| a.asset.ceiling == CeilingChoice::None && family(&a.asset).is_some())
        .collect();
    if without_ceiling.is_empty() {
        return Ok(HashMap::new());
    }
    let ctx = context(store, user, today).await?;
    references_reading_fundamentals(store, &ctx, &without_ceiling, today).await
}
