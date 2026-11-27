import { http, HttpResponse, type HttpHandler } from "msw";
import type { NetWorth, Performance, PerformancePeriod, PerformanceYear } from "../api/performanceTypes";
import type { Plan } from "../api/types";
import { sampleAlerts } from "../test/alertsHandlers";
import { samplePlan } from "../test/fixtures";
import { sampleCash } from "../test/incomeHandlers";
import { demoAgendaHandlers } from "./demoAgenda";
import { demoIncomeEvents, demoIncomeSummary } from "./demoIncome";
import {
  achievedObjective,
  defaultObjectivesResponse,
  sampleImpact,
  sampleIncomeObjective,
  sampleObjective,
} from "../test/objectivesHandlers";
import { sampleProjectionHandler } from "../test/projectionHandlers";

/*
 * O que o modo de demonstração serve por cima do banco falso dos testes
 * (`test/handlers.ts`): as telas que, nos testes, começam vazias (Renda,
 * Desempenho, Alertas, objetivos) aparecem preenchidas, para medir e olhar o
 * desenho com a página cheia. Tudo FICTÍCIO, como os fixtures.
 */

/** O "hoje" dos fixtures. */
const TODAY = new Date("2026-11-25T12:00:00Z");
const DAY_MS = 86_400_000;

const iso = (date: Date) => date.toISOString().slice(0, 10);

/** Os dias do período, do mais velho para hoje (a série começa em 2024-11-25). */
function periodDays(period: PerformancePeriod): string[] {
  const start = new Date("2024-11-25T12:00:00Z");
  const from =
    period === "month"
      ? new Date(TODAY.getTime() - 30 * DAY_MS)
      : period === "ytd"
        ? new Date("2026-01-01T12:00:00Z")
        : period === "12m"
          ? new Date(TODAY.getTime() - 365 * DAY_MS)
          : start;
  const days: string[] = [];
  for (let t = from.getTime(); t <= TODAY.getTime(); t += DAY_MS) days.push(iso(new Date(t)));
  return days;
}

/** Uma curva suave e determinística: tendência + ondas, sem aleatório. */
const wave = (i: number, drift: number, amp: number, freq: number) =>
  Math.round((drift * i + amp * Math.sin(i / freq) + (amp / 3) * Math.sin(i / (freq / 3.7))) * 100) / 100;

function demoNetWorth(period: PerformancePeriod): NetWorth {
  const days = periodDays(period);
  const points = days.map((date, i) => {
    const invested = 90_000 + Math.floor(i / 30) * 1_500;
    const market = Math.round((invested * (1 + wave(i, 0.0002, 0.02, 40))) * 100) / 100;
    return {
      date,
      market_value: market,
      invested,
      gain: Math.round((market - invested) * 100) / 100,
      income: Math.round(i * 4.2 * 100) / 100,
      partial: i === days.length - 1,
    };
  });
  return { period, starts_on: "2024-11-25", rebuilding_since: null, points };
}

function demoPerformance(period: PerformancePeriod): Performance {
  const days = periodDays(period);
  const points = days.map((date, i) => ({
    date,
    portfolio: wave(i, 0.035, 2.2, 40),
    benchmarks: { cdi: Math.round(i * 0.04 * 100) / 100, ipca: Math.round(i * 0.013 * 100) / 100, ibov: wave(i, 0.03, 4, 28) },
  }));
  const last = points.at(-1);
  const portfolio = last?.portfolio ?? 0;
  const cdi = last?.benchmarks.cdi ?? 0;
  const years: PerformanceYear[] = [
    { year: 2026, months: [1.2, -0.4, 0.9, 1.4, -1.1, 0.6, 1.8, 0.3, -0.2, 1.1, 0.7, null], return_percent: 6.48, cdi_percent: 12.1 },
    { year: 2025, months: [0.8, 0.5, -0.9, 1.6, 0.4, 0.9, -0.3, 1.2, 0.6, 0.2, 1.4, 0.9], return_percent: 7.53, cdi_percent: 12.8 },
    { year: 2024, months: [null, null, null, null, null, null, null, null, null, null, 0.4, 0.6], return_percent: 1.0, cdi_percent: null },
  ];
  const versus = (from: string, full: boolean) => ({
    from,
    to: iso(TODAY),
    return_percent: portfolio,
    cdi_percent: cdi,
    cdi_as_of: iso(TODAY),
    excess_over_cdi: Math.round((portfolio - cdi) * 100) / 100,
    percent_of_cdi: cdi > 0 ? Math.round((portfolio / cdi) * 100) : null,
    full_window: full,
  });
  return {
    period,
    starts_on: "2024-11-25",
    rebuilding_since: null,
    window: { from: days[0] ?? iso(TODAY), to: iso(TODAY) },
    return_percent: portfolio,
    benchmarks: [
      { code: "cdi", label: "CDI", return_percent: cdi, as_of: iso(TODAY) },
      { code: "ipca", label: "IPCA", return_percent: last?.benchmarks.ipca ?? null, as_of: "2026-10-31" },
      { code: "ibov", label: "Ibovespa", return_percent: last?.benchmarks.ibov ?? null, as_of: "2026-11-24" },
    ],
    points,
    summary: { twelve_months: versus("2025-11-25", true), since_start: versus("2024-11-25", true) },
    years,
  };
}

/** O plano dos testes com o que o desenho tem de mais longo: renda, referências de preço e um aviso de teto. */
function demoPlan(amount: number): Plan {
  const plan = samplePlan(amount);
  const references = [{ bazin: 9.5, graham: 14.8 }, { bazin: 120, graham: 130.5 }, { bazin: null, graham: 40.1 }, null];
  const income = [19.44, 6.3, 8.1, null];
  plan.items = plan.items.map((item, index) => ({
    ...item,
    price_references: references[index] ?? null,
    estimated_monthly_income: income[index] ?? null,
  }));
  // Um aviso que abre a folha do ativo (o checklist), como na vida real.
  plan.notices = [
    ...plan.notices,
    { code: "asset_above_ceiling", asset_id: "a-hglg", symbol: "HGLG11", unit_price: 140.8, max_price: 130 },
  ];
  plan.income = { available: true, missing: [], added_monthly: 33.84, before_monthly: 410.5, after_monthly: 444.34 };
  return plan;
}

const asPeriod = (request: Request): PerformancePeriod =>
  (new URL(request.url).searchParams.get("period") as PerformancePeriod | null) ?? "12m";

export const demoHandlers: HttpHandler[] = [
  ...demoAgendaHandlers,
  http.post("/api/v1/plans", async ({ request }) => {
    const { amount } = (await request.json()) as { amount: number };
    return HttpResponse.json(demoPlan(amount));
  }),
  http.get("/api/v1/history/net-worth", ({ request }) => HttpResponse.json(demoNetWorth(asPeriod(request)))),
  http.get("/api/v1/performance", ({ request }) => HttpResponse.json(demoPerformance(asPeriod(request)))),
  http.get("/api/v1/income/summary", () => HttpResponse.json(demoIncomeSummary())),
  http.get("/api/v1/income", () => HttpResponse.json(demoIncomeEvents())),
  http.get("/api/v1/cash", () => HttpResponse.json(sampleCash())),
  http.get("/api/v1/alerts", () => HttpResponse.json(sampleAlerts())),
  http.get("/api/v1/objectives", () =>
    HttpResponse.json({
      ...defaultObjectivesResponse,
      objectives: [
        sampleObjective(),
        sampleObjective({ id: "o-fii", kind: "class_value", asset_class: "br_reits", target: 200_000, current: 82_000, percent: 41 }),
        sampleIncomeObjective({ id: "g-1", target: 500, income_mode: "floor", current: 410.5, percent: 82.1 }),
        achievedObjective(),
      ],
    }),
  ),
  sampleProjectionHandler,
  http.post("/api/v1/objectives/impact", () => HttpResponse.json({ objectives: [sampleImpact()] })),
];
