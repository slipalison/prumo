import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cashKeys } from "./cashQueries";
import { incomeApi } from "./income";
import type { CreateIncomeRequest, IncomeFilters } from "./incomeTypes";

export const incomeKeys = {
  all: ["income"] as const,
  summary: ["income", "summary"] as const,
  yield: ["income", "yield"] as const,
  entitlement: (asset: string, exDate: string, payDate: string) => ["income", "entitlement", asset, exDate, payDate] as const,
  events: (filters: IncomeFilters) => ["income", "events", filters] as const,
};

export function useIncomeSummary() {
  return useQuery({ queryKey: incomeKeys.summary, queryFn: incomeApi.summary });
}

export function useIncomeEvents(filters: IncomeFilters = {}) {
  return useQuery({ queryKey: incomeKeys.events(filters), queryFn: () => incomeApi.events(filters) });
}

/** Yield on cost de cada ativo e da carteira (a folha do ativo lê o dela daqui). */
export function useYieldOnCost() {
  return useQuery({ queryKey: incomeKeys.yield, queryFn: incomeApi.yieldOnCost, retry: false });
}

/**
 * As cotas do livro na data com de um provento, para a tela mostrar antes de
 * lançar o que o servidor vai usar. Sem ativo, não pergunta.
 */
export function useEntitlement(asset: string, exDate: string, payDate: string, enabled: boolean) {
  return useQuery({
    queryKey: incomeKeys.entitlement(asset, exDate, payDate),
    queryFn: () => incomeApi.entitlement(asset, exDate, payDate),
    enabled: enabled && asset !== "",
    retry: false,
  });
}

/** Tudo de renda (resumo, lista, yield) depende do livro: uma invalidação só. */
function useInvalidateIncome() {
  const client = useQueryClient();
  return () =>
    Promise.all([
      client.invalidateQueries({ queryKey: incomeKeys.all }),
      // Provento recebido é caixa.
      client.invalidateQueries({ queryKey: cashKeys.all }),
    ]);
}

export function useCreateIncome() {
  const invalidate = useInvalidateIncome();
  return useMutation({ mutationFn: (body: CreateIncomeRequest) => incomeApi.createEvent(body), onSuccess: invalidate });
}

export function useDeleteIncome() {
  const invalidate = useInvalidateIncome();
  return useMutation({ mutationFn: (id: string) => incomeApi.deleteEvent(id), onSuccess: invalidate });
}

export function useSaveIncomeGoal() {
  const invalidate = useInvalidateIncome();
  return useMutation({ mutationFn: (target: number) => incomeApi.saveGoal(target), onSuccess: invalidate });
}

export function useDeleteIncomeGoal() {
  const invalidate = useInvalidateIncome();
  return useMutation({ mutationFn: () => incomeApi.deleteGoal(), onSuccess: invalidate });
}
