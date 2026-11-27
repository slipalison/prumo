//! O que a projeção lê do banco e como a estimativa de renda vira entrada
//! dela. A regra está em `domain::projection`; o método, em
//! `docs/projecao.md`. Nada aqui fala com a rede: o Focus vem da busca diária
//! (`crate::focus`), o CDI e o IPCA da série do Desempenho (`index_history`),
//! e a renda da estimativa da tela Renda (`crate::income`) — a mesma conta,
//! não uma segunda.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use time::{Date, Duration, OffsetDateTime};

use crate::domain::income::estimate::UnitPayment;
use crate::domain::objectives::{cdi_annual, MIN_RETURN_WINDOW_DAYS};
use crate::domain::performance::{daily_rate_accrued, monthly_rate_accrued};
use crate::domain::projection::income::{AssetInput, Group, Shape};
use crate::domain::projection::rates::{
    rate_path, Focus, PathInputs, PathSource, RatePath, INFLATION_TARGET,
};
use crate::domain::projection::{fraction, to_decimal};
use crate::history::{IndexCode, Series};
use crate::income::PortfolioIncome;
use crate::service::PortfolioView;
use crate::store::{Store, StoreError};

/// Quanto da série de IPCA a projeção lê: dois anos e pouco cobrem a janela de
/// 12 meses do Desempenho com o atraso de publicação.
const IPCA_LOOKBACK_DAYS: i64 = 800;
/// E do CDI: o ano que passou e uma folga.
const CDI_LOOKBACK_DAYS: i64 = 400;
/// O IPCA de 12 meses só vale se o último mês publicado é recente.
const IPCA_FRESH_DAYS: i64 = 120;

/// De onde veio a inflação usada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InflationSource {
    /// A pessoa informou.
    Informed,
    /// A média do IPCA anual do Focus para os próximos anos.
    Focus,
    /// Sem Focus: o IPCA dos últimos 12 meses publicados.
    Ipca12m,
    /// Sem Focus nem IPCA no banco: a meta de inflação (3% ao ano).
    Target,
}

/// O cenário de juro e inflação de hoje, como está no banco.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outlook {
    pub focus: Focus,
    /// A última busca do Focus que deu certo.
    pub fetched_at: Option<OffsetDateTime>,
    /// O CDI de hoje ao ano (a taxa diária mais recente).
    pub cdi_now: Option<f64>,
    /// O CDI dos 12 meses que passaram, ao ano.
    pub cdi_trailing: Option<f64>,
    /// O IPCA acumulado dos 12 últimos meses publicados.
    pub ipca_12m: Option<f64>,
    /// A série mensal do IPCA (em % no mês), para descontar a rentabilidade
    /// medida no Desempenho.
    pub ipca_series: Series,
    /// A série diária do CDI (em % ao dia).
    pub cdi_series: Series,
}

/// A inflação padrão e de onde veio; no Focus, os anos da média.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultInflation {
    pub percent: Decimal,
    pub source: InflationSource,
    pub years: Option<(i32, i32)>,
}

impl Outlook {
    /// Lê tudo do banco.
    pub async fn load(store: &Store, today: Date) -> Result<Self, StoreError> {
        let focus = Focus::new(store.focus_expectations().await?);
        let fetched_at = store.focus_fetched_at().await?;
        let cdi_series = store
            .index_series(
                IndexCode::Cdi,
                today - Duration::days(CDI_LOOKBACK_DAYS),
                today,
            )
            .await?;
        let ipca_series = store
            .index_series(
                IndexCode::Ipca,
                today - Duration::days(IPCA_LOOKBACK_DAYS),
                today,
            )
            .await?;
        Ok(Self::from_series(
            focus,
            fetched_at,
            cdi_series,
            ipca_series,
            today,
        ))
    }

    /// O mesmo, a partir das séries já lidas (o teste monta sem banco).
    pub fn from_series(
        focus: Focus,
        fetched_at: Option<OffsetDateTime>,
        cdi_series: Series,
        ipca_series: Series,
        today: Date,
    ) -> Self {
        let cdi_now = cdi_series
            .last()
            .and_then(|(_, rate)| cdi_annual(*rate))
            .and_then(|r| r.to_f64());
        let cdi_trailing = {
            let from = today - Duration::days(365);
            let accrued = daily_rate_accrued(&cdi_series, from, today);
            accrued.as_of.and_then(|as_of| {
                let days = (as_of - from).whole_days();
                (days >= MIN_RETURN_WINDOW_DAYS)
                    .then(|| {
                        (1.0 + accrued.ret.to_f64().unwrap_or(0.0)).powf(365.0 / days as f64) - 1.0
                    })
                    .filter(|r| r.is_finite())
            })
        };
        let ipca_12m = {
            let recent = ipca_series
                .last()
                .is_some_and(|(date, _)| (today - *date).whole_days() <= IPCA_FRESH_DAYS);
            let last: Vec<f64> = ipca_series
                .iter()
                .rev()
                .take(12)
                .map(|(_, v)| v.to_f64().unwrap_or(0.0) / 100.0)
                .collect();
            (recent && last.len() == 12)
                .then(|| last.iter().map(|v| 1.0 + v).product::<f64>() - 1.0)
        };
        Self {
            focus,
            fetched_at,
            cdi_now,
            cdi_trailing,
            ipca_12m,
            ipca_series,
            cdi_series,
        }
    }

    /// A inflação de quem não informou a sua: a de longo prazo do Focus, senão
    /// o IPCA de 12 meses, senão a meta de inflação.
    pub fn default_inflation(&self, today: Date) -> DefaultInflation {
        if let Some((rate, first, last)) = self.focus.long_run_ipca(today) {
            return DefaultInflation {
                percent: to_decimal(rate * 100.0, 2),
                source: InflationSource::Focus,
                years: Some((first, last)),
            };
        }
        match self.ipca_12m {
            Some(rate) => DefaultInflation {
                percent: to_decimal(rate * 100.0, 2),
                source: InflationSource::Ipca12m,
                years: None,
            },
            None => DefaultInflation {
                percent: INFLATION_TARGET,
                source: InflationSource::Target,
                years: None,
            },
        }
    }

    /// A inflação medida de `from` a `to` (o IPCA publicado na janela), ao
    /// ano. `None` se o banco cobre menos de seis meses dela.
    pub fn ipca_between(&self, from: Date, to: Date) -> Option<f64> {
        let accrued = monthly_rate_accrued(&self.ipca_series, from, to);
        let days = (accrued.as_of? - from).whole_days();
        if days < MIN_RETURN_WINDOW_DAYS {
            return None;
        }
        let rate = (1.0 + accrued.ret.to_f64()?).powf(365.0 / days as f64) - 1.0;
        rate.is_finite().then_some(rate)
    }

    /// O caminho de `horizon` meses, com o que a pessoa trocou (taxas ao ano,
    /// em %).
    pub fn path(
        &self,
        today: Date,
        horizon: u32,
        cdi_informed: Option<Decimal>,
        ipca_informed: Option<Decimal>,
    ) -> RatePath {
        let fallback = self.default_inflation(today);
        let fallback_source = match fallback.source {
            InflationSource::Ipca12m => PathSource::Measured,
            _ => PathSource::Fallback,
        };
        rate_path(
            &self.focus,
            today,
            horizon,
            PathInputs {
                cdi_now: self.cdi_now,
                ipca_fallback: fraction(fallback.percent),
                ipca_fallback_source: Some(fallback_source),
                cdi_informed: cdi_informed.map(fraction),
                ipca_informed: ipca_informed.map(fraction),
            },
        )
    }
}

/// De onde sai a renda de partida de um ativo na projeção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelBasis {
    /// Os 12 meses da estimativa.
    #[serde(rename = "trailing_12m")]
    Trailing12m,
    /// A média do habitual (até 5 anos).
    Usual,
}

/// Um ativo que paga provento, pronto para a projeção.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedAsset {
    pub input: AssetInput,
    pub symbol: Option<String>,
    pub display_name: String,
    pub basis: LevelBasis,
    /// Quantos anos a média do habitual tem (só em `Usual`).
    pub usual_years: Option<u32>,
}

/// O calendário de um ativo: o líquido de cada pagamento no mês em que cai (a
/// data de pagamento da B3; sem ela, a data ex).
pub fn asset_shape(payments: &[UnitPayment]) -> Shape {
    Shape::from_payments(payments.iter().map(|p| {
        (
            p.pay_date.unwrap_or(p.ex_date),
            p.net.to_f64().unwrap_or(0.0),
        )
    }))
}

/// O calendário da carteira: o de cada ativo, com o peso da renda mensal dele
/// na estimativa.
pub fn portfolio_shape(income: &PortfolioIncome) -> Shape {
    Shape::combine(income.assets.values().map(|a| {
        (
            asset_shape(&a.payments),
            a.estimate.monthly.net.to_f64().unwrap_or(0.0),
        )
    }))
}

/// Os ativos da carteira que têm estimativa, na ordem da carteira, com o
/// grupo (pela classe e pelo tipo de FII), a renda de partida e o calendário.
pub fn projected_assets(view: &PortfolioView, income: &PortfolioIncome) -> Vec<ProjectedAsset> {
    view.assets
        .iter()
        .filter_map(|a| {
            let found = income.asset(a.asset.id)?;
            let group = Group::of(
                a.asset.asset_class,
                a.sector.as_ref().map(|s| s.sector.as_str()),
            )?;
            let usual = found.estimate.usual.filter(|_| group.prefers_usual());
            let (monthly, basis, usual_years) = match usual {
                Some(u) => (u.monthly.net, LevelBasis::Usual, Some(u.years)),
                None => (found.estimate.monthly.net, LevelBasis::Trailing12m, None),
            };
            Some(ProjectedAsset {
                input: AssetInput {
                    asset_id: a.asset.id,
                    group,
                    monthly,
                    shape: asset_shape(&found.payments),
                },
                symbol: a.asset.symbol.clone(),
                display_name: a.asset.display_name.clone(),
                basis,
                usual_years,
            })
        })
        .collect()
}

/// A renda mensal estimada (líquida) dos ativos de uma classe, ou da carteira
/// inteira (`None`).
pub fn scope_income(
    view: &PortfolioView,
    income: &PortfolioIncome,
    class: Option<crate::domain::asset_class::AssetClass>,
) -> Decimal {
    match class {
        None => income.monthly_total,
        Some(class) => view
            .assets
            .iter()
            .filter(|a| a.asset.asset_class == class)
            .filter_map(|a| income.asset(a.asset.id))
            .map(|a| a.estimate.monthly.net)
            .sum(),
    }
}

/// O rendimento líquido ao ano (em %) de uma renda mensal sobre um valor.
/// `None` sem valor.
pub fn net_yield_percent(monthly: Decimal, value: Decimal) -> Option<Decimal> {
    (value > Decimal::ZERO)
        .then(|| (monthly * Decimal::from(12) / value * Decimal::ONE_HUNDRED).round_dp(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    const TODAY: Date = date!(2026 - 11 - 27);

    fn ipca_months(count: i64, percent: Decimal) -> Series {
        (0..count)
            .map(|i| {
                let month = crate::domain::income::YearMonth::of(TODAY)
                    .shift(-(i32::try_from(i).unwrap() + 2));
                (
                    crate::domain::projection::rates::first_day(month).unwrap(),
                    percent,
                )
            })
            .rev()
            .collect()
    }

    #[test]
    fn twelve_months_of_ipca_are_needed() {
        let o = Outlook::from_series(
            Focus::default(),
            None,
            Vec::new(),
            ipca_months(12, dec!(0.5)),
            TODAY,
        );
        let expected = 1.005f64.powi(12) - 1.0;
        assert!((o.ipca_12m.unwrap() - expected).abs() < 1e-12);
        let inflation = o.default_inflation(TODAY);
        assert_eq!(inflation.source, InflationSource::Ipca12m);
        assert_eq!(inflation.percent, to_decimal(expected * 100.0, 2));

        let short = Outlook::from_series(
            Focus::default(),
            None,
            Vec::new(),
            ipca_months(11, dec!(0.5)),
            TODAY,
        );
        assert_eq!(short.ipca_12m, None);
        let target = short.default_inflation(TODAY);
        assert_eq!(target.source, InflationSource::Target);
        assert_eq!(target.percent, dec!(3));
    }

    #[test]
    fn cdi_now_and_over_the_year() {
        let cdi: Series = (0..300)
            .map(|i| (TODAY - Duration::days(300 - i), dec!(0.05)))
            .collect();
        let o = Outlook::from_series(Focus::default(), None, cdi, Vec::new(), TODAY);
        let now = o.cdi_now.unwrap();
        assert!((now - (1.0005f64.powi(252) - 1.0)).abs() < 1e-6);
        // 300 dias de taxa (corridos, para o teste), anualizados por 365/299.
        assert!(o.cdi_trailing.unwrap() > 0.0);
        let none = Outlook::from_series(Focus::default(), None, Vec::new(), Vec::new(), TODAY);
        assert_eq!((none.cdi_now, none.cdi_trailing), (None, None));
    }

    #[test]
    fn yield_needs_a_value() {
        assert_eq!(net_yield_percent(dec!(100), dec!(12000)), Some(dec!(10)));
        assert_eq!(net_yield_percent(dec!(100), dec!(0)), None);
    }

    #[test]
    fn shapes_use_the_pay_date_when_there_is_one() {
        let payments = [
            UnitPayment {
                ex_date: date!(2026 - 03 - 02),
                pay_date: Some(date!(2026 - 04 - 15)),
                gross: dec!(1),
                net: dec!(1),
            },
            UnitPayment {
                ex_date: date!(2026 - 09 - 01),
                pay_date: None,
                gross: dec!(1),
                net: dec!(1),
            },
        ];
        let shape = asset_shape(&payments);
        assert!((shape.weight(4) - 6.0).abs() < 1e-12);
        assert!((shape.weight(9) - 6.0).abs() < 1e-12);
        assert_eq!(shape.weight(3), 0.0);
    }
}
