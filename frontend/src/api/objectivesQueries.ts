import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { incomeKeys } from "./incomeQueries";
import { objectivesApi, objectivesKeys } from "./objectives";
import type { CreateObjectiveRequest, ImpactItem, UpdateObjectiveRequest } from "./objectivesTypes";

export { objectivesKeys };

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

/**
 * Objetivo, progresso e conta dependem do livro e da carteira: uma invalidação
 * só. A meta de renda também mora na Renda, que é refeita junto.
 */
function useInvalidateObjectives() {
  const client = useQueryClient();
  return () =>
    Promise.all([
      client.invalidateQueries({ queryKey: objectivesKeys.all }),
      client.invalidateQueries({ queryKey: incomeKeys.all }),
    ]);
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
