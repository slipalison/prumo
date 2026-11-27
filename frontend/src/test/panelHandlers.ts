import { http, HttpResponse, type HttpHandler } from "msw";
import type { DayReferences, PanelMonth } from "../api/panelTypes";

/** O mês FICTÍCIO do painel: novembro de 2026, com outubro fechado para comparar. */
export function sampleMonth(overrides: Partial<PanelMonth> = {}): PanelMonth {
  return {
    as_of: "2026-11-25",
    month_starts_on: "2026-11-01",
    previous_month_end: { date: "2026-10-30", value: 112_000 },
    current_return: { from: "2026-10-30", to: "2026-11-25", return_percent: 1.84, full_window: true },
    previous_return: { from: "2026-09-30", to: "2026-10-30", return_percent: 2.31, full_window: true },
    current: { income: 410.25, contribution: 1750 },
    previous: { income: 380, contribution: 1200 },
    ...overrides,
  };
}

/**
 * O fechamento anterior dos ativos cotados do banco falso: ITSA4 e KNRI11 em
 * alta, HGLG11 e MXRF11 em baixa, BTLG11 sem mudança; BTC e TRXF11 sem referência.
 */
export function sampleReferences(): DayReferences {
  const date = "2026-11-24";
  return {
    assets: [
      { asset_id: "a-itsa", previous_close: 11.0, date },
      { asset_id: "a-knri", previous_close: 140, date },
      { asset_id: "a-hglg", previous_close: 142.5, date },
      { asset_id: "a-btlg", previous_close: 94.55, date },
      { asset_id: "a-mxrf", previous_close: 8.4, date },
    ],
  };
}

export const panelState: { month: PanelMonth; references: DayReferences } = {
  month: sampleMonth(),
  references: sampleReferences(),
};

export function resetPanel(): void {
  panelState.month = sampleMonth();
  panelState.references = sampleReferences();
}

export const panelHandlers: HttpHandler[] = [
  http.get("/api/v1/portfolio/month", () => HttpResponse.json(panelState.month)),
  http.get("/api/v1/portfolio/day-reference", () => HttpResponse.json(panelState.references)),
];
