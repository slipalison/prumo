//! A configuração do motor do aporte: o limite de concentração por pessoa
//! (docs/api.md § `/settings/allocation`).

use axum::extract::State;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::dto::patch_decimal;
use super::{ApiJson, AppState, CurrentUser};
use crate::domain::allocation::{Limits, FULL_BP};
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Deserialize)]
#[serde(try_from = "AllocationSettingsBody")]
pub struct AllocationSettingsRequest {
    /// Obrigatório: um número de 0,01 a 100, ou `null` para desligar.
    pub max_asset_percent: Option<Decimal>,
}

/// O corpo como chega, com os três estados do campo: ausente (`None`), `null`
/// (`Some(None)`) e número (`Some(Some)`). A exigência do campo fica explícita
/// no `TryFrom`, e não num efeito colateral do `deserialize_with`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllocationSettingsBody {
    #[serde(default, deserialize_with = "patch_decimal")]
    max_asset_percent: Option<Option<Decimal>>,
}

impl TryFrom<AllocationSettingsBody> for AllocationSettingsRequest {
    type Error = &'static str;

    /// Sem o campo, a mesma recusa (e o mesmo texto) que o serde dá a um campo
    /// obrigatório que falta: desligar o limite pede `null` explícito.
    fn try_from(body: AllocationSettingsBody) -> Result<Self, Self::Error> {
        body.max_asset_percent
            .map(|max_asset_percent| Self { max_asset_percent })
            .ok_or("missing field `max_asset_percent`")
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AllocationSettingsDto {
    pub max_asset_percent: Option<f64>,
}

impl From<Limits> for AllocationSettingsDto {
    fn from(limits: Limits) -> Self {
        Self {
            max_asset_percent: limits.max_asset_bp.map(|bp| f64::from(bp) / 100.0),
        }
    }
}

/// O percentual vira pontos-base: de 0,01% a 100%, até duas casas.
fn max_asset_bp(percent: Decimal) -> ApiResult<u32> {
    Some(percent)
        .filter(|p| *p > Decimal::ZERO && *p <= Decimal::ONE_HUNDRED && p.normalize().scale() <= 2)
        .and_then(|p| (p * Decimal::ONE_HUNDRED).to_u32())
        .filter(|bp| (1..=FULL_BP).contains(bp))
        .ok_or_else(|| {
            ApiError::validation("O limite por ativo vai de 0,01% a 100%, com ate duas casas.")
        })
}

pub async fn settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<AllocationSettingsDto>> {
    let limits = state.store.allocation_limits(user.id).await?;
    Ok(Json(limits.into()))
}

pub async fn save_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<AllocationSettingsRequest>,
) -> ApiResult<Json<AllocationSettingsDto>> {
    let limits = Limits {
        max_asset_bp: body.max_asset_percent.map(max_asset_bp).transpose()?,
    };
    state.store.set_allocation_limits(user.id, limits).await?;
    Ok(Json(limits.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn the_percent_becomes_basis_points_within_range() {
        assert_eq!(max_asset_bp(dec!(15)).ok(), Some(1500));
        assert_eq!(max_asset_bp(dec!(12.5)).ok(), Some(1250));
        assert_eq!(max_asset_bp(dec!(0.01)).ok(), Some(1));
        assert_eq!(max_asset_bp(dec!(100)).ok(), Some(10_000));
        assert_eq!(max_asset_bp(dec!(20.000)).ok(), Some(2000));
        for bad in [dec!(0), dec!(-5), dec!(100.01), dec!(12.345)] {
            assert!(max_asset_bp(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_field_is_required_and_null_turns_it_off() {
        let off: AllocationSettingsRequest =
            serde_json::from_str(r#"{"max_asset_percent": null}"#).unwrap();
        assert_eq!(off.max_asset_percent, None);
        let on: AllocationSettingsRequest =
            serde_json::from_str(r#"{"max_asset_percent": 15}"#).unwrap();
        assert_eq!(on.max_asset_percent, Some(dec!(15)));
        assert!(serde_json::from_str::<AllocationSettingsRequest>("{}").is_err());
        assert!(serde_json::from_str::<AllocationSettingsRequest>(
            r#"{"max_asset_percent": 1, "x": 2}"#
        )
        .is_err());
        let dto = AllocationSettingsDto::from(Limits {
            max_asset_bp: Some(1250),
        });
        assert_eq!(dto.max_asset_percent, Some(12.5));
    }

    #[test]
    fn a_missing_field_reads_as_none_and_is_refused_with_the_serde_message() {
        let missing: AllocationSettingsBody = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.max_asset_percent, None);
        let null: AllocationSettingsBody =
            serde_json::from_str(r#"{"max_asset_percent": null}"#).unwrap();
        assert_eq!(null.max_asset_percent, Some(None));
        let error = serde_json::from_value::<AllocationSettingsRequest>(serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert_eq!(error, "missing field `max_asset_percent`");
    }
}
