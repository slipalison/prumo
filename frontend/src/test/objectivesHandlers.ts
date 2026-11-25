import { http, HttpResponse, type HttpHandler } from "msw";
import type {
  Assumptions,
  CreateObjectiveRequest,
  ImpactEntry,
  Objective,
  ObjectivesResponse,
  UpdateObjectiveRequest,
} from "../api/objectivesTypes";

export const sampleAssumptions: Assumptions = {
  monthly_contribution: 2400,
  contribution_source: "ledger",
  annual_return_percent: 9.8,
  return_source: "performance",
};

/** Objetivo FICTÍCIO em andamento: patrimônio total, aporte do livro, rentabilidade do Desempenho. */
export function sampleObjective(overrides: Partial<Objective> = {}): Objective {
  return {
    id: "o-1",
    kind: "net_worth",
    asset_class: null,
    target: 1_000_000,
    monthly_contribution: null,
    expected_return: null,
    current: 250_000,
    percent: 25,
    status: "active",
    achieved_at: null,
    created_at: "2026-11-01T12:00:00Z",
    assumptions: sampleAssumptions,
    projection: { state: "eta", months: 52, month: "2031-03" },
    ...overrides,
  };
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

export const defaultObjectivesResponse: ObjectivesResponse = {
  defaults: { annual_return_percent: 9.8, return_source: "performance" },
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
      const created = sampleObjective({
        id: `o-new-${state.list.length + 1}`,
        kind: body.kind,
        asset_class: body.asset_class ?? null,
        target: body.target,
        monthly_contribution: body.monthly_contribution ?? null,
        expected_return: body.expected_return ?? null,
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
      const next: Objective = {
        ...current,
        target: body.target ?? current.target,
        monthly_contribution:
          body.monthly_contribution === undefined ? current.monthly_contribution : body.monthly_contribution,
        expected_return: body.expected_return === undefined ? current.expected_return : body.expected_return,
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
