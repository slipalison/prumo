import { http, HttpResponse } from "msw";
import type { IncomeProjection, ProjectedAsset, ProjectionGroup } from "../api/projectionTypes";

/*
 * A renda projetada FICTÍCIA: três ativos inventados (um FII de papel, um de
 * tijolo e uma ação), com o Focus de uma pesquisa qualquer. Valores sem
 * relação com carteira nenhuma.
 */

const START: Array<[string, ProjectionGroup, number]> = [
  ["PAPL11", "paper_reits", 120],
  ["TIJL11", "brick_reits", 80],
  ["ACAO3", "br_stocks", 50],
];

const DRIVER = { paper_reits: "cdi", brick_reits: "ipca", br_stocks: "flat", other_reits: "flat", foreign: "flat" } as const;

const round2 = (v: number) => Math.round(v * 100) / 100;

/** O fator de cada tipo no mês `k` (1 = o mês que vem), como a conta do servidor. */
function factor(group: ProjectionGroup, k: number): number {
  if (group === "paper_reits") return (14.9 - (k * 1.5) / 12) / 14.6;
  if (group === "brick_reits") return 1.0425 ** (k / 12);
  return 1;
}

export function sampleProjection(months: 12 | 24 = 12, spread = 2): IncomeProjection {
  const d = spread / 100;
  const monthList = Array.from({ length: months }, (_, i) => {
    const k = i + 1;
    const month = new Date(Date.UTC(2026, 10 + k, 1)).toISOString().slice(0, 7);
    // A ação paga só em maio e novembro: o calendário aparece nas barras.
    const by_group = Object.fromEntries(
      START.map(([, group, start]) => {
        const shape = group === "br_stocks" ? (month.endsWith("-05") || month.endsWith("-11") ? 6 : 0) : 1;
        return [group, round2(start * factor(group, k) * shape)];
      }),
    ) as Partial<Record<ProjectionGroup, number>>;
    const base = round2(Object.values(by_group).reduce((acc, v) => acc + v, 0));
    return { month, base, low: round2(base * (1 - d) ** (k / 12)), high: round2(base * (1 + d) ** (k / 12)), by_group };
  });
  const sum = (key: "low" | "base" | "high") => round2(monthList.reduce((acc, m) => acc + m[key], 0));
  const assets: ProjectedAsset[] = START.map(([symbol, group, start]) => {
    const base = round2(monthList.reduce((acc, m) => acc + (m.by_group[group] ?? 0), 0));
    return {
      asset_id: `a-${symbol.toLowerCase()}`,
      symbol,
      display_name: symbol,
      group,
      driver: DRIVER[group],
      basis: group === "br_stocks" ? "usual" : "trailing_12m",
      usual_years: group === "br_stocks" ? 5 : null,
      monthly_start: start,
      totals: { low: round2(base * (1 - d / 2)), base, high: round2(base * (1 + d / 2)) },
    };
  });
  return {
    today: "2026-11-27",
    estimate_monthly: 262.5,
    months: monthList,
    totals: { low: sum("low"), base: sum("base"), high: sum("high") },
    groups: assets.map((a) => ({ group: a.group, driver: a.driver, assets: 1, monthly_start: a.monthly_start, totals: a.totals })),
    assets,
    premises: {
      horizon_months: months,
      scenario_spread_percent: spread,
      spread_source: spread === 2 ? "default" : "informed",
      cdi: { source: "focus", now_percent: 14.9, trailing_percent: 14.6, end_percent: round2(14.9 - (months * 1.5) / 12) },
      ipca: { source: "focus", annual_percent: 4.25 },
      focus: { survey_date: "2026-11-19", fetched_at: "2026-11-27T09:00:00Z" },
    },
    missing: [],
  };
}

export function emptyProjection(): IncomeProjection {
  const sample = sampleProjection();
  return { ...sample, months: [], totals: { low: 0, base: 0, high: 0 }, groups: [], assets: [], estimate_monthly: 0 };
}

/** O padrão das telas: sem ativo com estimativa, a projeção não tem o que mostrar. */
export const projectionHandlers = [http.get("/api/v1/income/projection", () => HttpResponse.json(emptyProjection()))];

/** A projeção de exemplo, respondendo ao horizonte e à amplitude da consulta. */
export const sampleProjectionHandler = http.get("/api/v1/income/projection", ({ request }) => {
  const params = new URL(request.url).searchParams;
  const months = params.get("months") === "24" ? 24 : 12;
  const spread = Number(params.get("spread") ?? "2");
  return HttpResponse.json(sampleProjection(months, Number.isFinite(spread) ? spread : 2));
});
