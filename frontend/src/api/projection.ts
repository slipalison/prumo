import { useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { IncomeProjection, ProjectionQuery } from "./projectionTypes";

/** A consulta como a rota espera: só o que a pessoa trocou vai junto. */
export function projectionPath(query: ProjectionQuery): string {
  const params = new URLSearchParams({ months: String(query.months) });
  if (query.spread !== undefined) params.set("spread", String(query.spread));
  if (query.cdi !== undefined) params.set("cdi", String(query.cdi));
  if (query.ipca !== undefined) params.set("ipca", String(query.ipca));
  return `/income/projection?${params.toString()}`;
}

export const projectionApi = {
  income: (query: ProjectionQuery) => request<IncomeProjection>("GET", projectionPath(query)),
};

export const projectionKeys = {
  income: (query: ProjectionQuery) => ["income", "projection", query] as const,
};

/**
 * A renda projetada. Mora sob a chave `income`: lançar ou apagar um provento
 * invalida a Renda inteira, e a projeção vai junto. Falha aqui não derruba a
 * tela (o painel diz que não carregou).
 */
export function useIncomeProjection(query: ProjectionQuery) {
  return useQuery({
    queryKey: projectionKeys.income(query),
    queryFn: () => projectionApi.income(query),
    retry: false,
    placeholderData: (previous) => previous,
  });
}
