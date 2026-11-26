import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cashKeys } from "../../api/cashQueries";
import { incomeAutoApi, incomeAutoKeys } from "../../api/incomeAuto";
import { incomeKeys } from "../../api/incomeQueries";

/** A chave dos proventos automáticos e as pendências de eventos em ações. */
export function useIncomeAuto() {
  return useQuery({ queryKey: incomeAutoKeys.all, queryFn: incomeAutoApi.get, retry: false });
}

/** Liga ou desliga. Os automáticos entram ou saem do livro de proventos: a Renda e o caixa recarregam. */
export function useSaveIncomeAuto() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => incomeAutoApi.save(enabled),
    onSuccess: (data) =>
      Promise.all([
        client.setQueryData(incomeAutoKeys.all, data),
        client.invalidateQueries({ queryKey: incomeKeys.all }),
        client.invalidateQueries({ queryKey: cashKeys.all }),
      ]),
  });
}
