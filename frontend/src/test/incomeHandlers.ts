import { http, HttpResponse } from "msw";
import type { CashSuggestion, IncomeEvent, IncomeMonth, IncomeSummary } from "../api/incomeTypes";

/** Os 12 meses que terminam em novembro de 2026, todos zerados. */
export function emptyMonths(): IncomeMonth[] {
  const labels = ["2025-12", "2026-01", "2026-02", "2026-03", "2026-04", "2026-05", "2026-06", "2026-07", "2026-08", "2026-09", "2026-10", "2026-11"];
  return labels.map((month) => ({ month, received: 0, upcoming: 0 }));
}

export function emptySummary(): IncomeSummary {
  return {
    today: "2026-11-25",
    total_12m: 0,
    monthly_average: 0,
    upcoming_total: 0,
    months: emptyMonths(),
    by_payer: [],
    years: [],
    upcoming: [],
    estimate: { monthly: 0, updated_at: null, assets: [], missing: [] },
    goal: null,
  };
}

/** Carteira FICTÍCIA com renda: três pagadores, um ano e meio de histórico. */
export function sampleSummary(): IncomeSummary {
  const months = emptyMonths().map((m, index) => ({
    ...m,
    received: index === 11 ? 150 : index % 3 === 0 ? 0 : 120 + index * 5,
    upcoming: index === 11 ? 90 : 0,
  }));
  const total = months.reduce((acc, m) => acc + m.received, 0);
  return {
    today: "2026-11-25",
    total_12m: total,
    monthly_average: Math.round((total / 12) * 100) / 100,
    upcoming_total: 90,
    months,
    by_payer: [
      { asset_id: "a-knri", label: "KNRI11", total: total * 0.6, percent: 60 },
      { asset_id: "a-itsa", label: "ITSA4", total: total * 0.3, percent: 30 },
      { asset_id: "a-mxrf", label: "MXRF11", total: total * 0.1, percent: 10 },
    ],
    years: [
      { year: 2026, months: [0, 125, 130, 0, 140, 145, 0, 155, 160, 0, 170, 240], total: 1265 },
      { year: 2025, months: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100], total: 100 },
    ],
    upcoming: [
      {
        id: "e-next",
        asset_id: "a-knri",
        symbol: "KNRI11",
        display_name: "Kinea Renda Imobiliária",
        kind: "reit_income",
        ex_date: null,
        pay_date: "2026-11-30",
        amount_per_unit: 9,
        quantity: 10,
        gross: 90,
        withholding: 0,
        net: 90,
        status: "upcoming",
        source: "manual",
        external_ref: null,
      },
    ],
    estimate: {
      monthly: 410.5,
      updated_at: "2026-11-25T11:00:00Z",
      assets: [
        { asset_id: "a-knri", symbol: "KNRI11", display_name: "Kinea Renda Imobiliária", quantity: 10, per_unit_12m: 12.6, monthly: 10.5 },
        { asset_id: "a-itsa", symbol: "ITSA4", display_name: "Itaúsa", quantity: 2500, per_unit_12m: 0.96, monthly: 200 },
      ],
      missing: ["ZZZZ3"],
    },
    goal: {
      id: "g-1",
      target: 500,
      achieved_at: null,
      current_monthly: total / 12,
      basis: "received",
      percent: Math.round((total / 12 / 500) * 10000) / 100,
      reached: false,
    },
  };
}

export function sampleEvent(overrides: Partial<IncomeEvent> = {}): IncomeEvent {
  return {
    id: "e-1",
    asset_id: "a-knri",
    symbol: "KNRI11",
    display_name: "Kinea Renda Imobiliária",
    kind: "reit_income",
    ex_date: null,
    pay_date: "2026-11-14",
    amount_per_unit: 1,
    quantity: 10,
    gross: 10,
    withholding: 0,
    net: 10,
    status: "received",
    source: "manual",
    external_ref: null,
    ...overrides,
  };
}

const noCash: CashSuggestion = { amount: 0, count: 0, since: null };

/**
 * O padrão da renda nos testes das outras telas: livro vazio, nada de caixa.
 * Os testes da tela Renda trocam por `server.use(...)` o que precisam.
 */
export const incomeHandlers = [
  http.get("/api/v1/income/summary", () => HttpResponse.json(emptySummary())),
  http.get("/api/v1/income/cash-suggestion", () => HttpResponse.json(noCash)),
  http.get("/api/v1/income", () => HttpResponse.json([])),
];
