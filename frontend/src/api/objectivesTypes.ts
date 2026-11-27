// Espelho do JSON de `/objectives` (docs/api.md § Objetivos; método em docs/projecao.md).

import type { AssetClass } from "./types";

/** Patrimônio total, valor numa classe ou a meta de renda mensal (uma por pessoa). */
export type ObjectiveKind = "net_worth" | "class_value" | "monthly_income";

/** O que a meta de renda mede: a média por mês ou o menor mês de 12. */
export type IncomeMode = "average" | "floor";

/** De onde veio a rentabilidade real usada na conta. */
export type ReturnSource = "informed" | "performance" | "cdi_net" | "fallback";

/** De onde veio o aporte mensal usado na conta. */
export type ContributionSource = "informed" | "ledger";

/** De onde veio a inflação usada na conta. */
export type InflationSource = "informed" | "focus" | "ipca_12m" | "target";

/** Premissa sem outra fonte além do padrão. */
export type PremiseSource = "informed" | "default";

/** De onde veio o rendimento do dinheiro novo. */
export type YieldSource = "informed" | "measured";

/** O que a pessoa informou; `null` = o padrão. Taxas em % ao ano. */
export type ObjectivePremises = {
  monthly_contribution: number | null;
  real_return: number | null;
  inflation: number | null;
  reinvest_income: boolean | null;
  income_growth: number | null;
  income_yield: number | null;
  scenario_spread: number | null;
};

/** O que a conta usou, sempre à vista. Taxas em % ao ano. */
export type Assumptions = {
  /** Reais de hoje por mês. */
  monthly_contribution: number;
  contribution_source: ContributionSource;
  real_return_percent: number;
  return_source: ReturnSource;
  inflation_percent: number;
  inflation_source: InflationSource;
  reinvest_income: boolean;
  reinvest_source: PremiseSource;
  income_growth_percent: number;
  income_growth_source: PremiseSource;
  income_yield_percent: number;
  income_yield_source: YieldSource;
  scenario_spread_percent: number;
  spread_source: PremiseSource;
  /** O valor de hoje e a renda mensal estimada de hoje (da Renda). */
  starting_value: number;
  starting_income: number;
};

export type ProjectionState = "reached" | "eta" | "unreachable" | "beyond";

export type Projection = {
  state: ProjectionState;
  /** Meses até a meta, só em `eta`. */
  months: number | null;
  /** `AAAA-MM`, só em `eta`. */
  month: string | null;
};

/** A data de um cenário com a meta lida dos dois jeitos. */
export type ScenarioDates = {
  /** A meta como foi digitada, em reais da época. */
  nominal: Projection;
  /** A meta com o poder de compra de hoje. */
  today: Projection;
};

export type Scenarios = {
  pessimistic: ScenarioDates;
  base: ScenarioDates;
  optimistic: ScenarioDates;
};

export type CurvePoint = {
  months: number;
  /** `AAAA-MM`. */
  month: string;
  /** Em reais da época. */
  low: number;
  base: number;
  high: number;
  /** Em reais de hoje. */
  low_today: number;
  base_today: number;
  high_today: number;
};

export type Curve = {
  measure: "value" | "income_average" | "income_floor";
  points: CurvePoint[];
};

export type Objective = {
  id: string;
  kind: ObjectiveKind;
  asset_class: AssetClass | null;
  target: number;
  /** Só na meta de renda. */
  income_mode: IncomeMode | null;
  premises: ObjectivePremises;
  current: number;
  /** De 0 a 100 (a meta de renda pode passar de 100). */
  percent: number;
  /** Só na meta de renda: o número vem do recebido ou da estimativa. */
  basis: "received" | "estimate" | null;
  status: "active" | "achieved";
  achieved_at: string | null;
  created_at: string;
  /** `null` num objetivo concluído. */
  assumptions: Assumptions | null;
  /** O cenário base, meta em reais da época. */
  projection: Projection;
  scenarios: Scenarios | null;
  curve: Curve | null;
};

export type ObjectivesDefaults = {
  real_return_percent: number;
  return_source: ReturnSource;
  inflation_percent: number;
  inflation_source: InflationSource;
  /** Os anos do Focus na média da inflação. */
  inflation_years: [number, number] | null;
  reinvest_income: boolean;
  income_growth_percent: number;
  /** O rendimento líquido da carteira de hoje. */
  income_yield_percent: number;
  scenario_spread_percent: number;
  /** A pesquisa do Focus que está no banco; `null` sem Focus. */
  focus: { survey_date: string; fetched_at: string | null } | null;
};

export type ObjectivesResponse = {
  defaults: ObjectivesDefaults;
  objectives: Objective[];
};

type PremiseFields = {
  monthly_contribution?: number;
  real_return?: number;
  inflation?: number;
  reinvest_income?: boolean;
  income_growth?: number;
  income_yield?: number;
  scenario_spread?: number;
};

export type CreateObjectiveRequest = PremiseFields & {
  kind: ObjectiveKind;
  asset_class?: AssetClass;
  target: number;
  income_mode?: IncomeMode;
};

/** `null` apaga a premissa (volta ao padrão). */
export type UpdateObjectiveRequest = {
  target?: number;
  income_mode?: IncomeMode | null;
  monthly_contribution?: number | null;
  real_return?: number | null;
  inflation?: number | null;
  reinvest_income?: boolean | null;
  income_growth?: number | null;
  income_yield?: number | null;
  scenario_spread?: number | null;
};

export type ImpactItem = { asset_class: AssetClass; value: number };

export type ImpactEntry = {
  id: string;
  kind: ObjectiveKind;
  asset_class: AssetClass | null;
  target: number;
  current: number;
  /** O valor com este aporte. */
  after: number;
  percent_before: number;
  percent_after: number;
  assumptions: Assumptions;
  /** A data (cenário base) contada a partir do valor com este aporte. */
  projection: Projection;
};

export type ImpactResponse = { objectives: ImpactEntry[] };
