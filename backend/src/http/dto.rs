//! O JSON da API: o que entra e o que sai.
//!
//! Dinheiro sai como número JSON (o navegador soma e formata) e entra como
//! número convertido pela representação decimal — `0.1` vira `0.1` exato, não
//! o `0.1000000000000000055…` do ponto flutuante.

use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::income::PlanIncomeDto;
use crate::domain::allocation::{ClassOutcome, Notice, Plan};
use crate::domain::asset_class::{AssetClass, Checklist};
use crate::domain::scoring::ScoreSource;
use crate::service::{AssetView, ClassView, PortfolioView};
use crate::store::{Contribution, ContributionItem, Question};

pub(crate) mod decimal_out {
    use rust_decimal::prelude::ToPrimitive;
    use rust_decimal::Decimal;
    use serde::Serializer;

    pub fn serialize<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(value.to_f64().unwrap_or(0.0))
    }

    pub mod option {
        use super::*;

        pub fn serialize<S: Serializer>(
            value: &Option<Decimal>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match value {
                Some(v) => super::serialize(v, serializer),
                None => serializer.serialize_none(),
            }
        }
    }
}

/// Número JSON (ou texto numérico) → `Decimal` exato.
fn parse_number(value: serde_json::Value) -> Result<Decimal, String> {
    let text = match value {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s,
        other => return Err(format!("esperado numero, veio {other}")),
    };
    Decimal::from_str(text.trim())
        .or_else(|_| Decimal::from_scientific(text.trim()))
        .map_err(|_| format!("numero invalido: {text}"))
}

pub fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
    parse_number(serde_json::Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

pub fn optional_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Decimal>, D::Error> {
    match Option::<serde_json::Value>::deserialize(deserializer)? {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => parse_number(value)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

/// Três estados: ausente (`None`), `null` (`Some(None)`) e valor (`Some(Some)`).
pub fn patch_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<Decimal>>, D::Error> {
    optional_decimal(deserializer).map(Some)
}

pub fn patch_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(deserializer).map(Some)
}

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalsRequest {
    pub targets: HashMap<AssetClass, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAssetRequest {
    pub asset_class: AssetClass,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub quantity: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub amount: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub manual_price: Option<Decimal>,
    #[serde(default)]
    pub score_override: Option<i64>,
    #[serde(default)]
    pub listed_on_b3: Option<bool>,
    /// Título do Tesouro Direto: `symbol` é a chave do título (`TD-IPCA-20350515`,
    /// a que `GET /symbols` devolve).
    #[serde(default)]
    pub treasury: Option<bool>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub max_price: Option<Decimal>,
    /// Preço médio de entrada, em reais (por unidade). Sem ele, o saldo
    /// inicial fica de custo desconhecido.
    #[serde(default, deserialize_with = "optional_decimal")]
    pub average_price: Option<Decimal>,
    /// Renda fixa por valor: quanto foi aplicado.
    #[serde(default, deserialize_with = "optional_decimal")]
    pub invested: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAssetRequest {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub quantity: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub amount: Option<Decimal>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub manual_price: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_i64")]
    pub score_override: Option<Option<i64>>,
    #[serde(default)]
    pub answers: Option<HashMap<Uuid, Option<bool>>>,
    #[serde(default)]
    pub listed_on_b3: Option<bool>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub max_price: Option<Option<Decimal>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionRequest {
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    #[serde(deserialize_with = "decimal")]
    pub amount: Decimal,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContributionItemRequest {
    pub asset_id: Uuid,
    #[serde(deserialize_with = "decimal")]
    pub quantity: Decimal,
    #[serde(deserialize_with = "decimal")]
    pub unit_price: Decimal,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContributionRequest {
    #[serde(deserialize_with = "decimal")]
    pub amount: Decimal,
    pub items: Vec<ContributionItemRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SymbolQuery {
    pub query: String,
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize)]
pub struct InfoResponse {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct QuestionDto {
    pub id: Uuid,
    pub position: i32,
    pub prompt: String,
}

impl From<&Question> for QuestionDto {
    fn from(q: &Question) -> Self {
        Self {
            id: q.id,
            position: q.position,
            prompt: q.prompt.clone(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ClassDto {
    pub asset_class: AssetClass,
    #[serde(with = "decimal_out")]
    pub target_percent: Decimal,
    #[serde(with = "decimal_out")]
    pub current_value: Decimal,
    #[serde(with = "decimal_out")]
    pub current_percent: Decimal,
    pub asset_count: usize,
    pub eligible: bool,
}

impl From<&ClassView> for ClassDto {
    fn from(c: &ClassView) -> Self {
        Self {
            asset_class: c.asset_class,
            target_percent: c.target_percent,
            current_value: c.current_value,
            current_percent: c.current_percent,
            asset_count: c.asset_count,
            eligible: c.eligible,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssetDto {
    pub id: Uuid,
    pub asset_class: AssetClass,
    pub symbol: Option<String>,
    pub display_name: String,
    pub listed_on_b3: bool,
    pub treasury: bool,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out::option")]
    pub unit_price: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub manual_price: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub max_price: Option<Decimal>,
    pub currency: String,
    pub price_source: &'static str,
    #[serde(with = "time::serde::rfc3339::option")]
    pub quote_updated_at: Option<OffsetDateTime>,
    #[serde(with = "decimal_out")]
    pub current_value: Decimal,
    #[serde(with = "decimal_out")]
    pub class_percent: Decimal,
    #[serde(with = "decimal_out")]
    pub ideal_class_percent: Decimal,
    pub score: u8,
    pub score_source: ScoreSource,
    pub score_override: Option<u8>,
    pub answers: BTreeMap<Uuid, bool>,
    /// Setor, subsetor e segmento da fonte pública (B3 nas ações, CVM nos
    /// FIIs); nulos onde não há fonte ou ela ainda não respondeu.
    pub sector: Option<String>,
    pub subsector: Option<String>,
    pub segment: Option<String>,
    /// Do livro (ver docs/api.md § Lançamentos). Nulo = desconhecido.
    #[serde(with = "decimal_out::option")]
    pub average_price: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub cost_basis: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub unrealized_gain: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub realized_gain: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub return_percent: Option<Decimal>,
}

impl From<&AssetView> for AssetDto {
    fn from(a: &AssetView) -> Self {
        Self {
            id: a.asset.id,
            asset_class: a.asset.asset_class,
            symbol: a.asset.symbol.clone(),
            display_name: a.asset.display_name.clone(),
            listed_on_b3: a.asset.listed_on_b3,
            treasury: a.asset.treasury,
            quantity: a.asset.quantity,
            unit_price: a.unit_price,
            manual_price: a.asset.manual_price,
            max_price: a.asset.max_price,
            currency: a.currency.clone(),
            price_source: a.price_source.as_str(),
            quote_updated_at: a.quote_updated_at,
            current_value: a.current_value,
            class_percent: a.class_percent,
            ideal_class_percent: a.ideal_class_percent,
            score: a.score,
            score_source: a.score_source,
            score_override: a.asset.score_override,
            answers: a.answers.iter().map(|(k, v)| (*k, *v)).collect(),
            sector: a.sector.as_ref().map(|s| s.sector.clone()),
            subsector: a.sector.as_ref().and_then(|s| s.subsector.clone()),
            segment: a.sector.as_ref().and_then(|s| s.segment.clone()),
            average_price: a.performance.average_price,
            cost_basis: a.performance.cost_basis,
            unrealized_gain: a.performance.unrealized_gain,
            realized_gain: a.performance.realized_gain,
            return_percent: a.performance.return_percent,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ChecklistsDto {
    pub stocks: Vec<QuestionDto>,
    pub real_estate: Vec<QuestionDto>,
}

impl ChecklistsDto {
    pub fn from_questions(questions: &[Question]) -> Self {
        let of = |c: Checklist| {
            questions
                .iter()
                .filter(|q| q.checklist == c)
                .map(QuestionDto::from)
                .collect()
        };
        Self {
            stocks: of(Checklist::Stocks),
            real_estate: of(Checklist::RealEstate),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PortfolioDto {
    #[serde(with = "decimal_out")]
    pub total_value: Decimal,
    #[serde(with = "time::serde::rfc3339::option")]
    pub quotes_updated_at: Option<OffsetDateTime>,
    pub classes: Vec<ClassDto>,
    pub assets: Vec<AssetDto>,
    pub checklists: ChecklistsDto,
}

impl From<&PortfolioView> for PortfolioDto {
    fn from(p: &PortfolioView) -> Self {
        Self {
            total_value: p.total_value,
            quotes_updated_at: p.quotes_updated_at,
            classes: p.classes.iter().map(ClassDto::from).collect(),
            assets: p.assets.iter().map(AssetDto::from).collect(),
            checklists: ChecklistsDto::from_questions(&p.questions),
        }
    }
}

/// O evento `prices` do stream da carteira: só o que muda com o preço. O
/// resto (metas, notas, checklists) a tela já tem do `GET /portfolio`.
#[derive(Debug, Serialize, PartialEq)]
pub struct LivePricesDto {
    #[serde(with = "decimal_out")]
    pub total_value: Decimal,
    #[serde(with = "time::serde::rfc3339::option")]
    pub quotes_updated_at: Option<OffsetDateTime>,
    pub classes: Vec<ClassDto>,
    pub assets: Vec<LiveAssetDto>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LiveAssetDto {
    pub id: Uuid,
    #[serde(with = "decimal_out::option")]
    pub unit_price: Option<Decimal>,
    pub currency: String,
    pub price_source: &'static str,
    #[serde(with = "time::serde::rfc3339::option")]
    pub quote_updated_at: Option<OffsetDateTime>,
    #[serde(with = "decimal_out")]
    pub current_value: Decimal,
    #[serde(with = "decimal_out")]
    pub class_percent: Decimal,
    #[serde(with = "decimal_out")]
    pub ideal_class_percent: Decimal,
    /// Mudam com o preço: sem eles, a linha ao vivo mostraria o ganho velho.
    #[serde(with = "decimal_out::option")]
    pub unrealized_gain: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    pub return_percent: Option<Decimal>,
}

impl From<&PortfolioView> for LivePricesDto {
    fn from(p: &PortfolioView) -> Self {
        Self {
            total_value: p.total_value,
            quotes_updated_at: p.quotes_updated_at,
            classes: p.classes.iter().map(ClassDto::from).collect(),
            assets: p
                .assets
                .iter()
                .map(|a| LiveAssetDto {
                    id: a.asset.id,
                    unit_price: a.unit_price,
                    currency: a.currency.clone(),
                    price_source: a.price_source.as_str(),
                    quote_updated_at: a.quote_updated_at,
                    current_value: a.current_value,
                    class_percent: a.class_percent,
                    ideal_class_percent: a.ideal_class_percent,
                    unrealized_gain: a.performance.unrealized_gain,
                    return_percent: a.performance.return_percent,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GoalsResponse {
    pub targets: BTreeMap<AssetClass, f64>,
}

#[derive(Debug, Serialize)]
pub struct PlanItemDto {
    pub asset_id: Uuid,
    pub asset_class: AssetClass,
    pub symbol: Option<String>,
    pub display_name: String,
    pub listed_on_b3: bool,
    pub treasury: bool,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out")]
    pub unit_price: Decimal,
    #[serde(with = "decimal_out")]
    pub value: Decimal,
    pub divisible: bool,
    #[serde(with = "decimal_out")]
    pub class_percent_after: Decimal,
    /// Renda mensal que o item soma, estimada (`null` sem histórico do ativo).
    #[serde(with = "decimal_out::option")]
    pub estimated_monthly_income: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct PlanClassDto {
    pub asset_class: AssetClass,
    #[serde(with = "decimal_out")]
    pub target_percent: Decimal,
    #[serde(with = "decimal_out")]
    pub percent_before: Decimal,
    #[serde(with = "decimal_out")]
    pub percent_after: Decimal,
    #[serde(with = "decimal_out")]
    pub allocated: Decimal,
    pub status: &'static str,
}

impl From<&ClassOutcome> for PlanClassDto {
    fn from(c: &ClassOutcome) -> Self {
        Self {
            asset_class: c.asset_class,
            target_percent: c.target_percent,
            percent_before: c.percent_before,
            percent_after: c.percent_after,
            allocated: c.allocated,
            status: c.status.as_str(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum NoticeDto {
    ClassBlocked {
        asset_class: AssetClass,
        #[serde(with = "decimal_out")]
        amount: Decimal,
        reason: &'static str,
    },
    AssetsWithoutPrice {
        symbols: Vec<String>,
    },
    AssetsWithoutScore {
        symbols: Vec<String>,
    },
    NoTargets,
    AssetAboveCeiling {
        asset_id: Uuid,
        symbol: String,
        #[serde(with = "decimal_out")]
        unit_price: Decimal,
        #[serde(with = "decimal_out")]
        max_price: Decimal,
    },
    ConcentrationLimit {
        #[serde(with = "decimal_out")]
        max_percent: Decimal,
        asset_ids: Vec<Uuid>,
        symbols: Vec<String>,
        #[serde(with = "decimal_out")]
        amount: Decimal,
    },
}

impl From<&Notice> for NoticeDto {
    fn from(n: &Notice) -> Self {
        match n {
            Notice::ClassBlocked {
                asset_class,
                amount,
                reason,
            } => NoticeDto::ClassBlocked {
                asset_class: *asset_class,
                amount: *amount,
                reason: reason.as_str(),
            },
            Notice::AssetsWithoutPrice { labels } => NoticeDto::AssetsWithoutPrice {
                symbols: labels.clone(),
            },
            Notice::AssetsWithoutScore { labels } => NoticeDto::AssetsWithoutScore {
                symbols: labels.clone(),
            },
            Notice::NoTargets => NoticeDto::NoTargets,
            Notice::AssetAboveCeiling {
                asset_id,
                label,
                price,
                max_price,
            } => NoticeDto::AssetAboveCeiling {
                asset_id: *asset_id,
                symbol: label.clone(),
                unit_price: *price,
                max_price: *max_price,
            },
            Notice::ConcentrationLimit {
                max_percent,
                assets,
                amount,
            } => NoticeDto::ConcentrationLimit {
                max_percent: *max_percent,
                asset_ids: assets.iter().map(|(id, _)| *id).collect(),
                symbols: assets.iter().map(|(_, label)| label.clone()).collect(),
                amount: *amount,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlanDto {
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    #[serde(with = "decimal_out")]
    pub invested: Decimal,
    #[serde(with = "decimal_out")]
    pub change: Decimal,
    #[serde(with = "decimal_out")]
    pub unallocated: Decimal,
    #[serde(with = "decimal_out")]
    pub total_before: Decimal,
    #[serde(with = "decimal_out")]
    pub total_after: Decimal,
    pub items: Vec<PlanItemDto>,
    pub classes: Vec<PlanClassDto>,
    pub notices: Vec<NoticeDto>,
    /// A renda estimada que o aporte soma; `null` sem estimativa de nenhum ativo.
    pub income: Option<PlanIncomeDto>,
}

impl PlanDto {
    pub fn new(plan: &Plan, view: &PortfolioView) -> Self {
        let mut items: Vec<PlanItemDto> = plan
            .items
            .iter()
            .map(|item| {
                let asset = view.asset(item.asset_id).map(|a| &a.asset);
                PlanItemDto {
                    asset_id: item.asset_id,
                    asset_class: item.asset_class,
                    symbol: asset.and_then(|a| a.symbol.clone()),
                    display_name: asset.map(|a| a.display_name.clone()).unwrap_or_default(),
                    listed_on_b3: asset.is_some_and(|a| a.listed_on_b3),
                    treasury: asset.is_some_and(|a| a.treasury),
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    value: item.value,
                    divisible: item.divisible,
                    class_percent_after: item.class_percent_after,
                    estimated_monthly_income: None,
                }
            })
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.value));
        Self {
            amount: plan.amount,
            invested: plan.invested,
            change: plan.change,
            unallocated: plan.unallocated,
            total_before: plan.total_before,
            total_after: plan.total_after,
            items,
            classes: plan.classes.iter().map(PlanClassDto::from).collect(),
            notices: plan.notices.iter().map(NoticeDto::from).collect(),
            income: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContributionItemDto {
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub display_name: String,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out")]
    pub unit_price: Decimal,
    #[serde(with = "decimal_out")]
    pub value: Decimal,
}

impl From<&ContributionItem> for ContributionItemDto {
    fn from(i: &ContributionItem) -> Self {
        Self {
            asset_id: i.asset_id,
            symbol: i.symbol.clone(),
            display_name: i.display_name.clone(),
            quantity: i.quantity,
            unit_price: i.unit_price,
            value: i.value,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContributionDto {
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    #[serde(with = "decimal_out")]
    pub invested: Decimal,
    pub items: Vec<ContributionItemDto>,
}

impl From<&Contribution> for ContributionDto {
    fn from(c: &Contribution) -> Self {
        Self {
            id: c.id,
            created_at: c.created_at,
            amount: c.amount,
            invested: c.items.iter().map(|i| i.value).sum(),
            items: c.items.iter().map(ContributionItemDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SymbolDto {
    pub symbol: String,
    pub display_name: String,
    pub exchange: String,
    pub suggested_class: AssetClass,
    pub listed_on_b3: bool,
    /// Título do Tesouro Direto: quem escolhe a sugestão cadastra com
    /// `treasury: true`.
    pub treasury: bool,
}

#[derive(Debug, Serialize)]
pub struct RefreshDto {
    pub refreshed: usize,
    pub failed: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn numbers_become_exact_decimals() {
        let plan: PlanRequest = serde_json::from_str(r#"{"amount": 6000.10}"#).unwrap();
        assert_eq!(plan.amount, dec!(6000.10));
        let plan: PlanRequest = serde_json::from_str(r#"{"amount": "0.1"}"#).unwrap();
        assert_eq!(plan.amount, dec!(0.1));
        let plan: PlanRequest = serde_json::from_str(r#"{"amount": 1e3}"#).unwrap();
        assert_eq!(plan.amount, dec!(1000));
        assert!(serde_json::from_str::<PlanRequest>(r#"{"amount": true}"#).is_err());
        assert!(serde_json::from_str::<PlanRequest>(r#"{"amount": "abc"}"#).is_err());
        assert!(serde_json::from_str::<PlanRequest>(r#"{"amount": 1, "extra": 2}"#).is_err());
    }

    #[test]
    fn patch_distinguishes_absent_null_and_value() {
        let absent: UpdateAssetRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.manual_price, None);
        assert_eq!(absent.score_override, None);
        let null: UpdateAssetRequest =
            serde_json::from_str(r#"{"manual_price": null, "score_override": null}"#).unwrap();
        assert_eq!(null.manual_price, Some(None));
        assert_eq!(null.score_override, Some(None));
        let value: UpdateAssetRequest =
            serde_json::from_str(r#"{"manual_price": 9.5, "score_override": 7, "quantity": 3}"#)
                .unwrap();
        assert_eq!(value.manual_price, Some(Some(dec!(9.5))));
        assert_eq!(value.score_override, Some(Some(7)));
        assert_eq!(value.quantity, Some(dec!(3)));
        let ceiling: UpdateAssetRequest = serde_json::from_str(r#"{"max_price": 30.5}"#).unwrap();
        assert_eq!(ceiling.max_price, Some(Some(dec!(30.5))));
        let cleared: UpdateAssetRequest = serde_json::from_str(r#"{"max_price": null}"#).unwrap();
        assert_eq!(cleared.max_price, Some(None));
        assert_eq!(absent.max_price, None);
    }

    #[test]
    fn notices_carry_their_code() {
        let json = serde_json::to_value(NoticeDto::from(&Notice::ClassBlocked {
            asset_class: AssetClass::IntlStocks,
            amount: dec!(145921.30),
            reason: crate::domain::allocation::BlockReason::NoAssets,
        }))
        .unwrap();
        assert_eq!(json["code"], "class_blocked");
        assert_eq!(json["reason"], "no_assets");
        assert_eq!(json["asset_class"], "intl_stocks");
        assert_eq!(json["amount"], 145921.3);
        let json = serde_json::to_value(NoticeDto::from(&Notice::NoTargets)).unwrap();
        assert_eq!(json["code"], "no_targets");
        let json = serde_json::to_value(NoticeDto::from(&Notice::AssetsWithoutPrice {
            labels: vec!["X".into()],
        }))
        .unwrap();
        assert_eq!(json["symbols"][0], "X");
        let json = serde_json::to_value(NoticeDto::from(&Notice::AssetsWithoutScore {
            labels: vec!["Y".into()],
        }))
        .unwrap();
        assert_eq!(json["code"], "assets_without_score");
        let id = Uuid::new_v4();
        let json = serde_json::to_value(NoticeDto::from(&Notice::AssetAboveCeiling {
            asset_id: id,
            label: "XPTO3".into(),
            price: dec!(32.5),
            max_price: dec!(30),
        }))
        .unwrap();
        assert_eq!(json["code"], "asset_above_ceiling");
        assert_eq!(json["asset_id"], id.to_string());
        assert_eq!(json["symbol"], "XPTO3");
        assert_eq!(json["unit_price"], 32.5);
        assert_eq!(json["max_price"], 30.0);
        let json = serde_json::to_value(NoticeDto::from(&Notice::ConcentrationLimit {
            max_percent: dec!(15),
            assets: vec![(id, "XPTO3".into())],
            amount: dec!(120.5),
        }))
        .unwrap();
        assert_eq!(json["code"], "concentration_limit");
        assert_eq!(json["max_percent"], 15.0);
        assert_eq!(json["asset_ids"][0], id.to_string());
        assert_eq!(json["symbols"][0], "XPTO3");
        assert_eq!(json["amount"], 120.5);
    }
}
