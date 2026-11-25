// Espelho do JSON de `/objectives` (docs/api.md § Objetivos).

import type { AssetClass } from "./types";

/** A meta de renda mensal não está aqui: ela mora em Renda (`/income/goal`). */
export type ObjectiveKind = "net_worth" | "class_value";

/** De onde veio a rentabilidade usada na conta. */
export type ReturnSource = "informed" | "performance" | "cdi_net" | "fallback";

/** De onde veio o aporte mensal usado na conta. */
export type ContributionSource = "informed" | "ledger";

export type Assumptions = {
  /** Reais por mês. */
  monthly_contribution: number;
  contribution_source: ContributionSource;
  /** Em % ao ano. */
  annual_return_percent: number;
  return_source: ReturnSource;
};

export type ProjectionState = "reached" | "eta" | "unreachable" | "beyond";

export type Projection = {
  state: ProjectionState;
  /** Meses até a meta, só em `eta`. */
  months: number | null;
  /** `AAAA-MM`, só em `eta`. */
  month: string | null;
};

export type Objective = {
  id: string;
  kind: ObjectiveKind;
  asset_class: AssetClass | null;
  target: number;
  /** O que a pessoa informou; `null` = a média real do livro. */
  monthly_contribution: number | null;
  /** O que a pessoa informou, em % ao ano; `null` = o padrão. */
  expected_return: number | null;
  current: number;
  /** De 0 a 100. */
  percent: number;
  status: "active" | "achieved";
  achieved_at: string | null;
  created_at: string;
  /** `null` num objetivo concluído. */
  assumptions: Assumptions | null;
  projection: Projection;
};

export type ObjectivesResponse = {
  defaults: { annual_return_percent: number; return_source: ReturnSource };
  objectives: Objective[];
};

export type CreateObjectiveRequest = {
  kind: ObjectiveKind;
  asset_class?: AssetClass;
  target: number;
  monthly_contribution?: number;
  expected_return?: number;
};

/** `null` apaga a premissa (volta ao padrão). */
export type UpdateObjectiveRequest = {
  target?: number;
  monthly_contribution?: number | null;
  expected_return?: number | null;
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
  /** A data contada a partir do valor com este aporte. */
  projection: Projection;
};

export type ImpactResponse = { objectives: ImpactEntry[] };
