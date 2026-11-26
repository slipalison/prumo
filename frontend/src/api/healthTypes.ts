// Os avisos das sete regras originais da Saúde (docs/saude.md). Eles chegam
// dentro da Análise, em `RuleResult.detail` (`kind: "findings"`); o formato é o
// mesmo dos avisos de `GET /health`, que continua respondendo (docs/api.md).

import type { AssetClass } from "./types";

/** Em que o número de uma regra (e o limite) é medido: fatia (0–100) ou quantidade (de ativos ou de meses). */
export type RuleUnit = "percent" | "count";

export type WarningSubject =
  | { kind: "asset"; asset_id: string; label: string }
  | { kind: "top_assets"; labels: string[] }
  | { kind: "sector"; asset_class: AssetClass; sector: string }
  | { kind: "class"; asset_class: AssetClass }
  | { kind: "income_months"; empty_months: string[]; payers: GapPayer[] }
  | { kind: "payer"; asset_id: string | null; label: string; received: number; total: number };

/** Ativo da carteira que, pelo histórico, já pagou num dos meses hoje vazios. */
export type GapPayer = {
  asset_id: string;
  label: string;
  asset_class: AssetClass;
  /** Meses do calendário (1 a 12) entre os vazios em que ele já pagou. */
  months: number[];
};

export type HealthWarning = {
  rule: "max_asset" | "top_five" | "max_sector" | "min_assets" | "empty_class" | "income_every_month" | "max_payer";
  subject: WarningSubject;
  unit: RuleUnit;
  /** O número de agora, na unidade da regra. */
  current: number;
  limit: number;
  /** Aporte (R$) em outros ativos que traz o número de volta ao limite; nulo nas regras de contagem. */
  needed_contribution: number | null;
};

export type SectorSourceId = "b3_stocks" | "cvm_reits";

export type SectorSourceInfo = {
  source: SectorSourceId;
  asset_class: AssetClass;
  /** Da última busca que deu certo; nulo se nunca deu. */
  updated_at: string | null;
  /** A última tentativa falhou: o setor mostrado é o da busca anterior. */
  failing: boolean;
};
