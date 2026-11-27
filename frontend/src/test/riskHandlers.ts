import { http, HttpResponse } from "msw";
import type { AssetRisk, Risk, RiskMetrics, RiskWindow } from "../api/riskTypes";
import type { AssetClass } from "../api/types";

/**
 * O risco FICTÍCIO da carteira de exemplo (`fixtures.ts`): números redondos e
 * plausíveis, nada de carteira de verdade. As partes do risco somam 100% e a
 * matriz é simétrica, como a do servidor.
 */
const ORDER = ["a-itsa", "a-hglg", "a-knri", "a-btlg", "a-trxf", "a-mxrf", "a-btc"] as const;

const MATRIX: number[][] = [
  [1, 0.31, 0.28, 0.3, 0.26, 0.18, 0.12],
  [0.31, 1, 0.64, 0.71, 0.52, 0.33, 0.05],
  [0.28, 0.64, 1, 0.6, 0.49, 0.35, 0.03],
  [0.3, 0.71, 0.6, 1, 0.55, 0.29, 0.08],
  [0.26, 0.52, 0.49, 0.55, 1, 0.27, -0.04],
  [0.18, 0.33, 0.35, 0.29, 0.27, 1, -0.02],
  [0.12, 0.05, 0.03, 0.08, -0.04, -0.02, 1],
];

const FROM: Record<RiskWindow, string> = {
  "6m": "2026-05-26",
  "12m": "2025-11-26",
  "24m": "2024-11-26",
  "36m": "2023-11-24",
  "60m": "2021-11-26",
};

const DAYS: Record<RiskWindow, number> = { "6m": 124, "12m": 249, "24m": 498, "36m": 747, "60m": 1243 };

function metrics(window: RiskWindow, volatility: number, ret: number, depth: number, recovered: string | null): RiskMetrics {
  return {
    from: FROM[window],
    to: "2026-11-26",
    full_window: true,
    returns: DAYS[window],
    volatility_percent: volatility,
    return_percent: ret,
    drawdown: { depth_percent: depth, peak: "2026-03-04", trough: "2026-04-08", recovered },
  };
}

function asset(
  id: string,
  label: string,
  assetClass: AssetClass,
  weight: number,
  data: Pick<AssetRisk, "status" | "metrics" | "risk_share_percent">,
): AssetRisk {
  return {
    asset_id: id,
    label,
    asset_class: assetClass,
    weight_percent: weight,
    in_correlation: (ORDER as readonly string[]).includes(id),
    ...data,
  };
}

export function sampleRisk(window: RiskWindow = "12m"): Risk {
  const m = (v: number, r: number, d: number, recovered: string | null = "2026-05-21") => metrics(window, v, r, d, recovered);
  return {
    window,
    from: FROM[window],
    to: "2026-11-26",
    trading_days: DAYS[window],
    limits: { min_returns: 20, min_common: 60, trading_days_per_year: 252 },
    rebuilding_since: null,
    portfolio: m(8.42, 11.37, -6.85),
    benchmark: { code: "ibov", label: "Ibovespa", ...m(17.9, 9.12, -12.4, null) },
    model: { volatility_percent: 5.61, days: DAYS[window], covered_weight_percent: 29.26 },
    assets: [
      asset("a-ipca", "Tesouro IPCA+ 2035", "fixed_income", 70.74, { status: "no_series", metrics: null, risk_share_percent: null }),
      asset("a-itsa", "ITSA4", "br_stocks", 23.31, { status: "ok", metrics: m(21.4, 18.2, -14.1), risk_share_percent: 61.2 }),
      asset("a-hglg", "HGLG11", "br_reits", 1.41, { status: "ok", metrics: m(14.2, 6.1, -9.3), risk_share_percent: 8.4 }),
      asset("a-knri", "KNRI11", "br_reits", 1.18, { status: "ok", metrics: m(12.8, 9.7, -7.9), risk_share_percent: 6.1 }),
      asset("a-btlg", "BTLG11", "br_reits", 1.18, { status: "ok", metrics: m(13.5, 4.2, -10.2, null), risk_share_percent: 6.6 }),
      asset("a-trxf", "TRXF11", "br_reits", 0.9, { status: "ok", metrics: m(16.3, -3.1, -15.8, null), risk_share_percent: 6.4 }),
      asset("a-mxrf", "MXRF11", "br_reits", 0.7, { status: "ok", metrics: m(9.8, 11.2, -4.6), risk_share_percent: 2.1 }),
      asset("a-btc", "BTC", "crypto", 0.58, { status: "ok", metrics: m(48.6, 32.5, -27.3), risk_share_percent: 9.2 }),
    ],
    correlation: {
      asset_ids: [...ORDER],
      values: MATRIX,
      top_pairs: [
        { a: "a-hglg", b: "a-btlg", value: 0.71, days: DAYS[window] },
        { a: "a-hglg", b: "a-knri", value: 0.64, days: DAYS[window] },
        { a: "a-knri", b: "a-btlg", value: 0.6, days: DAYS[window] },
        { a: "a-btlg", b: "a-trxf", value: 0.55, days: DAYS[window] },
        { a: "a-hglg", b: "a-trxf", value: 0.52, days: DAYS[window] },
      ],
    },
  };
}

const WINDOWS: readonly RiskWindow[] = ["6m", "12m", "24m", "36m", "60m"];

export const riskHandlers = [
  http.get("/api/v1/risk", ({ request }) => {
    const asked = new URL(request.url).searchParams.get("window") ?? "12m";
    const window = WINDOWS.find((w) => w === asked);
    if (!window) {
      return HttpResponse.json(
        {
          type: "urn:prumo:error:validation_failed",
          title: "Dados inválidos",
          status: 422,
          code: "validation_failed",
          detail: "A janela precisa ser 6m, 12m, 24m, 36m ou 60m.",
        },
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      );
    }
    return HttpResponse.json(sampleRisk(window));
  }),
];

type Group = "stock" | "fii" | "bond" | "crypto" | "us";

/** Os 36 ativos fictícios da carteira grande: tickers curtos e nomes longos de título. */
const LARGE: Array<[string, AssetClass, Group]> = [
  ["ITSA4", "br_stocks", "stock"],
  ["WEGE3", "br_stocks", "stock"],
  ["BBAS3", "br_stocks", "stock"],
  ["EGIE3", "br_stocks", "stock"],
  ["TAEE11", "br_stocks", "stock"],
  ["VALE3", "br_stocks", "stock"],
  ["PETR4", "br_stocks", "stock"],
  ["BBSE3", "br_stocks", "stock"],
  ["CPLE6", "br_stocks", "stock"],
  ["KLBN11", "br_stocks", "stock"],
  ["SAPR11", "br_stocks", "stock"],
  ["ABCB4", "br_stocks", "stock"],
  ["KNRI11", "br_reits", "fii"],
  ["HGLG11", "br_reits", "fii"],
  ["BTLG11", "br_reits", "fii"],
  ["TRXF11", "br_reits", "fii"],
  ["MXRF11", "br_reits", "fii"],
  ["XPLG11", "br_reits", "fii"],
  ["VISC11", "br_reits", "fii"],
  ["HGRU11", "br_reits", "fii"],
  ["KNCR11", "br_reits", "fii"],
  ["RBRR11", "br_reits", "fii"],
  ["ALZR11", "br_reits", "fii"],
  ["PVBI11", "br_reits", "fii"],
  ["IVVB11", "intl_stocks", "us"],
  ["AAPL", "intl_stocks", "us"],
  ["MSFT", "intl_stocks", "us"],
  ["O", "us_reits", "us"],
  ["VNQ", "us_reits", "us"],
  ["BTC", "crypto", "crypto"],
  ["ETH", "crypto", "crypto"],
  ["Tesouro Prefixado 2031", "fixed_income", "bond"],
  ["Tesouro IPCA+ 2045", "fixed_income", "bond"],
  ["Tesouro Selic 2029", "fixed_income", "bond"],
  ["CDB Banco Fictício 120% do CDI", "fixed_income", "bond"],
  ["Tesouro IPCA+ com Juros Semestrais 2035", "fixed_income", "bond"],
];

const BASE: Record<Group, Record<Group, number>> = {
  stock: { stock: 0.55, fii: 0.3, bond: 0.15, crypto: 0.12, us: 0.2 },
  fii: { stock: 0.3, fii: 0.6, bond: 0.25, crypto: 0.05, us: 0.08 },
  bond: { stock: 0.15, fii: 0.25, bond: 0.7, crypto: 0, us: -0.05 },
  crypto: { stock: 0.12, fii: 0.05, bond: 0, crypto: 0.8, us: 0.25 },
  us: { stock: 0.2, fii: 0.08, bond: -0.05, crypto: 0.25, us: 0.65 },
};

/** Um número fixo em [−1, 1] por par: a mesma matriz em toda rodada. */
const wiggle = (i: number, j: number) => Math.sin(i * 12.9898 + j * 78.233) * 0.15;
const round2 = (n: number) => Math.round(n * 100) / 100;

/** O peso do ativo `i` na carteira grande: o CDB (34) e o título (35) pesam mais. */
function largeWeight(i: number): number {
  if (i === 34) return 12;
  if (i === 35) return 6;
  return 1 + ((i * 7) % 5) * 0.6;
}

/** Quanto o tipo pesa no risco além do peso na carteira: título pouco, cripto muito. */
function riskFactor(group: Group): number {
  if (group === "bond") return 0.2;
  return group === "crypto" ? 3 : 1;
}

/** A volatilidade fictícia do ativo `i`, na faixa do tipo dele. */
function largeVolatility(group: Group, i: number): number {
  switch (group) {
    case "crypto":
      return 52 + i;
    case "bond":
      return 4 + i / 10;
    case "fii":
      return 10 + (i % 6);
    default:
      return 18 + (i % 9);
  }
}

/**
 * Uma carteira FICTÍCIA grande (36 ativos) para medir o desenho com muitos
 * ativos: a demonstração usa esta. O CDB não tem série; o título com juros
 * semestrais paga no caminho; o resto entra na correlação.
 */
export function largeRisk(window: RiskWindow = "12m"): Risk {
  const base = sampleRisk(window);
  const total = LARGE.reduce((sum, _, i) => sum + largeWeight(i), 0);
  const shares = LARGE.map(([, , group], i) => (i < 34 ? largeWeight(i) * riskFactor(group) : 0));
  const shareTotal = shares.reduce((a, b) => a + b, 0);
  const assets: AssetRisk[] = LARGE.map(([label, assetClass, group], i) => {
    const id = `l-${i}`;
    const weight = round2((largeWeight(i) / total) * 100);
    if (i === 34) return asset(id, label, assetClass, weight, { status: "no_series", metrics: null, risk_share_percent: null });
    if (i === 35) return asset(id, label, assetClass, weight, { status: "pays_along_the_way", metrics: null, risk_share_percent: null });
    const volatility = largeVolatility(group, i);
    const ret = round2(((i * 37) % 41) - 12 + (group === "crypto" ? 15 : 0));
    return {
      ...asset(id, label, assetClass, weight, {
        status: "ok",
        metrics: metrics(window, volatility, ret, -round2(volatility * 0.6), i % 3 === 0 ? null : "2026-05-21"),
        risk_share_percent: round2(((shares[i] ?? 0) / shareTotal) * 100),
      }),
      in_correlation: true,
    };
  });
  const inMatrix = assets.filter((a) => a.in_correlation);
  const index = inMatrix.map((a) => LARGE.findIndex((_, i) => `l-${i}` === a.asset_id));
  const values = index.map((i, r) =>
    index.map((j, c) => {
      if (r === c) return 1;
      const [a, b] = r < c ? [i, j] : [j, i];
      const gi = LARGE[a]?.[2] ?? "stock";
      const gj = LARGE[b]?.[2] ?? "stock";
      return round2(Math.max(-1, Math.min(1, BASE[gi][gj] + wiggle(a, b))));
    }),
  );
  const pairs = values
    .flatMap((row, r) => row.map((value, c) => ({ r, c, value })).filter((cell) => cell.c > cell.r))
    .sort((x, y) => y.value - x.value || x.r - y.r || x.c - y.c)
    .slice(0, 5)
    .map(({ r, c, value }) => ({ a: inMatrix[r]?.asset_id ?? "", b: inMatrix[c]?.asset_id ?? "", value, days: DAYS[window] }));
  return {
    ...base,
    assets: [...assets].sort((a, b) => b.weight_percent - a.weight_percent),
    model: { volatility_percent: 9.84, days: DAYS[window], covered_weight_percent: round2(100 - (assets[34]?.weight_percent ?? 0) - (assets[35]?.weight_percent ?? 0)) },
    correlation: { asset_ids: inMatrix.map((a) => a.asset_id), values, top_pairs: pairs },
  };
}
