//! Risco da carteira: lê do banco a cota da carteira, o Ibovespa e o histórico
//! de cada ativo (fechamento real, desdobramentos, proventos por cota, câmbio)
//! e passa para a regra pura em [`crate::domain::risk`]. Nenhuma requisição
//! espera pela rede: quem enche `price_history`, `price_splits` e o Ibovespa é
//! a sincronia do Desempenho (`performance.rs`), em segundo plano. Método e
//! limites em `docs/risco.md`.

use std::collections::{BTreeSet, HashMap};

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use time::{Date, Duration, OffsetDateTime, Weekday};
use uuid::Uuid;

use crate::domain::asset_class::{AssetClass, Market};
use crate::domain::compare;
use crate::domain::risk::{
    self as rules, Holding, Metrics, RiskWindow, MIN_COMMON_RETURNS, TOP_PAIRS,
};
use crate::domain::treasury::Bond;
use crate::history::{IndexCode, Series};
use crate::performance::{local_date, price_currency, price_symbol};
use crate::quotes::fx_symbol;
use crate::service::{self, AssetView};
use crate::store::{Store, StoreError};

/// Folga antes do começo da janela ao ler o banco: o fechamento de base fica
/// antes dela (e o câmbio do dia de base também).
const LOAD_MARGIN_DAYS: i64 = 15;

/// Por que um ativo tem (ou não tem) os números.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskStatus {
    /// Retornos bastantes na janela.
    Ok,
    /// Tem preço de mercado, mas menos de [`rules::MIN_RETURNS`] retornos na
    /// janela (lançado há pouco, ou o histórico ainda está chegando).
    Short,
    /// Sem preço de mercado: renda fixa por valor, preço manual, sem símbolo.
    NoSeries,
    /// Título do Tesouro que paga no caminho
    /// ([`rules::pays_before_maturity`]).
    PaysAlongTheWay,
}

impl RiskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RiskStatus::Ok => "ok",
            RiskStatus::Short => "short",
            RiskStatus::NoSeries => "no_series",
            RiskStatus::PaysAlongTheWay => "pays_along_the_way",
        }
    }
}

/// Um ativo da carteira na tela de risco.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetRisk {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    /// Peso na carteira hoje, em fração.
    pub weight: f64,
    pub status: RiskStatus,
    /// `None` sem série (ver `status`).
    pub metrics: Option<Metrics>,
    /// Parte do risco da carteira de hoje que vem deste ativo, em fração.
    /// `None` fora da conta (sem série, histórico mais curto que a janela).
    pub risk_share: Option<f64>,
    /// Entra na matriz de correlação.
    pub correlated: bool,
}

/// A carteira de hoje medida na janela.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelReport {
    /// Volatilidade anualizada com os pesos de hoje, em fração.
    pub volatility: f64,
    /// Pregões em comum que entraram.
    pub days: usize,
    /// O peso, na carteira, dos ativos que entraram na conta.
    pub covered_weight: f64,
}

/// Um par de ativos com a correlação dos retornos diários.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairReport {
    pub a: Uuid,
    pub b: Uuid,
    pub value: f64,
    pub days: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CorrelationReport {
    /// A ordem das linhas e colunas (a mesma da lista de ativos).
    pub assets: Vec<Uuid>,
    /// `values[i][j]`: −1 a 1; `None` com menos de
    /// [`MIN_COMMON_RETURNS`] pregões em comum.
    pub values: Vec<Vec<Option<f64>>>,
    /// Os pares que mais andam juntos, do maior para o menor.
    pub pairs: Vec<PairReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskReport {
    pub window: RiskWindow,
    /// O fechamento de base e o último pregão da janela; `None` sem pregão
    /// nenhum (sem Ibovespa e sem ativo da B3 guardados).
    pub from: Option<Date>,
    pub to: Option<Date>,
    /// Pregões da janela, sem contar o de base.
    pub trading_days: usize,
    pub portfolio: Option<Metrics>,
    pub benchmark: Option<Metrics>,
    pub model: Option<ModelReport>,
    /// Do maior peso para o menor.
    pub assets: Vec<AssetRisk>,
    pub correlation: CorrelationReport,
}

/// O que se sabe da série de um ativo antes de ler os preços.
struct Candidate<'a> {
    view: &'a AssetView,
    weight: f64,
    /// Símbolo do preço e, fora do real, o do câmbio.
    symbol: Option<(String, Option<String>)>,
    status: RiskStatus,
}

fn candidate(view: &AssetView, total: Decimal) -> Candidate<'_> {
    let weight = if total > Decimal::ZERO {
        (view.current_value / total).to_f64().unwrap_or(0.0)
    } else {
        0.0
    };
    let market = view.asset.market();
    let symbol = price_symbol(view);
    let pays = market == Market::Treasury
        && symbol
            .as_deref()
            .and_then(Bond::from_symbol)
            .is_some_and(|bond| rules::pays_before_maturity(bond.kind));
    let status = match (&symbol, pays) {
        (None, _) => RiskStatus::NoSeries,
        (Some(_), true) => RiskStatus::PaysAlongTheWay,
        (Some(_), false) => RiskStatus::Ok,
    };
    let symbol = symbol.filter(|_| status == RiskStatus::Ok).map(|s| {
        let currency = price_currency(view);
        (s, (currency != "BRL").then(|| fx_symbol(&currency)))
    });
    Candidate {
        view,
        weight,
        symbol,
        status,
    }
}

fn weekend(date: Date) -> bool {
    matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
}

/// Os dias úteis (segunda a sexta) de `from` a `to`: o calendário quando não
/// há Ibovespa nem ativo da B3 guardado (carteira só de exterior e cripto com a
/// fonte do índice fora do ar).
fn weekdays(from: Date, to: Date) -> Vec<Date> {
    let mut out = Vec::new();
    let mut date = from;
    while date <= to {
        if !weekend(date) {
            out.push(date);
        }
        match date.next_day() {
            Some(next) => date = next,
            None => break,
        }
    }
    out
}

/// O índice de retorno total do ativo em reais, alinhado ao calendário: o
/// fechamento real volta ao pé de hoje pelos desdobramentos e ganha os
/// proventos por cota (as mesmas contas do comparador), na moeda do ativo, e o
/// câmbio do dia o leva para reais.
fn asset_levels(
    symbol: &str,
    fx: Option<&str>,
    closes: &HashMap<String, Series>,
    splits: &HashMap<String, Series>,
    dividends: &HashMap<String, Series>,
    calendar: &[Date],
) -> Vec<Option<f64>> {
    let empty = Series::new();
    let adjusted = compare::adjust_for_splits(
        closes.get(symbol).unwrap_or(&empty),
        splits.get(symbol).unwrap_or(&empty),
    );
    let total = compare::total_return_levels(&adjusted, dividends.get(symbol).unwrap_or(&empty));
    let aligned = rules::align(&rules::levels(&total), calendar);
    match fx {
        None => aligned,
        Some(fx) => {
            let rate = rules::levels(closes.get(fx).unwrap_or(&empty));
            rules::times(&aligned, &rules::align(&rate, calendar))
        }
    }
}

/// O risco da carteira da pessoa na janela que termina hoje.
pub async fn report(
    store: &Store,
    user: Uuid,
    window: RiskWindow,
    now: OffsetDateTime,
) -> Result<RiskReport, StoreError> {
    let today = local_date(now);
    let start = window.start(today);
    let load_from = start - Duration::days(LOAD_MARGIN_DAYS);

    let view = service::portfolio(store, user).await?;
    let mut candidates: Vec<Candidate<'_>> = view
        .assets
        .iter()
        .filter(|a| a.asset.quantity > Decimal::ZERO)
        .map(|a| candidate(a, view.total_value))
        .collect();
    candidates.sort_by(|x, y| {
        y.weight
            .total_cmp(&x.weight)
            .then_with(|| x.view.label().cmp(&y.view.label()))
    });

    let priced: BTreeSet<String> = candidates
        .iter()
        .filter_map(|c| c.symbol.as_ref().map(|(s, _)| s.clone()))
        .collect();
    let mut symbols: BTreeSet<String> = priced.clone();
    symbols.extend(
        candidates
            .iter()
            .filter_map(|c| c.symbol.as_ref().and_then(|(_, fx)| fx.clone())),
    );
    let priced: Vec<String> = priced.into_iter().collect();
    let symbols: Vec<String> = symbols.into_iter().collect();
    let closes = store.price_series(&symbols, load_from, today).await?;
    let splits = store.price_splits(&priced).await?;
    let dividends: HashMap<String, Series> = store
        .dividend_cache(&priced)
        .await?
        .into_iter()
        .map(|(symbol, cached)| {
            let events = cached
                .events
                .into_iter()
                .map(|e| (e.ex_date, e.amount))
                .collect();
            (symbol, events)
        })
        .collect();
    let ibov = store
        .index_series(IndexCode::Ibov, load_from, today)
        .await?;

    // Os pregões: as datas do Ibovespa e de todo fechamento da B3 guardado, de
    // segunda a sexta (fonte que date um ponto pelo UTC não cria pregão no
    // sábado) e nunca depois de hoje em Brasília.
    let b3_dates: Vec<Date> = candidates
        .iter()
        .filter(|c| c.view.asset.market() == Market::B3)
        .filter_map(|c| c.symbol.as_ref())
        .filter_map(|(s, _)| closes.get(s))
        .flat_map(|series| series.iter().map(|(d, _)| *d))
        .collect();
    let ibov_dates: Vec<Date> = ibov.iter().map(|(d, _)| *d).collect();
    let mut calendar: Vec<Date> = rules::trading_calendar(&[&ibov_dates, &b3_dates])
        .into_iter()
        .filter(|d| *d >= load_from && *d <= today && !weekend(*d))
        .collect();
    if calendar.is_empty() {
        calendar = weekdays(load_from, today);
    }
    // O fechamento de base: o último pregão em ou antes do começo da janela.
    // Sem pregão guardado tão antigo, a janela começa no primeiro, e nenhuma
    // série cobre a janela inteira.
    let base = calendar.partition_point(|d| *d <= start).saturating_sub(1);
    let calendar = calendar.split_off(base);
    let covers = calendar.first().is_some_and(|d| *d <= start);
    let measure = |levels: &[Option<f64>]| {
        rules::metrics(&calendar, levels).map(|mut m| {
            m.full &= covers;
            m
        })
    };

    let quotas: Series = store
        .snapshot_points(user)
        .await?
        .into_iter()
        .filter(|p| p.date >= load_from)
        .map(|p| (p.date, p.quota_value))
        .collect();
    let portfolio = measure(&rules::align(&rules::levels(&quotas), &calendar));
    let benchmark = measure(&rules::align(&rules::levels(&ibov), &calendar));

    let mut assets: Vec<AssetRisk> = Vec::with_capacity(candidates.len());
    let mut returns: Vec<Vec<Option<f64>>> = Vec::with_capacity(candidates.len());
    for c in &candidates {
        let (metrics, daily) = match &c.symbol {
            Some((symbol, fx)) => {
                let levels = asset_levels(
                    symbol,
                    fx.as_deref(),
                    &closes,
                    &splits,
                    &dividends,
                    &calendar,
                );
                (measure(&levels), rules::returns(&levels))
            }
            None => (None, Vec::new()),
        };
        let status = match (c.status, &metrics) {
            (RiskStatus::Ok, Some(m)) if m.enough() => RiskStatus::Ok,
            (RiskStatus::Ok, _) => RiskStatus::Short,
            (other, _) => other,
        };
        let correlated =
            status == RiskStatus::Ok && metrics.is_some_and(|m| m.returns >= MIN_COMMON_RETURNS);
        assets.push(AssetRisk {
            asset_id: c.view.asset.id,
            label: c.view.label(),
            asset_class: c.view.asset.asset_class,
            weight: c.weight,
            status,
            metrics,
            risk_share: None,
            correlated,
        });
        returns.push(daily);
    }

    // A correlação: os ativos com pregões bastantes, na ordem da lista.
    let in_matrix: Vec<usize> = (0..assets.len())
        .filter(|&i| assets[i].correlated)
        .collect();
    let series: Vec<&[Option<f64>]> = in_matrix.iter().map(|&i| returns[i].as_slice()).collect();
    let matrix = rules::correlation_matrix(&series);
    let correlation = CorrelationReport {
        assets: in_matrix.iter().map(|&i| assets[i].asset_id).collect(),
        values: matrix
            .iter()
            .map(|row| row.iter().map(|cell| cell.map(|(v, _)| v)).collect())
            .collect(),
        pairs: rules::top_pairs(&matrix, TOP_PAIRS)
            .into_iter()
            .map(|p| PairReport {
                a: assets[in_matrix[p.a]].asset_id,
                b: assets[in_matrix[p.b]].asset_id,
                value: p.value,
                days: p.days,
            })
            .collect(),
    };

    // A contribuição: só quem cobre a janela inteira, com os pesos de hoje.
    let in_model: Vec<usize> = in_matrix
        .iter()
        .copied()
        .filter(|&i| assets[i].metrics.is_some_and(|m| m.full))
        .collect();
    let holdings: Vec<Holding<'_>> = in_model
        .iter()
        .map(|&i| Holding {
            weight: assets[i].weight,
            returns: &returns[i],
        })
        .collect();
    let model = rules::risk_model(&holdings).map(|m| {
        for (&i, share) in in_model.iter().zip(&m.shares) {
            assets[i].risk_share = Some(*share);
        }
        ModelReport {
            volatility: m.volatility,
            days: m.days,
            covered_weight: in_model.iter().map(|&i| assets[i].weight).sum(),
        }
    });

    Ok(RiskReport {
        window,
        from: calendar.first().copied(),
        to: calendar.last().copied(),
        trading_days: calendar.len().saturating_sub(1),
        portfolio,
        benchmark,
        model,
        assets,
        correlation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn weekdays_skip_the_weekend() {
        // 27/11/2026 é sexta.
        let days = weekdays(date!(2026 - 11 - 27), date!(2026 - 11 - 30));
        assert_eq!(days, vec![date!(2026 - 11 - 27), date!(2026 - 11 - 30)]);
        assert!(weekdays(date!(2026 - 11 - 28), date!(2026 - 11 - 27)).is_empty());
    }

    #[test]
    fn statuses_have_stable_names() {
        assert_eq!(RiskStatus::Ok.as_str(), "ok");
        assert_eq!(RiskStatus::Short.as_str(), "short");
        assert_eq!(RiskStatus::NoSeries.as_str(), "no_series");
        assert_eq!(RiskStatus::PaysAlongTheWay.as_str(), "pays_along_the_way");
    }
}
