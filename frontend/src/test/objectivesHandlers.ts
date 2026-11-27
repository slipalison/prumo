import { http, HttpResponse, type HttpHandler } from "msw";
import type {
  Assumptions,
  CreateObjectiveRequest,
  Curve,
  ImpactEntry,
  Objective,
  ObjectivePremises,
  ObjectivesDefaults,
  ObjectivesResponse,
  Scenarios,
  UpdateObjectiveRequest,
} from "../api/objectivesTypes";

export const sampleAssumptions: Assumptions = {
  monthly_contribution: 2400,
  contribution_source: "ledger",
  real_return_percent: 5.1,
  return_source: "performance",
  inflation_percent: 4.25,
  inflation_source: "focus",
  reinvest_income: true,
  reinvest_source: "default",
  income_growth_percent: 0,
  income_growth_source: "default",
  income_yield_percent: 7.9,
  income_yield_source: "measured",
  scenario_spread_percent: 2,
  spread_source: "default",
  starting_value: 250_000,
  starting_income: 1_645.83,
};

export const noPremises: ObjectivePremises = {
  monthly_contribution: null,
  real_return: null,
  inflation: null,
  reinvest_income: null,
  income_growth: null,
  income_yield: null,
  scenario_spread: null,
};

export const sampleScenarios: Scenarios = {
  pessimistic: { nominal: { state: "eta", months: 70, month: "2032-09" }, today: { state: "eta", months: 96, month: "2034-11" } },
  base: { nominal: { state: "eta", months: 52, month: "2031-03" }, today: { state: "eta", months: 66, month: "2032-05" } },
  optimistic: { nominal: { state: "eta", months: 40, month: "2030-03" }, today: { state: "eta", months: 50, month: "2031-01" } },
};

/** Uma curva FICTÍCIA de 82 meses, de 12 em 12 (e o último). */
export function sampleCurve(start = 250_000, measure: Curve["measure"] = "value"): Curve {
  const months = [0, 12, 24, 36, 48, 60, 72, 82];
  return {
    measure,
    points: months.map((m) => {
      const month = new Date(Date.UTC(2026, 10 + m, 1)).toISOString().slice(0, 7);
      const base = start * (1 + m / 50);
      const inflate = 1.0425 ** (m / 12);
      const low = base * 0.98 ** (m / 12);
      const high = base * 1.02 ** (m / 12);
      return {
        months: m,
        month,
        low: low * inflate,
        base: base * inflate,
        high: high * inflate,
        low_today: low,
        base_today: base,
        high_today: high,
      };
    }),
  };
}

/** Objetivo FICTÍCIO em andamento: patrimônio total, aporte do livro, rentabilidade do Desempenho. */
export function sampleObjective(overrides: Partial<Objective> = {}): Objective {
  return {
    id: "o-1",
    kind: "net_worth",
    asset_class: null,
    target: 1_000_000,
    income_mode: null,
    premises: noPremises,
    current: 250_000,
    percent: 25,
    basis: null,
    status: "active",
    achieved_at: null,
    created_at: "2026-11-01T12:00:00Z",
    assumptions: sampleAssumptions,
    projection: { state: "eta", months: 52, month: "2031-03" },
    scenarios: sampleScenarios,
    curve: sampleCurve(),
    ...overrides,
  };
}

/** A meta de renda FICTÍCIA: R$ 3.000 por mês, pela estimativa. */
export function sampleIncomeObjective(overrides: Partial<Objective> = {}): Objective {
  return sampleObjective({
    id: "o-renda",
    kind: "monthly_income",
    target: 3000,
    income_mode: "average",
    current: 1_645.83,
    percent: 54.86,
    basis: "estimate",
    curve: sampleCurve(1_645.83, "income_average"),
    ...overrides,
  });
}

export function achievedObjective(overrides: Partial<Objective> = {}): Objective {
  return sampleObjective({
    id: "o-done",
    kind: "class_value",
    asset_class: "br_reits",
    target: 50_000,
    current: 61_000,
    percent: 100,
    status: "achieved",
    achieved_at: "2026-10-15T15:00:00Z",
    assumptions: null,
    projection: { state: "reached", months: null, month: null },
    scenarios: null,
    curve: null,
    ...overrides,
  });
}

export function sampleImpact(overrides: Partial<ImpactEntry> = {}): ImpactEntry {
  return {
    id: "o-1",
    kind: "net_worth",
    asset_class: null,
    target: 1_000_000,
    current: 250_000,
    after: 256_000,
    percent_before: 25,
    percent_after: 25.6,
    assumptions: sampleAssumptions,
    projection: { state: "eta", months: 51, month: "2031-02" },
    ...overrides,
  };
}

export const sampleDefaults: ObjectivesDefaults = {
  real_return_percent: 5.1,
  return_source: "performance",
  inflation_percent: 4.25,
  inflation_source: "focus",
  inflation_years: [2027, 2030],
  reinvest_income: true,
  income_growth_percent: 0,
  income_yield_percent: 7.9,
  scenario_spread_percent: 2,
  focus: { survey_date: "2026-11-19", fetched_at: "2026-11-27T09:00:00Z" },
};

export const defaultObjectivesResponse: ObjectivesResponse = {
  defaults: sampleDefaults,
  objectives: [],
};

/** O padrão das outras telas: nenhum objetivo, então o Aportar não mostra o cartão. */
export const objectivesHandlers = [
  http.get("/api/v1/objectives", () => HttpResponse.json(defaultObjectivesResponse)),
  http.post("/api/v1/objectives/impact", () => HttpResponse.json({ objectives: [] })),
];

export type FakeObjectives = {
  handlers: HttpHandler[];
  list: Objective[];
  requests: Array<{ method: string; path: string; body: unknown }>;
};

type PremiseKey = keyof ObjectivePremises;
const PREMISE_KEYS: PremiseKey[] = [
  "monthly_contribution",
  "real_return",
  "inflation",
  "reinvest_income",
  "income_growth",
  "income_yield",
  "scenario_spread",
];

/**
 * Objetivos em memória, no formato do servidor: criar, trocar e apagar mexem
 * na lista, e `impact` devolve o que o teste pôs em `impact`.
 */
export function fakeObjectives(initial: Objective[], impact: ImpactEntry[] = []): FakeObjectives {
  const state: FakeObjectives = { handlers: [], list: [...initial], requests: [] };
  const track = (method: string, path: string, body: unknown) => state.requests.push({ method, path, body });

  state.handlers = [
    http.get("/api/v1/objectives", () => HttpResponse.json({ ...defaultObjectivesResponse, objectives: state.list })),
    http.post("/api/v1/objectives", async ({ request }) => {
      const body = (await request.json()) as CreateObjectiveRequest;
      track("POST", "/api/v1/objectives", body);
      const premises = { ...noPremises };
      for (const key of PREMISE_KEYS) {
        const value = body[key];
        if (value !== undefined) (premises as Record<PremiseKey, unknown>)[key] = value;
      }
      const created = sampleObjective({
        id: `o-new-${state.list.length + 1}`,
        kind: body.kind,
        asset_class: body.asset_class ?? null,
        target: body.target,
        income_mode: body.kind === "monthly_income" ? (body.income_mode ?? "average") : null,
        premises,
        current: 0,
        percent: 0,
      });
      state.list.push(created);
      return HttpResponse.json(created, { status: 201 });
    }),
    http.patch("/api/v1/objectives/:id", async ({ request, params }) => {
      const body = (await request.json()) as UpdateObjectiveRequest;
      track("PATCH", `/api/v1/objectives/${String(params.id)}`, body);
      const index = state.list.findIndex((o) => o.id === params.id);
      const current = state.list[index];
      if (!current) return new HttpResponse(null, { status: 404 });
      const premises = { ...current.premises };
      for (const key of PREMISE_KEYS) {
        const value = body[key];
        if (value !== undefined) (premises as Record<PremiseKey, unknown>)[key] = value;
      }
      const next: Objective = {
        ...current,
        target: body.target ?? current.target,
        income_mode: body.income_mode === undefined ? current.income_mode : (body.income_mode ?? "average"),
        premises,
      };
      state.list[index] = next;
      return HttpResponse.json(next);
    }),
    http.delete("/api/v1/objectives/:id", ({ params }) => {
      track("DELETE", `/api/v1/objectives/${String(params.id)}`, undefined);
      state.list = state.list.filter((o) => o.id !== params.id);
      return new HttpResponse(null, { status: 204 });
    }),
    http.post("/api/v1/objectives/impact", async ({ request }) => {
      track("POST", "/api/v1/objectives/impact", await request.json());
      return HttpResponse.json({ objectives: impact });
    }),
  ];
  return state;
}
