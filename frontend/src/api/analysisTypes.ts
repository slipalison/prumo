// Espelho do JSON de `GET /analysis` e `GET|PUT /analysis/rules` (docs/api.md § Análise).

import type { HealthWarning, RuleUnit, SectorSourceInfo } from "./healthTypes";
import type { AssetClass } from "./types";

export const ANALYSIS_RULE_IDS = [
  // Diversificação
  "max_asset",
  "top_five",
  "max_sector",
  "min_assets",
  "empty_class",
  "min_portfolio_assets",
  "min_classes",
  "min_sectors",
  "min_abroad",
  "min_low_risk",
  // Dividendos
  "min_yield",
  "min_paying_share",
  "income_growth",
  "income_every_month",
  "max_payer",
  "brick_reit",
  // Rentabilidade
  "beat_cdi_12m",
  "beat_cdi_5y",
  "beat_ibov_12m",
  "beat_ibov_5y",
  "beat_ipca_12m",
  "beat_ipca_5y",
  "min_positive_years",
  "max_volatility",
  "max_loss",
  // Qualidade dos ativos
  "quality_roe",
  "quality_profit",
  "quality_leverage",
  "quality_margin",
  "quality_cash",
  "quality_recovery",
  "quality_fii_pvp",
  "quality_fii_vacancy",
] as const;
export type AnalysisRuleId = (typeof ANALYSIS_RULE_IDS)[number];

export type DimensionId = "diversification" | "dividends" | "returns" | "quality";

/** O que é "bom" no número: em ou abaixo do limite, ou em ou acima. */
export type Direction = "at_most" | "at_least";

/**
 * - `off`: desligada pela pessoa.
 * - `no_data`: ligada, mas ainda não há dado para medir. Não é falha: fica fora da pontuação.
 */
export type RuleStatus = "off" | "no_data" | "ok" | "warning";

/** A regra como a pessoa a deixou. */
export type RuleConfig = {
  rule: AnalysisRuleId;
  dimension: DimensionId;
  enabled: boolean;
  /** Em que o número (e o limite) é medido. */
  unit: RuleUnit;
  direction: Direction | null;
  /** Nulo nas regras sem limite editável. */
  threshold: number | null;
  default_threshold: number | null;
};

export type Benchmark = "cdi" | "ibov" | "ipca";

export type RuleDetail =
  | { kind: "findings"; warnings: HealthWarning[] }
  | { kind: "classes"; asset_classes: AssetClass[] }
  | { kind: "sectors"; sectors: string[] }
  | { kind: "yield"; income_12m: number; base_value: number }
  | { kind: "paying"; paying: number; total: number; silent: string[] }
  | { kind: "income_growth"; current_12m: number; previous_12m: number }
  | { kind: "brick"; reits: number; brick: number }
  | {
      kind: "benchmark";
      benchmark: Benchmark;
      horizon: "12m" | "5y";
      from: string;
      to: string;
      portfolio_percent: number;
      benchmark_percent: number;
    }
  | { kind: "years"; positive: number; total: number }
  | { kind: "losses"; assets: Array<{ asset_id: string; label: string; asset_class: AssetClass; loss_percent: number }> }
  /** Regra de Qualidade: quantos ativos do grupo entraram na conta (os que têm o dado) e quais reprovaram. */
  | { kind: "quality"; covered: number; total: number; failing: string[] };

export type RuleResult = RuleConfig & {
  status: RuleStatus;
  /** O número de agora. */
  current: number | null;
  /** O que o número precisa alcançar: o limite da pessoa ou, contra índice, o que o índice rendeu. */
  limit: number | null;
  /** Aporte (R$) que traz o número de volta ao limite sem vender nada. */
  needed_contribution: number | null;
  detail: RuleDetail | null;
};

/** O percentual de regras ligadas, com dado, que estão ok. */
export type Score = {
  ok: number;
  /** Regras ligadas com dado (ok + ponto a ver). */
  measured: number;
  /** Ligadas e sem dado: ficam fora da conta. */
  no_data: number;
  percent: number | null;
};

export type Dimension = {
  id: DimensionId;
  score: Score;
  rules: RuleResult[];
};

export type RiskProfile = "conservative" | "moderate" | "aggressive";

export type Mix = { fixed_income: number; variable: number; profile: RiskProfile };

export type Mover = { asset_id: string; label: string; asset_class: AssetClass; change_percent: number };

export type Ranking = { best: Mover[]; worst: Mover[] };

export type Reference = {
  asset_id: string;
  label: string;
  asset_class: AssetClass;
  unit_price: number | null;
  /** O teto de preço que a pessoa pôs no ativo. */
  max_price: number | null;
  bazin: { price: number; annual_average: number; years: number } | null;
  graham: number | null;
  /** O teto pelas duas: o menor de Bazin e Graham, ou o que existir. */
  ceiling: number | null;
};

/** Um ponto de atenção de um ativo: curto e factual, sem recomendação. */
export type AttentionPoint =
  | { kind: "income_drop"; percent: number }
  | { kind: "low_revenue_growth"; percent: number }
  | { kind: "low_earnings_growth"; percent: number }
  | { kind: "low_roe"; percent: number }
  | { kind: "low_net_margin"; percent: number }
  /** Dívida líquida sobre patrimônio líquido, em vezes. */
  | { kind: "debt_above_equity"; times: number }
  | { kind: "judicial_recovery" };

export type AssetAttention = { asset_id: string; label: string; asset_class: AssetClass; points: AttentionPoint[] };

export type Attention = {
  /** Ativos com fundamentos (empresa ou FII). */
  assessed: number;
  /** Ações e FIIs com valor que ainda não têm fundamentos. */
  without_data: number;
  items: AssetAttention[];
};

export type Analysis = {
  as_of: string;
  total_value: number;
  /** Quantas regras estão em aviso: "N pontos a ver". */
  attention_count: number;
  dimensions: Dimension[];
  attention: Attention;
  profile: { current: Mix | null; target: Mix | null; conservative_max: number; moderate_max: number };
  highlights: { price_12m: Ranking; since_purchase: Ranking };
  references: { bazin_yield: number; items: Reference[] };
  unclassified_count: number;
  sources: SectorSourceInfo[];
};

export type AnalysisConfig = { rules: RuleConfig[]; bazin_yield: number };

export type RuleChange = { enabled?: boolean; threshold?: number };
export type ConfigRequest = { rules?: Partial<Record<AnalysisRuleId, RuleChange>>; bazin_yield?: number };
