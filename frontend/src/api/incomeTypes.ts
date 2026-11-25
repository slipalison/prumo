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

/** Por que não há percentual de yield on cost. */
export type YieldGap = "unknown_cost" | "zero_cost" | "no_position";

/** Yield on cost de um ativo ou pagador: renda de 12 meses ÷ custo. */
export type YieldOnCost = {
  /** A renda usada: a recebida (líquida) ou, sem recebido, a estimada (bruta). */
  income_12m: number;
  basis: "received" | "estimate";
  /** `null`: custo desconhecido. */
  cost: number | null;
  /** De 0 a 100; `null` com o motivo em `gap`. */
  percent: number | null;
  gap: YieldGap | null;
  /** Ex.: `informe o preco medio`. */
  message: string | null;
};

export type PortfolioYield = Omit<YieldOnCost, "cost"> & {
  cost: number;
  /** Algum ativo com posição ficou de fora por custo desconhecido. */
  partial: boolean;
  unknown_cost: string[];
};

export type AssetYield = YieldOnCost & { asset_id: string };

export type YieldReport = { portfolio: PortfolioYield; assets: AssetYield[] };

export type IncomePayer = {
  asset_id: string | null;
  label: string;
  total: number;
  percent: number;
  yield_on_cost: YieldOnCost;
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
  /** O que a posição teria rendido em 12 meses (bruto), com as cotas da data com de cada dividendo. */
  trailing_12m: number | null;
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
  yield_on_cost: PortfolioYield;
  goal: IncomeGoal | null;
};

/** `GET /income/entitlement`: as cotas do livro na data com. */
export type Entitlement = {
  quantity: number;
  /** `AAAA-MM-DD`. */
  record_date: string;
};
