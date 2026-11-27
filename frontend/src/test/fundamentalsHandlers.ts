import { http, HttpResponse } from "msw";
import type { AssetFundamentals, Figure, FundMarket, PeriodFigures, Rates } from "../api/fundamentalsTypes";
import { defaultMarket } from "./referencesFixtures";

let assets: AssetFundamentals[] = [];
let rates: Rates = sampleRates();

/** O que o servidor falso devolve em `GET /fundamentals` (cada teste começa vazio). */
export function setFundamentals(next: AssetFundamentals[]) {
  assets = next;
}

/** As taxas ao lado do rendimento dos FIIs (`null` nas duas: sem taxa guardada). */
export function setFundamentalsRates(next: Rates) {
  rates = next;
}

export function resetFundamentals() {
  assets = [];
  rates = sampleRates();
}

export const fundamentalsHandlers = [http.get("/api/v1/fundamentals", () => HttpResponse.json({ assets, rates }))];

const known = (value: number): Figure => ({ value, gap: null });

/** O CDI de 12 meses (14,20%; 12,07% líquido) e o juro real (7,12%) das referências fictícias. */
export function sampleRates(): Rates {
  const market = defaultMarket();
  return { cdi_12m: market.cdi_12m, real_rate: market.real_rate };
}

/** O histórico de mercado de um FII fictício (números inventados). */
export function sampleFundMarket(overrides: Partial<FundMarket> = {}): FundMarket {
  return {
    liquidity_30d: known(8_400_000),
    liquidity_90d: known(7_900_000),
    dividend_cagr_3y: known(0.052),
    dividend_cagr_5y: known(0.041),
    price_cagr_3y: known(-0.018),
    price_cagr_5y: known(0.012),
    dividend_yield_12m: known(0.0929),
    dividends_12m: 13.2,
    ...overrides,
  };
}

/** Um período fictício de uma indústria (números inventados). */
export function samplePeriod(overrides: Partial<PeriodFigures> = {}): PeriodFigures {
  return {
    period_start: "2025-10-01",
    period_end: "2026-09-30",
    template: "commercial",
    sources: [
      { kind: "annual", reference_date: "2025-12-31", version: 1, scope: "consolidated", column: "current" },
      { kind: "quarterly", reference_date: "2026-09-30", version: 1, scope: "consolidated", column: "current" },
      { kind: "quarterly", reference_date: "2026-09-30", version: 1, scope: "consolidated", column: "previous" },
    ],
    revenue: known(1_500_000_000),
    net_income: known(180_000_000),
    equity: known(1_200_000_000),
    equity_controlling: known(1_100_000_000),
    gross_debt: known(400_000_000),
    cash: known(100_000_000),
    net_debt: known(300_000_000),
    ebit: known(250_000_000),
    depreciation_amortization: known(50_000_000),
    ebitda: known(300_000_000),
    operating_cash_flow: known(200_000_000),
    roe: known(0.15),
    net_margin: known(0.12),
    ebitda_margin: known(0.2),
    net_debt_to_ebitda: known(1),
    net_debt_to_equity: known(0.25),
    ...overrides,
  };
}
