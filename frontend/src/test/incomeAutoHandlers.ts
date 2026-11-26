import { http, HttpResponse } from "msw";
import type { IncomeAuto, PendingStockEvent } from "../api/incomeAutoTypes";

/** Ligado, sem pendência: o padrão das outras telas. */
export function sampleIncomeAuto(overrides: Partial<IncomeAuto> = {}): IncomeAuto {
  return {
    enabled: true,
    fetched_at: "2026-11-26T09:00:00Z",
    covered: ["ITSA4", "KNRI11"],
    unmatched: [],
    pending_events: [],
    ...overrides,
  };
}

/** Uma bonificação FICTÍCIA de 2% em ITSA4, com 140 cotas na data com. */
export function sampleBonus(overrides: Partial<PendingStockEvent> = {}): PendingStockEvent {
  return {
    asset_id: "a-itsa",
    symbol: "ITSA4",
    kind: "bonus",
    record_date: "2025-12-18",
    factor: 2,
    quantity: 140,
    ...overrides,
  };
}

/** O servidor em memória da chave: guarda o que o `PUT` mandou. */
export function incomeAutoHandlers(initial: IncomeAuto = sampleIncomeAuto()) {
  let state = initial;
  const saved: boolean[] = [];
  return {
    saved,
    handlers: [
      http.get("/api/v1/income/auto", () => HttpResponse.json(state)),
      http.put("/api/v1/income/auto", async ({ request }) => {
        const { enabled } = (await request.json()) as { enabled: boolean };
        saved.push(enabled);
        state = { ...state, enabled };
        return HttpResponse.json(state);
      }),
    ],
  };
}

/** O padrão: ligado, sem pendência. */
export const defaultIncomeAutoHandlers = [
  http.get("/api/v1/income/auto", () => HttpResponse.json(sampleIncomeAuto())),
];
