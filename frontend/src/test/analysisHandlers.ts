import { http, HttpResponse, type HttpHandler } from "msw";
import type {
  Analysis,
  AnalysisConfig,
  AnalysisRuleId,
  ConfigRequest,
  Dimension,
  DimensionId,
  RuleConfig,
  RuleDetail,
  RuleResult,
  RuleStatus,
  Score,
} from "../api/analysisTypes";
import { ANALYSIS_RULE_IDS } from "../api/analysisTypes";
import type { HealthWarning, SectorSourceInfo } from "../api/healthTypes";

/** Uma regra de fábrica: dimensão, unidade, direção e limite padrão. */
const FACTORY: Record<AnalysisRuleId, Pick<RuleConfig, "dimension" | "unit" | "direction" | "default_threshold">> = {
  max_asset: { dimension: "diversification", unit: "percent", direction: "at_most", default_threshold: 15 },
  top_five: { dimension: "diversification", unit: "percent", direction: "at_most", default_threshold: 60 },
  max_sector: { dimension: "diversification", unit: "percent", direction: "at_most", default_threshold: 40 },
  min_assets: { dimension: "diversification", unit: "count", direction: "at_least", default_threshold: 3 },
  empty_class: { dimension: "diversification", unit: "count", direction: null, default_threshold: null },
  min_portfolio_assets: { dimension: "diversification", unit: "count", direction: "at_least", default_threshold: 10 },
  min_classes: { dimension: "diversification", unit: "count", direction: "at_least", default_threshold: 3 },
  min_sectors: { dimension: "diversification", unit: "count", direction: "at_least", default_threshold: 4 },
  min_abroad: { dimension: "diversification", unit: "percent", direction: "at_least", default_threshold: 10 },
  min_low_risk: { dimension: "diversification", unit: "percent", direction: "at_least", default_threshold: 10 },
  min_yield: { dimension: "dividends", unit: "percent", direction: "at_least", default_threshold: 5 },
  min_paying_share: { dimension: "dividends", unit: "percent", direction: "at_least", default_threshold: 70 },
  income_growth: { dimension: "dividends", unit: "percent", direction: null, default_threshold: null },
  income_every_month: { dimension: "dividends", unit: "count", direction: "at_least", default_threshold: 12 },
  max_payer: { dimension: "dividends", unit: "percent", direction: "at_most", default_threshold: 25 },
  brick_reit: { dimension: "dividends", unit: "count", direction: null, default_threshold: null },
  beat_cdi_12m: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: null },
  beat_cdi_5y: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: null },
  beat_ibov_12m: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: null },
  beat_ibov_5y: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: null },
  beat_ipca_12m: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: null },
  beat_ipca_5y: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: null },
  min_positive_years: { dimension: "returns", unit: "percent", direction: "at_least", default_threshold: 70 },
  max_volatility: { dimension: "returns", unit: "percent", direction: "at_most", default_threshold: 20 },
  max_loss: { dimension: "returns", unit: "percent", direction: "at_most", default_threshold: 30 },
  quality_roe: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
  quality_profit: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
  quality_leverage: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
  quality_margin: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
  quality_cash: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
  quality_recovery: { dimension: "quality", unit: "count", direction: null, default_threshold: null },
  quality_fii_pvp: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
  quality_fii_vacancy: { dimension: "quality", unit: "percent", direction: "at_least", default_threshold: 70 },
};

/** As 25 regras como o servidor as devolve de fábrica. */
export function defaultAnalysisRules(): RuleConfig[] {
  return ANALYSIS_RULE_IDS.map((rule) => ({
    rule,
    enabled: true,
    threshold: FACTORY[rule].default_threshold,
    ...FACTORY[rule],
  }));
}

/**
 * Os achados da carteira fictícia nas sete regras originais: Tesouro e Itaúsa
 * pesam demais, os cinco maiores são quase tudo, os FIIs são só tijolo e Ações
 * Internacionais tem meta sem nada dentro.
 */
export const FINDINGS: Record<string, HealthWarning[]> = {
  max_asset: [
    {
      rule: "max_asset",
      subject: { kind: "asset", asset_id: "a-ipca", label: "Tesouro IPCA+ 2035" },
      unit: "percent",
      current: 70.76,
      limit: 15,
      needed_contribution: 446_000,
    },
    {
      rule: "max_asset",
      subject: { kind: "asset", asset_id: "a-itsa", label: "ITSA4" },
      unit: "percent",
      current: 23.31,
      limit: 15,
      needed_contribution: 66_500.5,
    },
  ],
  top_five: [
    {
      rule: "top_five",
      subject: { kind: "top_assets", labels: ["Tesouro IPCA+ 2035", "ITSA4", "HGLG11", "KNRI11", "BTLG11"] },
      unit: "percent",
      current: 97.84,
      limit: 60,
      needed_contribution: 75_900,
    },
  ],
  max_sector: [
    {
      rule: "max_sector",
      subject: { kind: "sector", asset_class: "br_reits", sector: "Tijolo" },
      unit: "percent",
      current: 87.01,
      limit: 40,
      needed_contribution: 8_200,
    },
  ],
  min_assets: [
    {
      rule: "min_assets",
      subject: { kind: "class", asset_class: "br_stocks" },
      unit: "count",
      current: 1,
      limit: 3,
      needed_contribution: null,
    },
  ],
  empty_class: [
    {
      rule: "empty_class",
      subject: { kind: "class", asset_class: "intl_stocks" },
      unit: "count",
      current: 0,
      limit: 1,
      needed_contribution: null,
    },
  ],
  income_every_month: [
    {
      rule: "income_every_month",
      subject: {
        kind: "income_months",
        empty_months: ["2026-02", "2026-04"],
        payers: [{ asset_id: "a-hglg", label: "HGLG11", asset_class: "br_reits", months: [2, 4] }],
      },
      unit: "count",
      current: 10,
      limit: 12,
      needed_contribution: null,
    },
  ],
  max_payer: [
    {
      rule: "max_payer",
      subject: { kind: "payer", asset_id: "a-knri", label: "KNRI11", received: 4_800.5, total: 6_000 },
      unit: "percent",
      current: 80.01,
      limit: 25,
      needed_contribution: null,
    },
  ],
};

export function defaultSectorSources(): SectorSourceInfo[] {
  return [
    { source: "b3_stocks", asset_class: "br_stocks", updated_at: "2026-11-20T09:00:00Z", failing: false },
    { source: "cvm_reits", asset_class: "br_reits", updated_at: "2026-11-21T09:00:00Z", failing: false },
  ];
}

type Partial_ = Partial<Omit<RuleResult, "rule" | "status">>;

/** Um resultado de regra, a partir da regra de fábrica. */
export function result(rule: AnalysisRuleId, status: RuleStatus, patch: Partial_ = {}): RuleResult {
  const config = defaultAnalysisRules().find((r) => r.rule === rule) as RuleConfig;
  return {
    ...config,
    enabled: status !== "off",
    status,
    current: null,
    limit: status === "ok" || status === "warning" ? config.threshold : null,
    needed_contribution: null,
    detail: null,
    ...patch,
  };
}

const withFindings = (rule: AnalysisRuleId, current: number): RuleResult =>
  result(rule, "warning", {
    current,
    detail: { kind: "findings", warnings: FINDINGS[rule] ?? [] } satisfies RuleDetail,
  });

export function scoreOf(rules: RuleResult[]): Score {
  const ok = rules.filter((r) => r.status === "ok").length;
  const warning = rules.filter((r) => r.status === "warning").length;
  const measured = ok + warning;
  return {
    ok,
    measured,
    no_data: rules.filter((r) => r.status === "no_data").length,
    percent: measured > 0 ? Math.round((ok * 1000) / measured) / 10 : null,
  };
}

function dimension(id: DimensionId, rules: RuleResult[]): Dimension {
  return { id, score: scoreOf(rules), rules };
}

/**
 * A análise da carteira fictícia (a mesma dos outros testes), com os números
 * escolhidos para os testes: 13 pontos a ver, as quatro dimensões com pontuação
 * e pontos de atenção em dois ativos.
 */
export function defaultAnalysis(): Analysis {
  const diversification = [
    withFindings("max_asset", 70.76),
    withFindings("top_five", 97.84),
    withFindings("max_sector", 87.01),
    withFindings("min_assets", 1),
    withFindings("empty_class", 1),
    result("min_portfolio_assets", "ok", {
      current: 12,
      detail: null,
    }),
    result("min_classes", "ok", {
      current: 4,
      detail: { kind: "classes", asset_classes: ["br_stocks", "br_reits", "crypto", "fixed_income"] },
    }),
    result("min_sectors", "no_data"),
    result("min_abroad", "warning", { current: 0, needed_contribution: 14_500 }),
    result("min_low_risk", "ok", { current: 70.76 }),
  ];
  const dividends = [
    result("min_yield", "ok", {
      current: 7.12,
      detail: { kind: "yield", income_12m: 6_000, base_value: 84_300 },
    }),
    result("min_paying_share", "warning", {
      current: 50,
      detail: { kind: "paying", paying: 2, total: 4, silent: ["BTLG11", "ITSA4"] },
    }),
    result("income_growth", "ok", {
      current: 18.5,
      limit: null,
      detail: { kind: "income_growth", current_12m: 6_000, previous_12m: 5_063.29 },
    }),
    withFindings("income_every_month", 10),
    withFindings("max_payer", 80.01),
    result("brick_reit", "no_data"),
  ];
  const returns = [
    result("beat_cdi_12m", "warning", {
      current: 9.4,
      limit: 13.2,
      detail: {
        kind: "benchmark",
        benchmark: "cdi",
        horizon: "12m",
        from: "2025-11-25",
        to: "2026-11-25",
        portfolio_percent: 9.4,
        benchmark_percent: 13.2,
      },
    }),
    result("beat_cdi_5y", "no_data"),
    result("beat_ibov_12m", "ok", {
      current: 9.4,
      limit: 6.1,
      detail: {
        kind: "benchmark",
        benchmark: "ibov",
        horizon: "12m",
        from: "2025-11-25",
        to: "2026-11-25",
        portfolio_percent: 9.4,
        benchmark_percent: 6.1,
      },
    }),
    result("beat_ibov_5y", "no_data"),
    result("beat_ipca_12m", "ok", {
      current: 9.4,
      limit: 4.3,
      detail: {
        kind: "benchmark",
        benchmark: "ipca",
        horizon: "12m",
        from: "2025-10-31",
        to: "2026-10-31",
        portfolio_percent: 9.4,
        benchmark_percent: 4.3,
      },
    }),
    result("beat_ipca_5y", "no_data"),
    result("min_positive_years", "ok", {
      current: 100,
      detail: { kind: "years", positive: 2, total: 2 },
    }),
    result("max_volatility", "ok", { current: 8.4 }),
    result("max_loss", "warning", {
      current: 41.5,
      detail: {
        kind: "losses",
        assets: [{ asset_id: "a-btlg", label: "BTLG11", asset_class: "br_reits", loss_percent: 41.5 }],
      },
    }),
  ];

  const quality = [
    result("quality_roe", "warning", {
      current: 0,
      detail: { kind: "quality", covered: 1, total: 1, failing: ["ITSA4"] },
    }),
    result("quality_profit", "ok", { current: 100, detail: { kind: "quality", covered: 1, total: 1, failing: [] } }),
    result("quality_leverage", "no_data"),
    result("quality_margin", "ok", { current: 100, detail: { kind: "quality", covered: 1, total: 1, failing: [] } }),
    result("quality_cash", "no_data"),
    result("quality_recovery", "ok", {
      current: 0,
      detail: { kind: "quality", covered: 1, total: 1, failing: [] },
    }),
    result("quality_fii_pvp", "warning", {
      current: 40.5,
      detail: { kind: "quality", covered: 4, total: 4, failing: ["KNRI11", "HGLG11"] },
    }),
    result("quality_fii_vacancy", "ok", {
      current: 100,
      detail: { kind: "quality", covered: 3, total: 4, failing: [] },
    }),
  ];

  const dimensions: Dimension[] = [
    dimension("diversification", diversification),
    dimension("dividends", dividends),
    dimension("returns", returns),
    dimension("quality", quality),
  ];
  const attention = dimensions.flatMap((d) => d.rules).filter((r) => r.status === "warning").length;

  return {
    as_of: "2026-11-25",
    total_value: 84_300,
    attention_count: attention,
    dimensions,
    attention: {
      assessed: 5,
      without_data: 1,
      items: [
        {
          asset_id: "a-itsa",
          label: "ITSA4",
          asset_class: "br_stocks",
          points: [
            { kind: "low_roe", percent: 4.12 },
            { kind: "low_net_margin", percent: 8.33 },
            { kind: "debt_above_equity", times: 1.5 },
            { kind: "low_revenue_growth", percent: 2.34 },
          ],
        },
        {
          asset_id: "a-btlg",
          label: "BTLG11",
          asset_class: "br_reits",
          points: [{ kind: "income_drop", percent: 22.5 }],
        },
      ],
    },
    profile: {
      current: { fixed_income: 70.76, variable: 29.24, profile: "conservative" },
      target: { fixed_income: 35, variable: 65, profile: "moderate" },
      conservative_max: 30,
      moderate_max: 70,
    },
    highlights: {
      price_12m: {
        best: [
          { asset_id: "a-knri", label: "KNRI11", asset_class: "br_reits", change_percent: 18.4 },
          { asset_id: "a-itsa", label: "ITSA4", asset_class: "br_stocks", change_percent: 6.25 },
        ],
        worst: [{ asset_id: "a-btlg", label: "BTLG11", asset_class: "br_reits", change_percent: -12.8 }],
      },
      since_purchase: {
        best: [{ asset_id: "a-knri", label: "KNRI11", asset_class: "br_reits", change_percent: 24.5 }],
        worst: [{ asset_id: "a-btlg", label: "BTLG11", asset_class: "br_reits", change_percent: -41.5 }],
      },
    },
    references: {
      bazin_yield: 6,
      items: [
        {
          asset_id: "a-itsa",
          label: "ITSA4",
          asset_class: "br_stocks",
          unit_price: 11.2,
          max_price: 10,
          bazin: { price: 12.5, annual_average: 0.75, years: 4 },
          graham: 9.5,
        },
        {
          asset_id: "a-knri",
          label: "KNRI11",
          asset_class: "br_reits",
          unit_price: 160,
          max_price: null,
          bazin: { price: 140, annual_average: 8.4, years: 2 },
          graham: null,
        },
      ],
    },
    unclassified_count: 1,
    sources: defaultSectorSources(),
  };
}

type State = {
  analysis: Analysis;
  rules: RuleConfig[];
  bazinYield: number;
  /** O que a tela mandou em `PUT /analysis/rules`. */
  saved: ConfigRequest[];
};

function fresh(): State {
  return { analysis: defaultAnalysis(), rules: defaultAnalysisRules(), bazinYield: 6, saved: [] };
}

let state = fresh();

/** O que o servidor falso devolve, para o teste mudar. */
export function analysisState(): State {
  return state;
}

export function resetAnalysis() {
  state = fresh();
}

const problem = (detail: string) =>
  HttpResponse.json(
    { type: "urn:prumo:error:validation_failed", title: "Dados inválidos", status: 422, code: "validation_failed", detail },
    { status: 422, headers: { "Content-Type": "application/problem+json" } },
  );

export const analysisHandlers: HttpHandler[] = [
  http.get("/api/v1/analysis", () => HttpResponse.json(state.analysis)),

  http.get("/api/v1/analysis/rules", () =>
    HttpResponse.json({ rules: state.rules, bazin_yield: state.bazinYield } satisfies AnalysisConfig),
  ),

  http.put("/api/v1/analysis/rules", async ({ request }) => {
    const body = (await request.json()) as ConfigRequest;
    state.saved.push(body);
    for (const [rule, change] of Object.entries(body.rules ?? {})) {
      if (change.threshold !== undefined && change.threshold <= 0) {
        return problem(`${rule}: o limite precisa ser maior que 0.`);
      }
    }
    if (body.bazin_yield !== undefined && (body.bazin_yield <= 0 || body.bazin_yield > 100)) {
      return problem("bazin_yield: o rendimento-alvo precisa ser maior que 0 e até 100.");
    }
    for (const [rule, change] of Object.entries(body.rules ?? {})) {
      const stored = state.rules.find((r) => r.rule === rule);
      if (!stored) continue;
      if (change?.enabled !== undefined) stored.enabled = change.enabled;
      if (change?.threshold !== undefined) stored.threshold = change.threshold;
    }
    if (body.bazin_yield !== undefined) state.bazinYield = body.bazin_yield;
    return HttpResponse.json({ rules: state.rules, bazin_yield: state.bazinYield } satisfies AnalysisConfig);
  }),
];
