import { useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { Fundamentals } from "./fundamentalsTypes";

export const fundamentalsApi = {
  list: () => request<Fundamentals>("GET", "/fundamentals"),
};

export const fundamentalsKeys = {
  all: ["fundamentals"] as const,
};

/**
 * Os indicadores de todos os ativos (uma consulta para a carteira toda). A CVM
 * publica uma vez por dia: cinco minutos de cache bastam, e o ativo recém-
 * -adicionado aparece ao abrir a folha de novo.
 */
export function useFundamentals() {
  return useQuery({
    queryKey: fundamentalsKeys.all,
    queryFn: fundamentalsApi.list,
    staleTime: 5 * 60_000,
    retry: false,
  });
}
