import { http, HttpResponse } from "msw";
import type {
  AmountKind,
  DateBasis,
  HeatCell,
  HeatmapParams,
  HistoryBucket,
  HistoryGroup,
  HistoryGrouping,
  HistoryInterval,
  HistoryParams,
  IncomeHeatmap,
  IncomeHistory,
} from "../api/incomeHistoryTypes";
import type { AssetClass } from "../api/types";
import { intervalStart, shiftMonth } from "../lib/incomeHistory";

/*
 * Um livro de proventos FICTÍCIO e a mesma soma do servidor
 * (`domain/income_history.rs`), para os testes e a demonstração pedirem o
 * histórico e o mapa com qualquer escolha. Nenhum valor tem relação com
 * carteira nenhuma.
 */

export type FakeIncome = {
  /** `AAAA-MM-DD`. */
  pay: string;
  /** A data com; `AAAA-MM-DD`. */
  record: string;
  gross: number;
  net: number;
  /** `null`: ativo apagado da carteira. */
  asset: { id: string; label: string; asset_class: AssetClass; segment: string | null } | null;
  /** Só o nome, quando o ativo foi apagado. */
  label?: string;
};

const round2 = (value: number) => Math.round(value * 100) / 100;
const round4 = (value: number) => Math.round(value * 10000) / 10000;
const monthOf = (date: string) => date.slice(0, 7);

function keyOf(income: FakeIncome, group: HistoryGrouping): string {
  if (group === "total") return "total";
  if (group === "asset") return `asset:${income.asset?.id ?? income.label ?? "?"}`;
  if (!income.asset) return "removed";
  if (group === "class") return `class:${income.asset.asset_class}`;
  return `segment:${income.asset.asset_class}:${income.asset.segment ?? ""}`;
}

function groupOf(income: FakeIncome, group: HistoryGrouping, key: string): HistoryGroup {
  const base = { key, asset_class: null, segment: null, asset_id: null, label: null, total: 0 };
  if (group === "total") return { ...base, kind: "total" };
  if (group === "asset") return { ...base, kind: "asset", asset_id: income.asset?.id ?? null, label: income.asset?.label ?? income.label ?? null };
  if (!income.asset) return { ...base, kind: "removed" };
  if (group === "class") return { ...base, kind: "class", asset_class: income.asset.asset_class };
  return { ...base, kind: "segment", asset_class: income.asset.asset_class, segment: income.asset.segment };
}

const dateOf = (income: FakeIncome, basis: DateBasis) => (basis === "payment" ? income.pay : income.record);
const amountOf = (income: FakeIncome, value: AmountKind) => (value === "net" ? income.net : income.gross);

const SPAN: Record<HistoryInterval, number> = { month: 1, quarter: 3, half_year: 6, year: 12 };

/** O histórico como o servidor o soma. */
export function historyFrom(incomes: FakeIncome[], params: HistoryParams, today: string): IncomeHistory {
  const sums = new Map<string, Map<string, { received: number; upcoming: number }>>();
  const groups = new Map<string, HistoryGroup>();
  for (const income of incomes) {
    const start = intervalStart(monthOf(dateOf(income, params.basis)), params.interval);
    const key = keyOf(income, params.group);
    const amount = amountOf(income, params.value);
    const upcoming = income.pay > today;
    const bucket = sums.get(start) ?? new Map<string, { received: number; upcoming: number }>();
    const part = bucket.get(key) ?? { received: 0, upcoming: 0 };
    if (upcoming) part.upcoming = round2(part.upcoming + amount);
    else part.received = round2(part.received + amount);
    bucket.set(key, part);
    sums.set(start, bucket);
    const group = groups.get(key) ?? groupOf(income, params.group, key);
    group.total = round2(group.total + amount);
    groups.set(key, group);
  }
  const ordered = [...groups.values()].sort((a, b) => b.total - a.total || (a.label ?? "").localeCompare(b.label ?? "") || a.key.localeCompare(b.key));
  const order = new Map(ordered.map((g, i) => [g.key, i]));
  const starts = [...sums.keys()].sort();
  const buckets: HistoryBucket[] = [];
  const first = starts[0];
  if (first) {
    const now = intervalStart(monthOf(today), params.interval);
    const lastKey = starts[starts.length - 1] ?? first;
    const end = lastKey > now ? lastKey : now;
    for (let start = first; start <= end; start = shiftMonth(start, SPAN[params.interval])) {
      const parts = [...(sums.get(start) ?? new Map<string, { received: number; upcoming: number }>()).entries()]
        .sort(([a], [b]) => (order.get(a) ?? 0) - (order.get(b) ?? 0))
        .map(([key, s]) => ({ key, ...s }));
      buckets.push({
        start,
        received: round2(parts.reduce((acc, p) => acc + p.received, 0)),
        upcoming: round2(parts.reduce((acc, p) => acc + p.upcoming, 0)),
        groups: parts,
      });
    }
  }
  return { today, ...params, groups: ordered, buckets };
}

/** O patrimônio e o custo médios de cada mês (`AAAA-MM`), ou `null` sem base. */
export type MonthBase = (month: string) => { market_value: number | null; cost: number | null };

/** O mapa de calor como o servidor o soma (sem custo desconhecido). */
export function heatmapFrom(incomes: FakeIncome[], params: HeatmapParams, today: string, base: MonthBase): IncomeHeatmap {
  if (incomes.length === 0) return { today, ...params, years: [] };
  const dates = incomes.map((i) => dateOf(i, params.basis));
  const firstYear = Number(dates.reduce((a, b) => (a < b ? a : b)).slice(0, 4));
  const lastDate = dates.reduce((a, b) => (a > b ? a : b));
  const lastYear = Number((lastDate > today ? lastDate : today).slice(0, 4));
  const current = monthOf(today);
  const years = [];
  for (let year = lastYear; year >= firstYear; year -= 1) {
    const months: HeatCell[] = [];
    const dy = { numerator: 0, base: 0, months: 0, partial: false };
    const yoc = { numerator: 0, base: 0, months: 0, partial: false };
    let amountYear = 0;
    let upcomingYear = 0;
    for (let m = 1; m <= 12; m += 1) {
      const month = `${year}-${String(m).padStart(2, "0")}`;
      const inside = incomes.filter((i) => monthOf(dateOf(i, params.basis)) === month);
      const amount = round2(inside.reduce((acc, i) => acc + amountOf(i, params.value), 0));
      const upcoming = round2(inside.filter((i) => i.pay > today).reduce((acc, i) => acc + amountOf(i, params.value), 0));
      const kept = round2(inside.filter((i) => i.asset).reduce((acc, i) => acc + amountOf(i, params.value), 0));
      const removed = kept < amount;
      const { market_value, cost } = month <= current ? base(month) : { market_value: null, cost: null };
      months.push({
        amount,
        upcoming,
        market_value,
        dy: market_value ? round4((kept * 100) / market_value) : null,
        dy_partial: removed,
        cost,
        yield_on_cost: cost ? round4((kept * 100) / cost) : null,
        yield_on_cost_partial: removed,
      });
      amountYear += amount;
      upcomingYear += upcoming;
      if (month <= current) {
        for (const [part, b] of [
          [dy, market_value],
          [yoc, cost],
        ] as const) {
          if (b) {
            part.numerator += kept;
            part.base += b;
            part.months += 1;
            part.partial ||= removed;
          } else part.partial ||= amount > 0;
        }
      }
    }
    const result = (part: typeof dy) => {
      const mean = part.months > 0 ? round2(part.base / part.months) : null;
      return { base: mean, value: mean ? round4((part.numerator * 100) / mean) : null, partial: part.partial };
    };
    const d = result(dy);
    const y = result(yoc);
    years.push({
      year,
      months,
      total: {
        amount: round2(amountYear),
        upcoming: round2(upcomingYear),
        market_value: d.base,
        dy: d.value,
        dy_partial: d.partial,
        cost: y.base,
        yield_on_cost: y.value,
        yield_on_cost_partial: y.partial,
      },
    });
  }
  return { today, ...params, years };
}

const KNRI = { id: "a-knri", label: "KNRI11", asset_class: "br_reits", segment: "Tijolo" } as const;
const MXRF = { id: "a-mxrf", label: "MXRF11", asset_class: "br_reits", segment: "Papel" } as const;
const ITSA = { id: "a-itsa", label: "ITSA4", asset_class: "br_stocks", segment: "Bancos" } as const;

/** Três pagadores fictícios e um ativo já apagado, de 2025 a um pagamento a receber. */
export function sampleIncomes(): FakeIncome[] {
  return [
    { pay: "2025-12-12", record: "2025-11-28", gross: 100, net: 100, asset: KNRI },
    { pay: "2026-02-13", record: "2026-01-30", gross: 125, net: 125, asset: KNRI },
    { pay: "2026-03-10", record: "2026-02-19", gross: 157.58, net: 130, asset: ITSA },
    { pay: "2026-05-14", record: "2026-04-30", gross: 140, net: 140, asset: MXRF },
    { pay: "2026-06-20", record: "2026-06-01", gross: 45, net: 45, asset: null, label: "OLDX11" },
    { pay: "2026-11-14", record: "2026-10-30", gross: 150, net: 150, asset: KNRI },
    { pay: "2026-11-30", record: "2026-11-13", gross: 90, net: 90, asset: MXRF },
  ];
}

/** Patrimônio de R$ 20 mil e custo de R$ 16 mil em todo mês desde 2026. */
export const sampleBase: MonthBase = (month) =>
  month >= "2026-01" ? { market_value: 20000, cost: 16000 } : { market_value: null, cost: 16000 };

const paramsOf = (request: Request) => {
  const q = new URL(request.url).searchParams;
  return {
    interval: (q.get("interval") ?? "month") as HistoryInterval,
    basis: (q.get("basis") ?? "payment") as DateBasis,
    group: (q.get("group") ?? "total") as HistoryGrouping,
    value: (q.get("value") ?? "net") as AmountKind,
  };
};

/** Os handlers de um livro dado (`today` é o dia do servidor). */
export function historyHandlers(incomes: FakeIncome[], base: MonthBase, today = "2026-11-25") {
  return [
    http.get("/api/v1/income/history", ({ request }) => HttpResponse.json(historyFrom(incomes, paramsOf(request), today))),
    http.get("/api/v1/income/heatmap", ({ request }) => {
      const { basis, value } = paramsOf(request);
      return HttpResponse.json(heatmapFrom(incomes, { basis, value }, today, base));
    }),
  ];
}

/** O padrão das outras telas: livro vazio. Os testes do histórico trocam por `historyHandlers(...)`. */
export const incomeHistoryHandlers = historyHandlers([], () => ({ market_value: null, cost: null }));
