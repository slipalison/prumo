import { useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { References } from "./ceilingTypes";

export const ceilingKeys = {
  all: ["references"] as const,
  asset: (id: string) => ["references", id] as const,
};

export const ceilingApi = {
  /** As referências e o teto de um ativo (listas vazias para quem não tem referência). */
  references: (id: string) => request<References>("GET", `/assets/${id}/references`),
};

/** As referências de um ativo, para a escolha do teto na folha dele. */
export function useAssetReferences(id: string, enabled: boolean) {
  return useQuery({
    queryKey: ceilingKeys.asset(id),
    queryFn: () => ceilingApi.references(id),
    enabled,
    retry: false,
  });
}
