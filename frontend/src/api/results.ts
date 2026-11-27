import { useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { Results } from "./resultsTypes";

export const resultsApi = {
  list: () => request<Results>("GET", "/results"),
};

export const resultsKeys = {
  all: ["results"] as const,
};

/**
 * Os resultados trimestrais da carteira. A CVM publica uma vez por dia: cinco
 * minutos de cache bastam, como nos fundamentos.
 */
export function useResults() {
  return useQuery({
    queryKey: resultsKeys.all,
    queryFn: resultsApi.list,
    staleTime: 5 * 60_000,
    retry: false,
  });
}
