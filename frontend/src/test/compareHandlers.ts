import { http, HttpResponse } from "msw";
import type {
  CompareAsset,
  CompareCell,
  CompareCurve,
  CompareDirection,
  CompareFamily,
  CompareReturns,
  CompareRow,
  CompareTable,
} from "../api/compareTypes";
import type { AssetClass } from "../api/types";
import { defaultMarket } from "./referencesFixtures";

/** O que o servidor falso responde em `/compare` e `/compare/returns` (dados fictícios). */
let tableOverride: CompareTable | null = null;
let returnsOverride: CompareReturns | null = null;
let failing = false;
/** Cada pedido que chegou, com o endereço inteiro: o teste confere os parâmetros. */
export const compareRequests: string[] = [];

export function setCompareTable(next: CompareTable | null) {
  tableOverride = next;
}

export function setCompareReturns(next: CompareReturns | null) {
  returnsOverride = next;
}

export function failCompare(on: boolean) {
  failing = on;
}

export function resetCompare() {
  tableOverride = null;
  returnsOverride = null;
  failing = false;
  compareRequests.length = 0;
}

const IN_PORTFOLIO = new Set(["ITSA4", "KNRI11", "HGLG11", "MXRF11", "BTLG11", "BTC"]);
const PRICES: Record<string, number> = { ITSA4: 11.2, KNRI11: 142, HGLG11: 140.8, MXRF11: 8.37, VALE3: 62, PETR4: 38.5, AAPL: 200 };

function assetClassOf(fund: boolean, us: boolean): AssetClass {
  if (fund) return "br_reits";
  return us ? "intl_stocks" : "br_stocks";
}

function assetOf(key: string): CompareAsset {
  const symbol = key.split(":")[0] ?? key;
  const fund = symbol.endsWith("11");
  const us = key.includes("intl_stocks") && !key.endsWith(":b3");
  const assetClass = assetClassOf(fund, us);
  return {
    key,
    symbol,
    name: symbol,
    asset_class: assetClass,
    in_portfolio: IN_PORTFOLIO.has(symbol),
    family: fund ? "fund" : "company",
    currency: us ? "USD" : "BRL",
    price: PRICES[symbol] ?? 10,
  };
}

const known = (value: number, mark: CompareCell["mark"] = null): CompareCell => ({ value, text: null, reason: null, mark });
const missing = (reason: NonNullable<CompareCell["reason"]>): CompareCell => ({ value: null, text: null, reason, mark: null });

/** Marca o melhor valor entre os ativos de um tipo; com menos de dois com valor, nenhum. */
function markFamily(
  cells: CompareCell[],
  assets: CompareAsset[],
  family: CompareFamily,
  direction: Exclude<CompareDirection, "none">,
): void {
  const idx = assets.flatMap((a, i) => (a.family === family && cells[i]?.value != null ? [i] : []));
  if (idx.length < 2) return;
  const values = idx.map((i) => cells[i]?.value as number);
  const extreme = direction === "higher" ? Math.max(...values) : Math.min(...values);
  const best = direction === "higher" ? "max" : "min";
  for (const i of idx) {
    const cell = cells[i];
    if (cell?.value === extreme) cell.mark = best;
  }
}

/** O destaque da linha dentro de cada tipo, como o servidor faz. */
function mark(row: CompareRow, assets: CompareAsset[]): CompareRow {
  if (row.direction === "none") return row;
  const cells = row.cells.map((cell) => ({ ...cell }));
  for (const family of ["company", "fund", "other"] as const) markFamily(cells, assets, family, row.direction);
  return { ...row, cells };
}

/** O ROE fictício: só de empresa, e só da que está na carteira ou na primeira coluna. */
function roeCell(asset: CompareAsset, i: number): CompareCell {
  if (asset.family !== "company") return missing("not_applicable");
  return asset.in_portfolio || i === 0 ? known(12 + i) : missing("not_tracked");
}

export function sampleTable(keys: readonly string[]): CompareTable {
  const assets = keys.map(assetOf);
  const rows: CompareRow[] = [
    { id: "price", group: "general", unit: "money", direction: "none", cells: assets.map((a) => (a.price === null ? missing("no_price") : known(a.price))) },
    {
      id: "dividend_yield_12m",
      group: "general",
      unit: "percent",
      direction: "higher",
      cells: assets.map((_, i) => known(4 + 2 * i)),
    },
    {
      id: "price_to_book",
      group: "general",
      unit: "multiple",
      direction: "lower",
      cells: assets.map((_, i) => known(0.9 + 0.3 * i)),
    },
  ];
  if (assets.some((a) => a.family === "company")) {
    rows.push({
      id: "roe",
      group: "company",
      unit: "percent",
      direction: "higher",
      cells: assets.map(roeCell),
    });
  }
  if (assets.some((a) => a.family === "fund")) {
    rows.push(
      {
        id: "vacancy",
        group: "fund",
        unit: "percent",
        direction: "lower",
        cells: assets.map((a, i) => (a.family !== "fund" ? missing("not_applicable") : known(3 + i))),
      },
      {
        id: "cash_share",
        group: "fund",
        unit: "percent",
        direction: "none",
        cells: assets.map((a, i) => (a.family !== "fund" ? missing("not_applicable") : known(6 + i))),
      },
      {
        id: "liquidity_30d",
        group: "fund",
        unit: "money_compact",
        direction: "higher",
        cells: assets.map((a, i) => (a.family !== "fund" ? missing("not_applicable") : known(8_000_000 + 1_000_000 * i))),
      },
    );
  }
  const market = defaultMarket();
  return {
    as_of: "2026-11-27",
    assets,
    rows: rows.map((row) => mark(row, assets)),
    rates: { cdi_12m: market.cdi_12m, real_rate: market.real_rate },
  };
}

const DATES = Array.from({ length: 6 }, (_, i) => `2026-0${i + 1}-15`);

export function sampleReturns(keys: readonly string[], params: URLSearchParams): CompareReturns {
  const period = (params.get("period") ?? "12m") as CompareReturns["period"];
  const totalReturn = params.get("total_return") === "true";
  const alignStart = params.get("align_start") === "true";
  const benchmarks = (params.get("benchmarks") ?? "").split(",").filter(Boolean);
  const curve = (key: string, kind: CompareCurve["kind"], name: string, slope: number): CompareCurve => ({
    key,
    kind,
    name,
    values: DATES.map((_, i) => Math.round(slope * i * 100) / 100),
    return_percent: Math.round(slope * (DATES.length - 1) * 100) / 100,
    starts_on: DATES[0] ?? null,
    reason: null,
    includes_income: totalReturn || kind !== "asset",
    converted_to_brl: key.includes("intl_stocks"),
  });
  const series = keys.map((key, i) => curve(key, "asset", key.split(":")[0] ?? key, 1.5 + i));
  if (params.get("portfolio") === "true") series.push(curve("portfolio", "portfolio", "Carteira", 1.2));
  if (benchmarks.includes("cdi")) series.push(curve("cdi", "index", "CDI", 0.9));
  if (benchmarks.includes("ibov")) series.push(curve("ibov", "index", "Ibovespa", 0.4));
  return { period, total_return: totalReturn, align_start: alignStart, from: DATES[0] ?? "", to: DATES.at(-1) ?? "", dates: DATES, series };
}

const problem = () =>
  HttpResponse.json({ type: "about:blank", title: "Falha", status: 500, code: "internal", detail: "falhou" }, { status: 500 });

const keysOf = (request: Request) => (new URL(request.url).searchParams.get("assets") ?? "").split(",").filter(Boolean);

export const compareHandlers = [
  http.get("/api/v1/compare/returns", ({ request }) => {
    compareRequests.push(new URL(request.url).search);
    if (failing) return problem();
    return HttpResponse.json(returnsOverride ?? sampleReturns(keysOf(request), new URL(request.url).searchParams));
  }),
  http.get("/api/v1/compare", ({ request }) => {
    compareRequests.push(new URL(request.url).search);
    if (failing) return problem();
    return HttpResponse.json(tableOverride ?? sampleTable(keysOf(request)));
  }),
];
