// Espelho do JSON de proventos e renda de docs/api.md (§ Renda).

export const INCOME_KINDS = ["dividend", "jcp", "reit_income", "amortization", "other"] as const;
export type IncomeKind = (typeof INCOME_KINDS)[number];

/** `b3_announcement`: o provento automático, calculado do anúncio da B3 e do livro. */
export type IncomeSource = "manual" | "estimate" | "b3_import" | "b3_announcement";
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

/** De onde veio o valor por cota: os anúncios da B3 (JCP separado) ou o Yahoo (JCP junto; nos EUA, menos os 30% retidos lá). */
export type IncomeEstimateSource = "b3" | "yahoo";

/** A média anual do que o ativo costuma pagar (até 5 anos de histórico). */
export type IncomeUsual = {
  /** Quantos anos inteiros entraram (2 a 5). */
  years: number;
  /** Por cota, por ano, líquido (aproximado: usa a parte de JCP dos 12 meses). */
  per_unit_year: number;
  per_unit_year_gross: number;
  /** Com a quantidade de hoje, por mês, líquido. */
  monthly: number;
  monthly_gross: number;
};

export type IncomeEstimateAsset = {
  asset_id: string;
  symbol: string | null;
  display_name: string;
  quantity: number;
  source: IncomeEstimateSource;
  /** O que a cota pagou em 12 meses, líquido do IR retido na fonte (JCP aqui, 30% nos EUA). */
  per_unit_12m: number;
  per_unit_12m_gross: number;
  /** Por mês com a quantidade de hoje, líquido. */
  monthly: number;
  monthly_gross: number;
  /** `null` com menos de 2 anos de histórico. */
  usual: IncomeUsual | null;
  /** Os 12 meses passam de 1,5 × a média do habitual. */
  above_usual: boolean;
  /** O que a posição teria rendido em 12 meses, com as cotas da data com de cada provento (líquido pela B3 e nos EUA; bruto numa ação da B3 pelo Yahoo). */
  trailing_12m: number | null;
};

/** A renda mensal estimada pelo que cada cota pagou — nunca é extrato. */
export type IncomeEstimate = {
  /** Líquido do IR retido na fonte (JCP aqui, 30% nos EUA). */
  monthly: number;
  monthly_gross: number;
  /** A parte de `monthly` que vem de ativos acima do habitual. */
  above_usual_monthly: number;
  updated_at: string | null;
  assets: IncomeEstimateAsset[];
  missing: string[];
};

export type IncomeGoal = {
  id: string;
  target: number;
  /** `average` (a média por mês) ou `floor` (o menor mês de 12). */
  mode: "average" | "floor";
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
