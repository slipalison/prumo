import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useDebouncedValue } from "../lib/useDebouncedValue";
import { cashKeys } from "./cashQueries";
import { api } from "./client";
import { healthKeys } from "./health";
import type {
  Asset,
  ChecklistKind,
  ContributionRequest,
  CreateAssetRequest,
  GoalTargets,
  Portfolio,
  UpdateAssetRequest,
} from "./types";

export const keys = {
  portfolio: ["portfolio"] as const,
  contributions: ["contributions"] as const,
  info: ["info"] as const,
  me: ["me"] as const,
  symbols: (query: string) => ["symbols", query] as const,
};

function invalidatePortfolio(client: QueryClient) {
  // Os avisos de saúde saem da mesma carteira: mudou uma, mudou a outra.
  void client.invalidateQueries({ queryKey: healthKeys.all });
  return client.invalidateQueries({ queryKey: keys.portfolio });
}

export function usePortfolio() {
  return useQuery({ queryKey: keys.portfolio, queryFn: api.portfolio });
}

export function useContributions() {
  return useQuery({ queryKey: keys.contributions, queryFn: api.contributions });
}

export function useInfo() {
  return useQuery({ queryKey: keys.info, queryFn: api.info, staleTime: Infinity, retry: false });
}

export function useMe() {
  return useQuery({ queryKey: keys.me, queryFn: api.me, staleTime: Infinity, retry: false });
}

export function usePlan() {
  return useMutation({ mutationFn: (amount: number) => api.plan(amount) });
}

export function useContribute() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: ContributionRequest) => api.contribute(body),
    onSuccess: async () => {
      await Promise.all([
        invalidatePortfolio(client),
        client.invalidateQueries({ queryKey: keys.contributions }),
        // O aporte baixou o caixa que usou.
        client.invalidateQueries({ queryKey: cashKeys.all }),
      ]);
    },
  });
}

/** Metas: a carteira já mostra o alvo novo antes de o servidor responder. */
export function useSaveGoals() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (targets: GoalTargets) => api.saveGoals({ targets }),
    onMutate: async (targets) => {
      await client.cancelQueries({ queryKey: keys.portfolio });
      const previous = client.getQueryData<Portfolio>(keys.portfolio);
      if (previous) {
        client.setQueryData<Portfolio>(keys.portfolio, {
          ...previous,
          classes: previous.classes.map((c) => ({ ...c, target_percent: targets[c.asset_class] })),
        });
      }
      return { previous };
    },
    onError: (_error, _targets, context) => {
      if (context?.previous) client.setQueryData(keys.portfolio, context.previous);
    },
    onSettled: () => invalidatePortfolio(client),
  });
}

export function useCreateAsset() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAssetRequest) => api.createAsset(body),
    onSuccess: () => invalidatePortfolio(client),
  });
}

export function useUpdateAsset() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateAssetRequest }) => api.updateAsset(id, body),
    onSuccess: (asset: Asset) => {
      const previous = client.getQueryData<Portfolio>(keys.portfolio);
      if (previous) {
        client.setQueryData<Portfolio>(keys.portfolio, {
          ...previous,
          assets: previous.assets.map((a) => (a.id === asset.id ? asset : a)),
        });
      }
      return invalidatePortfolio(client);
    },
  });
}

export function useDeleteAsset() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteAsset(id),
    onSuccess: () => invalidatePortfolio(client),
  });
}

export function useAddQuestion() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ checklist, prompt }: { checklist: ChecklistKind; prompt: string }) =>
      api.addQuestion(checklist, prompt),
    onSuccess: () => invalidatePortfolio(client),
  });
}

export function useUpdateQuestion() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, prompt }: { id: string; prompt: string }) => api.updateQuestion(id, prompt),
    onSuccess: () => invalidatePortfolio(client),
  });
}

export function useDeleteQuestion() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteQuestion(id),
    onSuccess: () => invalidatePortfolio(client),
  });
}

export function useResetChecklist() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (checklist: ChecklistKind) => api.resetChecklist(checklist),
    onSuccess: () => invalidatePortfolio(client),
  });
}

export function useRefreshQuotes() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => api.refreshQuotes(),
    onSuccess: () => invalidatePortfolio(client),
  });
}

/** Autocompletar: espera a pessoa parar de digitar e só busca com 2+ letras. */
export function useSymbolSearch(query: string) {
  const debounced = useDebouncedValue(query.trim(), 250);
  return useQuery({
    queryKey: keys.symbols(debounced.toUpperCase()),
    queryFn: ({ signal }) => api.searchSymbols(debounced, signal),
    enabled: debounced.length >= 2,
    staleTime: 5 * 60_000,
    retry: false,
  });
}
