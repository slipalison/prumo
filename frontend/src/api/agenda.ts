import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { Agenda, UnusualIncomeList } from "./agendaTypes";

export const agendaApi = {
  list: (from: string, to: string) =>
    request<Agenda>("GET", `/agenda?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`),
  unusualIncome: () => request<UnusualIncomeList>("GET", "/agenda/unusual-income"),
};

export const agendaKeys = {
  all: ["agenda"] as const,
  range: (from: string, to: string) => ["agenda", "range", from, to] as const,
  unusual: ["agenda", "unusual"] as const,
};

/**
 * Os eventos de um período. Trocar de mês mantém o anterior na tela até o novo
 * chegar; os documentos mudam uma vez por dia, então cinco minutos de cache
 * bastam.
 */
export function useAgenda(from: string, to: string) {
  return useQuery({
    queryKey: agendaKeys.range(from, to),
    queryFn: () => agendaApi.list(from, to),
    placeholderData: keepPreviousData,
    staleTime: 5 * 60_000,
  });
}

/** Os proventos a receber fora do padrão (o selo de "Próximos pagamentos" da Renda). */
export function useUnusualIncome() {
  return useQuery({
    queryKey: agendaKeys.unusual,
    queryFn: agendaApi.unusualIncome,
    staleTime: 5 * 60_000,
    retry: false,
  });
}
