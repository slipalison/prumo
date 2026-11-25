//! Objetivos com prazo: rotas, DTOs e a conta de premissas. A regra (progresso,
//! média de aporte, data estimada) está em `domain::objectives`.
//!
//! O nome é `objectives`; `goals` são as metas por classe. A meta de renda
//! mensal continua em `/income/goal` e não aparece aqui.

use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::{decimal, decimal_out, optional_decimal, patch_decimal};
use super::{ApiJson, ApiPath, AppState, CurrentUser};
use crate::domain::asset_class::AssetClass;
use crate::domain::income::{today_in_brazil, ObjectiveKind};
use crate::domain::objectives::{
    annualize, average_monthly_contribution, cdi_net_annual, is_reached, month_label,
    months_to_reach, percent_done, ContributionSource, Eta, ReturnSource, Target,
    CONTRIBUTION_WINDOW_MONTHS, FALLBACK_ANNUAL_RETURN,
};
use crate::domain::performance::{months_before, period_return, QuotaPoint};
use crate::error::{ApiError, ApiResult};
use crate::history::IndexCode;
use crate::objectives::current_value;
use crate::service::{self, PortfolioView};
use crate::store::{NewObjective, Objective, ObjectiveChanges};

/// Quantos itens o corpo de `/objectives/impact` aceita (o plano tem um por ativo).
const MAX_IMPACT_ITEMS: usize = 200;
/// Teto de valores em reais: o mesmo das outras rotas.
fn max_money() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}
/// Limites da premissa de rentabilidade, em % ao ano (iguais ao `check` da tabela).
const MIN_RETURN_PERCENT: i64 = -50;
const MAX_RETURN_PERCENT: i64 = 100;

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateObjectiveRequest {
    pub kind: ObjectiveKind,
    #[serde(default)]
    pub asset_class: Option<AssetClass>,
    #[serde(deserialize_with = "decimal")]
    pub target: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub monthly_contribution: Option<Decimal>,
    /// Rentabilidade esperada, em % ao ano.
    #[serde(default, deserialize_with = "optional_decimal")]
    pub expected_return: Option<Decimal>,
}

/// O tipo e a classe não mudam: para trocar, apague e crie outro.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateObjectiveRequest {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub target: Option<Decimal>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub monthly_contribution: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub expected_return: Option<Option<Decimal>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactRequest {
    pub items: Vec<ImpactItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactItem {
    pub asset_class: AssetClass,
    #[serde(deserialize_with = "decimal")]
    pub value: Decimal,
}

fn positive_money(name: &str, value: Decimal) -> ApiResult<Decimal> {
    if value <= Decimal::ZERO || value > max_money() {
        return Err(ApiError::validation(format!(
            "{name} precisa ser maior que zero."
        )));
    }
    Ok(value.round_dp(2))
}

fn contribution_input(value: Decimal) -> ApiResult<Decimal> {
    if value < Decimal::ZERO || value > max_money() {
        return Err(ApiError::validation(
            "O aporte mensal precisa estar entre 0 e 10^15.",
        ));
    }
    Ok(value.round_dp(2))
}

fn return_input(value: Decimal) -> ApiResult<Decimal> {
    if value < Decimal::from(MIN_RETURN_PERCENT) || value > Decimal::from(MAX_RETURN_PERCENT) {
        return Err(ApiError::validation(format!(
            "A rentabilidade esperada precisa estar entre {MIN_RETURN_PERCENT}% e {MAX_RETURN_PERCENT}% ao ano."
        )));
    }
    Ok(value.round_dp(4))
}

fn optional<T>(
    value: Option<Decimal>,
    check: impl Fn(Decimal) -> ApiResult<T>,
) -> ApiResult<Option<T>> {
    value.map(check).transpose()
}

// ------------------------------------------------------------------ saída --

/// As premissas da conta, sempre expostas: sem elas a data não diz nada.
#[derive(Debug, Serialize)]
pub struct AssumptionsDto {
    /// Quanto entra por mês, em reais.
    #[serde(with = "decimal_out")]
    monthly_contribution: Decimal,
    contribution_source: ContributionSource,
    /// Rentabilidade anual usada, em %.
    #[serde(with = "decimal_out")]
    annual_return_percent: Decimal,
    return_source: ReturnSource,
}

#[derive(Debug, Serialize)]
pub struct ProjectionDto {
    /// `reached`, `eta`, `unreachable` ou `beyond`.
    state: &'static str,
    /// Meses até a meta (só em `eta`).
    months: Option<u32>,
    /// `AAAA-MM`, o mês estimado (só em `eta`).
    month: Option<String>,
}

impl ProjectionDto {
    fn new(eta: Eta, today: Date) -> Self {
        let (state, months) = match eta {
            Eta::Reached => ("reached", None),
            Eta::In(months) => ("eta", Some(months)),
            Eta::Unreachable => ("unreachable", None),
            Eta::Beyond => ("beyond", None),
        };
        Self {
            state,
            months,
            month: months.map(|m| month_label(today, m)),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ObjectiveDto {
    id: Uuid,
    kind: ObjectiveKind,
    asset_class: Option<AssetClass>,
    #[serde(with = "decimal_out")]
    target: Decimal,
    /// O que a pessoa informou; `null` = a média real do livro.
    #[serde(with = "decimal_out::option")]
    monthly_contribution: Option<Decimal>,
    /// O que a pessoa informou, em % ao ano; `null` = a rentabilidade do
    /// Desempenho ou o padrão.
    #[serde(with = "decimal_out::option")]
    expected_return: Option<Decimal>,
    #[serde(with = "decimal_out")]
    current: Decimal,
    /// De 0 a 100.
    #[serde(with = "decimal_out")]
    percent: Decimal,
    /// `active` ou `achieved`.
    status: &'static str,
    #[serde(with = "time::serde::rfc3339::option")]
    achieved_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    /// `null` num objetivo concluído: já não há conta a fazer.
    assumptions: Option<AssumptionsDto>,
    projection: ProjectionDto,
}

#[derive(Debug, Serialize)]
pub struct DefaultsDto {
    /// A rentabilidade anual sem premissa da pessoa, em %.
    #[serde(with = "decimal_out")]
    annual_return_percent: Decimal,
    return_source: ReturnSource,
}

#[derive(Debug, Serialize)]
pub struct ObjectivesDto {
    defaults: DefaultsDto,
    objectives: Vec<ObjectiveDto>,
}

#[derive(Debug, Serialize)]
pub struct ImpactEntryDto {
    id: Uuid,
    kind: ObjectiveKind,
    asset_class: Option<AssetClass>,
    #[serde(with = "decimal_out")]
    target: Decimal,
    #[serde(with = "decimal_out")]
    current: Decimal,
    /// O valor com este aporte.
    #[serde(with = "decimal_out")]
    after: Decimal,
    #[serde(with = "decimal_out")]
    percent_before: Decimal,
    #[serde(with = "decimal_out")]
    percent_after: Decimal,
    assumptions: AssumptionsDto,
    /// A data contada a partir do valor com este aporte.
    projection: ProjectionDto,
}

#[derive(Debug, Serialize)]
pub struct ImpactDto {
    objectives: Vec<ImpactEntryDto>,
}

// ------------------------------------------------------------- a conta -----

/// Tudo o que uma resposta precisa para avaliar objetivos: a carteira de
/// agora, a premissa de rentabilidade padrão e a média de aporte por classe,
/// cada uma buscada uma vez só.
struct Evaluator<'a> {
    state: &'a AppState,
    user: Uuid,
    now: OffsetDateTime,
    today: Date,
    view: PortfolioView,
    default_return: Option<(Decimal, ReturnSource)>,
    averages: HashMap<Option<AssetClass>, Decimal>,
}

impl<'a> Evaluator<'a> {
    async fn new(state: &'a AppState, user: Uuid) -> ApiResult<Self> {
        let now = OffsetDateTime::now_utc();
        Ok(Self {
            state,
            user,
            now,
            today: today_in_brazil(now),
            view: service::portfolio(&state.store, user).await?,
            default_return: None,
            averages: HashMap::new(),
        })
    }

    fn target_of(objective: &Objective) -> ApiResult<Target> {
        Target::try_from(objective.kind).map_err(|()| ApiError::internal())
    }

    /// O que o objetivo mede hoje.
    fn current(&self, objective: &Objective) -> ApiResult<Decimal> {
        current_value(&self.view, objective).ok_or_else(ApiError::internal)
    }

    /// A rentabilidade anual (fração) de quem não informou a sua: a do
    /// Desempenho (cota da carteira, 12 meses, anualizada; só com 6 meses de
    /// série pelo menos), senão o CDI de hoje líquido de IR, senão o padrão.
    async fn default_return(&mut self) -> ApiResult<(Decimal, ReturnSource)> {
        if let Some(found) = self.default_return {
            return Ok(found);
        }
        let points: Vec<QuotaPoint> = self
            .state
            .store
            .snapshot_points(self.user)
            .await?
            .into_iter()
            .map(|p| QuotaPoint {
                date: p.date,
                value: p.quota_value,
            })
            .collect();
        let measured = period_return(&points, months_before(self.today, 12), self.today)
            .and_then(|w| annualize(w.ret, (w.to - w.from).whole_days()));
        let found = if let Some(annual) = measured {
            (annual, ReturnSource::Performance)
        } else {
            let cdi = self
                .state
                .store
                .index_series(
                    IndexCode::Cdi,
                    self.today - time::Duration::days(30),
                    self.today,
                )
                .await?;
            match cdi.last().and_then(|(_, rate)| cdi_net_annual(*rate)) {
                Some(annual) => (annual, ReturnSource::CdiNet),
                None => (FALLBACK_ANNUAL_RETURN, ReturnSource::Fallback),
            }
        };
        self.default_return = Some(found);
        Ok(found)
    }

    /// A média mensal do que foi comprado nos últimos 12 meses, no total ou
    /// numa classe.
    async fn average(&mut self, class: Option<AssetClass>) -> ApiResult<Decimal> {
        if let Some(found) = self.averages.get(&class) {
            return Ok(*found);
        }
        let since = months_before(self.today, CONTRIBUTION_WINDOW_MONTHS);
        let (buys, first) = self.state.store.buys_since(self.user, class, since).await?;
        let average = average_monthly_contribution(&buys, first, self.today);
        self.averages.insert(class, average);
        Ok(average)
    }

    async fn assumptions(&mut self, objective: &Objective) -> ApiResult<AssumptionsDto> {
        let (monthly_contribution, contribution_source) = match objective.monthly_contribution {
            Some(informed) => (informed, ContributionSource::Informed),
            None => (
                self.average(objective.asset_class).await?,
                ContributionSource::Ledger,
            ),
        };
        let (annual_return_percent, return_source) = match objective.expected_return {
            Some(percent) => (percent, ReturnSource::Informed),
            None => {
                let (annual, source) = self.default_return().await?;
                ((annual * Decimal::ONE_HUNDRED).round_dp(2), source)
            }
        };
        Ok(AssumptionsDto {
            monthly_contribution,
            contribution_source,
            annual_return_percent,
            return_source,
        })
    }

    /// O objetivo com progresso e projeção. Se o valor chegou à meta pela
    /// primeira vez, a data fica registrada: meta atingida não volta a ativa
    /// quando o patrimônio cai.
    async fn evaluate(&mut self, mut objective: Objective) -> ApiResult<ObjectiveDto> {
        let current = self.current(&objective)?;
        if objective.achieved_at.is_none() && is_reached(current, objective.target) {
            objective.achieved_at = self
                .state
                .store
                .mark_objective_achieved(self.user, objective.id, self.now)
                .await?;
        }
        let achieved = objective.achieved_at.is_some();
        let (assumptions, eta) = if achieved {
            (None, Eta::Reached)
        } else {
            let assumptions = self.assumptions(&objective).await?;
            let eta = project(current, objective.target, &assumptions);
            (Some(assumptions), eta)
        };
        Ok(ObjectiveDto {
            id: objective.id,
            kind: objective.kind,
            asset_class: objective.asset_class,
            target: objective.target,
            monthly_contribution: objective.monthly_contribution,
            expected_return: objective.expected_return,
            current,
            percent: if achieved {
                Decimal::ONE_HUNDRED
            } else {
                percent_done(current, objective.target)
            },
            status: if achieved { "achieved" } else { "active" },
            achieved_at: objective.achieved_at,
            created_at: objective.created_at,
            assumptions,
            projection: ProjectionDto::new(eta, self.today),
        })
    }
}

fn project(current: Decimal, target: Decimal, assumptions: &AssumptionsDto) -> Eta {
    months_to_reach(
        current,
        target,
        assumptions.monthly_contribution,
        assumptions.annual_return_percent / Decimal::ONE_HUNDRED,
    )
}

// --------------------------------------------------------------- handlers --

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<ObjectivesDto>> {
    let stored = state.store.objectives(user.id).await?;
    let mut evaluator = Evaluator::new(&state, user.id).await?;
    let (annual, source) = evaluator.default_return().await?;
    let mut objectives = Vec::with_capacity(stored.len());
    for objective in stored {
        objectives.push(evaluator.evaluate(objective).await?);
    }
    Ok(Json(ObjectivesDto {
        defaults: DefaultsDto {
            annual_return_percent: (annual * Decimal::ONE_HUNDRED).round_dp(2),
            return_source: source,
        },
        objectives,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<CreateObjectiveRequest>,
) -> ApiResult<impl IntoResponse> {
    let kind = Target::try_from(body.kind).map_err(|()| {
        ApiError::validation(
            "A meta de renda mensal fica em Renda (PUT /income/goal). Aqui: net_worth ou class_value.",
        )
    })?;
    let asset_class = match (kind, body.asset_class) {
        (Target::ClassValue, Some(class)) => Some(class),
        (Target::ClassValue, None) => {
            return Err(ApiError::validation(
                "O objetivo por classe precisa da asset_class.",
            ))
        }
        (Target::NetWorth, None) => None,
        (Target::NetWorth, Some(_)) => {
            return Err(ApiError::validation(
                "O objetivo de patrimonio nao tem asset_class.",
            ))
        }
    };
    let new = NewObjective {
        target_kind: kind,
        asset_class,
        target: positive_money("A meta", body.target)?,
        monthly_contribution: optional(body.monthly_contribution, contribution_input)?,
        expected_return: optional(body.expected_return, return_input)?,
    };
    let objective = state.store.insert_objective(user.id, &new).await?;
    let dto = Evaluator::new(&state, user.id)
        .await?
        .evaluate(objective)
        .await?;
    Ok((StatusCode::CREATED, Json(dto)))
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateObjectiveRequest>,
) -> ApiResult<Json<ObjectiveDto>> {
    let changes = ObjectiveChanges {
        target: optional(body.target, |v| positive_money("A meta", v))?,
        monthly_contribution: body
            .monthly_contribution
            .map(|v| optional(v, contribution_input))
            .transpose()?,
        expected_return: body
            .expected_return
            .map(|v| optional(v, return_input))
            .transpose()?,
    };
    let objective = state
        .store
        .update_objective(user.id, id, &changes)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let dto = Evaluator::new(&state, user.id)
        .await?
        .evaluate(objective)
        .await?;
    Ok(Json(dto))
}

pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<StatusCode> {
    if state.store.delete_objective(user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

/// O que um aporte faz com os objetivos ativos: o percentual com ele e a data
/// estimada contada a partir do valor que a carteira passa a ter. A tela do
/// Aportar manda o que o plano compra, por classe.
pub async fn impact(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<ImpactRequest>,
) -> ApiResult<Json<ImpactDto>> {
    if body.items.len() > MAX_IMPACT_ITEMS {
        return Err(ApiError::validation(format!(
            "No maximo {MAX_IMPACT_ITEMS} itens."
        )));
    }
    let mut added: HashMap<AssetClass, Decimal> = HashMap::new();
    for item in &body.items {
        if item.value < Decimal::ZERO || item.value > max_money() {
            return Err(ApiError::validation(
                "O valor do item precisa ser positivo.",
            ));
        }
        *added.entry(item.asset_class).or_default() += item.value;
    }
    let total_added: Decimal = added.values().sum();

    let stored = state.store.objectives(user.id).await?;
    let mut evaluator = Evaluator::new(&state, user.id).await?;
    let mut entries = Vec::new();
    for objective in stored.into_iter().filter(|o| o.achieved_at.is_none()) {
        let current = evaluator.current(&objective)?;
        // Já está na meta (a lista ainda vai marcar): não há o que projetar.
        if is_reached(current, objective.target) {
            continue;
        }
        let gain = match Evaluator::target_of(&objective)? {
            Target::NetWorth => total_added,
            Target::ClassValue => objective
                .asset_class
                .and_then(|class| added.get(&class).copied())
                .unwrap_or_default(),
        };
        let after = current + gain;
        let assumptions = evaluator.assumptions(&objective).await?;
        let eta = project(after, objective.target, &assumptions);
        entries.push(ImpactEntryDto {
            id: objective.id,
            kind: objective.kind,
            asset_class: objective.asset_class,
            target: objective.target,
            current,
            after,
            percent_before: percent_done(current, objective.target),
            percent_after: percent_done(after, objective.target),
            assumptions,
            projection: ProjectionDto::new(eta, evaluator.today),
        });
    }
    Ok(Json(ImpactDto {
        objectives: entries,
    }))
}
