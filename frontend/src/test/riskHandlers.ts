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
