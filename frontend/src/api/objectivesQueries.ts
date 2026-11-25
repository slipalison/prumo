import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { objectivesApi } from "./objectives";
import type { CreateObjectiveRequest, ImpactItem, UpdateObjectiveRequest } from "./objectivesTypes";

export const objectivesKeys = {
  all: ["objectives"] as const,
  list: ["objectives", "list"] as const,
  impact: (items: ImpactItem[]) => ["objectives", "impact", items] as const,
};

export function useObjectives() {
  return useQuery({ queryKey: objectivesKeys.list, queryFn: objectivesApi.list, retry: false });
}

/**
 * O que o aporte do plano faz com os objetivos. Sem item não há o que
 * perguntar; falha aqui não atrapalha o plano (o cartão só não aparece).
 */
export function useObjectiveImpact(items: ImpactItem[]) {
  return useQuery({
    queryKey: objectivesKeys.impact(items),
    queryFn: () => objectivesApi.impact(items),
    enabled: items.length > 0,
    retry: false,
  });
}

/** Objetivo, progresso e conta dependem do livro e da carteira: uma invalidação só. */
function useInvalidateObjectives() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: objectivesKeys.all });
}

export function useCreateObjective() {
  const invalidate = useInvalidateObjectives();
  return useMutation({ mutationFn: (body: CreateObjectiveRequest) => objectivesApi.create(body), onSuccess: invalidate });
}

export function useUpdateObjective() {
  const invalidate = useInvalidateObjectives();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateObjectiveRequest }) => objectivesApi.update(id, body),
    onSuccess: invalidate,
  });
}

export function useDeleteObjective() {
  const invalidate = useInvalidateObjectives();
  return useMutation({ mutationFn: (id: string) => objectivesApi.remove(id), onSuccess: invalidate });
}
