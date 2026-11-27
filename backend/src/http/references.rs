//! O teto de preço e as referências de preço na API: o teto que a pessoa
//! escolheu, com o valor de hoje (`ceiling` no ativo), o pedido que muda a
//! escolha, e as referências de ações (Bazin, Graham, Pelas duas) e de FIIs
//! (VP, Rendimento, Pelas duas) da Análise e da escolha do teto
//! (`GET /assets/{id}/references`). Contrato em `docs/api.md`.

use std::collections::HashMap;

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::{decimal_out, optional_decimal};
use super::fundamentals::FundIndicatorsDto;
use super::{ApiPath, AppState, CurrentUser};
use crate::ceiling::{self, ReferenceContext, ReferenceItem, ReferencesReport};
use crate::domain::analysis::fund_references::{
    brick_target, net_cdi, paper_target, FundReferences, TargetYield, DEFAULT_BRICK_PREMIUM,
    DEFAULT_PAPER_PREMIUM,
};
use crate::domain::analysis::references::{PriceReferences, ReferenceGap};
use crate::domain::asset_class::{AssetClass, Market, UnitKind};
use crate::domain::ceiling::{
    check_method, AssetReferences, Ceiling, CeilingChoice, CeilingMethod, CeilingSource,
};
use crate::domain::income::today_in_brazil;
use crate::domain::market_rates::{Cdi12m, RealRate};
use crate::domain::sector::{FundComposition, FundType};
use crate::error::{ApiError, ApiResult};
use crate::fund_indicators::{self, FundIndicators};
use crate::service;

/// `aaaa-mm-dd`.
pub(crate) fn day(date: Date) -> String {
    date.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

/// Como o teto aparece na API: o método (`manual` no valor próprio), o valor
/// de hoje e, quando o método está sem valor, o motivo.
#[derive(Debug, Serialize, PartialEq)]
pub struct CeilingDto {
    pub method: &'static str,
    #[serde(with = "decimal_out::option")]
    pub value: Option<Decimal>,
    pub unavailable_reason: Option<ReferenceGap>,
}

/// O nome de uma origem de teto no contrato.
pub fn source_name(source: CeilingSource) -> &'static str {
    match source {
        CeilingSource::Manual => "manual",
        CeilingSource::Method(method) => method.as_str(),
    }
}

impl CeilingDto {
    /// `None` = sem teto.
    pub fn of(ceiling: Ceiling) -> Option<Self> {
        match ceiling {
            Ceiling::None => None,
            Ceiling::Price { source, value } => Some(Self {
                method: source_name(source),
                value: Some(value),
                unavailable_reason: None,
            }),
            Ceiling::Unavailable { method, reason } => Some(Self {
                method: method.as_str(),
                value: None,
                unavailable_reason: Some(reason),
            }),
        }
    }
}

/// A escolha do teto no pedido: `{"method": "graham"}` ou
/// `{"method": "manual", "value": 30.5}`. `null` (no campo `ceiling`) é "sem
/// teto".
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CeilingRequest {
    pub method: String,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub value: Option<Decimal>,
}

/// Teto de valores aceitos (10^15), como o resto do dinheiro da API.
fn max_money() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

const NO_CEILING_BY_VALUE: &str =
    "Renda fixa por valor nao tem preco por unidade; o teto de preco nao se aplica.";

/// A escolha que o pedido descreve, conferida contra a classe e o mercado do
/// ativo. Método de outra família (Graham em FII, VP em ação) ou de classe sem
/// referência é recusado, como o valor próprio não positivo.
pub fn ceiling_choice(
    request: Option<&CeilingRequest>,
    class: AssetClass,
    market: Market,
) -> ApiResult<CeilingChoice> {
    let Some(request) = request else {
        return Ok(CeilingChoice::None);
    };
    if market.unit_kind() == UnitKind::Value {
        return Err(ApiError::validation(NO_CEILING_BY_VALUE));
    }
    if request.method == "manual" {
        let value = request
            .value
            .ok_or_else(|| ApiError::validation("O teto de valor proprio precisa de value."))?;
        if value <= Decimal::ZERO || value > max_money() {
            return Err(ApiError::validation(
                "O teto de preco precisa ser maior que zero.",
            ));
        }
        return Ok(CeilingChoice::Manual(value));
    }
    let method = CeilingMethod::parse(&request.method).ok_or_else(|| {
        ApiError::validation(
            "Metodo de teto desconhecido: use bazin, graham, combined, nav, fund_yield ou manual.",
        )
    })?;
    if request.value.is_some() {
        return Err(ApiError::validation(
            "O teto por metodo nao leva value: o valor sai da referencia de hoje.",
        ));
    }
    check_method(method, class, market).map_err(ApiError::validation)?;
    Ok(CeilingChoice::Method(method))
}

/// Uma referência: o valor ou o motivo de não haver.
#[derive(Debug, Serialize, PartialEq)]
pub struct RefValueDto {
    #[serde(with = "decimal_out::option")]
    pub value: Option<Decimal>,
    pub gap: Option<ReferenceGap>,
}

impl RefValueDto {
    fn of(result: Result<Decimal, ReferenceGap>) -> Self {
        Self {
            value: result.ok(),
            gap: result.err(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct BazinDto {
    #[serde(with = "decimal_out::option")]
    pub value: Option<Decimal>,
    pub gap: Option<ReferenceGap>,
    /// Dividendo anual médio por cota, em reais.
    #[serde(with = "decimal_out::option")]
    pub annual_average: Option<Decimal>,
    /// Quantos anos de 12 meses entraram na média.
    pub years: Option<u32>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TargetDto {
    /// O rendimento-alvo, em % ao ano.
    #[serde(with = "decimal_out")]
    pub percent: Decimal,
    /// O alvo do tijolo e o do papel, quando entram.
    #[serde(with = "decimal_out::option")]
    pub brick: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub paper: Option<Decimal>,
    #[serde(with = "decimal_out")]
    pub brick_weight: Decimal,
    #[serde(with = "decimal_out")]
    pub paper_weight: Decimal,
}

impl From<TargetYield> for TargetDto {
    fn from(t: TargetYield) -> Self {
        Self {
            percent: t.percent,
            brick: t.brick,
            paper: t.paper,
            brick_weight: t.weights.brick.normalize(),
            paper_weight: t.weights.paper.normalize(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FundYieldDto {
    #[serde(with = "decimal_out::option")]
    pub value: Option<Decimal>,
    pub gap: Option<ReferenceGap>,
    /// Proventos por cota dos últimos 12 meses, em reais.
    #[serde(with = "decimal_out::option")]
    pub dividends_12m: Option<Decimal>,
    pub target: Option<TargetDto>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CompositionDto {
    #[serde(with = "decimal_out")]
    pub brick: Decimal,
    #[serde(with = "decimal_out")]
    pub paper: Decimal,
    #[serde(with = "decimal_out")]
    pub funds: Decimal,
}

impl From<FundComposition> for CompositionDto {
    fn from(c: FundComposition) -> Self {
        Self {
            brick: c.brick.normalize(),
            paper: c.paper.normalize(),
            funds: c.funds.normalize(),
        }
    }
}

/// Uma ação: Bazin, Graham e Pelas duas.
#[derive(Debug, Serialize, PartialEq)]
pub struct StockReferenceDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    #[serde(with = "decimal_out::option")]
    pub unit_price: Option<Decimal>,
    /// O teto que a pessoa escolheu, com o valor de hoje; `null` = sem teto.
    pub ceiling: Option<CeilingDto>,
    pub bazin: BazinDto,
    pub graham: RefValueDto,
    pub combined: RefValueDto,
}

/// Um FII: o tipo, VP, Rendimento e Pelas duas.
#[derive(Debug, Serialize, PartialEq)]
pub struct FundReferenceDto {
    pub asset_id: Uuid,
    pub label: String,
    pub asset_class: AssetClass,
    #[serde(with = "decimal_out::option")]
    pub unit_price: Option<Decimal>,
    pub ceiling: Option<CeilingDto>,
    pub fund_type: Option<FundType>,
    pub composition: Option<CompositionDto>,
    pub nav: RefValueDto,
    pub fund_yield: FundYieldDto,
    pub combined: RefValueDto,
    /// O caixa, a liquidez, os CAGRs e o rendimento de 12 meses (só na Análise
    /// e na folha do ativo; `null` onde não foram lidos).
    pub indicators: Option<FundIndicatorsDto>,
}

fn stock_dto(item: &ReferenceItem, refs: &PriceReferences) -> StockReferenceDto {
    let bazin = refs.bazin.ok();
    StockReferenceDto {
        asset_id: item.asset_id,
        label: item.label.clone(),
        asset_class: item.asset_class,
        unit_price: item.unit_price,
        ceiling: CeilingDto::of(item.ceiling),
        bazin: BazinDto {
            value: bazin.map(|b| b.price),
            gap: refs.bazin.err(),
            annual_average: bazin.map(|b| b.annual_average),
            years: bazin.map(|b| b.years),
        },
        graham: RefValueDto::of(refs.graham),
        combined: RefValueDto::of(refs.combined()),
    }
}

fn fund_dto(
    item: &ReferenceItem,
    refs: &FundReferences,
    indicators: Option<&FundIndicators>,
) -> FundReferenceDto {
    let found = refs.fund_yield.ok();
    FundReferenceDto {
        asset_id: item.asset_id,
        label: item.label.clone(),
        asset_class: item.asset_class,
        unit_price: item.unit_price,
        ceiling: CeilingDto::of(item.ceiling),
        fund_type: refs.fund_type,
        composition: refs.composition.map(CompositionDto::from),
        nav: RefValueDto::of(refs.nav),
        fund_yield: FundYieldDto {
            value: found.map(|y| y.price),
            gap: refs.fund_yield.err(),
            dividends_12m: found.map(|y| y.dividends_12m),
            target: found.map(|y| TargetDto::from(y.target)),
        },
        combined: RefValueDto::of(refs.combined()),
        indicators: indicators.map(FundIndicatorsDto::from),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RealRateDto {
    /// % ao ano acima do IPCA (`Taxa Compra Manha`).
    #[serde(with = "decimal_out")]
    pub percent: Decimal,
    pub base_date: String,
    /// O nome do título (`Tesouro IPCA+ com Juros Semestrais 2037`).
    pub bond: String,
    pub bond_symbol: String,
    pub source: &'static str,
}

impl From<RealRate> for RealRateDto {
    fn from(r: RealRate) -> Self {
        Self {
            percent: r.percent,
            base_date: day(r.base_date),
            bond: r.bond.name(),
            bond_symbol: r.bond.symbol(),
            source: "tesouro_transparente",
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CdiDto {
    /// O CDI acumulado nos 12 meses, em %.
    #[serde(with = "decimal_out")]
    pub percent: Decimal,
    /// O CDI de 12 meses depois do IR de 15%, em %.
    #[serde(with = "decimal_out")]
    pub net_percent: Decimal,
    pub from: String,
    /// A data da última taxa publicada que entrou.
    pub to: String,
    pub source: &'static str,
}

impl From<Cdi12m> for CdiDto {
    fn from(c: Cdi12m) -> Self {
        Self {
            percent: c.percent,
            net_percent: net_cdi(c.percent),
            from: day(c.from),
            to: day(c.to),
            source: "bcb_sgs_12",
        }
    }
}

/// As taxas de mercado e os alvos que saem delas com os prêmios da pessoa.
#[derive(Debug, Serialize, PartialEq)]
pub struct MarketDto {
    pub real_rate: Option<RealRateDto>,
    pub cdi_12m: Option<CdiDto>,
    #[serde(with = "decimal_out")]
    pub brick_premium: Decimal,
    #[serde(with = "decimal_out")]
    pub paper_premium: Decimal,
    #[serde(with = "decimal_out")]
    pub default_brick_premium: Decimal,
    #[serde(with = "decimal_out")]
    pub default_paper_premium: Decimal,
    /// O alvo do tijolo hoje (juro real + prêmio) e o do papel (CDI líquido +
    /// prêmio), em %; `null` sem a taxa.
    #[serde(with = "decimal_out::option")]
    pub brick_target: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub paper_target: Option<Decimal>,
}

impl From<&ReferenceContext> for MarketDto {
    fn from(ctx: &ReferenceContext) -> Self {
        Self {
            real_rate: ctx.rates.real_rate.map(RealRateDto::from),
            cdi_12m: ctx.rates.cdi_12m.map(CdiDto::from),
            brick_premium: ctx.premiums.brick,
            paper_premium: ctx.premiums.paper,
            default_brick_premium: DEFAULT_BRICK_PREMIUM.normalize(),
            default_paper_premium: DEFAULT_PAPER_PREMIUM.normalize(),
            brick_target: brick_target(&ctx.rates, ctx.premiums),
            paper_target: paper_target(&ctx.rates, ctx.premiums),
        }
    }
}

/// As referências de preço: as ações, os FIIs e o que entra nas contas.
#[derive(Debug, Serialize, PartialEq)]
pub struct ReferencesDto {
    /// O rendimento-alvo do Bazin, em % ao ano.
    #[serde(with = "decimal_out")]
    pub bazin_yield: Decimal,
    pub market: MarketDto,
    pub stocks: Vec<StockReferenceDto>,
    pub funds: Vec<FundReferenceDto>,
}

impl From<&ReferencesReport> for ReferencesDto {
    fn from(report: &ReferencesReport) -> Self {
        Self::with_indicators(report, &HashMap::new())
    }
}

impl ReferencesDto {
    /// As referências com os indicadores de cada FII (por id do ativo).
    pub fn with_indicators(
        report: &ReferencesReport,
        indicators: &HashMap<Uuid, FundIndicators>,
    ) -> Self {
        let mut stocks = Vec::new();
        let mut funds = Vec::new();
        for item in report.stocks.iter().chain(&report.funds) {
            match &item.references {
                AssetReferences::Stock(refs) => stocks.push(stock_dto(item, refs)),
                AssetReferences::Fund(refs) => {
                    funds.push(fund_dto(item, refs, indicators.get(&item.asset_id)));
                }
            }
        }
        Self {
            bazin_yield: report.context.bazin_yield,
            market: MarketDto::from(&report.context),
            stocks,
            funds,
        }
    }
}

/// `GET /assets/{id}/references`: as referências e o teto de um ativo, para a
/// escolha do teto na folha do ativo. Ativo sem referências (exterior, cripto,
/// renda fixa) volta com as duas listas vazias.
pub async fn asset_references(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<Json<ReferencesDto>> {
    let view = service::portfolio(&state.store, user.id).await?;
    let asset = view.asset(id).ok_or_else(ApiError::not_found)?;
    let today = today_in_brazil(OffsetDateTime::now_utc());
    let report = ceiling::report(&state.store, user.id, &[asset], today, None).await?;
    let fundamentals = state.store.fundamentals_for_assets(&[&asset.asset]).await?;
    let indicators =
        fund_indicators::for_references(&state.store, &report, &[asset], &fundamentals, today)
            .await?;
    Ok(Json(ReferencesDto::with_indicators(&report, &indicators)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::analysis::fund_references::{fund_references, MarketRates, Premiums};
    use crate::domain::analysis::references::Bazin;
    use crate::domain::treasury::{Bond, BondKind};
    use rust_decimal_macros::dec;
    use serde_json::json;
    use time::macros::date;

    fn request(text: &str) -> CeilingRequest {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn the_ceiling_serializes_the_method_the_value_and_the_reason() {
        assert_eq!(CeilingDto::of(Ceiling::None), None);
        assert_eq!(
            serde_json::to_value(CeilingDto::of(Ceiling::Price {
                source: CeilingSource::Manual,
                value: dec!(30.5)
            }))
            .unwrap(),
            json!({"method": "manual", "value": 30.5, "unavailable_reason": null})
        );
        assert_eq!(
            serde_json::to_value(CeilingDto::of(Ceiling::Unavailable {
                method: CeilingMethod::Graham,
                reason: ReferenceGap::Loss
            }))
            .unwrap(),
            json!({"method": "graham", "value": null, "unavailable_reason": "loss"})
        );
    }

    #[test]
    fn the_choice_is_checked_against_the_class() {
        let stock = (AssetClass::BrStocks, Market::B3);
        let fund = (AssetClass::BrReits, Market::B3);
        let ok = |r: &str, (class, market): (AssetClass, Market)| {
            ceiling_choice(Some(&request(r)), class, market)
        };
        assert_eq!(
            ceiling_choice(None, stock.0, stock.1),
            Ok(CeilingChoice::None)
        );
        assert_eq!(
            ok(r#"{"method": "graham"}"#, stock),
            Ok(CeilingChoice::Method(CeilingMethod::Graham))
        );
        assert_eq!(
            ok(r#"{"method": "fund_yield"}"#, fund),
            Ok(CeilingChoice::Method(CeilingMethod::FundYield))
        );
        assert_eq!(
            ok(r#"{"method": "manual", "value": 30.5}"#, fund),
            Ok(CeilingChoice::Manual(dec!(30.5)))
        );
        for (body, target) in [
            (r#"{"method": "graham"}"#, fund),
            (r#"{"method": "nav"}"#, stock),
            (r#"{"method": "manual"}"#, stock),
            (r#"{"method": "manual", "value": 0}"#, stock),
            (r#"{"method": "manual", "value": -1}"#, stock),
            (r#"{"method": "bazin", "value": 10}"#, stock),
            (r#"{"method": "preco_justo"}"#, stock),
            (
                r#"{"method": "bazin"}"#,
                (AssetClass::Crypto, Market::Crypto),
            ),
            (
                r#"{"method": "bazin"}"#,
                (AssetClass::IntlStocks, Market::Us),
            ),
            (
                r#"{"method": "manual", "value": 1}"#,
                (AssetClass::FixedIncome, Market::Manual),
            ),
        ] {
            let error = ok(body, target).unwrap_err();
            assert_eq!(error.code, "validation_failed", "{body}");
            assert!(error.detail.is_ascii(), "{}", error.detail);
        }
        // Valor próprio vale em cripto e no exterior.
        assert!(ok(
            r#"{"method": "manual", "value": 9}"#,
            (AssetClass::Crypto, Market::Crypto)
        )
        .is_ok());
        assert!(serde_json::from_str::<CeilingRequest>(r#"{"method": "nav", "x": 1}"#).is_err());
    }

    #[test]
    fn the_references_carry_values_gaps_and_the_math() {
        let ctx = ReferenceContext {
            bazin_yield: dec!(6),
            premiums: Premiums::default(),
            rates: MarketRates {
                real_rate: Some(RealRate {
                    bond: Bond::new(BondKind::IpcaPlusWithCoupons, date!(2037 - 05 - 15)),
                    percent: dec!(7.12),
                    base_date: date!(2026 - 11 - 19),
                }),
                cdi_12m: None,
            },
        };
        let stock = ReferenceItem {
            asset_id: Uuid::nil(),
            label: "FICT3".into(),
            asset_class: AssetClass::BrStocks,
            unit_price: Some(dec!(12)),
            ceiling: Ceiling::Unavailable {
                method: CeilingMethod::Graham,
                reason: ReferenceGap::Loss,
            },
            references: AssetReferences::Stock(PriceReferences {
                bazin: Ok(Bazin {
                    annual_average: dec!(0.6),
                    years: 1,
                    price: dec!(10),
                }),
                graham: Err(ReferenceGap::Loss),
            }),
        };
        let fund = ReferenceItem {
            asset_id: Uuid::max(),
            label: "FICT11".into(),
            asset_class: AssetClass::BrReits,
            unit_price: Some(dec!(130)),
            ceiling: Ceiling::None,
            references: AssetReferences::Fund(fund_references(
                None,
                Some(dec!(13.20)),
                Some(FundType::Brick),
                None,
                &ctx.rates,
                ctx.premiums,
            )),
        };
        let dto = ReferencesDto::from(&ReferencesReport {
            context: ctx,
            stocks: vec![stock],
            funds: vec![fund],
        });
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["bazin_yield"], 6.0);
        assert_eq!(json["market"]["real_rate"]["percent"], 7.12);
        assert_eq!(json["market"]["real_rate"]["base_date"], "2026-11-19");
        assert_eq!(
            json["market"]["real_rate"]["bond"],
            "Tesouro IPCA+ com Juros Semestrais 2037"
        );
        assert_eq!(json["market"]["brick_target"], 9.62);
        assert!(json["market"]["cdi_12m"].is_null());
        assert!(json["market"]["paper_target"].is_null());
        assert_eq!(json["market"]["default_brick_premium"], 2.5);
        let s = &json["stocks"][0];
        assert_eq!(
            s["bazin"],
            json!({"value": 10.0, "gap": null, "annual_average": 0.6, "years": 1})
        );
        assert_eq!(s["graham"], json!({"value": null, "gap": "loss"}));
        assert_eq!(s["combined"], json!({"value": 10.0, "gap": null}));
        assert_eq!(s["ceiling"]["unavailable_reason"], "loss");
        let f = &json["funds"][0];
        assert_eq!(f["fund_type"], "brick");
        assert_eq!(f["nav"], json!({"value": null, "gap": "no_fundamentals"}));
        assert_eq!(f["fund_yield"]["value"], 137.21);
        assert_eq!(f["fund_yield"]["dividends_12m"], 13.2);
        assert_eq!(f["fund_yield"]["target"]["percent"], 9.62);
        assert_eq!(f["fund_yield"]["target"]["brick_weight"], 1.0);
        assert_eq!(f["combined"]["value"], 137.21);
        assert!(f["ceiling"].is_null());
    }
}
