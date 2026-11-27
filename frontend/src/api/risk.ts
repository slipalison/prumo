import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { request } from "./client";
import { whileRebuilding } from "./performance";
import type { Risk, RiskWindow } from "./riskTypes";

export const riskApi = {
  get: (window: RiskWindow) => request<Risk>("GET", `/risk?window=${encodeURIComponent(window)}`),
};

export const riskKeys = {
  risk: (window: RiskWindow) => ["risk", window] as const,
};

/** Trocar de janela mantém os números anteriores até os novos chegarem. */
export function useRisk(window: RiskWindow) {
  return useQuery({
    queryKey: riskKeys.risk(window),
    queryFn: () => riskApi.get(window),
    placeholderData: keepPreviousData,
    refetchInterval: (query) => whileRebuilding(query.state.data),
  });
}
