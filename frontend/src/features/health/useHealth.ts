import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { healthApi, healthKeys } from "../../api/health";
import type { RulesRequest } from "../../api/healthTypes";

/** Os avisos de agora. A carteira muda a cada aporte, então nunca fica "fresco". */
export function useHealth() {
  return useQuery({ queryKey: healthKeys.all, queryFn: healthApi.health, retry: false });
}

export function useSaveRules() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: RulesRequest) => healthApi.saveRules(body),
    onSuccess: () => client.invalidateQueries({ queryKey: healthKeys.all }),
  });
}
