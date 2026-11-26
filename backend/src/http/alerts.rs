//! Alertas pelo Telegram (docs/api.md § Alertas): estado, preferências,
//! vinculação do chat e o teste. Tudo pelo dono.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;

use super::dto::optional_decimal;
use super::{ApiJson, AppState, CurrentUser};
use crate::alerts::{tax_year_covered, AlertError};
use crate::domain::alerts::{self as rules, AlertConfig, AlertKind, TypeSetting};
use crate::domain::income::today_in_brazil;
use crate::error::{ApiError, ApiResult};
use crate::telegram::TelegramError;

impl From<AlertError> for ApiError {
    fn from(error: AlertError) -> Self {
        match error {
            AlertError::NotConfigured => ApiError::validation(
                "Os alertas estao desligados neste servidor: falta o token do bot do Telegram.",
            ),
            AlertError::NotLinked => {
                ApiError::validation("Vincule o Telegram antes: nenhum chat esta vinculado.")
            }
            AlertError::TooSoon => ApiError {
                status: StatusCode::TOO_MANY_REQUESTS,
                code: "too_many_requests",
                title: "Muitos pedidos",
                detail: "Aguarde alguns segundos antes de pedir outro teste.".to_owned(),
            },
            AlertError::Store(error) => error.into(),
            AlertError::Telegram(error) => {
                tracing::warn!(error = %error, "telegram nao aceitou o envio");
                let detail = match error {
                    TelegramError::Forbidden(_) => {
                        "O Telegram recusou o envio (o bot foi bloqueado?). O chat foi desvinculado."
                    }
                    _ => "O Telegram nao aceitou a mensagem agora. Tente de novo em instantes.",
                };
                ApiError {
                    status: StatusCode::BAD_GATEWAY,
                    code: "telegram_unavailable",
                    title: "Telegram indisponivel",
                    detail: detail.to_owned(),
                }
            }
        }
    }
}

// ------------------------------------------------------------------ saída --

fn number(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

#[derive(Debug, Serialize)]
pub struct TelegramDto {
    /// O servidor tem o token do bot. Sem ele, nada abaixo funciona.
    pub configured: bool,
    /// Com quem conversar (`@usuario`, sem o `@`).
    pub bot_username: Option<String>,
    pub linked: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub linked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct TypeDto {
    pub kind: &'static str,
    pub enabled: bool,
    /// Se o servidor já apura este tipo.
    pub available: bool,
    pub threshold: Option<f64>,
    pub default_threshold: Option<f64>,
    /// `percentage_points` ou `percent_of_limit`; `null` sem limite.
    pub unit: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct AlertsDto {
    pub telegram: TelegramDto,
    /// `HH:MM`, horário de Brasília.
    pub send_at: String,
    pub hide_values: bool,
    pub types: Vec<TypeDto>,
}

fn unit(kind: AlertKind) -> Option<&'static str> {
    match kind {
        AlertKind::ClassDrift => Some("percentage_points"),
        AlertKind::TaxExemption => Some("percent_of_limit"),
        _ => None,
    }
}

fn types_dto(config: &AlertConfig, tax_rules: bool) -> Vec<TypeDto> {
    AlertKind::ALL
        .into_iter()
        .map(|kind| {
            let setting = config.get(kind);
            TypeDto {
                kind: kind.as_str(),
                enabled: setting.enabled && kind.is_available(tax_rules),
                available: kind.is_available(tax_rules),
                threshold: setting.threshold.map(number),
                default_threshold: kind.default_threshold().map(number),
                unit: unit(kind),
            }
        })
        .collect()
}

async fn current(state: &AppState, user: &CurrentUser) -> ApiResult<AlertsDto> {
    let settings = state.store.alert_settings(user.id).await?;
    let (config, _) = state.store.alert_config(user.id).await?;
    Ok(AlertsDto {
        telegram: TelegramDto {
            configured: state.alerts.configured(),
            bot_username: state.alerts.bot_username().await,
            linked: settings.chat_id.is_some(),
            linked_at: settings.linked_at,
        },
        send_at: rules::format_send_at(settings.send_at),
        hide_values: settings.hide_values,
        types: types_dto(
            &config,
            tax_year_covered(today_in_brazil(OffsetDateTime::now_utc()).year()),
        ),
    })
}

#[derive(Debug, Serialize)]
pub struct LinkDto {
    pub code: String,
    /// O que a pessoa manda ao bot.
    pub command: String,
    pub bot_username: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeChange {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub threshold: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertsRequest {
    #[serde(default)]
    pub send_at: Option<String>,
    #[serde(default)]
    pub hide_values: Option<bool>,
    #[serde(default)]
    pub types: Option<BTreeMap<String, TypeChange>>,
}

// --------------------------------------------------------------- handlers --

pub async fn get(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<AlertsDto>> {
    Ok(Json(current(&state, &user).await?))
}

pub async fn save(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<AlertsRequest>,
) -> ApiResult<Json<AlertsDto>> {
    if body.send_at.is_none() && body.hide_values.is_none() && body.types.is_none() {
        return Err(ApiError::validation(
            "Informe send_at, hide_values ou types.",
        ));
    }
    let send_at = match body.send_at.as_deref() {
        None => None,
        Some(text) => Some(rules::parse_send_at(text).ok_or_else(|| {
            ApiError::validation("O horario precisa ser HH:MM, de 00:00 a 23:59.")
        })?),
    };
    let (config, _) = state.store.alert_config(user.id).await?;
    let tax_rules = tax_year_covered(today_in_brazil(OffsetDateTime::now_utc()).year());
    let mut changes = Vec::new();
    for (name, change) in body.types.unwrap_or_default() {
        let kind = AlertKind::parse(&name)
            .ok_or_else(|| ApiError::validation(format!("Tipo de alerta desconhecido: {name}.")))?;
        if change.enabled.is_none() && change.threshold.is_none() {
            return Err(ApiError::validation(format!(
                "O tipo {name} precisa de enabled ou threshold."
            )));
        }
        if change.enabled == Some(true) && !kind.is_available(tax_rules) {
            return Err(ApiError::validation(format!(
                "O tipo {name} nao esta disponivel: o Prumo ainda nao tem a regra de IR deste ano."
            )));
        }
        let before = config.get(kind);
        let threshold = match change.threshold {
            None => before.threshold,
            Some(value) => Some(
                kind.validate_threshold(value)
                    .map_err(|detail| ApiError::validation(format!("{name}: {detail}")))?,
            ),
        };
        changes.push((
            kind,
            TypeSetting {
                enabled: change.enabled.unwrap_or(before.enabled),
                threshold,
            },
        ));
    }
    if send_at.is_some() || body.hide_values.is_some() {
        state
            .store
            .save_alert_settings(user.id, send_at, body.hide_values)
            .await?;
    }
    if !changes.is_empty() {
        state.store.save_alert_types(user.id, &changes).await?;
    }
    Ok(Json(current(&state, &user).await?))
}

pub async fn link(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<(StatusCode, Json<LinkDto>)> {
    let code = state
        .alerts
        .new_link_code(user.id, OffsetDateTime::now_utc())
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(LinkDto {
            command: format!("/start {}", code.code),
            code: code.code,
            bot_username: state.alerts.bot_username().await,
            expires_at: code.expires_at,
        }),
    ))
}

pub async fn unlink(State(state): State<AppState>, user: CurrentUser) -> ApiResult<StatusCode> {
    state.alerts.unlink(user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn test(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<serde_json::Value>> {
    state.alerts.send_test(user.id).await?;
    Ok(Json(json!({ "sent": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_list_marks_the_unavailable_one_and_its_units() {
        let json = serde_json::to_value(types_dto(&AlertConfig::default(), false)).unwrap();
        let types = json.as_array().unwrap();
        assert_eq!(types.len(), 6);
        assert_eq!(types[0]["kind"], "income_tomorrow");
        assert_eq!(types[0]["enabled"], true);
        assert!(types[0]["threshold"].is_null());
        assert!(types[0]["unit"].is_null());
        assert_eq!(types[1]["kind"], "class_drift");
        assert_eq!(types[1]["threshold"], 5.0);
        assert_eq!(types[1]["unit"], "percentage_points");
        assert_eq!(types[4]["kind"], "tax_exemption");
        assert_eq!(types[4]["available"], false);
        assert_eq!(types[4]["enabled"], false);
        assert_eq!(types[4]["default_threshold"], 80.0);
        assert_eq!(types[4]["unit"], "percent_of_limit");
    }

    #[test]
    fn the_request_refuses_unknown_fields() {
        assert!(serde_json::from_str::<AlertsRequest>(r#"{"x": 1}"#).is_err());
        let ok: AlertsRequest = serde_json::from_str(
            r#"{"send_at":"09:30","types":{"class_drift":{"threshold":7.5}}}"#,
        )
        .unwrap();
        assert_eq!(ok.send_at.as_deref(), Some("09:30"));
        assert!(serde_json::from_str::<AlertsRequest>(
            r#"{"types":{"class_drift":{"limit":7.5}}}"#
        )
        .is_err());
    }
}
