/** Contrato de `GET /risk` (docs/api.md, docs/risco.md). Percentuais de 0 a 100. */

import type { AssetClass } from "./types";

export type RiskWindow = "6m" | "12m" | "24m" | "36m" | "60m";

export type Drawdown = {
  /** Negativo: −23,4 = o nível chegou a 23,4% abaixo do pico. */
  depth_percent: number;
  peak: string;
  trough: string;
  /** `null`: ainda não voltou ao pico. */
  recovered: string | null;
};

export type RiskMetrics = {
  /** A primeira e a última data com valor na janela. */
  from: string;
  to: string;
  /** A série tem valor já no fechamento de base: cobre a janela inteira. */
  full_window: boolean;
  /** Retornos diários que entraram. */
  returns: number;
  /** Anualizada; `null` com histórico curto. */
  volatility_percent: number | null;
  return_percent: number | null;
  /** `null` com histórico curto ou sem queda nenhuma. */
  drawdown: Drawdown | null;
};

export type RiskStatus = "ok" | "short" | "no_series" | "pays_along_the_way";

export type AssetRisk = {
  asset_id: string;
  label: string;
  asset_class: AssetClass;
  weight_percent: number;
  status: RiskStatus;
  metrics: RiskMetrics | null;
  /** Parte do risco da carteira de hoje; `null` fora da conta. */
  risk_share_percent: number | null;
  in_correlation: boolean;
};

export type RiskPair = { a: string; b: string; value: number; days: number };

export type Risk = {
  window: RiskWindow;
  /** O fechamento de base e o último pregão; `null` sem pregão nenhum guardado. */
  from: string | null;
  to: string | null;
  trading_days: number;
  limits: { min_returns: number; min_common: number; trading_days_per_year: number };
  rebuilding_since: string | null;
  portfolio: RiskMetrics | null;
  benchmark: (RiskMetrics & { code: "ibov"; label: string }) | null;
  model: { volatility_percent: number; days: number; covered_weight_percent: number } | null;
  /** Do maior peso para o menor. */
  assets: AssetRisk[];
  correlation: {
    asset_ids: string[];
    /** −1 a 1; `null` com poucos pregões em comum. */
    values: Array<Array<number | null>>;
    top_pairs: RiskPair[];
  };
};
