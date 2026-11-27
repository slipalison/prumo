import { http, HttpResponse, type HttpHandler } from "msw";
import type {
  BenchmarkCode,
  ClassesPerformance,
  NetWorth,
  Performance,
  PerformancePeriod,
  PerformanceYear,
  YearIndex,
} from "../api/performanceTypes";
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
  // 2 e 5 anos passam do começo da série (2024-11-25): a janela começa nela.
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
  const first = points[0];
  const last = points.at(-1);
  const fromZero = !first || first.date === "2024-11-25";
  const round = (value: number) => Math.round(value * 100) / 100;
  const summary =
    first && last
      ? {
          from: fromZero ? null : first.date,
          to: last.date,
          start_value: fromZero ? 0 : first.market_value,
          contributions: round(last.invested - (fromZero ? 0 : first.invested)),
          market: round(last.gain - (fromZero ? 0 : first.gain)),
          income: round(last.income - (fromZero ? 0 : first.income)),
          end_value: last.market_value,
          invested: last.invested,
          gain: last.gain,
          income_total: last.income,
        }
      : null;
  return { period, starts_on: "2024-11-25", rebuilding_since: null, points, summary };
}

/** Um índice da tabela ano a ano, a partir dos meses (fictícios); o acumulado vem desde o começo da série. */
function demoYearIndex(months: Array<number | null>, asOf: string, before: number | null): YearIndex {
  let factor = 1 + (before ?? 0) / 100;
  const accumulated = months.map((m) => {
    if (m === null) return null;
    factor *= 1 + m / 100;
    return Math.round((factor - 1) * 10_000) / 100;
  });
  const total = months.reduce<number>((acc, m) => (m === null ? acc : acc * (1 + m / 100)), 1);
  return {
    months,
    accumulated,
    return_percent: Math.round((total - 1) * 10_000) / 100,
    accumulated_percent: accumulated.filter((a) => a !== null).at(-1) ?? null,
    as_of: asOf,
  };
}

/** Um ano fictício: os meses da carteira, o acumulado e os índices nas mesmas janelas. */
function demoYear(
  year: number,
  months: Array<number | null>,
  cdi: number | null,
  indexes: Partial<Record<BenchmarkCode, Array<number | null>>>,
  previous: PerformanceYear | null,
): PerformanceYear {
  const before = previous?.accumulated_percent ?? 0;
  let factor = 1 + before / 100;
  const accumulated = months.map((m) => {
    if (m === null) return null;
    factor *= 1 + m / 100;
    return Math.round((factor - 1) * 10_000) / 100;
  });
  const total = months.reduce<number>((acc, m) => (m === null ? acc : acc * (1 + m / 100)), 1);
  const asOf = `${year}-12-31`;
  return {
    year,
    months,
    return_percent: Math.round((total - 1) * 10_000) / 100,
    cdi_percent: cdi,
    accumulated,
    accumulated_percent: accumulated.filter((a) => a !== null).at(-1) ?? 0,
    benchmarks: Object.fromEntries(
      Object.entries(indexes).map(([code, values]) => [
        code,
        // O IPCA de um mês sai no mês seguinte; os outros, no dia útil seguinte.
        demoYearIndex(
          values,
          year !== 2026 ? asOf : code === "ipca" ? "2026-10-31" : "2026-11-24",
          previous?.benchmarks[code as BenchmarkCode]?.accumulated_percent ?? null,
        ),
      ]),
    ),
  };
}

function demoPerformance(period: PerformancePeriod): Performance {
  const days = periodDays(period);
  const points = days.map((date, i) => ({
    date,
    portfolio: wave(i, 0.035, 2.2, 40),
    benchmarks: {
      cdi: Math.round(i * 0.04 * 100) / 100,
      ipca: Math.round(i * 0.013 * 100) / 100,
      ibov: wave(i, 0.03, 4, 28),
      ifix: wave(i, 0.022, 2.6, 34),
    },
  }));
  const last = points.at(-1);
  const portfolio = last?.portfolio ?? 0;
  const cdi = last?.benchmarks.cdi ?? 0;
  const n = null;
  const y2024 = demoYear(2024, [n, n, n, n, n, n, n, n, n, n, 0.4, 0.6], null, {
    cdi: [n, n, n, n, n, n, n, n, n, n, 0.2, 0.93],
    ipca: [n, n, n, n, n, n, n, n, n, n, 0.12, 0.52],
    ibov: [n, n, n, n, n, n, n, n, n, n, -1.1, -4.3],
    ifix: [n, n, n, n, n, n, n, n, n, n, -0.4, -0.7],
  }, null);
  const y2025 = demoYear(2025, [0.8, 0.5, -0.9, 1.6, 0.4, 0.9, -0.3, 1.2, 0.6, 0.2, 1.4, 0.9], 12.8, {
    cdi: [1.01, 0.99, 0.96, 1.06, 1.14, 1.1, 1.28, 1.16, 1.22, 1.28, 1.05, 1.22],
    ipca: [0.16, 1.31, 0.56, 0.43, 0.26, 0.24, 0.26, -0.11, 0.48, 0.09, 0.18, 0.33],
    ibov: [4.86, -2.64, 6.08, 3.69, 1.45, 1.33, -4.17, 6.28, 3.4, 2.26, 6.37, 1.29],
    ifix: [-3.07, 3.34, 6.14, 3.01, 1.44, 0.63, -1.36, 1.15, 3.25, 0.12, 1.86, 1.8],
  }, y2024);
  const y2026 = demoYear(2026, [1.2, -0.4, 0.9, 1.4, -1.1, 0.6, 1.8, 0.3, -0.2, 1.1, 0.7, n], 12.1, {
    cdi: [1.16, 0.99, 1.08, 1.05, 1.12, 1.09, 1.18, 1.1, 1.06, 1.14, 0.95, n],
    ipca: [0.33, 0.7, 0.41, 0.38, 0.29, 0.21, 0.19, 0.1, 0.25, 0.3, n, n],
    ibov: [1.4, 1.7, -0.8, 2.2, -1.5, 0.9, 2.4, -0.6, 1.1, 0.4, -2.7, n],
    ifix: [1.1, 0.5, 0.9, -0.4, 0.6, 1.3, -0.9, 0.2, 0.8, 0.5, -1.9, n],
  }, y2025);
  const years: PerformanceYear[] = [y2024, y2025, y2026];
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
      { code: "ifix", label: "IFIX", return_percent: last?.benchmarks.ifix ?? null, as_of: "2026-11-24" },
    ],
    points,
    summary: { twelve_months: versus("2025-11-25", true), since_start: versus("2024-11-25", true) },
    years,
    months_summary: {
      best: { year: 2026, month: 7, return_percent: 1.8 },
      worst: { year: 2026, month: 5, return_percent: -1.1 },
      positive: 20,
      negative: 5,
      flat: 0,
      versus: [
        { code: "cdi", label: "CDI", above: 11, compared: 25 },
        { code: "ipca", label: "IPCA", above: 19, compared: 24 },
        { code: "ibov", label: "Ibovespa", above: 12, compared: 25 },
        { code: "ifix", label: "IFIX", above: 13, compared: 25 },
      ],
    },
  };
}

/** Por classe, fictício: peso no fim do período e a rentabilidade de cada classe. */
function demoClasses(period: PerformancePeriod): ClassesPerformance {
  const days = periodDays(period);
  const from = days[0] ?? iso(TODAY);
  const to = iso(TODAY);
  const scale = Math.max(days.length / 365, 0.08);
  const r = (annual: number) => Math.round(annual * scale * 100) / 100;
  return {
    period,
    rebuilding_since: null,
    window: { from, to },
    classes: [
      {
        asset_class: "br_stocks",
        market_value: 41_250.4,
        weight_percent: 41.2,
        return_percent: r(9.8),
        from,
        to,
        full_window: true,
        benchmark: { code: "ibov", label: "Ibovespa", return_percent: r(7.4), as_of: "2026-11-24" },
      },
      {
        asset_class: "br_reits",
        market_value: 27_310.15,
        weight_percent: 27.3,
        return_percent: r(11.6),
        from,
        to,
        full_window: true,
        benchmark: { code: "ifix", label: "IFIX", return_percent: r(8.1), as_of: "2026-11-24" },
      },
      {
        asset_class: "intl_stocks",
        market_value: 12_080,
        weight_percent: 12.1,
        return_percent: r(14.2),
        from,
        to,
        full_window: true,
        benchmark: null,
      },
      {
        asset_class: "crypto",
        market_value: 4_120.7,
        weight_percent: 4.1,
        return_percent: r(-6.3),
        from: days[Math.floor(days.length / 3)] ?? from,
        to,
        full_window: false,
        benchmark: null,
      },
      {
        asset_class: "fixed_income",
        market_value: 15_300,
        weight_percent: 15.3,
        return_percent: r(12.4),
        from,
        to,
        full_window: true,
        benchmark: { code: "cdi", label: "CDI", return_percent: r(12.9), as_of: to },
      },
    ],
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
  http.get("/api/v1/performance/classes", ({ request }) => HttpResponse.json(demoClasses(asPeriod(request)))),
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
