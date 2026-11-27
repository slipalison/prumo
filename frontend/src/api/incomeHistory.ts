import { request } from "./client";
import type { HeatmapParams, HistoryParams, IncomeHeatmap, IncomeHistory } from "./incomeHistoryTypes";

const query = (params: Record<string, string>) => new URLSearchParams(params).toString();

export const incomeHistoryApi = {
  history: (params: HistoryParams) => request<IncomeHistory>("GET", `/income/history?${query(params)}`),
  heatmap: (params: HeatmapParams) => request<IncomeHeatmap>("GET", `/income/heatmap?${query(params)}`),
};
