import { http, HttpResponse } from "msw";
import type { AssetResults, Change, CompanyResults, FundResults, Quarter, QuarterMethod } from "../api/resultsTypes";

/*
 * `GET /results` dos testes e da demonstração: empresas e fundos FICTÍCIOS,
 * números inventados. Cada teste começa sem nenhum ativo.
 */

let assets: AssetResults[] = [];

export function setResults(next: AssetResults[]) {
  assets = next;
}

export function resetResults() {
  assets = [];
}

export const resultsHandlers = [http.get("/api/v1/results", () => HttpResponse.json({ assets }))];

const ENDS = ["03-31", "06-30", "09-30", "12-31"];
const STARTS = ["01-01", "04-01", "07-01", "10-01"];

export const percent = (value: number): Change => ({ kind: "percent", value, gap: null });
export const label = (kind: Change["kind"]): Change => ({ kind, value: null, gap: null });

/** Como o trimestre `k` sai dos formulários: o 1º direto, o 4º do anual menos nove meses, os outros pela diferença. */
function quarterMethod(k: number): QuarterMethod {
  if (k === 1) return "first_quarter";
  return k === 4 ? "annual_less_nine_months" : "year_to_date_difference";
}

/**
 * Até oito trimestres (terminando no 3º de 2026) com a linha de cima e o lucro
 * dados, em reais. `margin` troca a margem de todos (o motivo de banco ou
 * holding); sem ela, lucro ÷ linha de cima.
 */
export function quarters(values: Array<[number, number]>, margin: Quarter["net_margin"] | null = null): Quarter[] {
  const periods: Array<[number, number]> = [[2024, 4], [2025, 1], [2025, 2], [2025, 3], [2025, 4], [2026, 1], [2026, 2], [2026, 3]];
  return values.map(([top, profit], i) => {
    const [year, k] = periods[periods.length - values.length + i] ?? [2026, 3];
    const end = `${year}-${ENDS[k - 1]}`;
    return {
      fiscal_quarter: k,
      period_start: `${year}-${STARTS[k - 1]}`,
      period_end: end,
      method: quarterMethod(k),
      top_line: { value: top, gap: null },
      net_income: { value: profit, gap: null },
      net_margin: margin ?? (top > 0 ? { value: Math.round((profit / top) * 1e6) / 1e6, gap: null } : { value: null, gap: "non_positive_base" }),
      source: { kind: k === 4 ? "annual" : "quarterly", reference_date: end, version: 1, scope: "consolidated", column: "current" },
      received_on: i === values.length - 1 ? "2026-11-09" : null,
    };
  });
}

/** Uma indústria fictícia: receita e lucro subindo 25% em um ano. */
export function sampleCompany(overrides: Partial<CompanyResults> = {}): CompanyResults {
  return {
    name: "INDUSTRIA FICTICIA S.A.",
    template: "commercial",
    holding: false,
    top_line: "revenue",
    synced_at: "2026-11-26T15:00:00Z",
    quarters: quarters([
      [1_150_000_000, 120_000_000],
      [1_000_000_000, 100_000_000],
      [1_100_000_000, -50_000_000],
      [1_200_000_000, 120_000_000],
      [1_300_000_000, 150_000_000],
      [1_100_000_000, -20_000_000],
      [1_250_000_000, 80_000_000],
      [1_500_000_000, 150_000_000],
    ]),
    year_ago: { period_end: "2025-09-30", top_line: percent(0.25), net_income: percent(0.25), net_margin_points: { value: 0, gap: null } },
    previous_quarter: {
      period_end: "2026-06-30",
      top_line: percent(0.2),
      net_income: percent(0.875),
      net_margin_points: { value: 0.036, gap: null },
    },
    ...overrides,
  };
}

/** Um FII fictício com três meses do informe mensal. */
export function sampleFund(overrides: Partial<FundResults> = {}): FundResults {
  return {
    name: "FUNDO FICTICIO FII",
    synced_at: "2026-11-26T15:00:00Z",
    months: [
      { reference_date: "2026-08-01", nav_per_share: { value: 100, gap: null }, dividend_yield: { value: 0.007, gap: null } },
      { reference_date: "2026-09-01", nav_per_share: { value: 101, gap: null }, dividend_yield: { value: null, gap: "implausible" } },
      { reference_date: "2026-10-01", nav_per_share: { value: 99, gap: null }, dividend_yield: { value: 0.0069, gap: null } },
    ],
    ...overrides,
  };
}

export function companyAsset(id: string, symbol: string, weight: number, company: CompanyResults): AssetResults {
  return { asset_id: id, symbol, weight, kind: "company", company, fund: null, reason: null };
}

export function fundAsset(id: string, symbol: string, weight: number, fund: FundResults): AssetResults {
  return { asset_id: id, symbol, weight, kind: "fund", company: null, fund, reason: null };
}

/** Uma carteira de resultados FICTÍCIA com o que a tela tem de mais variado. */
export function demoResults(): AssetResults[] {
  const holding = sampleCompany({
    name: "HOLDING FICTICIA S.A.",
    holding: true,
    quarters: quarters(
      [
        [100_000_000, 300_000_000],
        [100_000_000, 300_000_000],
        [100_000_000, 320_000_000],
        [100_000_000, 340_000_000],
        [100_000_000, 360_000_000],
        [110_000_000, 350_000_000],
        [110_000_000, 380_000_000],
        [120_000_000, 400_000_000],
      ],
      { value: null, gap: "holding" },
    ),
    year_ago: { period_end: "2025-09-30", top_line: percent(0.2), net_income: percent(0.176471), net_margin_points: { value: null, gap: "holding" } },
    previous_quarter: { period_end: "2026-06-30", top_line: percent(0.090909), net_income: percent(0.052632), net_margin_points: { value: null, gap: "holding" } },
  });
  const bank = sampleCompany({
    name: "BANCO FICTICIO S.A.",
    template: "bank",
    top_line: "intermediation_result",
    quarters: quarters(
      [
        [480_000_000, 95_000_000],
        [500_000_000, 100_000_000],
        [520_000_000, 110_000_000],
        [540_000_000, 120_000_000],
        [560_000_000, 130_000_000],
        [600_000_000, 140_000_000],
        [610_000_000, 150_000_000],
        [650_000_000, 160_000_000],
      ],
      { value: null, gap: "financial_template" },
    ),
    year_ago: { period_end: "2025-09-30", top_line: percent(0.203704), net_income: percent(0.333333), net_margin_points: { value: null, gap: "financial_template" } },
    previous_quarter: { period_end: "2026-06-30", top_line: percent(0.065574), net_income: percent(0.066667), net_margin_points: { value: null, gap: "financial_template" } },
  });
  const recovering = sampleCompany({
    name: "VAREJO FICTICIO S.A.",
    quarters: quarters([
      [800_000_000, -40_000_000],
      [700_000_000, -60_000_000],
      [720_000_000, -30_000_000],
      [750_000_000, -10_000_000],
      [790_000_000, 5_000_000],
      [760_000_000, -8_000_000],
      [780_000_000, 2_000_000],
      [820_000_000, 12_000_000],
    ]),
    year_ago: { period_end: "2025-09-30", top_line: percent(0.093333), net_income: label("left_loss"), net_margin_points: { value: 0.028, gap: null } },
    previous_quarter: { period_end: "2026-06-30", top_line: percent(0.051282), net_income: percent(5), net_margin_points: { value: 0.012, gap: null } },
  });
  const shrinking = sampleCompany({
    name: "MINERADORA FICTICIA S.A.",
    quarters: quarters([
      [2_000_000_000, 400_000_000],
      [1_900_000_000, 380_000_000],
      [1_850_000_000, 350_000_000],
      [1_800_000_000, 300_000_000],
      [1_700_000_000, 280_000_000],
      [1_650_000_000, 250_000_000],
      [1_600_000_000, 230_000_000],
      [1_620_000_000, 210_000_000],
    ]),
    year_ago: { period_end: "2025-09-30", top_line: percent(-0.1), net_income: percent(-0.3), net_margin_points: { value: -0.037, gap: null } },
    previous_quarter: { period_end: "2026-06-30", top_line: percent(0.0125), net_income: percent(-0.086957), net_margin_points: { value: -0.014, gap: null } },
  });
  return [
    companyAsset("a-itsa", "ITSA4", 0.22, holding),
    companyAsset("d-indu", "INDU3", 0.12, sampleCompany()),
    companyAsset("d-banc", "BANC4", 0.09, bank),
    companyAsset("d-vare", "VARE3", 0.04, recovering),
    companyAsset("d-mine", "MINE3", 0.06, shrinking),
    fundAsset("a-knri", "KNRI11", 0.012, sampleFund({ name: "FUNDO TIJOLO FICTICIO FII" })),
    fundAsset("a-hglg", "HGLG11", 0.014, sampleFund({ name: "FUNDO LOGISTICO FICTICIO FII" })),
    { asset_id: "d-etf", symbol: "ETFX11", weight: 0.03, kind: "unsupported", company: null, fund: null, reason: "no_cvm_code" },
  ];
}
