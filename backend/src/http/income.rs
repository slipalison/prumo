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
use super::yoc::{PortfolioYieldDto, YieldDto, YieldReport};
use super::{ApiJson, ApiPath, ApiQuery, AppState, CurrentUser};
use crate::domain::asset_class::{Market, UnitKind};
use crate::domain::entitlement::{quantity_on, record_date};
use crate::domain::income::estimate::{EstimateSource, UsualEstimate};
use crate::domain::income::{
    amounts, monthly_estimate, summarize, today_in_brazil, AmountsError, Entry, GoalProgress,
    IncomeKind, IncomeSource, IncomeStatus, IncomeSummary, ProgressBasis,
};
use crate::domain::projection::goal::IncomeMode;
use crate::domain::projection::progress::{Estimated, Received};
use crate::domain::yoc::asset_yield;
use crate::error::{ApiError, ApiResult};
use crate::income::PortfolioIncome;
use crate::objectives::{estimated_income, income_progress, needs_estimate, received_income};
use crate::service;
use crate::store::{
    Asset, IncomeEvent, IncomeFields, IncomeFilter, NewIncomeEvent, Objective, StoreError,
};

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

/// `GET /income/entitlement`: quantas cotas davam direito a um provento.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntitlementQuery {
    pub asset: Uuid,
    #[serde(default, with = "iso_date::option")]
    pub ex_date: Option<Date>,
    #[serde(default, with = "iso_date::option")]
    pub pay_date: Option<Date>,
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
    /// Renda de 12 meses do pagador sobre o custo do ativo.
    pub yield_on_cost: YieldDto,
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

/// Um valor líquido do IR do JCP e o bruto ao lado.
#[derive(Debug, Serialize)]
pub struct UsualDto {
    /// Quantos anos de 365 dias entraram na média (2 a 5).
    pub years: u32,
    /// Por cota, por ano. O líquido usa a parte de JCP dos 12 meses do ativo
    /// (o histórico longo é do Yahoo, que não separa o JCP).
    #[serde(with = "decimal_out")]
    pub per_unit_year: Decimal,
    #[serde(with = "decimal_out")]
    pub per_unit_year_gross: Decimal,
    /// Com a quantidade de hoje, por mês.
    #[serde(with = "decimal_out")]
    pub monthly: Decimal,
    #[serde(with = "decimal_out")]
    pub monthly_gross: Decimal,
}

impl From<&UsualEstimate> for UsualDto {
    fn from(usual: &UsualEstimate) -> Self {
        Self {
            years: usual.years,
            per_unit_year: usual.per_unit_year.net,
            per_unit_year_gross: usual.per_unit_year.gross,
            monthly: usual.monthly.net,
            monthly_gross: usual.monthly.gross,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EstimateAssetDto {
    pub asset_id: Uuid,
    pub symbol: Option<String>,
    pub display_name: String,
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    /// De onde veio o valor por cota: `b3` (anúncios da B3, JCP separado) ou
    /// `yahoo` (JCP junto; nos EUA, menos os 30% retidos lá).
    pub source: EstimateSource,
    /// O que uma cota pagou em 12 meses, em reais, líquido do IR retido na
    /// fonte (o do JCP aqui, os 30% dos EUA lá).
    #[serde(with = "decimal_out")]
    pub per_unit_12m: Decimal,
    #[serde(with = "decimal_out")]
    pub per_unit_12m_gross: Decimal,
    /// `per_unit_12m × quantity ÷ 12`: líquido.
    #[serde(with = "decimal_out")]
    pub monthly: Decimal,
    #[serde(with = "decimal_out")]
    pub monthly_gross: Decimal,
    /// A média anual do habitual (até 5 anos); `null` com menos de 2 anos de
    /// histórico.
    pub usual: Option<UsualDto>,
    /// Os 12 meses passam de 1,5 × a média do habitual.
    pub above_usual: bool,
    /// O que a posição teria rendido em 12 meses, com as cotas que o livro
    /// tinha na data com de cada provento — não as de hoje. Líquido pela B3 e
    /// nos EUA; bruto numa ação da B3 pelo Yahoo (ele não separa o JCP).
    #[serde(with = "decimal_out::option")]
    pub trailing_12m: Option<Decimal>,
}

/// A estimativa de renda mensal da carteira, a partir do que cada cota pagou.
#[derive(Debug, Serialize)]
pub struct EstimateDto {
    /// Líquido do IR retido na fonte (JCP no Brasil, 30% nos EUA).
    #[serde(with = "decimal_out")]
    pub monthly: Decimal,
    #[serde(with = "decimal_out")]
    pub monthly_gross: Decimal,
    /// A parte líquida de `monthly` que vem de ativos acima do habitual.
    #[serde(with = "decimal_out")]
    pub above_usual_monthly: Decimal,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    pub assets: Vec<EstimateAssetDto>,
    /// Papéis sem histórico em nenhuma fonte (ou sem câmbio): ficam fora da conta.
    pub missing: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GoalDto {
    pub id: Uuid,
    #[serde(with = "decimal_out")]
    pub target: Decimal,
    /// `average` (a média por mês) ou `floor` (o menor mês de 12).
    pub mode: IncomeMode,
    #[serde(with = "time::serde::rfc3339::option")]
    pub achieved_at: Option<OffsetDateTime>,
    /// A renda mensal contra a meta, no modo dela: do recebido ou, sem nada
    /// recebido, da estimativa (`basis` diz qual).
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
    /// Yield on cost da carteira: renda de 12 meses ÷ custo.
    pub yield_on_cost: PortfolioYieldDto,
    pub goal: Option<GoalDto>,
}

#[derive(Debug, Serialize)]
pub struct EntitlementDto {
    /// As cotas do livro na data com (`1` em renda fixa por valor).
    #[serde(with = "decimal_out")]
    pub quantity: Decimal,
    /// A data com usada: o dia anterior à data ex, ou, sem ela, o pagamento
    /// (no máximo hoje).
    #[serde(with = "iso_date")]
    pub record_date: Date,
}

/// A renda estimada que o aporte soma, no `POST /plans`.
#[derive(Debug, Serialize)]
pub struct PlanIncomeDto {
    /// `false` quando nenhum ativo tem histórico em cache ainda: os três valores
    /// vêm zerados e a tela diz "estimativa indisponível". O plano nunca espera
    /// o provedor.
    pub available: bool,
    /// Ativos que pagam provento e ficaram sem estimativa (histórico ainda não
    /// buscado ou provedor fora do ar).
    pub missing: Vec<String>,
    /// O que os itens do plano rendem por mês, estimado e líquido do IR do JCP.
    #[serde(with = "decimal_out")]
    pub added_monthly: Decimal,
    /// A renda mensal estimada da carteira hoje (líquida, a mesma da Renda).
    #[serde(with = "decimal_out")]
    pub before_monthly: Decimal,
    #[serde(with = "decimal_out")]
    pub after_monthly: Decimal,
}

impl PlanDto {
    /// Junta a renda estimada (do banco) ao plano: a de cada item e a do topo,
    /// líquidas do IR do JCP como a estimativa da Renda e a meta.
    /// Sem ativo que pague provento não há o que estimar e o plano segue como
    /// estava; com ativo e sem cache, `income.available` é `false`.
    pub fn with_income(mut self, income: &PortfolioIncome) -> Self {
        if income.payers == 0 {
            return self;
        }
        let mut added = Decimal::ZERO;
        for item in &mut self.items {
            // Líquida, como a renda da carteira: a mesma conta da Renda.
            let monthly = income
                .asset(item.asset_id)
                .map(|asset| monthly_estimate(asset.estimate.per_unit_12m.net, item.quantity));
            item.estimated_monthly_income = monthly;
            added += monthly.unwrap_or(Decimal::ZERO);
        }
        self.income = Some(PlanIncomeDto {
            available: !income.assets.is_empty(),
            missing: income.missing.clone(),
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

/// Valida e calcula os campos de um provento (criação ou edição).
fn build_fields(
    market: Market,
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
    let amounts = amounts(
        market,
        kind,
        pay_date,
        amount_per_unit,
        quantity,
        withholding,
    )
    .map_err(|error| match error {
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
        mode: objective.income_mode.unwrap_or_default(),
        achieved_at: objective.achieved_at,
        current_monthly: progress.current_monthly,
        basis: progress.basis,
        percent: progress.percent,
        reached: progress.reached,
    }
}

/// O progresso da meta, no modo dela. Quando o recebido já alcança a meta pela
/// primeira vez, a data fica registrada.
async fn goal_with_progress(
    state: &AppState,
    user: Uuid,
    objective: Objective,
    received: Received,
    estimated: Estimated,
    now: OffsetDateTime,
) -> ApiResult<GoalDto> {
    let progress = income_progress(&objective, received, estimated);
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
    let entries: Vec<Entry> = ledger.iter().map(IncomeEvent::entry).collect();
    Ok((summarize(&entries, today), ledger))
}

/// A meta com o progresso, para as rotas que não calculam o resumo inteiro: a
/// estimativa só é buscada quando o livro ainda não tem nada recebido na
/// janela do modo da meta.
async fn goal_for(state: &AppState, user: Uuid, objective: Objective) -> ApiResult<GoalDto> {
    let (today, now) = today_and_now();
    let ledger = state.store.income_ledger(user).await?;
    let entries: Vec<Entry> = ledger.iter().map(IncomeEvent::entry).collect();
    let received = received_income(&entries, today);
    let estimated = if needs_estimate(&objective, received) {
        let view = service::portfolio(&state.store, user).await?;
        estimated_income(&state.income.estimate(&view, today).await)
    } else {
        Estimated::default()
    };
    goal_with_progress(state, user, objective, received, estimated, now).await
}

/// As cotas que davam direito ao provento: a posição do livro na data com. Em
/// renda fixa por valor não há cota, e o valor recebido é "1 × valor".
async fn entitled_quantity(
    state: &AppState,
    user: Uuid,
    asset: &Asset,
    ex_date: Option<Date>,
    pay_date: Option<Date>,
) -> ApiResult<EntitlementDto> {
    let (today, _) = today_and_now();
    let on = record_date(ex_date, pay_date.unwrap_or(today), today);
    let quantity = match asset.market().unit_kind() {
        UnitKind::Value => Decimal::ONE,
        UnitKind::Whole | UnitKind::Fractional | UnitKind::Hundredth => {
            let entries = state.store.asset_entries(user, asset.id).await?;
            quantity_on(&entries, on).map_err(StoreError::Ledger)?
        }
    };
    Ok(EntitlementDto {
        quantity,
        record_date: on,
    })
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
    let quantity = match body.quantity {
        Some(quantity) => quantity,
        None => {
            // Sem quantidade, vale a do livro na data com.
            let entitled =
                entitled_quantity(&state, user.id, &asset, body.ex_date, Some(body.pay_date))
                    .await?;
            if entitled.quantity <= Decimal::ZERO {
                return Err(ApiError::validation(format!(
                    "Voce nao tinha cotas desse ativo na data com ({}). Informe a quantidade se mesmo assim recebeu.",
                    entitled.record_date
                )));
            }
            entitled.quantity
        }
    };
    let market = asset
        .asset_class
        .market_of(asset.listed_on_b3, asset.treasury);
    let fields = build_fields(
        market,
        body.kind,
        body.ex_date,
        body.pay_date,
        body.amount_per_unit,
        quantity,
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
    // Evento sem ativo (o ativo foi apagado) não tem mercado: vale o da B3.
    let market = match current.asset_id {
        Some(asset_id) => state
            .store
            .asset(user.id, asset_id)
            .await?
            .map_or(Market::B3, |asset| {
                asset
                    .asset_class
                    .market_of(asset.listed_on_b3, asset.treasury)
            }),
        None => Market::B3,
    };
    let fields = build_fields(
        market,
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
    let estimate = state
        .income
        .estimate_with_ledger(user.id, &view, today)
        .await;
    let yields = YieldReport::build(&view, &summary, &estimate);

    let goal = match state.store.income_goal(user.id).await? {
        Some(objective) => {
            let entries: Vec<Entry> = ledger.iter().map(IncomeEvent::entry).collect();
            Some(
                goal_with_progress(
                    &state,
                    user.id,
                    objective,
                    received_income(&entries, today),
                    estimated_income(&estimate),
                    now,
                )
                .await?,
            )
        }
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
                source: e.estimate.source,
                per_unit_12m: e.estimate.per_unit_12m.net,
                per_unit_12m_gross: e.estimate.per_unit_12m.gross,
                monthly: e.estimate.monthly.net,
                monthly_gross: e.estimate.monthly.gross,
                usual: e.estimate.usual.as_ref().map(UsualDto::from),
                above_usual: e.estimate.above_usual,
                trailing_12m: e.trailing_12m,
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
            .map(|p| {
                let asset_id: Option<Uuid> = p.key.parse().ok();
                // Pagador sem ativo na carteira (removido): sem posição, sem custo.
                let fallback = asset_yield(p.total, None, Decimal::ZERO, None);
                let y = asset_id.and_then(|id| yields.of(id)).unwrap_or(&fallback);
                PayerDto {
                    asset_id,
                    label: p.label.clone(),
                    total: p.total,
                    percent: p.percent,
                    yield_on_cost: YieldDto::from(y),
                }
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
            monthly_gross: estimate.monthly_total_gross,
            above_usual_monthly: estimate.above_usual_monthly,
            updated_at: estimate.updated_at,
            assets: estimated_assets,
            missing: estimate.missing,
        },
        yield_on_cost: PortfolioYieldDto::from(&yields.portfolio),
        goal,
    }))
}

/// Quantas cotas davam direito a um provento: a posição do livro na data com.
/// A tela usa para mostrar, antes de lançar, a quantidade que o servidor vai usar.
pub async fn entitlement(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiQuery(query): ApiQuery<EntitlementQuery>,
) -> ApiResult<Json<EntitlementDto>> {
    let asset = state
        .store
        .asset(user.id, query.asset)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(
        entitled_quantity(&state, user.id, &asset, query.ex_date, query.pay_date).await?,
    ))
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
