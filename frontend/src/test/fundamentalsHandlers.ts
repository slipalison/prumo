import { http, HttpResponse } from "msw";
import type { AssetFundamentals, Figure, PeriodFigures } from "../api/fundamentalsTypes";

let assets: AssetFundamentals[] = [];

/** O que o servidor falso devolve em `GET /fundamentals` (cada teste começa vazio). */
export function setFundamentals(next: AssetFundamentals[]) {
  assets = next;
}

export function resetFundamentals() {
  assets = [];
}

export const fundamentalsHandlers = [http.get("/api/v1/fundamentals", () => HttpResponse.json({ assets }))];

const known = (value: number): Figure => ({ value, gap: null });

/** Um período fictício de uma indústria (números inventados). */
export function samplePeriod(overrides: Partial<PeriodFigures> = {}): PeriodFigures {
  return {
    period_start: "2025-10-01",
    period_end: "2026-09-30",
    template: "commercial",
    sources: [
      { kind: "annual", reference_date: "2025-12-31", version: 1, scope: "consolidated", column: "current" },
      { kind: "quarterly", reference_date: "2026-09-30", version: 1, scope: "consolidated", column: "current" },
      { kind: "quarterly", reference_date: "2026-09-30", version: 1, scope: "consolidated", column: "previous" },
    ],
    revenue: known(1_500_000_000),
    net_income: known(180_000_000),
    equity: known(1_200_000_000),
    gross_debt: known(400_000_000),
    cash: known(100_000_000),
    net_debt: known(300_000_000),
    ebit: known(250_000_000),
    depreciation_amortization: known(50_000_000),
    ebitda: known(300_000_000),
    operating_cash_flow: known(200_000_000),
    roe: known(0.15),
    net_margin: known(0.12),
    ebitda_margin: known(0.2),
    net_debt_to_ebitda: known(1),
    net_debt_to_equity: known(0.25),
    ...overrides,
  };
}
