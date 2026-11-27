import { http, HttpResponse } from "msw";
import type { Cash } from "../api/cashTypes";
import type {
  IncomeEstimate,
  IncomeEstimateAsset,
  IncomeEvent,
  IncomeMonth,
  IncomeSummary,
  PortfolioYield,
  YieldOnCost,
  YieldReport,
} from "../api/incomeTypes";

/** Os 12 meses que terminam em novembro de 2026, todos zerados. */
export function emptyMonths(): IncomeMonth[] {
  const labels = ["2025-12", "2026-01", "2026-02", "2026-03", "2026-04", "2026-05", "2026-06", "2026-07", "2026-08", "2026-09", "2026-10", "2026-11"];
  return labels.map((month) => ({ month, received: 0, upcoming: 0 }));
}

/** O yield on cost de um pagador com custo conhecido: renda ÷ custo. */
export function knownYield(income: number, cost: number): YieldOnCost {
  return { income_12m: income, basis: "received", cost, percent: Math.round((income / cost) * 10000) / 100, gap: null, message: null };
}

/** O yield de quem está sem preço médio: sem percentual, com a frase que o pede. */
export function unknownCostYield(income: number): YieldOnCost {
  return { income_12m: income, basis: "received", cost: null, percent: null, gap: "unknown_cost", message: "informe o preco medio" };
}

export function noPortfolioYield(): PortfolioYield {
  return { income_12m: 0, basis: "received", cost: 0, percent: null, gap: "no_position", message: "sem posicao", partial: false, unknown_cost: [] };
}

export function emptyYieldReport(): YieldReport {
  return { portfolio: noPortfolioYield(), assets: [] };
}

/**
 * Um ativo FICTÍCIO da estimativa: o que não for dito vale como pela B3, sem
 * JCP (líquido = bruto), sem média do habitual e sem a marca.
 */
export function estimateAsset(
  fields: Pick<IncomeEstimateAsset, "asset_id" | "symbol" | "display_name" | "quantity" | "per_unit_12m" | "monthly"> &
    Partial<IncomeEstimateAsset>,
): IncomeEstimateAsset {
  return {
    source: "b3",
    per_unit_12m_gross: fields.per_unit_12m,
    monthly_gross: fields.monthly,
    usual: null,
    above_usual: false,
    trailing_12m: fields.monthly * 12,
    ...fields,
  };
}

/** A estimativa de uma lista de ativos: os totais somam as linhas. */
export function estimateOf(assets: IncomeEstimateAsset[], missing: string[] = []): IncomeEstimate {
  const round2 = (value: number) => Math.round(value * 100) / 100;
  return {
    monthly: round2(assets.reduce((acc, a) => acc + a.monthly, 0)),
    monthly_gross: round2(assets.reduce((acc, a) => acc + a.monthly_gross, 0)),
    above_usual_monthly: round2(assets.filter((a) => a.above_usual).reduce((acc, a) => acc + a.monthly, 0)),
    updated_at: "2026-11-25T11:00:00Z",
    assets,
    missing,
  };
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
    estimate: { monthly: 0, monthly_gross: 0, above_usual_monthly: 0, updated_at: null, assets: [], missing: [] },
    yield_on_cost: noPortfolioYield(),
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
      { asset_id: "a-knri", label: "KNRI11", total: total * 0.6, percent: 60, yield_on_cost: knownYield(total * 0.6, 10000) },
      { asset_id: "a-itsa", label: "ITSA4", total: total * 0.3, percent: 30, yield_on_cost: knownYield(total * 0.3, 20000) },
      { asset_id: "a-mxrf", label: "MXRF11", total: total * 0.1, percent: 10, yield_on_cost: unknownCostYield(total * 0.1) },
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
      monthly_gross: 422.5,
      above_usual_monthly: 0,
      updated_at: "2026-11-25T11:00:00Z",
      assets: [
        estimateAsset({ asset_id: "a-knri", symbol: "KNRI11", display_name: "Kinea Renda Imobiliária", quantity: 10, per_unit_12m: 12.6, monthly: 10.5 }),
        estimateAsset({
          asset_id: "a-itsa",
          symbol: "ITSA4",
          display_name: "Itaúsa",
          quantity: 2500,
          per_unit_12m: 0.96,
          per_unit_12m_gross: 1.0176,
          monthly: 200,
          monthly_gross: 212,
        }),
      ],
      missing: ["ZZZZ3"],
    },
    yield_on_cost: {
      income_12m: total * 0.9,
      basis: "received",
      cost: 30000,
      percent: Math.round(((total * 0.9) / 30000) * 10000) / 100,
      gap: null,
      message: null,
      partial: true,
      unknown_cost: ["MXRF11"],
    },
    goal: {
      id: "g-1",
      target: 500,
      mode: "average",
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

/** Caixa vazio: nada entrou, nada saiu. */
export function emptyCash(): Cash {
  return { balance: 0, since: "2026-11-25", income: { amount: 0, count: 0 }, sales: { amount: 0, count: 0 }, used: 0, adjustment: 0, recent: [] };
}

/** Caixa FICTÍCIO: 812 de proventos e vendas, já com um aporte que usou 100. */
export function sampleCash(): Cash {
  return {
    balance: 812,
    since: "2026-10-01",
    income: { amount: 612, count: 3 },
    sales: { amount: 300, count: 1 },
    used: 100,
    adjustment: 0,
    recent: [
      { date: "2026-11-20", kind: "sale", label: "ITSA4", amount: 300 },
      { date: "2026-11-14", kind: "income", label: "KNRI11", amount: 12 },
      { date: "2026-11-02", kind: "contribution", label: "Aporte", amount: 100 },
    ],
  };
}

/**
 * O padrão da renda nos testes das outras telas: livro vazio, nada de caixa.
 * Os testes da tela Renda trocam por `server.use(...)` o que precisam.
 */
export const incomeHandlers = [
  http.get("/api/v1/income/summary", () => HttpResponse.json(emptySummary())),
  http.get("/api/v1/income/yield-on-cost", () => HttpResponse.json(emptyYieldReport())),
  http.get("/api/v1/cash", () => HttpResponse.json(emptyCash())),
  http.get("/api/v1/income", () => HttpResponse.json([])),
];
