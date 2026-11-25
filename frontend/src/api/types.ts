// Espelho exato do JSON de docs/api.md. Campo novo no contrato entra aqui
// primeiro; o compilador aponta o resto.

export const ASSET_CLASSES = [
  "br_stocks",
  "br_reits",
  "intl_stocks",
  "us_reits",
  "crypto",
  "fixed_income",
  "intl_fixed_income",
] as const;

export type AssetClass = (typeof ASSET_CLASSES)[number];

export type ChecklistKind = "stocks" | "real_estate";

export type ClassSummary = {
  asset_class: AssetClass;
  target_percent: number;
  current_value: number;
  current_percent: number;
  asset_count: number;
  eligible: boolean;
};

export type PriceSource = "quote" | "manual" | "missing";
export type ScoreSource = "checklist" | "manual" | "none";

export type Asset = {
  id: string;
  asset_class: AssetClass;
  symbol: string | null;
  display_name: string;
  /**
   * Negociado na B3 (ETF, BDR) — cotado em reais, cota inteira. Sempre `true`
   * em Ações Nacionais e FIIs.
   */
  listed_on_b3: boolean;
  /**
   * Título do Tesouro Direto: `symbol` é a chave do título (`TD-IPCA-20350515`),
   * a quantidade é fracionária e o preço de resgate vem da tabela do Tesouro.
   */
  treasury: boolean;
  quantity: number;
  /** Preço efetivo em reais (cotação ou manual). */
  unit_price: number | null;
  /** O preço manual gravado, se houver — vence a cotação. */
  manual_price: number | null;
  /**
   * Teto de preço da pessoa, em reais por unidade: acima dele o ativo fica fora
   * do aporte. `null` = sem teto.
   */
  max_price: number | null;
  currency: string;
  price_source: PriceSource;
  quote_updated_at: string | null;
  current_value: number;
  class_percent: number;
  /**
   * A fatia que o motor mira para o ativo dentro da classe (nota ÷ soma das
   * notas dos que recebem aporte), de 0 a 100. Zero para quem não recebe
   * (sem nota ou sem preço).
   */
  ideal_class_percent: number;
  score: number;
  score_source: ScoreSource;
  score_override: number | null;
  answers: Record<string, boolean>;
  /**
   * Setor, subsetor e segmento da fonte pública (B3 nas ações, CVM nos FIIs).
   * Nulos onde não há fonte (EUA, cripto, renda fixa) ou enquanto ela não respondeu.
   */
  sector: string | null;
  subsector: string | null;
  segment: string | null;
  /**
   * Do livro de lançamentos, em reais. `null` = desconhecido (saldo inicial
   * sem preço), diferente de zero. Renda fixa por valor não tem preço médio.
   */
  average_price: number | null;
  cost_basis: number | null;
  unrealized_gain: number | null;
  realized_gain: number | null;
  /** `unrealized_gain / cost_basis`, de 0 a 100 (pode passar de 100 ou ser negativo). */
  return_percent: number | null;
};

export type ChecklistQuestion = {
  id: string;
  position: number;
  prompt: string;
};

export type Checklists = Record<ChecklistKind, ChecklistQuestion[]>;

export type Portfolio = {
  total_value: number;
  quotes_updated_at: string | null;
  classes: ClassSummary[];
  assets: Asset[];
  checklists: Checklists;
};

/** Um ativo no evento `prices` do stream: só o que muda com o preço. */
export type LiveAsset = Pick<
  Asset,
  | "id"
  | "unit_price"
  | "currency"
  | "price_source"
  | "quote_updated_at"
  | "current_value"
  | "class_percent"
  | "unrealized_gain"
  | "return_percent"
  | "ideal_class_percent"
>;

/** Evento `prices` de `GET /portfolio/stream` (docs/api.md § Carteira ao vivo). */
export type LivePrices = {
  total_value: number;
  quotes_updated_at: string | null;
  classes: ClassSummary[];
  assets: LiveAsset[];
};

export type GoalTargets = Record<AssetClass, number>;

export type GoalsRequest = { targets: Partial<GoalTargets> };
export type GoalsResponse = { targets: GoalTargets };

export type CreateAssetRequest = {
  asset_class: AssetClass;
  symbol?: string;
  display_name?: string;
  quantity?: number;
  amount?: number;
  manual_price?: number | null;
  score_override?: number | null;
  listed_on_b3?: boolean;
  /** Título do Tesouro Direto: `symbol` é a chave que `GET /symbols` devolve. */
  treasury?: boolean;
  max_price?: number | null;
  /** Preço médio de entrada, em reais (por unidade). Sem ele, custo desconhecido. */
  average_price?: number;
  /** Renda fixa por valor: quanto foi aplicado. */
  invested?: number;
};

export type UpdateAssetRequest = {
  quantity?: number;
  amount?: number;
  display_name?: string;
  manual_price?: number | null;
  score_override?: number | null;
  answers?: Record<string, boolean | null>;
  listed_on_b3?: boolean;
  max_price?: number | null;
};

export type PlanItem = {
  asset_id: string;
  asset_class: AssetClass;
  symbol: string | null;
  display_name: string;
  listed_on_b3: boolean;
  treasury: boolean;
  quantity: number;
  unit_price: number;
  value: number;
  divisible: boolean;
  class_percent_after: number;
  /** Renda mensal que este item soma, estimada pelo histórico por cota; `null` sem histórico. */
  estimated_monthly_income: number | null;
};

export type PlanClassStatus =
  | "receiving"
  | "below_target"
  | "on_target"
  | "above_target"
  | "blocked"
  | "no_target";

export type PlanClass = {
  asset_class: AssetClass;
  target_percent: number;
  percent_before: number;
  percent_after: number;
  allocated: number;
  status: PlanClassStatus;
};

/**
 * Por que a classe abaixo da meta não recebe: sem ativo, sem nota, sem preço,
 * acima do teto de preço ou com os ativos todos no limite de concentração.
 */
export type BlockReason = "no_assets" | "unrated" | "unpriced" | "above_ceiling" | "concentration_limit";

export type PlanNotice =
  | { code: "class_blocked"; asset_class: AssetClass; amount: number; reason: BlockReason }
  | { code: "assets_without_price"; symbols: string[] }
  | { code: "assets_without_score"; symbols: string[] }
  | { code: "no_targets" }
  | { code: "asset_above_ceiling"; asset_id: string; symbol: string; unit_price: number; max_price: number }
  | { code: "concentration_limit"; max_percent: number; asset_ids: string[]; symbols: string[]; amount: number };

/** A renda mensal estimada da carteira antes e depois do aporte (estimativa, não extrato). */
export type PlanIncome = {
  /** `false` quando nenhum ativo tem histórico em cache ainda (o plano não espera o provedor). */
  available: boolean;
  /** Ativos que pagam provento e ficaram sem estimativa. */
  missing: string[];
  added_monthly: number;
  before_monthly: number;
  after_monthly: number;
};

export type Plan = {
  amount: number;
  invested: number;
  change: number;
  unallocated: number;
  total_before: number;
  total_after: number;
  items: PlanItem[];
  classes: PlanClass[];
  notices: PlanNotice[];
  /** `null` quando nenhum ativo da carteira paga provento (só renda fixa e cripto). */
  income: PlanIncome | null;
};

export type ContributionItemRequest = {
  asset_id: string;
  quantity: number;
  unit_price: number;
};

export type ContributionRequest = {
  amount: number;
  items: ContributionItemRequest[];
};

export type ContributionItem = {
  asset_id: string | null;
  symbol: string | null;
  display_name: string;
  quantity: number;
  unit_price: number;
  value: number;
};

export type Contribution = {
  id: string;
  created_at: string;
  amount: number;
  invested: number;
  items: ContributionItem[];
};

export type SymbolSuggestion = {
  symbol: string;
  display_name: string;
  exchange: string;
  suggested_class: AssetClass;
  listed_on_b3: boolean;
  /** Título do Tesouro Direto: cadastre com `treasury: true`. */
  treasury: boolean;
};

export type QuotesRefreshResult = {
  refreshed: number;
  failed: string[];
};

export type Info = { version: string; commit: string };
export type Me = { email: string };

export type Problem = {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  code?: string;
};
