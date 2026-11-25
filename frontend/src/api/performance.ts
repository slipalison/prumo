import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { BenchmarkCode, NetWorth, Performance, PerformancePeriod } from "./performanceTypes";

export const performanceApi = {
  netWorth: (period: PerformancePeriod) =>
    request<NetWorth>("GET", `/history/net-worth?period=${encodeURIComponent(period)}`),
  performance: (period: PerformancePeriod, benchmarks: readonly BenchmarkCode[]) =>
    request<Performance>(
      "GET",
      `/performance?period=${encodeURIComponent(period)}&benchmarks=${benchmarks.join(",")}`,
    ),
};

export const performanceKeys = {
  netWorth: (period: PerformancePeriod) => ["net-worth", period] as const,
  performance: (period: PerformancePeriod) => ["performance", period] as const,
};

/** Os índices que a tela compara; a legenda liga e desliga cada um. */
export const SCREEN_BENCHMARKS: readonly BenchmarkCode[] = ["cdi", "ipca", "ibov"];

/** Trocar de período mantém o gráfico anterior até o novo chegar. */
export function useNetWorth(period: PerformancePeriod) {
  return useQuery({
    queryKey: performanceKeys.netWorth(period),
    queryFn: () => performanceApi.netWorth(period),
    placeholderData: keepPreviousData,
  });
}

export function usePerformance(period: PerformancePeriod) {
  return useQuery({
    queryKey: performanceKeys.performance(period),
    queryFn: () => performanceApi.performance(period, SCREEN_BENCHMARKS),
    placeholderData: keepPreviousData,
  });
}
