import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { BenchmarkCode, ClassesPerformance, NetWorth, Performance, PerformancePeriod } from "./performanceTypes";

export const performanceApi = {
  netWorth: (period: PerformancePeriod) =>
    request<NetWorth>("GET", `/history/net-worth?period=${encodeURIComponent(period)}`),
  performance: (period: PerformancePeriod, benchmarks: readonly BenchmarkCode[]) =>
    request<Performance>(
      "GET",
      `/performance?period=${encodeURIComponent(period)}&benchmarks=${benchmarks.join(",")}`,
    ),
  classes: (period: PerformancePeriod) =>
    request<ClassesPerformance>("GET", `/performance/classes?period=${encodeURIComponent(period)}`),
};

export const performanceKeys = {
  netWorth: (period: PerformancePeriod) => ["net-worth", period] as const,
  performance: (period: PerformancePeriod) => ["performance", period] as const,
  /** Sob "performance": quem invalida o desempenho (a importação da B3) leva as classes junto. */
  classes: (period: PerformancePeriod) => ["performance", "classes", period] as const,
};

/** Os índices que a tela compara; a legenda liga e desliga cada um. */
export const SCREEN_BENCHMARKS: readonly BenchmarkCode[] = ["cdi", "ipca", "ibov", "ifix"];

/** De quanto em quanto tempo a tela pergunta de novo enquanto a série é refeita. */
export const REBUILD_POLL_MS = 5000;

/** Enquanto a série é refeita (importação, lançamento antigo), pergunta de novo até acabar. */
export function whileRebuilding(data: { rebuilding_since: string | null } | undefined): number | false {
  return data?.rebuilding_since ? REBUILD_POLL_MS : false;
}

/** Trocar de período mantém o gráfico anterior até o novo chegar. */
export function useNetWorth(period: PerformancePeriod) {
  return useQuery({
    queryKey: performanceKeys.netWorth(period),
    queryFn: () => performanceApi.netWorth(period),
    placeholderData: keepPreviousData,
    refetchInterval: (query) => whileRebuilding(query.state.data),
  });
}

export function usePerformance(period: PerformancePeriod) {
  return useQuery({
    queryKey: performanceKeys.performance(period),
    queryFn: () => performanceApi.performance(period, SCREEN_BENCHMARKS),
    placeholderData: keepPreviousData,
    refetchInterval: (query) => whileRebuilding(query.state.data),
  });
}

export function usePerformanceClasses(period: PerformancePeriod) {
  return useQuery({
    queryKey: performanceKeys.classes(period),
    queryFn: () => performanceApi.classes(period),
    placeholderData: keepPreviousData,
    refetchInterval: (query) => whileRebuilding(query.state.data),
  });
}
