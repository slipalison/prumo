//! Proventos e renda: rotas, DTOs e a renda estimada que o plano de aporte
//! mostra. A regra está em `domain::income`; a estimativa, em `crate::income`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::{decimal, decimal_out, optional_decimal, PlanDto};
use super::{ApiJson, ApiPath, ApiQuery, AppState, CurrentUser};
use crate::domain::allocation::round_cents;
use crate::domain::asset_class::UnitKind;
use crate::domain::income::{
    amounts, cash_cutoff, goal_progress, summarize, today_in_brazil, AmountsError, Entry,
    GoalProgress, IncomeKind, IncomeSource, IncomeStatus, IncomeSummary, ProgressBasis,
    WINDOW_MONTHS,
};
use crate::error::{ApiError, ApiResult};
use crate::income::PortfolioIncome;
use crate::service;
use crate::store::{IncomeEvent, IncomeFields, IncomeFilter, NewIncomeEvent, Objective};

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

/// Tamanho padrão e teto de `GET /income`.
const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 1000;
/// Quantos "próximos pagamentos" o resumo traz.
const UPCOMING_SHOWN: usize = 20;
/// Datas de pagamento aceitas (um erro de digitação de ano não entra).
const MIN_YEAR: i32 = 2000;
const MAX_YEAR: i32 = 2100;

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeQuery {
    #[serde(default, with = "iso_date::option")]
    pub from: Option<Date>,
    #[serde(default, with = "iso_date::option")]
    pub to: Option<Date>,
    #[serde(default)]
    pub status: Option<IncomeStatus>,
    #[serde(default)]
    pub asset: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateIncomeRequest {
    pub asset_id: Uuid,
    pub kind: IncomeKind,
    #[serde(default, with = "iso_date::option")]
    pub ex_date: Option<Date>,
    #[serde(with = "iso_date")]
    pub pay_date: Date,
    #[serde(deserialize_with = "decimal")]
    pub amount_per_unit: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub quantity: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub withholding: Option<Decimal>,
}

/// Três estados: ausente, `null` e valor.
fn patch_date<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<Date>>, D::Error> {
    iso_date::option::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateIncomeRequest {
    #[serde(default)]
    pub kind: Option<IncomeKind>,
    #[serde(default, deserialize_with = "patch_date")]
    pub ex_date: Option<Option<Date>>,
    #[serde(default, with = "iso_date::option")]
    pub pay_date: Option<Date>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub amount_per_unit: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub quantity: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub withholding: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeGoalRequest {
    #[serde(deserialize_with = "decimal")]
    pub target: Decimal,
}

// ------------------------------------------------------------------ saída --

#[derive(Debug, Serialize)]
pub struct IncomeEventDto {
    pub id: Uuid,
    pub asset_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub display_name: String,
    pub kind: IncomeKind,
    #[serde(with = "iso_date::option")]
    pub ex_date: Option<Date>,
    #[serde(with = "iso_date")]
    pub pay_date: Date,
    #[serde(with = "decimal_out")]
    pub amount_per_unit: Decimal,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    #[serde(with = "decimal_out")]
    pub gross: Decimal,
    #[serde(with = "decimal_out")]
    pub withholding: Decimal,
    #[serde(with = "decimal_out")]
    pub net: Decimal,
    pub status: IncomeStatus,
    pub source: IncomeSource,
    pub external_ref: Option<String>,
}

impl IncomeEventDto {
    fn new(event: &IncomeEvent, today: Date) -> Self {
        Self {
            id: event.id,
            asset_id: event.asset_id,
            symbol: event.symbol.clone(),
            display_name: event.display_name.clone(),
            kind: event.kind,
            ex_date: event.ex_date,
            pay_date: event.pay_date,
            amount_per_unit: event.amount_per_unit,
            quantity: event.quantity,
            gross: event.gross,
            withholding: event.withholding,
            net: event.net,
            status: event.status(today),
            source: event.source,
            external_ref: event.external_ref.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MonthDto {
    /// `2026-11`.
    pub month: String,
    #[serde(with = "decimal_out")]
    pub received: Decimal,
    #[serde(with = "decimal_out")]
    pub upcoming: Decimal,
}

#[derive(Debug, Serialize)]
pub struct PayerDto {
    pub asset_id: Option<Uuid>,
    pub label: String,
    #[serde(with = "decimal_out")]
    pub total: Decimal,
    #[serde(with = "decimal_out")]
    pub percent: Decimal,
}

#[derive(Debug, Serialize)]
pub struct YearDto {
    pub year: i32,
    /// Janeiro a dezembro: líquido recebido e a receber.
    #[serde(serialize_with = "serialize_decimals")]
    pub months: [Decimal; 12],
    #[serde(with = "decimal_out")]
    pub total: Decimal,
}

fn serialize_decimals<S: serde::Serializer>(
    values: &[Decimal; 12],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use rust_decimal::prelude::ToPrimitive;
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        seq.serialize_element(&value.to_f64().unwrap_or(0.0))?;
    }
    seq.end()
}

#[derive(Debug, Serialize)]
pub struct EstimateAssetDto {
    pub asset_id: Uuid,
    pub symbol: Option<String>,
    pub display_name: String,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    /// O que uma cota pagou em 12 meses, em reais, bruto.
    #[serde(with = "decimal_out")]
    pub per_unit_12m: Decimal,
    #[serde(with = "decimal_out")]
    pub monthly: Decimal,
}

/// A estimativa de renda mensal da carteira, a partir do histórico por cota.
#[derive(Debug, Serialize)]
pub struct EstimateDto {
    #[serde(with = "decimal_out")]
    pub monthly: Decimal,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    pub assets: Vec<EstimateAssetDto>,
    /// Papéis sem histórico no provedor (ou sem câmbio): ficam fora da conta.
    pub missing: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GoalDto {
    pub id: Uuid,
    #[serde(with = "decimal_out")]
    pub target: Decimal,
    #[serde(with = "time::serde::rfc3339::option")]
    pub achieved_at: Option<OffsetDateTime>,
    /// A renda mensal contra a meta: a média do recebido ou, sem nada
    /// recebido, a estimativa (`basis` diz qual).
    #[serde(with = "decimal_out")]
    pub current_monthly: Decimal,
    pub basis: ProgressBasis,
    #[serde(with = "decimal_out")]
    pub percent: Decimal,
    pub reached: bool,
}

#[derive(Debug, Serialize)]
pub struct IncomeSummaryDto {
    /// O dia de hoje no Brasil, que separa recebido de a receber.
    #[serde(with = "iso_date")]
    pub today: Date,
    /// Líquido recebido nos 12 meses que terminam no mês de hoje.
    #[serde(with = "decimal_out")]
    pub total_12m: Decimal,
    #[serde(with = "decimal_out")]
    pub monthly_average: Decimal,
    /// Líquido que ainda vai cair (pagamento depois de hoje).
    #[serde(with = "decimal_out")]
    pub upcoming_total: Decimal,
    pub months: Vec<MonthDto>,
    pub by_payer: Vec<PayerDto>,
    pub years: Vec<YearDto>,
    pub upcoming: Vec<IncomeEventDto>,
    pub estimate: EstimateDto,
    pub goal: Option<GoalDto>,
}

#[derive(Debug, Serialize)]
pub struct CashSuggestionDto {
    /// Líquido recebido desde o último aporte registrado.
    #[serde(with = "decimal_out")]
    pub amount: Decimal,
    /// Quantos pagamentos formam o valor.
    pub count: i64,
    /// O dia do último aporte (Brasília); `null` sem aporte registrado.
    #[serde(with = "iso_date::option")]
    pub since: Option<Date>,
}

/// A renda estimada que o aporte soma, no `POST /plans`.
#[derive(Debug, Serialize)]
pub struct PlanIncomeDto {
    /// O que os itens do plano rendem por mês, estimado.
    #[serde(with = "decimal_out")]
    pub added_monthly: Decimal,
    /// A renda mensal estimada da carteira hoje.
    #[serde(with = "decimal_out")]
    pub before_monthly: Decimal,
    #[serde(with = "decimal_out")]
    pub after_monthly: Decimal,
}

impl PlanDto {
    /// Junta a renda estimada ao plano: a de cada item e a do topo. Sem
    /// estimativa de nenhum ativo, o plano segue como estava.
    pub fn with_income(mut self, income: &PortfolioIncome) -> Self {
        if income.assets.is_empty() {
            return self;
        }
        let mut added = Decimal::ZERO;
        for item in &mut self.items {
            let monthly = income.asset(item.asset_id).map(|estimate| {
                round_cents(estimate.per_unit_12m * item.quantity / Decimal::from(WINDOW_MONTHS))
            });
            item.estimated_monthly_income = monthly;
            added += monthly.unwrap_or(Decimal::ZERO);
        }
        self.income = Some(PlanIncomeDto {
            added_monthly: added,
            before_monthly: income.monthly_total,
            after_monthly: income.monthly_total + added,
        });
        self
    }
}

// ---------------------------------------------------------------- helpers --

fn today_and_now() -> (Date, OffsetDateTime) {
    let now = OffsetDateTime::now_utc();
    (today_in_brazil(now), now)
}

fn max_money() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

fn positive(name: &str, value: Decimal) -> ApiResult<Decimal> {
    if value <= Decimal::ZERO || value > max_money() {
        return Err(ApiError::validation(format!(
            "{name} precisa ser maior que zero."
        )));
    }
    Ok(value)
}

fn pay_date_in_range(date: Date) -> ApiResult<Date> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&date.year()) {
        return Err(ApiError::validation(format!(
            "A data de pagamento precisa estar entre {MIN_YEAR} e {MAX_YEAR}."
        )));
    }
    Ok(date)
}

fn entry(event: &IncomeEvent) -> Entry {
    Entry {
        pay_date: event.pay_date,
        net: event.net,
        payer_key: event.payer_key(),
        payer_label: event.payer_label(),
    }
}

/// Valida e calcula os campos de um provento (criação ou edição).
fn build_fields(
    kind: IncomeKind,
    ex_date: Option<Date>,
    pay_date: Date,
    amount_per_unit: Decimal,
    quantity: Decimal,
    withholding: Option<Decimal>,
) -> ApiResult<IncomeFields> {
    let pay_date = pay_date_in_range(pay_date)?;
    if ex_date.is_some_and(|ex| ex > pay_date) {
        return Err(ApiError::validation(
            "A data ex nao pode ser depois da data de pagamento.",
        ));
    }
    let amount_per_unit = positive("O valor por cota", amount_per_unit)?;
    let quantity = positive("A quantidade", quantity)?;
    let amounts =
        amounts(kind, amount_per_unit, quantity, withholding).map_err(|error| match error {
            AmountsError::WithholdingAboveGross => {
                ApiError::validation("O imposto retido precisa ficar entre 0 e o valor bruto.")
            }
        })?;
    if amounts.gross <= Decimal::ZERO {
        return Err(ApiError::validation(
            "O valor bruto do provento precisa ser de ao menos um centavo.",
        ));
    }
    Ok(IncomeFields {
        kind,
        ex_date,
        pay_date,
        amount_per_unit,
        quantity,
        amounts,
    })
}

fn goal_dto(objective: &Objective, progress: &GoalProgress) -> GoalDto {
    GoalDto {
        id: objective.id,
        target: objective.target,
        achieved_at: objective.achieved_at,
        current_monthly: progress.current_monthly,
        basis: progress.basis,
        percent: progress.percent,
        reached: progress.reached,
    }
}

/// O progresso da meta. Quando o recebido já alcança a meta pela primeira vez,
/// a data fica registrada.
async fn goal_with_progress(
    state: &AppState,
    user: Uuid,
    objective: Objective,
    received_average: Decimal,
    estimated: Decimal,
    now: OffsetDateTime,
) -> ApiResult<GoalDto> {
    let progress = goal_progress(objective.target, received_average, estimated);
    let mut objective = objective;
    if progress.reached && objective.achieved_at.is_none() {
        objective.achieved_at = state
            .store
            .mark_objective_achieved(user, objective.id, now)
            .await?;
    }
    Ok(goal_dto(&objective, &progress))
}

async fn summary_of(
    state: &AppState,
    user: Uuid,
    today: Date,
) -> ApiResult<(IncomeSummary, Vec<IncomeEvent>)> {
    let ledger = state.store.income_ledger(user).await?;
    let entries: Vec<Entry> = ledger.iter().map(entry).collect();
    Ok((summarize(&entries, today), ledger))
}

/// A meta com o progresso, para as rotas que não calculam o resumo inteiro: a
/// estimativa só é buscada quando o livro ainda não tem nada recebido.
async fn goal_for(state: &AppState, user: Uuid, objective: Objective) -> ApiResult<GoalDto> {
    let (today, now) = today_and_now();
    let (summary, _) = summary_of(state, user, today).await?;
    let estimated = if summary.monthly_average > Decimal::ZERO {
        Decimal::ZERO
    } else {
        let view = service::portfolio(&state.store, user).await?;
        state.income.estimate(&view, today, now).await.monthly_total
    };
    goal_with_progress(
        state,
        user,
        objective,
        summary.monthly_average,
        estimated,
        now,
    )
    .await
}

// --------------------------------------------------------------- handlers --

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<IncomeQuery>,
) -> ApiResult<Json<Vec<IncomeEventDto>>> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::validation(format!(
            "O limite precisa estar entre 1 e {MAX_LIMIT}."
        )));
    }
    if query.from.zip(query.to).is_some_and(|(from, to)| from > to) {
        return Err(ApiError::validation(
            "A data inicial nao pode ser depois da final.",
        ));
    }
    let (today, _) = today_and_now();
    let filter = IncomeFilter {
        from: query.from,
        to: query.to,
        asset: query.asset,
        status: query.status,
        today,
        limit,
    };
    let events = state.store.income_events(user.id, &filter).await?;
    Ok(Json(
        events
            .iter()
            .map(|event| IncomeEventDto::new(event, today))
            .collect(),
    ))
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<CreateIncomeRequest>,
) -> ApiResult<impl IntoResponse> {
    let asset = state
        .store
        .asset(user.id, body.asset_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    // Renda fixa por valor não tem cota: o valor recebido é "1 × valor".
    let default_quantity = match asset.market().unit_kind() {
        UnitKind::Value => Decimal::ONE,
        UnitKind::Whole | UnitKind::Fractional | UnitKind::Hundredth => asset.quantity,
    };
    let fields = build_fields(
        body.kind,
        body.ex_date,
        body.pay_date,
        body.amount_per_unit,
        body.quantity.unwrap_or(default_quantity),
        body.withholding,
    )?;
    let new = NewIncomeEvent {
        asset_id: Some(asset.id),
        symbol: asset.symbol,
        display_name: asset.display_name,
        kind: fields.kind,
        ex_date: fields.ex_date,
        pay_date: fields.pay_date,
        amount_per_unit: fields.amount_per_unit,
        quantity: fields.quantity,
        amounts: fields.amounts,
        source: IncomeSource::Manual,
        external_ref: None,
    };
    let event = state.store.insert_income_event(user.id, &new).await?;
    let (today, _) = today_and_now();
    Ok((
        StatusCode::CREATED,
        Json(IncomeEventDto::new(&event, today)),
    ))
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateIncomeRequest>,
) -> ApiResult<Json<IncomeEventDto>> {
    let current = state
        .store
        .income_event(user.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if current.source != IncomeSource::Manual {
        return Err(ApiError::validation(
            "So se edita provento lancado a mao. O que veio de extrato fica como o extrato diz.",
        ));
    }
    let fields = build_fields(
        body.kind.unwrap_or(current.kind),
        body.ex_date.unwrap_or(current.ex_date),
        body.pay_date.unwrap_or(current.pay_date),
        body.amount_per_unit.unwrap_or(current.amount_per_unit),
        body.quantity.unwrap_or(current.quantity),
        body.withholding,
    )?;
    let event = state
        .store
        .update_manual_income_event(user.id, id, &fields)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let (today, _) = today_and_now();
    Ok(Json(IncomeEventDto::new(&event, today)))
}

pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> ApiResult<StatusCode> {
    if state.store.delete_income_event(user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}

pub async fn summary(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<IncomeSummaryDto>> {
    let (today, now) = today_and_now();
    let (summary, ledger) = summary_of(&state, user.id, today).await?;
    let view = service::portfolio(&state.store, user.id).await?;
    let estimate = state.income.estimate(&view, today, now).await;

    let goal = match state.store.income_goal(user.id).await? {
        Some(objective) => Some(
            goal_with_progress(
                &state,
                user.id,
                objective,
                summary.monthly_average,
                estimate.monthly_total,
                now,
            )
            .await?,
        ),
        None => None,
    };

    let mut estimated_assets: Vec<EstimateAssetDto> = view
        .assets
        .iter()
        .filter_map(|a| {
            estimate.asset(a.asset.id).map(|e| EstimateAssetDto {
                asset_id: a.asset.id,
                symbol: a.asset.symbol.clone(),
                display_name: a.asset.display_name.clone(),
                quantity: a.asset.quantity,
                per_unit_12m: e.per_unit_12m,
                monthly: e.monthly,
            })
        })
        .collect();
    estimated_assets.sort_by(|a, b| {
        b.monthly
            .cmp(&a.monthly)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    let upcoming: Vec<IncomeEventDto> = ledger
        .iter()
        .filter(|e| e.status(today) == IncomeStatus::Upcoming)
        .take(UPCOMING_SHOWN)
        .map(|e| IncomeEventDto::new(e, today))
        .collect();

    Ok(Json(IncomeSummaryDto {
        today,
        total_12m: summary.total_12m,
        monthly_average: summary.monthly_average,
        upcoming_total: summary.upcoming_total,
        months: summary
            .months
            .iter()
            .map(|m| MonthDto {
                month: m.month.label(),
                received: m.received,
                upcoming: m.upcoming,
            })
            .collect(),
        by_payer: summary
            .by_payer
            .iter()
            .map(|p| PayerDto {
                asset_id: p.key.parse().ok(),
                label: p.label.clone(),
                total: p.total,
                percent: p.percent,
            })
            .collect(),
        years: summary
            .years
            .iter()
            .map(|y| YearDto {
                year: y.year,
                months: y.months,
                total: y.total,
            })
            .collect(),
        upcoming,
        estimate: EstimateDto {
            monthly: estimate.monthly_total,
            updated_at: estimate.updated_at,
            assets: estimated_assets,
            missing: estimate.missing,
        },
        goal,
    }))
}

/// Os proventos líquidos recebidos desde o último aporte, para o campo do
/// aporte oferecer somá-los ao valor. O motor não muda: só o valor de entrada.
pub async fn cash_suggestion(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<CashSuggestionDto>> {
    let (today, _) = today_and_now();
    let Some(last) = state.store.last_contribution_at(user.id).await? else {
        return Ok(Json(CashSuggestionDto {
            amount: Decimal::ZERO,
            count: 0,
            since: None,
        }));
    };
    let since = cash_cutoff(last);
    let (amount, count) = state
        .store
        .income_received_between(user.id, since, today)
        .await?;
    Ok(Json(CashSuggestionDto {
        amount,
        count,
        since: Some(since),
    }))
}

pub async fn get_goal(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Option<GoalDto>>> {
    match state.store.income_goal(user.id).await? {
        Some(objective) => Ok(Json(Some(goal_for(&state, user.id, objective).await?))),
        None => Ok(Json(None)),
    }
}

pub async fn set_goal(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<IncomeGoalRequest>,
) -> ApiResult<Json<GoalDto>> {
    let target = positive("A meta de renda", body.target)?;
    let objective = state.store.set_income_goal(user.id, target).await?;
    Ok(Json(goal_for(&state, user.id, objective).await?))
}

pub async fn delete_goal(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<StatusCode> {
    if state.store.delete_income_goal(user.id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}
