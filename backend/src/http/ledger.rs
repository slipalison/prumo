//! `/api/v1/transactions`: o livro de lançamentos (docs/api.md § Lançamentos).
//!
//! Validação aqui; a conta da posição em `domain::ledger`; a gravação, com a
//! posição refeita na mesma transação, em `store::ledger`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::macros::format_description;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::{decimal, decimal_out, optional_decimal, patch_decimal};
use super::{ApiJson, ApiPath, ApiQuery, AppState, CurrentUser};
use crate::domain::asset_class::{AssetClass, UnitKind};
use crate::domain::ledger::{check_for_unit, Kind, LedgerError, PriceRule};
use crate::error::{ApiError, ApiResult};
use crate::store::{Asset, NewTransaction, Transaction, TransactionFields, TransactionFilter};

/// Teto de quantidade, preço e taxa (10^15), como no resto da API.
fn max_number() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

const DATE: &[time::format_description::BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

fn parse_date(text: &str) -> Result<Date, String> {
    Date::parse(text.trim(), DATE).map_err(|_| format!("data invalida: {text} (use AAAA-MM-DD)"))
}

fn date_in<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
    parse_date(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

fn optional_date_in<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Date>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|t| parse_date(&t))
        .transpose()
        .map_err(serde::de::Error::custom)
}

fn date_out<S: Serializer>(date: &Date, serializer: S) -> Result<S::Ok, S::Error> {
    let text = date.format(DATE).map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(&text)
}

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTransactionRequest {
    pub asset_id: Uuid,
    pub kind: Kind,
    #[serde(deserialize_with = "date_in")]
    pub trade_date: Date,
    #[serde(deserialize_with = "decimal")]
    pub quantity: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub unit_price: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub fees: Option<Decimal>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub fx_rate: Option<Decimal>,
}

fn patch_date<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Date>, D::Error> {
    date_in(deserializer).map(Some)
}

fn patch_some_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Decimal>, D::Error> {
    decimal(deserializer).map(Some)
}

/// Qualquer subconjunto; `null` em `unit_price`/`fx_rate` limpa.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UpdateTransactionRequest {
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default, deserialize_with = "patch_date")]
    pub trade_date: Option<Date>,
    #[serde(default, deserialize_with = "patch_some_decimal")]
    pub quantity: Option<Decimal>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub unit_price: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_some_decimal")]
    pub fees: Option<Decimal>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub fx_rate: Option<Option<Decimal>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionQuery {
    #[serde(default)]
    pub asset_id: Option<Uuid>,
    #[serde(default, deserialize_with = "optional_date_in")]
    pub from: Option<Date>,
    #[serde(default, deserialize_with = "optional_date_in")]
    pub to: Option<Date>,
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize)]
pub struct TransactionDto {
    pub id: Uuid,
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub display_name: String,
    pub asset_class: AssetClass,
    pub kind: Kind,
    #[serde(serialize_with = "date_out")]
    pub trade_date: Date,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out::option")]
    pub unit_price: Option<Decimal>,
    #[serde(with = "decimal_out")]
    pub fees: Decimal,
    pub currency: String,
    #[serde(with = "decimal_out::option")]
    pub fx_rate: Option<Decimal>,
    pub source: &'static str,
    pub external_ref: Option<String>,
    pub contribution_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<&Transaction> for TransactionDto {
    fn from(t: &Transaction) -> Self {
        Self {
            id: t.id,
            asset_id: t.asset_id,
            symbol: t.symbol.clone(),
            display_name: t.display_name.clone(),
            asset_class: t.asset_class,
            kind: t.fields.kind,
            trade_date: t.fields.trade_date,
            quantity: t.fields.quantity,
            unit_price: t.fields.unit_price,
            fees: t.fields.fees,
            currency: t.fields.currency.clone(),
            fx_rate: t.fields.fx_rate,
            source: t.source.as_str(),
            external_ref: t.external_ref.clone(),
            contribution_id: t.contribution_id,
            created_at: t.created_at,
        }
    }
}

// -------------------------------------------------------------- validação --

/// Nome do tipo para a mensagem que a pessoa lê (sem acento: atravessa log).
fn kind_label(kind: &str) -> &'static str {
    match Kind::parse(kind) {
        Some(Kind::Opening) => "O saldo inicial",
        Some(Kind::Buy) => "A compra",
        Some(Kind::Sell) => "A venda",
        Some(Kind::Split) => "O desdobramento",
        Some(Kind::ReverseSplit) => "O grupamento",
        Some(Kind::Bonus) => "A bonificacao",
        Some(Kind::Amortization) => "A amortizacao",
        Some(Kind::Redemption) => "O resgate",
        Some(Kind::Adjustment) | None => "O ajuste",
    }
}

/// O erro do livro como a API o devolve.
pub(super) fn ledger_error(error: &LedgerError) -> ApiError {
    match error {
        LedgerError::ExceedsPosition {
            date,
            kind,
            quantity,
            held,
        } => {
            let date = date.format(DATE).unwrap_or_default();
            ApiError::exceeds_position(format!(
                "{} de {date} ({}) passa da posicao de {} naquela data.",
                kind_label(kind),
                quantity.normalize(),
                held.normalize()
            ))
        }
        LedgerError::Invalid { reason, .. } => {
            ApiError::validation(format!("Lancamento invalido: {reason}."))
        }
        LedgerError::Overflow { .. } => ApiError::validation("Numero grande demais."),
    }
}

fn currency(text: Option<&str>) -> ApiResult<String> {
    let code = text.map(str::trim).unwrap_or("BRL").to_uppercase();
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(ApiError::validation(
            "A moeda tem tres letras, como BRL ou USD.",
        ));
    }
    Ok(code)
}

/// Confere os campos de um lançamento contra o ativo dele. Em renda fixa por
/// valor o preço é sempre 1: a pessoa informa só o valor.
fn validated(asset: &Asset, mut f: TransactionFields) -> ApiResult<TransactionFields> {
    let unit = asset.market().unit_kind();
    let kind = f.kind;
    if f.quantity.abs() > max_number() {
        return Err(ApiError::validation("Quantidade grande demais."));
    }
    if kind.signed() {
        if f.quantity.is_zero() {
            return Err(ApiError::validation("O ajuste precisa mudar a quantidade."));
        }
    } else if f.quantity <= Decimal::ZERO {
        return Err(ApiError::validation(
            "A quantidade precisa ser maior que zero.",
        ));
    }
    if unit == UnitKind::Value && kind.price_rule() == PriceRule::Required && f.unit_price.is_none()
    {
        f.unit_price = Some(Decimal::ONE);
    }
    match (kind.price_rule(), f.unit_price) {
        (PriceRule::Required, None) => {
            return Err(ApiError::validation(format!(
                "{} precisa de preco.",
                kind_label(kind.as_str())
            )))
        }
        (PriceRule::Forbidden, Some(_)) => {
            return Err(ApiError::validation(format!(
                "{} nao tem preco.",
                kind_label(kind.as_str())
            )))
        }
        (rule, Some(price)) => {
            let floor_ok = if rule == PriceRule::Required {
                price > Decimal::ZERO
            } else {
                price >= Decimal::ZERO
            };
            if !floor_ok || price > max_number() {
                return Err(ApiError::validation(
                    "O preco precisa ser positivo e menor que 10^15.",
                ));
            }
        }
        _ => {}
    }
    if f.fees < Decimal::ZERO || f.fees > max_number() || f.fees.scale() > 2 {
        return Err(ApiError::validation(
            "A taxa vai de 0 a 10^15, em centavos.",
        ));
    }
    if !kind.takes_fees() && !f.fees.is_zero() {
        return Err(ApiError::validation(format!(
            "{} nao tem taxa.",
            kind_label(kind.as_str())
        )));
    }
    f.currency = currency(Some(&f.currency))?;
    if f.currency == "BRL" {
        if f.fx_rate.is_some() {
            return Err(ApiError::validation("Lancamento em reais nao tem cambio."));
        }
    } else {
        if unit == UnitKind::Value {
            return Err(ApiError::validation(
                "Renda fixa por valor e sempre em reais.",
            ));
        }
        match f.fx_rate {
            Some(rate) if rate > Decimal::ZERO && rate <= max_number() => {}
            Some(_) => return Err(ApiError::validation("O cambio precisa ser positivo.")),
            None if f.unit_price.is_some() => {
                return Err(ApiError::validation(format!(
                    "Informe o cambio: quantos reais valia 1 {} no dia.",
                    f.currency
                )))
            }
            None => {}
        }
    }
    check_for_unit(kind, f.quantity, f.unit_price, unit).map_err(ApiError::validation)?;
    Ok(f)
}

// ---------------------------------------------------------------- rotas --

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<TransactionQuery>,
) -> ApiResult<Json<Vec<TransactionDto>>> {
    if let (Some(from), Some(to)) = (query.from, query.to) {
        if from > to {
            return Err(ApiError::validation("O periodo comeca depois de terminar."));
        }
    }
    let list = state
        .store
        .transactions(
            user.id,
            TransactionFilter {
                asset_id: query.asset_id,
                from: query.from,
                to: query.to,
            },
        )
        .await?;
    Ok(Json(list.iter().map(TransactionDto::from).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<CreateTransactionRequest>,
) -> ApiResult<impl IntoResponse> {
    let asset = state
        .store
        .asset(user.id, body.asset_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let fields = validated(
        &asset,
        TransactionFields {
            kind: body.kind,
            trade_date: body.trade_date,
            quantity: body.quantity,
            unit_price: body.unit_price,
            fees: body.fees.unwrap_or_default(),
            currency: currency(body.currency.as_deref())?,
            fx_rate: body.fx_rate,
        },
    )?;
    let stored = state
        .store
        .insert_transaction(user.id, &NewTransaction::manual(asset.id, fields))
        .await?;
    Ok((StatusCode::CREATED, Json(TransactionDto::from(&stored))))
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateTransactionRequest>,
) -> ApiResult<Json<TransactionDto>> {
    let current = state
        .store
        .transaction(user.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let asset_id = current.asset_id.ok_or_else(|| {
        ApiError::validation("O ativo deste lancamento foi removido; so da para apagar.")
    })?;
    let asset = state
        .store
        .asset(user.id, asset_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let mut fields = current.fields;
    if let Some(kind) = body.kind {
        fields.kind = kind;
    }
    if let Some(date) = body.trade_date {
        fields.trade_date = date;
    }
    if let Some(quantity) = body.quantity {
        fields.quantity = quantity;
    }
    if let Some(price) = body.unit_price {
        fields.unit_price = price;
    }
    if let Some(fees) = body.fees {
        fields.fees = fees;
    }
    if let Some(code) = body.currency.as_deref() {
        fields.currency = currency(Some(code))?;
    }
    if let Some(rate) = body.fx_rate {
        fields.fx_rate = rate;
    }
    let fields = validated(&asset, fields)?;
    let stored = state
        .store
        .update_transaction(user.id, id, &fields)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(TransactionDto::from(&stored)))
}

pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<StatusCode> {
    if state.store.delete_transaction(user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use time::macros::date;

    fn asset(class: AssetClass, listed_on_b3: bool) -> Asset {
        Asset {
            id: Uuid::new_v4(),
            asset_class: class,
            symbol: Some("X".into()),
            display_name: "X".into(),
            quantity: dec!(0),
            manual_price: None,
            score_override: None,
            listed_on_b3,
            treasury: false,
            ceiling: crate::domain::ceiling::CeilingChoice::None,
            tax_instrument: crate::domain::tax_book::TaxInstrument::Stock,
            tax_instrument_custom: false,
        }
    }

    fn fields(kind: Kind, quantity: Decimal, price: Option<Decimal>) -> TransactionFields {
        TransactionFields::brl(kind, date!(2026 - 03 - 01), quantity, price)
    }

    #[test]
    fn validation_follows_kind_unit_and_currency() {
        let fii = asset(AssetClass::BrReits, true);
        let rf = asset(AssetClass::FixedIncome, false);
        let us = asset(AssetClass::IntlStocks, false);
        assert!(validated(&fii, fields(Kind::Buy, dec!(10), Some(dec!(100)))).is_ok());
        // Renda fixa: o preço é 1, preenchido.
        let ok = validated(&rf, fields(Kind::Buy, dec!(1000.5), None)).unwrap();
        assert_eq!(ok.unit_price, Some(dec!(1)));
        let dollar = TransactionFields {
            currency: "usd".into(),
            fx_rate: Some(dec!(5.4)),
            ..fields(Kind::Buy, dec!(2), Some(dec!(200)))
        };
        assert_eq!(validated(&us, dollar.clone()).unwrap().currency, "USD");
        let bad = [
            (
                fii.clone(),
                fields(Kind::Buy, dec!(1.5), Some(dec!(1))),
                "inteira",
            ),
            (
                fii.clone(),
                fields(Kind::Buy, dec!(0), Some(dec!(1))),
                "maior que zero",
            ),
            (
                fii.clone(),
                fields(Kind::Adjustment, dec!(0), None),
                "mudar",
            ),
            (
                fii.clone(),
                fields(Kind::Buy, dec!(1), None),
                "precisa de preco",
            ),
            (
                fii.clone(),
                fields(Kind::Split, dec!(1), Some(dec!(1))),
                "nao tem preco",
            ),
            (
                fii.clone(),
                fields(Kind::Sell, dec!(1), Some(dec!(0))),
                "positivo",
            ),
            (
                fii.clone(),
                fields(Kind::Bonus, dec!(1), Some(dec!(-1))),
                "positivo",
            ),
            (
                fii.clone(),
                fields(Kind::Buy, dec!(1e16), Some(dec!(1))),
                "grande",
            ),
            (
                fii.clone(),
                TransactionFields {
                    fees: dec!(1),
                    ..fields(Kind::Split, dec!(1), None)
                },
                "nao tem taxa",
            ),
            (
                fii.clone(),
                TransactionFields {
                    fees: dec!(0.001),
                    ..fields(Kind::Buy, dec!(1), Some(dec!(1)))
                },
                "centavos",
            ),
            (
                fii.clone(),
                TransactionFields {
                    fx_rate: Some(dec!(1)),
                    ..fields(Kind::Buy, dec!(1), Some(dec!(1)))
                },
                "cambio",
            ),
            (
                us.clone(),
                TransactionFields {
                    fx_rate: None,
                    ..dollar.clone()
                },
                "Informe o cambio",
            ),
            (
                us.clone(),
                TransactionFields {
                    fx_rate: Some(dec!(0)),
                    ..dollar.clone()
                },
                "cambio",
            ),
            (
                us.clone(),
                TransactionFields {
                    currency: "DOLAR".into(),
                    ..dollar.clone()
                },
                "tres letras",
            ),
            (rf.clone(), fields(Kind::Split, dec!(1), None), "renda fixa"),
            (
                rf.clone(),
                fields(Kind::Buy, dec!(1), Some(dec!(2))),
                "so o valor",
            ),
            (
                rf.clone(),
                TransactionFields {
                    currency: "USD".into(),
                    fx_rate: Some(dec!(5)),
                    ..fields(Kind::Buy, dec!(1), None)
                },
                "sempre em reais",
            ),
        ];
        for (asset, f, why) in bad {
            let error = validated(&asset, f).unwrap_err();
            assert!(error.detail.contains(why), "{why}: {}", error.detail);
        }
    }

    #[test]
    fn ledger_errors_read_well() {
        let error = ledger_error(&LedgerError::ExceedsPosition {
            date: date!(2026 - 03 - 02),
            kind: "sell",
            quantity: dec!(15.000),
            held: dec!(10),
        });
        assert_eq!(error.code, "exceeds_position");
        assert_eq!(
            error.detail,
            "A venda de 2026-03-02 (15) passa da posicao de 10 naquela data."
        );
        let invalid = ledger_error(&LedgerError::Invalid {
            date: date!(2026 - 03 - 02),
            reason: "x",
        });
        assert_eq!(invalid.code, "validation_failed");
        let overflow = ledger_error(&LedgerError::Overflow {
            date: date!(2026 - 03 - 02),
        });
        assert_eq!(overflow.code, "validation_failed");
        for kind in Kind::ALL {
            assert!(!kind_label(kind.as_str()).is_empty());
        }
    }

    #[test]
    fn dates_and_patches_parse() {
        let body: CreateTransactionRequest = serde_json::from_str(
            r#"{"asset_id":"00000000-0000-0000-0000-000000000001","kind":"reverse_split","trade_date":"2026-03-01","quantity":"10"}"#,
        )
        .unwrap();
        assert_eq!(
            (body.kind, body.trade_date),
            (Kind::ReverseSplit, date!(2026 - 03 - 01))
        );
        assert!(serde_json::from_str::<CreateTransactionRequest>(
            r#"{"asset_id":"00000000-0000-0000-0000-000000000001","kind":"buy","trade_date":"01/03/2026","quantity":1}"#
        )
        .is_err());
        let patch: UpdateTransactionRequest = serde_json::from_str(
            r#"{"unit_price": null, "trade_date": "2026-01-02", "quantity": 3, "fees": 1}"#,
        )
        .unwrap();
        assert_eq!(patch.unit_price, Some(None));
        assert_eq!(patch.trade_date, Some(date!(2026 - 01 - 02)));
        assert_eq!((patch.quantity, patch.fees), (Some(dec!(3)), Some(dec!(1))));
        let empty: UpdateTransactionRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.fx_rate, None);
    }
}
