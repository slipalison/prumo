// Espelho do JSON de proventos e renda de docs/api.md (§ Renda).

export const INCOME_KINDS = ["dividend", "jcp", "reit_income", "amortization", "other"] as const;
export type IncomeKind = (typeof INCOME_KINDS)[number];

export type IncomeSource = "manual" | "estimate" | "b3_import";
export type IncomeStatus = "received" | "upcoming";

export type IncomeEvent = {
  id: string;
  asset_id: string | null;
  symbol: string | null;
  display_name: string;
  kind: IncomeKind;
  /** `AAAA-MM-DD`. */
  ex_date: string | null;
  /** `AAAA-MM-DD`. */
  pay_date: string;
  amount_per_unit: number;
  quantity: number;
  gross: number;
  withholding: number;
  net: number;
  status: IncomeStatus;
  source: IncomeSource;
  external_ref: string | null;
};

export type IncomeFilters = {
  from?: string;
  to?: string;
  status?: IncomeStatus;
  asset?: string;
  limit?: number;
};

export type CreateIncomeRequest = {
  asset_id: string;
  kind: IncomeKind;
  ex_date?: string;
  pay_date: string;
  amount_per_unit: number;
  quantity?: number;
  withholding?: number;
};

export type IncomeMonth = {
  /** `AAAA-MM`. */
  month: string;
  received: number;
  upcoming: number;
};

export type IncomePayer = {
  asset_id: string | null;
  label: string;
  total: number;
  percent: number;
};

export type IncomeYear = {
  year: number;
  /** Janeiro a dezembro: líquido recebido e a receber. */
  months: number[];
  total: number;
};

export type IncomeEstimateAsset = {
  asset_id: string;
  symbol: string | null;
  display_name: string;
  quantity: number;
  per_unit_12m: number;
  monthly: number;
};

/** A renda mensal estimada pelo histórico por cota — nunca é extrato. */
export type IncomeEstimate = {
  monthly: number;
  updated_at: string | null;
  assets: IncomeEstimateAsset[];
  missing: string[];
};

export type IncomeGoal = {
  id: string;
  target: number;
  achieved_at: string | null;
  current_monthly: number;
  /** De onde vem `current_monthly`: o recebido ou, sem nada recebido, a estimativa. */
  basis: "received" | "estimate";
  percent: number;
  reached: boolean;
};

export type IncomeSummary = {
  today: string;
  total_12m: number;
  monthly_average: number;
  upcoming_total: number;
  months: IncomeMonth[];
  by_payer: IncomePayer[];
  years: IncomeYear[];
  upcoming: IncomeEvent[];
  estimate: IncomeEstimate;
  goal: IncomeGoal | null;
};

export type CashSuggestion = {
  amount: number;
  count: number;
  /** O dia do último aporte; `null` sem aporte registrado. */
  since: string | null;
};
