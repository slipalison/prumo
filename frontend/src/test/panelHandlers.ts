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
 * Sem negócio depois do fechamento: as cotas são as de agora. O dia da carteira
 * dá +R$ 496,60 (ações +500,00; FIIs −3,40) sobre R$ 32.868,25.
 */
export function sampleReferences(): DayReferences {
  const date = "2026-11-24";
  const held = (asset_id: string, previous_close: number, previous_quantity: number) => ({
    asset_id,
    previous_close,
    date,
    previous_quantity,
    bought: 0,
    sold: 0,
  });
  return {
    assets: [
      held("a-itsa", 11.0, 2500),
      held("a-knri", 140, 10),
      held("a-hglg", 142.5, 12),
      held("a-btlg", 94.55, 15),
      held("a-mxrf", 8.4, 100),
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
