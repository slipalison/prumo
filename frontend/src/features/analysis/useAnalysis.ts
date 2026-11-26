import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { analysisApi, analysisKeys } from "../../api/analysis";
import type { ConfigRequest } from "../../api/analysisTypes";

/** A análise de agora. A carteira muda a cada aporte, então nunca fica "fresca". */
export function useAnalysis() {
  return useQuery({ queryKey: analysisKeys.all, queryFn: analysisApi.analysis, retry: false });
}

/** As regras da pessoa (ligadas, limites e o rendimento-alvo do Bazin). */
export function useAnalysisConfig() {
  return useQuery({ queryKey: analysisKeys.config, queryFn: analysisApi.config, retry: false });
}

export function useSaveAnalysisConfig() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: ConfigRequest) => analysisApi.saveConfig(body),
    // A chave da configuração mora debaixo da da análise: uma invalidação refaz as duas.
    onSuccess: () => client.invalidateQueries({ queryKey: analysisKeys.all }),
  });
}
