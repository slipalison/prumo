//! Os indicadores de mercado dos FIIs (liquidez média diária, CAGR dos
//! proventos e da cota, rendimento de 12 meses) a partir do que já está
//! guardado: o fechamento e o volume de `price_history`, os desdobramentos de
//! `price_splits` e os proventos por cota de `dividend_history`. Nada vai à
//! rede aqui: quem enche o banco é a sincronia de preços do Desempenho (a
//! carteira) e o comparador (o ativo de fora dela). A regra é
//! [`crate::domain::fund_market`]; o método, `docs/fundamentos.md` ("FII
//! completo").

use std::collections::HashMap;

use rust_decimal::Decimal;
use time::{Date, Duration};

use crate::ceiling::ReferencesReport;
use crate::domain::asset_class::Market;
use crate::domain::compare::BASE_SLACK_DAYS;
use crate::domain::fund_market::{
    fund_market, FundMarket, MarketHistory, LONG_YEARS, TRADED_LOOKBACK_DAYS,
};
use crate::domain::fundamentals::{AssetFundamentals, Figure, Gap};
use crate::domain::performance::months_before;
use crate::service::AssetView;
use crate::store::{Store, StoreError};

/// O que a tabela de FIIs mostra além das referências: o caixa do informe da
/// CVM e o que sai do histórico de mercado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundIndicators {
    pub cash_share: Figure,
    pub market: FundMarket,
}

/// Desde quando o histórico de preço precisa estar lido para o CAGR de 5 anos.
pub fn history_from(today: Date) -> Date {
    months_before(today, 12 * LONG_YEARS) - Duration::days(BASE_SLACK_DAYS)
}

/// Os indicadores de mercado de cada FII (`symbol` da B3, `KNRI11`), com a
/// cotação de agora (para o rendimento de 12 meses), por símbolo.
pub async fn market_of(
    store: &Store,
    funds: &[(String, Option<Decimal>)],
    today: Date,
) -> Result<HashMap<String, FundMarket>, StoreError> {
    let quoted: Vec<(String, String, Option<Decimal>)> = funds
        .iter()
        .filter_map(|(symbol, price)| {
            Market::B3
                .quote_symbol(symbol)
                .map(|q| (symbol.clone(), q, *price))
        })
        .collect();
    if quoted.is_empty() {
        return Ok(HashMap::new());
    }
    let symbols: Vec<String> = quoted.iter().map(|(_, q, _)| q.clone()).collect();
    let closes = store
        .price_series(&symbols, history_from(today), today)
        .await?;
    let traded = store
        .traded_values(
            &symbols,
            today - Duration::days(TRADED_LOOKBACK_DAYS),
            today,
        )
        .await?;
    let splits = store.price_splits(&symbols).await?;
    let cached = store.dividend_cache(&symbols).await?;
    let dividends: HashMap<&String, Vec<(Date, Decimal)>> = cached
        .iter()
        .map(|(symbol, c)| {
            (
                symbol,
                c.events.iter().map(|e| (e.ex_date, e.amount)).collect(),
            )
        })
        .collect();
    let empty: Vec<(Date, Decimal)> = Vec::new();
    Ok(quoted
        .into_iter()
        .map(|(symbol, quote, price)| {
            let history = MarketHistory {
                closes: closes.get(&quote).unwrap_or(&empty),
                traded: traded.get(&quote).unwrap_or(&empty),
                splits: splits.get(&quote).unwrap_or(&empty),
                dividends: dividends.get(&quote).map(Vec::as_slice),
                price,
            };
            (symbol, fund_market(&history, today))
        })
        .collect())
}

/// O caixa do informe da CVM de um FII, ou o motivo de não haver.
pub fn cash_share_of(fundamentals: Option<&AssetFundamentals>) -> Figure {
    match fundamentals {
        Some(AssetFundamentals::Fund(m)) => m.cash_share,
        _ => Figure::missing(Gap::NotReported),
    }
}

/// Os indicadores de cada FII das referências (a tabela de FIIs da Análise e a
/// folha do ativo), por id do ativo.
pub async fn for_references(
    store: &Store,
    report: &ReferencesReport,
    assets: &[&AssetView],
    fundamentals: &HashMap<String, AssetFundamentals>,
    today: Date,
) -> Result<HashMap<uuid::Uuid, FundIndicators>, StoreError> {
    let wanted: Vec<(uuid::Uuid, String, Option<Decimal>)> = report
        .funds
        .iter()
        .filter_map(|item| {
            let view = assets.iter().find(|a| a.asset.id == item.asset_id)?;
            let symbol = view.asset.symbol.clone()?;
            Some((item.asset_id, symbol, item.unit_price))
        })
        .collect();
    let funds: Vec<(String, Option<Decimal>)> = wanted
        .iter()
        .map(|(_, symbol, price)| (symbol.clone(), *price))
        .collect();
    let market = market_of(store, &funds, today).await?;
    Ok(wanted
        .into_iter()
        .filter_map(|(id, symbol, _)| {
            Some((
                id,
                FundIndicators {
                    cash_share: cash_share_of(fundamentals.get(&symbol)),
                    market: *market.get(&symbol)?,
                },
            ))
        })
        .collect())
}
