import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { incomeApi } from "./income";
import type { CreateIncomeRequest, IncomeFilters } from "./incomeTypes";

export const incomeKeys = {
  all: ["income"] as const,
  summary: ["income", "summary"] as const,
  cash: ["income", "cash"] as const,
  events: (filters: IncomeFilters) => ["income", "events", filters] as const,
};

export function useIncomeSummary() {
  return useQuery({ queryKey: incomeKeys.summary, queryFn: incomeApi.summary });
}

export function useIncomeEvents(filters: IncomeFilters = {}) {
  return useQuery({ queryKey: incomeKeys.events(filters), queryFn: () => incomeApi.events(filters) });
}

/** Os proventos recebidos desde o último aporte, para o campo do aporte. */
export function useCashSuggestion() {
  return useQuery({ queryKey: incomeKeys.cash, queryFn: incomeApi.cashSuggestion, retry: false });
}

/** Tudo de renda (resumo, lista, caixa) depende do livro: uma invalidação só. */
function useInvalidateIncome() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: incomeKeys.all });
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
