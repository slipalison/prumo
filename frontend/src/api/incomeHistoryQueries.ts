import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { incomeHistoryApi } from "./incomeHistory";
import type { HeatmapParams, HistoryParams } from "./incomeHistoryTypes";
import { incomeKeys } from "./incomeQueries";

/**
 * Debaixo de `incomeKeys.all`: lançar ou apagar um provento invalida o
 * histórico e o mapa junto com o resto da Renda.
 */
export const incomeHistoryKeys = {
  history: (params: HistoryParams) => [...incomeKeys.all, "history", params] as const,
  heatmap: (params: HeatmapParams) => [...incomeKeys.all, "heatmap", params] as const,
};

/** O histórico com as escolhas da tela; trocar uma escolha mantém o desenho anterior até o novo chegar. */
export function useIncomeHistory(params: HistoryParams) {
  return useQuery({
    queryKey: incomeHistoryKeys.history(params),
    queryFn: () => incomeHistoryApi.history(params),
    placeholderData: keepPreviousData,
  });
}

export function useIncomeHeatmap(params: HeatmapParams, enabled: boolean) {
  return useQuery({
    queryKey: incomeHistoryKeys.heatmap(params),
    queryFn: () => incomeHistoryApi.heatmap(params),
    placeholderData: keepPreviousData,
    enabled,
  });
}
