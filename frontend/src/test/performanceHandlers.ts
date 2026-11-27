import { http, HttpResponse, type HttpHandler } from "msw";
import type {
  ClassesPerformance,
  NetWorth,
  Performance,
  PerformanceYear,
  YearIndex,
} from "../api/performanceTypes";

/*
 * O Desempenho nos testes: carteira FICTÍCIA de três fechamentos (23 a
 * 25/11/2026). Os handlers padrão servem a série vazia de quem ainda não tem
 * livro; os testes da tela trocam pelos fixtures abaixo.
 */

const year12 = <T,>(values: Record<number, T>): Array<T | null> =>
  Array.from({ length: 12 }, (_, i) => values[i] ?? null);

function yearIndex(months: Record<number, number>, total: number | null, asOf: string | null): YearIndex {
  return {
    months: year12(months),
    accumulated: year12(months),
    return_percent: total,
    accumulated_percent: total,
    as_of: asOf,
  };
}

export function sampleYears(): PerformanceYear[] {
  return [
    {
      year: 2025,
      months: year12({ 11: 0.5 }),
      return_percent: 0.5,
      cdi_percent: null,
      accumulated: year12({ 11: 0.5 }),
      accumulated_percent: 0.5,
      benchmarks: { cdi: yearIndex({}, null, null), ifix: yearIndex({ 11: 0.9 }, 0.9, "2025-12-31") },
    },
    {
      year: 2026,
      months: year12({ 0: 1, 1: -0.25, 10: 1.2345 }),
      return_percent: 1.99,
      cdi_percent: 12.1,
      accumulated: year12({ 0: 1.505, 1: 1.2513, 10: 2.5011 }),
      accumulated_percent: 2.5011,
      benchmarks: {
        cdi: yearIndex({ 0: 1.1, 1: 0.9, 10: 1.05 }, 12.1, "2026-11-25"),
        ipca: yearIndex({ 0: 0.4, 1: 0.3 }, 3.9, "2026-10-31"),
        ibov: yearIndex({ 0: 2, 1: -1, 10: -0.4321 }, 4.2, "2026-11-24"),
        ifix: yearIndex({ 0: 0.7, 1: 0.2, 10: 0.6 }, 6.1, "2026-11-24"),
      },
    },
  ];
}

export function sampleNetWorth(overrides: Partial<NetWorth> = {}): NetWorth {
  return {
    period: "12m",
    starts_on: "2026-11-23",
    rebuilding_since: null,
    points: [
      { date: "2026-11-23", market_value: 1000, invested: 1000, gain: 0, income: 0, partial: false },
      { date: "2026-11-24", market_value: 1510, invested: 1500, gain: 10, income: 0, partial: false },
      { date: "2026-11-25", market_value: 1490, invested: 1500, gain: -10, income: 12.5, partial: true },
    ],
    summary: {
      from: null,
      to: "2026-11-25",
      start_value: 0,
      contributions: 1500,
      market: -10,
      income: 12.5,
      end_value: 1490,
      invested: 1500,
      gain: -10,
      income_total: 12.5,
    },
    ...overrides,
  };
}

export function samplePerformance(overrides: Partial<Performance> = {}): Performance {
  return {
    period: "12m",
    starts_on: "2026-11-23",
    rebuilding_since: null,
    window: { from: "2026-11-23", to: "2026-11-25" },
    return_percent: 1.2345,
    benchmarks: [
      { code: "cdi", label: "CDI", return_percent: 0.1016, as_of: "2026-11-25" },
      { code: "ipca", label: "IPCA", return_percent: null, as_of: null },
      { code: "ibov", label: "Ibovespa", return_percent: -0.4321, as_of: "2026-11-24" },
      { code: "ifix", label: "IFIX", return_percent: 0.61, as_of: "2026-11-24" },
    ],
    points: [
      { date: "2026-11-23", portfolio: 0, benchmarks: { cdi: 0, ipca: 0, ibov: 0, ifix: 0 } },
      { date: "2026-11-24", portfolio: 1, benchmarks: { cdi: 0.05, ipca: null, ibov: -0.4321, ifix: 0.61 } },
      { date: "2026-11-25", portfolio: 1.2345, benchmarks: { cdi: 0.1016, ipca: null, ibov: null, ifix: null } },
    ],
    summary: {
      twelve_months: {
        from: "2026-11-23",
        to: "2026-11-25",
        return_percent: 1.2345,
        cdi_percent: 0.1016,
        cdi_as_of: "2026-11-25",
        excess_over_cdi: 1.1317,
        percent_of_cdi: 1215,
        full_window: false,
      },
      since_start: {
        from: "2026-11-23",
        to: "2026-11-25",
        return_percent: -0.2,
        cdi_percent: 0.1016,
        cdi_as_of: "2026-11-25",
        excess_over_cdi: -0.3014,
        percent_of_cdi: null,
        full_window: true,
      },
    },
    years: sampleYears(),
    months_summary: {
      best: { year: 2026, month: 11, return_percent: 1.2345 },
      worst: { year: 2026, month: 2, return_percent: -0.25 },
      positive: 3,
      negative: 1,
      flat: 0,
      versus: [
        { code: "cdi", label: "CDI", above: 1, compared: 3 },
        { code: "ipca", label: "IPCA", above: 2, compared: 2 },
        { code: "ibov", label: "Ibovespa", above: 2, compared: 3 },
        { code: "ifix", label: "IFIX", above: 3, compared: 4 },
      ],
    },
    ...overrides,
  };
}

export function sampleClasses(overrides: Partial<ClassesPerformance> = {}): ClassesPerformance {
  return {
    period: "12m",
    rebuilding_since: null,
    window: { from: "2026-11-23", to: "2026-11-25" },
    classes: [
      {
        asset_class: "br_reits",
        market_value: 1032.5,
        weight_percent: 69.2953,
        return_percent: 2.15,
        from: "2026-11-23",
        to: "2026-11-25",
        full_window: true,
        benchmark: { code: "ifix", label: "IFIX", return_percent: 0.61, as_of: "2026-11-24" },
      },
      {
        asset_class: "crypto",
        market_value: 457.5,
        weight_percent: 30.7047,
        return_percent: -1.5,
        from: "2026-11-24",
        to: "2026-11-25",
        full_window: false,
        benchmark: null,
      },
    ],
    ...overrides,
  };
}

/** A série de quem ainda não tem livro: tudo vazio, nada inventado. */
export const emptyNetWorth = (): NetWorth => sampleNetWorth({ starts_on: null, points: [], summary: null });

export const emptyPerformance = (): Performance =>
  samplePerformance({
    starts_on: null,
    window: null,
    return_percent: null,
    benchmarks: [],
    points: [],
    years: [],
    summary: { twelve_months: null, since_start: null },
    months_summary: { best: null, worst: null, positive: 0, negative: 0, flat: 0, versus: [] },
  });

export const emptyClasses = (): ClassesPerformance => sampleClasses({ window: null, classes: [] });

export const performanceHandlers: HttpHandler[] = [
  http.get("/api/v1/history/net-worth", () => HttpResponse.json(emptyNetWorth())),
  http.get("/api/v1/performance/classes", () => HttpResponse.json(emptyClasses())),
  http.get("/api/v1/performance", () => HttpResponse.json(emptyPerformance())),
];
