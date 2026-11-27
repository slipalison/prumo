//! Objetivos: rotas, DTOs e a conta das premissas. A simulação (três
//! cenários, em reais de hoje) está em `domain::projection::goal`; de onde vem
//! cada premissa padrão, em `docs/projecao.md`.
//!
//! O nome é `objectives`; `goals` são as metas por classe. A meta de renda
//! mensal é um objetivo como os outros aqui (uma por pessoa) e continua também
//! em `/income/goal`, que a tela Renda usa.

use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::dto::{decimal, decimal_out, optional_decimal, patch_decimal, patch_value};
use super::{ApiJson, ApiPath, AppState, CurrentUser};
use crate::domain::asset_class::AssetClass;
use crate::domain::income::{today_in_brazil, ObjectiveKind, ProgressBasis, YearMonth};
use crate::domain::objectives::{
    annualize, average_monthly_contribution, cdi_net_annual, deflate, is_reached, month_label,
    percent_done, ContributionSource, Eta, ReturnSource, CONTRIBUTION_WINDOW_MONTHS,
    FALLBACK_REAL_RETURN,
};
use crate::domain::performance::{months_before, period_return, QuotaPoint};
use crate::domain::projection::goal::{
    chart_horizon, chart_step, curve, months_to, IncomeMode, Measure, Premises, Start, TargetBasis,
};
use crate::domain::projection::{fraction, to_decimal, to_f64, Scenario, DEFAULT_SPREAD_PERCENT};
use crate::error::{ApiError, ApiResult};
use crate::income::PortfolioIncome;
use crate::objectives::{
    class_value, current_value, estimated_income, income_progress, needs_estimate, received_income,
};
use crate::projection::{
    net_yield_percent, portfolio_shape, scope_income, InflationSource, Outlook,
};
use crate::service::{self, PortfolioView};
use crate::store::{
    IncomeEvent, NewObjective, Objective, ObjectiveChanges, ObjectivePremises, StoreError,
};

/// Quantos itens o corpo de `/objectives/impact` aceita (o plano tem um por ativo).
const MAX_IMPACT_ITEMS: usize = 200;
/// Teto de valores em reais: o mesmo das outras rotas.
fn max_money() -> Decimal {
    Decimal::new(1_000_000_000_000_000, 0)
}

/// Os limites de cada premissa em % ao ano (iguais aos `check` da tabela).
struct Range {
    name: &'static str,
    min: i64,
    max: i64,
}

const REAL_RETURN: Range = Range {
    name: "A rentabilidade real",
    min: -50,
    max: 50,
};
const INFLATION: Range = Range {
    name: "A inflacao",
    min: -10,
    max: 50,
};
const INCOME_GROWTH: Range = Range {
    name: "O crescimento dos proventos",
    min: -50,
    max: 50,
};
const INCOME_YIELD: Range = Range {
    name: "O rendimento do dinheiro novo",
    min: 0,
    max: 50,
};
const SPREAD: Range = Range {
    name: "A amplitude dos cenarios",
    min: 0,
    max: 20,
};

// ---------------------------------------------------------------- entrada --

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateObjectiveRequest {
    pub kind: ObjectiveKind,
    #[serde(default)]
    pub asset_class: Option<AssetClass>,
    #[serde(deserialize_with = "decimal")]
    pub target: Decimal,
    /// Só na meta de renda.
    #[serde(default)]
    pub income_mode: Option<IncomeMode>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub monthly_contribution: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub real_return: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub inflation: Option<Decimal>,
    #[serde(default)]
    pub reinvest_income: Option<bool>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub income_growth: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub income_yield: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub scenario_spread: Option<Decimal>,
}

/// O tipo e a classe não mudam: para trocar, apague e crie outro. `null` numa
/// premissa volta ao padrão.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateObjectiveRequest {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub target: Option<Decimal>,
    #[serde(default, deserialize_with = "patch_value")]
    pub income_mode: Option<Option<IncomeMode>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub monthly_contribution: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub real_return: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub inflation: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_value")]
    pub reinvest_income: Option<Option<bool>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub income_growth: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub income_yield: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "patch_decimal")]
    pub scenario_spread: Option<Option<Decimal>>,
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

fn rate_input(range: &Range, value: Decimal) -> ApiResult<Decimal> {
    if value < Decimal::from(range.min) || value > Decimal::from(range.max) {
        return Err(ApiError::validation(format!(
            "{} precisa estar entre {}% e {}% ao ano.",
            range.name, range.min, range.max
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

fn patched(
    value: Option<Option<Decimal>>,
    check: impl Fn(Decimal) -> ApiResult<Decimal>,
) -> ApiResult<Option<Option<Decimal>>> {
    value.map(|v| optional(v, &check)).transpose()
}

// ------------------------------------------------------------------ saída --

/// De onde veio uma premissa que não tem outra fonte além do padrão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PremiseSource {
    Informed,
    Default,
}

/// De onde veio o rendimento do dinheiro novo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldSource {
    Informed,
    /// A renda estimada da Renda sobre o valor de hoje (da classe, no
    /// objetivo de classe).
    Measured,
}

/// O que a pessoa informou; `null` = o padrão.
#[derive(Debug, Serialize)]
pub struct PremisesDto {
    #[serde(with = "decimal_out::option")]
    monthly_contribution: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    real_return: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    inflation: Option<Decimal>,
    reinvest_income: Option<bool>,
    #[serde(with = "decimal_out::option")]
    income_growth: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    income_yield: Option<Decimal>,
    #[serde(with = "decimal_out::option")]
    scenario_spread: Option<Decimal>,
}

impl From<&ObjectivePremises> for PremisesDto {
    fn from(p: &ObjectivePremises) -> Self {
        Self {
            monthly_contribution: p.monthly_contribution,
            real_return: p.real_return,
            inflation: p.inflation,
            reinvest_income: p.reinvest_income,
            income_growth: p.income_growth,
            income_yield: p.income_yield,
            scenario_spread: p.scenario_spread,
        }
    }
}

/// As premissas que a conta usou, sempre expostas: sem elas a data não diz
/// nada. Taxas em % ao ano.
#[derive(Debug, Clone, Serialize)]
pub struct AssumptionsDto {
    /// Reais de hoje por mês.
    #[serde(with = "decimal_out")]
    monthly_contribution: Decimal,
    contribution_source: ContributionSource,
    #[serde(with = "decimal_out")]
    real_return_percent: Decimal,
    return_source: ReturnSource,
    #[serde(with = "decimal_out")]
    inflation_percent: Decimal,
    inflation_source: InflationSource,
    reinvest_income: bool,
    reinvest_source: PremiseSource,
    #[serde(with = "decimal_out")]
    income_growth_percent: Decimal,
    income_growth_source: PremiseSource,
    #[serde(with = "decimal_out")]
    income_yield_percent: Decimal,
    income_yield_source: YieldSource,
    #[serde(with = "decimal_out")]
    scenario_spread_percent: Decimal,
    spread_source: PremiseSource,
    /// O ponto de partida: o valor de hoje e a renda mensal estimada de hoje
    /// (da Renda; da classe, no objetivo de classe).
    #[serde(with = "decimal_out")]
    starting_value: Decimal,
    #[serde(with = "decimal_out")]
    starting_income: Decimal,
}

impl AssumptionsDto {
    fn premises(&self) -> Premises {
        Premises {
            monthly_contribution: to_f64(self.monthly_contribution),
            real_return: fraction(self.real_return_percent),
            inflation: fraction(self.inflation_percent),
            reinvest: self.reinvest_income,
            income_growth: fraction(self.income_growth_percent),
            income_yield: fraction(self.income_yield_percent),
            spread: fraction(self.scenario_spread_percent),
        }
    }

    fn start(&self) -> Start {
        Start {
            value: to_f64(self.starting_value),
            monthly_income: to_f64(self.starting_income),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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

/// A data de um cenário, com a meta lida dos dois jeitos.
#[derive(Debug, Serialize)]
pub struct ScenarioDto {
    /// A meta como foi digitada, em reais da época.
    nominal: ProjectionDto,
    /// A meta com o poder de compra de hoje.
    today: ProjectionDto,
}

#[derive(Debug, Serialize)]
pub struct ScenariosDto {
    pessimistic: ScenarioDto,
    base: ScenarioDto,
    optimistic: ScenarioDto,
}

#[derive(Debug, Serialize)]
pub struct CurvePointDto {
    /// Meses a partir de hoje.
    months: u32,
    /// `AAAA-MM`.
    month: String,
    /// Em reais da época.
    #[serde(with = "decimal_out")]
    low: Decimal,
    #[serde(with = "decimal_out")]
    base: Decimal,
    #[serde(with = "decimal_out")]
    high: Decimal,
    /// Em reais de hoje.
    #[serde(with = "decimal_out")]
    low_today: Decimal,
    #[serde(with = "decimal_out")]
    base_today: Decimal,
    #[serde(with = "decimal_out")]
    high_today: Decimal,
}

#[derive(Debug, Serialize)]
pub struct CurveDto {
    /// `value`, `income_average` ou `income_floor`: o que a curva mede.
    measure: &'static str,
    points: Vec<CurvePointDto>,
}

#[derive(Debug, Serialize)]
pub struct ObjectiveDto {
    id: Uuid,
    kind: ObjectiveKind,
    asset_class: Option<AssetClass>,
    #[serde(with = "decimal_out")]
    target: Decimal,
    /// Só na meta de renda: `average` ou `floor`.
    income_mode: Option<IncomeMode>,
    premises: PremisesDto,
    #[serde(with = "decimal_out")]
    current: Decimal,
    /// De 0 a 100 (na meta de renda pode passar de 100, como em Renda).
    #[serde(with = "decimal_out")]
    percent: Decimal,
    /// Só na meta de renda: o número vem do recebido ou da estimativa.
    basis: Option<ProgressBasis>,
    /// `active` ou `achieved`.
    status: &'static str,
    #[serde(with = "time::serde::rfc3339::option")]
    achieved_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    /// `null` num objetivo concluído: já não há conta a fazer.
    assumptions: Option<AssumptionsDto>,
    /// O cenário base, com a meta em reais da época.
    projection: ProjectionDto,
    scenarios: Option<ScenariosDto>,
    curve: Option<CurveDto>,
}

#[derive(Debug, Serialize)]
pub struct FocusDto {
    /// A data da pesquisa das medianas guardadas.
    #[serde(with = "iso_date")]
    survey_date: Date,
    #[serde(with = "time::serde::rfc3339::option")]
    fetched_at: Option<OffsetDateTime>,
}

time::serde::format_description!(iso_date, Date, "[year]-[month]-[day]");

#[derive(Debug, Serialize)]
pub struct DefaultsDto {
    #[serde(with = "decimal_out")]
    real_return_percent: Decimal,
    return_source: ReturnSource,
    #[serde(with = "decimal_out")]
    inflation_percent: Decimal,
    inflation_source: InflationSource,
    /// Os anos do Focus que entram na média da inflação.
    inflation_years: Option<[i32; 2]>,
    reinvest_income: bool,
    #[serde(with = "decimal_out")]
    income_growth_percent: Decimal,
    /// O rendimento líquido da carteira de hoje.
    #[serde(with = "decimal_out")]
    income_yield_percent: Decimal,
    #[serde(with = "decimal_out")]
    scenario_spread_percent: Decimal,
    focus: Option<FocusDto>,
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
    /// A data (cenário base, meta em reais da época) contada a partir do valor
    /// com este aporte.
    projection: ProjectionDto,
}

#[derive(Debug, Serialize)]
pub struct ImpactDto {
    objectives: Vec<ImpactEntryDto>,
}

// ------------------------------------------------------------- a conta -----

/// A premissa de inflação padrão e de rentabilidade padrão, como o
/// `Evaluator` as acha.
#[derive(Debug, Clone, Copy)]
struct Defaults {
    real_return: Decimal,
    return_source: ReturnSource,
    inflation: Decimal,
    inflation_source: InflationSource,
    inflation_years: Option<(i32, i32)>,
}

/// Tudo o que uma resposta precisa para avaliar objetivos — a carteira de
/// agora, a estimativa de renda, o Focus, as premissas padrão e a média de
/// aporte por classe —, cada coisa buscada uma vez só.
struct Evaluator<'a> {
    state: &'a AppState,
    user: Uuid,
    now: OffsetDateTime,
    today: Date,
    view: PortfolioView,
    income: PortfolioIncome,
    outlook: Outlook,
    defaults: Option<Defaults>,
    averages: HashMap<Option<AssetClass>, Decimal>,
    entries: Option<Vec<crate::domain::income::Entry>>,
}

impl<'a> Evaluator<'a> {
    async fn new(state: &'a AppState, user: Uuid) -> ApiResult<Self> {
        let now = OffsetDateTime::now_utc();
        let today = today_in_brazil(now);
        let view = service::portfolio(&state.store, user).await?;
        let income = state.income.estimate(&view, today).await;
        let outlook = Outlook::load(&state.store, today).await?;
        Ok(Self {
            state,
            user,
            now,
            today,
            view,
            income,
            outlook,
            defaults: None,
            averages: HashMap::new(),
            entries: None,
        })
    }

    /// A inflação e a rentabilidade real de quem não informou as suas.
    ///
    /// Rentabilidade: a do Desempenho (cota da carteira, 12 meses, anualizada,
    /// com pelo menos 6 meses de série) descontado o IPCA do mesmo período;
    /// senão o CDI de hoje líquido de IR descontada a inflação padrão; senão o
    /// padrão do Prumo.
    async fn defaults(&mut self) -> ApiResult<Defaults> {
        if let Some(found) = self.defaults {
            return Ok(found);
        }
        let inflation = self.outlook.default_inflation(self.today);
        let expected = fraction(inflation.percent);
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
        let measured =
            period_return(&points, months_before(self.today, 12), self.today).and_then(|w| {
                let annual = annualize(w.ret, (w.to - w.from).whole_days())?;
                let ipca = self.outlook.ipca_between(w.from, w.to).unwrap_or(expected);
                deflate(annual, to_decimal(ipca, 6))
            });
        let (real, source) = if let Some(real) = measured {
            (real, ReturnSource::Performance)
        } else {
            match self
                .outlook
                .cdi_series
                .last()
                .and_then(|(_, rate)| cdi_net_annual(*rate))
                .and_then(|net| deflate(net, to_decimal(expected, 6)))
            {
                Some(real) => (real, ReturnSource::CdiNet),
                None => (FALLBACK_REAL_RETURN, ReturnSource::Fallback),
            }
        };
        let found = Defaults {
            real_return: (real * Decimal::ONE_HUNDRED).round_dp(2),
            return_source: source,
            inflation: inflation.percent,
            inflation_source: inflation.source,
            inflation_years: inflation.years,
        };
        self.defaults = Some(found);
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

    async fn entries(&mut self) -> ApiResult<&[crate::domain::income::Entry]> {
        if self.entries.is_none() {
            let ledger = self.state.store.income_ledger(self.user).await?;
            self.entries = Some(ledger.iter().map(IncomeEvent::entry).collect());
        }
        Ok(self.entries.as_deref().unwrap_or(&[]))
    }

    /// O valor e a renda de hoje do que o objetivo olha: a classe ou a
    /// carteira.
    fn scope(&self, objective: &Objective) -> (Decimal, Decimal) {
        let class = match objective.kind {
            ObjectiveKind::ClassValue => objective.asset_class,
            _ => None,
        };
        let value = class.map_or(self.view.total_value, |c| class_value(&self.view, c));
        (value, scope_income(&self.view, &self.income, class))
    }

    /// O rendimento líquido de hoje (em % ao ano) do que o objetivo olha. A
    /// classe ainda vazia usa o da carteira.
    fn measured_yield(&self, objective: &Objective) -> Decimal {
        let (value, income) = self.scope(objective);
        net_yield_percent(income, value)
            .or_else(|| net_yield_percent(self.income.monthly_total, self.view.total_value))
            .unwrap_or(Decimal::ZERO)
    }

    async fn assumptions(&mut self, objective: &Objective) -> ApiResult<AssumptionsDto> {
        let p = objective.premises;
        let class = match objective.kind {
            ObjectiveKind::ClassValue => objective.asset_class,
            _ => None,
        };
        let (monthly_contribution, contribution_source) = match p.monthly_contribution {
            Some(informed) => (informed, ContributionSource::Informed),
            None => (self.average(class).await?, ContributionSource::Ledger),
        };
        let defaults = self.defaults().await?;
        let (real_return_percent, return_source) = match p.real_return {
            Some(informed) => (informed, ReturnSource::Informed),
            None => (defaults.real_return, defaults.return_source),
        };
        let (inflation_percent, inflation_source) = match p.inflation {
            Some(informed) => (informed, InflationSource::Informed),
            None => (defaults.inflation, defaults.inflation_source),
        };
        let informed = |v: bool| {
            if v {
                PremiseSource::Informed
            } else {
                PremiseSource::Default
            }
        };
        let (income_yield_percent, income_yield_source) = match p.income_yield {
            Some(informed) => (informed, YieldSource::Informed),
            None => (self.measured_yield(objective), YieldSource::Measured),
        };
        let (starting_value, starting_income) = self.scope(objective);
        Ok(AssumptionsDto {
            monthly_contribution,
            contribution_source,
            real_return_percent,
            return_source,
            inflation_percent,
            inflation_source,
            reinvest_income: p.reinvest_income.unwrap_or(true),
            reinvest_source: informed(p.reinvest_income.is_some()),
            income_growth_percent: p.income_growth.unwrap_or(Decimal::ZERO),
            income_growth_source: informed(p.income_growth.is_some()),
            income_yield_percent,
            income_yield_source,
            scenario_spread_percent: p.scenario_spread.unwrap_or(DEFAULT_SPREAD_PERCENT),
            spread_source: informed(p.scenario_spread.is_some()),
            starting_value,
            starting_income,
        })
    }

    /// O que a simulação mede para o objetivo.
    fn measure(&self, objective: &Objective) -> Measure {
        match (objective.kind, objective.income_mode.unwrap_or_default()) {
            (ObjectiveKind::MonthlyIncome, IncomeMode::Average) => Measure::IncomeAverage,
            (ObjectiveKind::MonthlyIncome, IncomeMode::Floor) => Measure::IncomeFloor {
                shape: portfolio_shape(&self.income),
                start: YearMonth::of(self.today),
            },
            _ => Measure::Value,
        }
    }

    /// O valor de hoje, o percentual, a base (meta de renda) e se chegou.
    async fn progress(
        &mut self,
        objective: &Objective,
    ) -> ApiResult<(Decimal, Decimal, Option<ProgressBasis>, bool)> {
        if objective.kind == ObjectiveKind::MonthlyIncome {
            let today = self.today;
            let received = received_income(self.entries().await?, today);
            let estimated = if needs_estimate(objective, received) {
                estimated_income(&self.income)
            } else {
                Default::default()
            };
            let progress = income_progress(objective, received, estimated);
            return Ok((
                progress.current_monthly,
                progress.percent,
                Some(progress.basis),
                progress.reached,
            ));
        }
        let current = current_value(&self.view, objective).ok_or_else(ApiError::internal)?;
        Ok((
            current,
            percent_done(current, objective.target),
            None,
            is_reached(current, objective.target),
        ))
    }

    /// O objetivo com progresso, premissas, as datas dos três cenários e a
    /// curva. Se chegou à meta pela primeira vez, a data fica registrada: meta
    /// atingida não volta a ativa quando o valor cai.
    async fn evaluate(&mut self, mut objective: Objective) -> ApiResult<ObjectiveDto> {
        let (current, percent, basis, reached) = self.progress(&objective).await?;
        if objective.achieved_at.is_none() && reached {
            objective.achieved_at = self
                .state
                .store
                .mark_objective_achieved(self.user, objective.id, self.now)
                .await?;
        }
        let achieved = objective.achieved_at.is_some();
        let mut dto = ObjectiveDto {
            id: objective.id,
            kind: objective.kind,
            asset_class: objective.asset_class,
            target: objective.target,
            income_mode: (objective.kind == ObjectiveKind::MonthlyIncome)
                .then(|| objective.income_mode.unwrap_or_default()),
            premises: PremisesDto::from(&objective.premises),
            current,
            percent: if achieved && basis.is_none() {
                Decimal::ONE_HUNDRED
            } else {
                percent
            },
            basis,
            status: if achieved { "achieved" } else { "active" },
            achieved_at: objective.achieved_at,
            created_at: objective.created_at,
            assumptions: None,
            projection: ProjectionDto::new(Eta::Reached, self.today),
            scenarios: None,
            curve: None,
        };
        if achieved {
            return Ok(dto);
        }
        let assumptions = self.assumptions(&objective).await?;
        let measure = self.measure(&objective);
        let premises = assumptions.premises();
        let start = assumptions.start();
        let target = to_f64(objective.target);
        let eta = |scenario, basis| months_to(start, target, measure, &premises, scenario, basis);
        let pair = |scenario| ScenarioDto {
            nominal: ProjectionDto::new(eta(scenario, TargetBasis::Nominal), self.today),
            today: ProjectionDto::new(eta(scenario, TargetBasis::Today), self.today),
        };
        let horizon = chart_horizon(&[
            eta(Scenario::Base, TargetBasis::Nominal),
            eta(Scenario::Base, TargetBasis::Today),
            eta(Scenario::Pessimistic, TargetBasis::Nominal),
        ]);
        let month_zero = YearMonth::of(self.today);
        let points = curve(start, measure, &premises, horizon, chart_step(horizon))
            .into_iter()
            .map(|p| CurvePointDto {
                months: p.month,
                month: month_zero
                    .shift(i32::try_from(p.month).unwrap_or(0))
                    .label(),
                low: to_decimal(p.nominal[0], 2),
                base: to_decimal(p.nominal[1], 2),
                high: to_decimal(p.nominal[2], 2),
                low_today: to_decimal(p.today[0], 2),
                base_today: to_decimal(p.today[1], 2),
                high_today: to_decimal(p.today[2], 2),
            })
            .collect();
        dto.projection = ProjectionDto::new(eta(Scenario::Base, TargetBasis::Nominal), self.today);
        dto.scenarios = Some(ScenariosDto {
            pessimistic: pair(Scenario::Pessimistic),
            base: pair(Scenario::Base),
            optimistic: pair(Scenario::Optimistic),
        });
        dto.curve = Some(CurveDto {
            measure: match measure {
                Measure::Value => "value",
                Measure::IncomeAverage => "income_average",
                Measure::IncomeFloor { .. } => "income_floor",
            },
            points,
        });
        dto.assumptions = Some(assumptions);
        Ok(dto)
    }

    async fn defaults_dto(&mut self) -> ApiResult<DefaultsDto> {
        let d = self.defaults().await?;
        Ok(DefaultsDto {
            real_return_percent: d.real_return,
            return_source: d.return_source,
            inflation_percent: d.inflation,
            inflation_source: d.inflation_source,
            inflation_years: d.inflation_years.map(|(a, b)| [a, b]),
            reinvest_income: true,
            income_growth_percent: Decimal::ZERO,
            income_yield_percent: net_yield_percent(
                self.income.monthly_total,
                self.view.total_value,
            )
            .unwrap_or(Decimal::ZERO),
            scenario_spread_percent: DEFAULT_SPREAD_PERCENT,
            focus: self
                .outlook
                .focus
                .survey_date()
                .map(|survey_date| FocusDto {
                    survey_date,
                    fetched_at: self.outlook.fetched_at,
                }),
        })
    }
}

// --------------------------------------------------------------- handlers --

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<ObjectivesDto>> {
    let stored = state.store.objectives(user.id).await?;
    let mut evaluator = Evaluator::new(&state, user.id).await?;
    let defaults = evaluator.defaults_dto().await?;
    let mut objectives = Vec::with_capacity(stored.len());
    for objective in stored {
        objectives.push(evaluator.evaluate(objective).await?);
    }
    Ok(Json(ObjectivesDto {
        defaults,
        objectives,
    }))
}

fn premises_of(body: &CreateObjectiveRequest) -> ApiResult<ObjectivePremises> {
    Ok(ObjectivePremises {
        monthly_contribution: optional(body.monthly_contribution, contribution_input)?,
        real_return: optional(body.real_return, |v| rate_input(&REAL_RETURN, v))?,
        inflation: optional(body.inflation, |v| rate_input(&INFLATION, v))?,
        reinvest_income: body.reinvest_income,
        income_growth: optional(body.income_growth, |v| rate_input(&INCOME_GROWTH, v))?,
        income_yield: optional(body.income_yield, |v| rate_input(&INCOME_YIELD, v))?,
        scenario_spread: optional(body.scenario_spread, |v| rate_input(&SPREAD, v))?,
    })
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    ApiJson(body): ApiJson<CreateObjectiveRequest>,
) -> ApiResult<impl IntoResponse> {
    let asset_class = match (body.kind, body.asset_class) {
        (ObjectiveKind::ClassValue, Some(class)) => Some(class),
        (ObjectiveKind::ClassValue, None) => {
            return Err(ApiError::validation(
                "O objetivo por classe precisa da asset_class.",
            ))
        }
        (_, None) => None,
        (_, Some(_)) => {
            return Err(ApiError::validation(
                "So o objetivo por classe tem asset_class.",
            ))
        }
    };
    if body.income_mode.is_some() && body.kind != ObjectiveKind::MonthlyIncome {
        return Err(ApiError::validation(
            "So a meta de renda mensal tem income_mode.",
        ));
    }
    let new = NewObjective {
        kind: body.kind,
        asset_class,
        target: positive_money("A meta", body.target)?,
        premises: premises_of(&body)?,
        income_mode: body.income_mode,
    };
    let objective = match state.store.insert_objective(user.id, &new).await {
        Ok(objective) => objective,
        Err(StoreError::Duplicate) => {
            return Err(ApiError::validation(
                "Ja existe uma meta de renda mensal. Edite a que existe.",
            ))
        }
        Err(error) => return Err(error.into()),
    };
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
        monthly_contribution: patched(body.monthly_contribution, contribution_input)?,
        real_return: patched(body.real_return, |v| rate_input(&REAL_RETURN, v))?,
        inflation: patched(body.inflation, |v| rate_input(&INFLATION, v))?,
        reinvest_income: body.reinvest_income,
        income_growth: patched(body.income_growth, |v| rate_input(&INCOME_GROWTH, v))?,
        income_yield: patched(body.income_yield, |v| rate_input(&INCOME_YIELD, v))?,
        scenario_spread: patched(body.scenario_spread, |v| rate_input(&SPREAD, v))?,
        income_mode: body.income_mode,
    };
    if changes.income_mode.is_some() {
        let stored = state.store.objectives(user.id).await?;
        match stored.iter().find(|o| o.id == id) {
            None => return Err(ApiError::not_found()),
            Some(o) if o.kind != ObjectiveKind::MonthlyIncome => {
                return Err(ApiError::validation(
                    "So a meta de renda mensal tem income_mode.",
                ))
            }
            Some(_) => {}
        }
    }
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

/// O que um aporte faz com os objetivos de valor ativos: o percentual com ele
/// e a data estimada (cenário base, meta em reais da época) contada a partir
/// do valor que a carteira passa a ter. A tela do Aportar manda o que o plano
/// compra, por classe. A meta de renda não entra: a renda que o aporte soma já
/// está no plano (`income.added_monthly`).
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
    for objective in stored
        .into_iter()
        .filter(|o| o.achieved_at.is_none() && o.kind != ObjectiveKind::MonthlyIncome)
    {
        let current = current_value(&evaluator.view, &objective).ok_or_else(ApiError::internal)?;
        // Já está na meta (a lista ainda vai marcar): não há o que projetar.
        if is_reached(current, objective.target) {
            continue;
        }
        let gain = match objective.kind {
            ObjectiveKind::ClassValue => objective
                .asset_class
                .and_then(|class| added.get(&class).copied())
                .unwrap_or_default(),
            _ => total_added,
        };
        let after = current + gain;
        let assumptions = evaluator.assumptions(&objective).await?;
        let start = Start {
            value: to_f64(after),
            ..assumptions.start()
        };
        let eta = months_to(
            start,
            to_f64(objective.target),
            Measure::Value,
            &assumptions.premises(),
            Scenario::Base,
            TargetBasis::Nominal,
        );
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
