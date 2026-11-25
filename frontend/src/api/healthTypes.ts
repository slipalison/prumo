// Espelho do JSON de `GET /health` e `GET|PUT /health/rules` (docs/api.md § Saúde).

import type { AssetClass } from "./types";

export const RULE_IDS = ["max_asset", "top_five", "max_sector", "min_assets", "empty_class"] as const;
export type RuleId = (typeof RULE_IDS)[number];

/** Em que o limite é medido: fatia da carteira (0–100) ou quantidade de ativos. */
export type RuleUnit = "percent" | "count";

export type WarningSubject =
  | { kind: "asset"; asset_id: string; label: string }
  | { kind: "top_assets"; labels: string[] }
  | { kind: "sector"; asset_class: AssetClass; sector: string }
  | { kind: "class"; asset_class: AssetClass };

export type HealthWarning = {
  rule: RuleId;
  subject: WarningSubject;
  unit: RuleUnit;
  /** O número de agora, na unidade da regra. */
  current: number;
  limit: number;
  /** Aporte (R$) em outros ativos que traz o número de volta ao limite; nulo nas regras de contagem. */
  needed_contribution: number | null;
};

/**
 * - `off`: desligada pela pessoa.
 * - `not_applicable`: ligada, mas a carteira ainda não dá o que medir.
 */
export type RuleStatus = "off" | "not_applicable" | "ok" | "warning";

export type HealthRule = {
  rule: RuleId;
  enabled: boolean;
  unit: RuleUnit | null;
  threshold: number | null;
  default_threshold: number | null;
};

export type HealthRuleResult = HealthRule & { status: RuleStatus };

export type SectorSourceId = "b3_stocks" | "cvm_reits";

export type SectorSourceInfo = {
  source: SectorSourceId;
  asset_class: AssetClass;
  /** Da última busca que deu certo; nulo se nunca deu. */
  updated_at: string | null;
  /** A última tentativa falhou: o setor mostrado é o da busca anterior. */
  failing: boolean;
};

export type Health = {
  total_value: number;
  warnings: HealthWarning[];
  rules: HealthRuleResult[];
  /** Ativos com valor em Ações Nacionais e FIIs que a fonte não classificou. */
  unclassified_count: number;
  sources: SectorSourceInfo[];
};

export type RulesResponse = { rules: HealthRule[] };

export type RuleChange = { enabled?: boolean; threshold?: number };
export type RulesRequest = { rules: Partial<Record<RuleId, RuleChange>> };
