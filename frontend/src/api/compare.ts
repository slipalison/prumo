import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { CompareReturns, CompareTable, ReturnsOptions } from "./compareTypes";

const list = (keys: readonly string[]) => encodeURIComponent(keys.join(","));

export const compareApi = {
  table: (keys: readonly string[]) => request<CompareTable>("GET", `/compare?assets=${list(keys)}`),
  returns: (keys: readonly string[], o: ReturnsOptions) =>
    request<CompareReturns>(
      "GET",
      `/compare/returns?assets=${list(keys)}&period=${o.period}&total_return=${o.totalReturn}` +
        `&align_start=${o.alignStart}&portfolio=${o.portfolio}&benchmarks=${o.benchmarks.join(",")}`,
    ),
};

export const compareKeys = {
  table: (keys: readonly string[]) => ["compare", "table", keys.join(",")] as const,
  returns: (keys: readonly string[], o: ReturnsOptions) =>
    ["compare", "returns", keys.join(","), o.period, o.totalReturn, o.alignStart, o.portfolio, o.benchmarks.join(",")] as const,
};

/**
 * A tabela lado a lado. Fundamentos e proventos mudam no máximo uma vez por
 * dia: um minuto de cache poupa a busca de quem troca de aba e volta.
 */
export function useCompareTable(keys: readonly string[]) {
  return useQuery({
    queryKey: compareKeys.table(keys),
    queryFn: () => compareApi.table(keys),
    enabled: keys.length > 0,
    staleTime: 60_000,
    retry: false,
    placeholderData: keepPreviousData,
  });
}

/** As curvas de rentabilidade; trocar de período mantém o gráfico anterior até o novo chegar. */
export function useCompareReturns(keys: readonly string[], options: ReturnsOptions) {
  return useQuery({
    queryKey: compareKeys.returns(keys, options),
    queryFn: () => compareApi.returns(keys, options),
    enabled: keys.length > 0,
    staleTime: 60_000,
    retry: false,
    placeholderData: keepPreviousData,
  });
}
