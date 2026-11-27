import { useQuery } from "@tanstack/react-query";
import { request } from "./client";
import type { DayReferences, PanelMonth } from "./panelTypes";

export const panelApi = {
  month: () => request<PanelMonth>("GET", "/portfolio/month"),
  dayReference: () => request<DayReferences>("GET", "/portfolio/day-reference"),
};

export const panelKeys = {
  month: ["panel", "month"] as const,
  dayReference: ["panel", "day-reference"] as const,
};

/** De quanto em quanto tempo o painel pergunta de novo (o mês anda com o preço). */
export const PANEL_REFRESH_MS = 60_000;

export function usePanelMonth() {
  return useQuery({ queryKey: panelKeys.month, queryFn: panelApi.month, refetchInterval: PANEL_REFRESH_MS });
}

/** O fechamento anterior muda uma vez por dia: não precisa de repetição. */
export function useDayReferences() {
  return useQuery({ queryKey: panelKeys.dayReference, queryFn: panelApi.dayReference });
}
