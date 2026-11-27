// Espelho do JSON do histórico de proventos e do mapa de calor de docs/api.md
// (§ Renda › Histórico e mapa de calor).

import type { AssetClass } from "./types";

export const HISTORY_INTERVALS = ["month", "quarter", "half_year", "year"] as const;
export type HistoryInterval = (typeof HISTORY_INTERVALS)[number];

/** Que data põe o provento no mês: a de pagamento ou a data com. */
export type DateBasis = "payment" | "record";

/** Líquido (o que caiu na conta) ou bruto (antes do IR retido na fonte). */
export type AmountKind = "net" | "gross";

export const HISTORY_GROUPINGS = ["total", "class", "segment", "asset"] as const;
export type HistoryGrouping = (typeof HISTORY_GROUPINGS)[number];

export type HistoryGroup = {
  /** `total`, `class:br_reits`, `segment:br_stocks:Bancos`, `asset:<pagador>` ou `removed`. */
  key: string;
  kind: "total" | "class" | "segment" | "asset" | "removed";
  asset_class: AssetClass | null;
  /** O segmento; `null` na classe sem segmento. */
  segment: string | null;
  /** Só no agrupamento por ativo (e se o ativo ainda existe). */
  asset_id: string | null;
  /** O nome do pagador, só no agrupamento por ativo. */
  label: string | null;
  /** Recebido e a receber no histórico inteiro: a ordem da legenda e das cores. */
  total: number;
};

export type HistoryPart = { key: string; received: number; upcoming: number };

export type HistoryBucket = {
  /** O primeiro mês do intervalo: `2026-10` (o 4º trimestre de 2026). */
  start: string;
  received: number;
  upcoming: number;
  /** Os grupos com valor, na ordem de `groups`. */
  groups: HistoryPart[];
};

export type IncomeHistory = {
  today: string;
  interval: HistoryInterval;
  basis: DateBasis;
  group: HistoryGrouping;
  value: AmountKind;
  groups: HistoryGroup[];
  /** Do mais velho ao mais novo, sem buraco, até o intervalo de hoje (ou o do último a receber). */
  buckets: HistoryBucket[];
};

export type HistoryParams = {
  interval: HistoryInterval;
  basis: DateBasis;
  group: HistoryGrouping;
  value: AmountKind;
};

export type HeatCell = {
  amount: number;
  /** A parte de `amount` que ainda vai cair. */
  upcoming: number;
  /** O patrimônio médio do mês (no ano, a média dos meses com foto). */
  market_value: number | null;
  /** Em %, de 0 a 100: o provento do mês ÷ o patrimônio médio do mês. */
  dy: number | null;
  /** Algum provento (de ativo apagado) ficou fora do DY. */
  dy_partial: boolean;
  /** O custo médio do mês (no ano, a média dos meses com custo). */
  cost: number | null;
  /** Em %: o provento do mês ÷ o custo médio do mês. */
  yield_on_cost: number | null;
  /** Algum provento (de ativo apagado ou sem preço médio) ficou fora do yield on cost. */
  yield_on_cost_partial: boolean;
};

export type HeatYear = {
  year: number;
  /** Janeiro a dezembro. */
  months: HeatCell[];
  total: HeatCell;
};

export type IncomeHeatmap = {
  today: string;
  basis: DateBasis;
  value: AmountKind;
  /** Do ano mais novo ao mais velho. */
  years: HeatYear[];
};

export type HeatmapParams = { basis: DateBasis; value: AmountKind };
