import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";
import type {
  AssetClass,
  ChecklistKind,
  Checklists,
  Contribution,
  ContributionRequest,
  CreateAssetRequest,
  GoalTargets,
  GoalsRequest,
  SymbolSuggestion,
  UpdateAssetRequest,
} from "../api/types";
import { ASSET_CLASSES } from "../api/types";
import {
  defaultAssets,
  defaultChecklists,
  defaultGoals,
  samplePlan,
  toAsset,
  toPortfolio,
  type StoredAsset,
} from "./fixtures";

/** Banco em memória que segue docs/api.md; cada teste começa do zero. */
export type FakeDb = {
  assets: StoredAsset[];
  goals: GoalTargets;
  checklists: Checklists;
  contributions: Contribution[];
  requests: Array<{ method: string; path: string; body: unknown }>;
  nextId: number;
};

function createDb(): FakeDb {
  return {
    assets: defaultAssets(),
    goals: defaultGoals(),
    checklists: defaultChecklists(),
    contributions: [],
    requests: [],
    nextId: 1,
  };
}

let db = createDb();

export function getDb(): FakeDb {
  return db;
}

export function resetDb(overrides: Partial<FakeDb> = {}): FakeDb {
  db = { ...createDb(), ...overrides };
  return db;
}

const SYMBOLS: SymbolSuggestion[] = [
  { symbol: "KNRI11", display_name: "Kinea Renda Imobiliária", exchange: "SAO", suggested_class: "br_reits" },
  { symbol: "KNCR11", display_name: "Kinea Rendimentos Imobiliários", exchange: "SAO", suggested_class: "br_reits" },
  { symbol: "ITSA4", display_name: "Itaúsa", exchange: "SAO", suggested_class: "br_stocks" },
  { symbol: "VOO", display_name: "Vanguard S&P 500 ETF", exchange: "PCX", suggested_class: "intl_stocks" },
  { symbol: "ETH", display_name: "Ethereum", exchange: "CCC", suggested_class: "crypto" },
];

const problem = (status: number, code: string, title: string, detail?: string) =>
  HttpResponse.json(
    { type: `urn:prumo:error:${code}`, title, status, code, ...(detail ? { detail } : {}) },
    { status, headers: { "Content-Type": "application/problem+json" } },
  );

const classTotal = (assetClass: AssetClass) => {
  const p = toPortfolio(db.assets, db.goals, db.checklists);
  return p.classes.find((c) => c.asset_class === assetClass)?.current_value ?? 0;
};

async function record(request: Request) {
  const url = new URL(request.url);
  let body: unknown = undefined;
  if (request.method !== "GET" && request.method !== "DELETE") {
    try {
      body = await request.clone().json();
    } catch {
      body = undefined;
    }
  }
  db.requests.push({ method: request.method, path: url.pathname + url.search, body });
}

const isValueClass = (c: AssetClass) => c === "fixed_income" || c === "intl_fixed_income";

export const handlers = [
  http.all("/api/v1/*", async ({ request }) => {
    await record(request);
    return undefined;
  }),
  http.get("/api/v1/info", () => HttpResponse.json({ version: "0.1.0", commit: "abc1234" })),
  http.get("/api/v1/me", () => HttpResponse.json({ email: "pessoa@exemplo.com" })),
  http.get("/api/v1/portfolio", () => HttpResponse.json(toPortfolio(db.assets, db.goals, db.checklists))),

  http.put("/api/v1/goals", async ({ request }) => {
    const body = (await request.json()) as GoalsRequest;
    const targets = Object.fromEntries(ASSET_CLASSES.map((c) => [c, body.targets[c] ?? 0])) as GoalTargets;
    const sum = Object.values(targets).reduce((acc, v) => acc + v, 0);
    if (Math.abs(sum - 100) > 0.001) {
      return problem(422, "targets_must_sum_100", "As metas precisam somar 100%", `A soma enviada foi ${sum}%.`);
    }
    db.goals = targets;
    return HttpResponse.json({ targets });
  }),

  http.post("/api/v1/assets", async ({ request }) => {
    const body = (await request.json()) as CreateAssetRequest;
    const symbol = body.symbol?.toUpperCase() ?? null;
    if (symbol && db.assets.some((a) => a.asset_class === body.asset_class && a.symbol === symbol)) {
      return problem(409, "asset_already_exists", "Ativo já existe");
    }
    const valueBased = isValueClass(body.asset_class);
    const stored: StoredAsset = {
      id: `new-${db.nextId++}`,
      asset_class: body.asset_class,
      symbol: valueBased ? null : symbol,
      display_name: body.display_name ?? symbol ?? "Ativo",
      quantity: valueBased ? (body.amount ?? 0) : (body.quantity ?? 0),
      quote_price: valueBased ? 1 : 100,
      manual_price: valueBased ? 1 : (body.manual_price ?? null),
      currency: "BRL",
      score_override: body.score_override ?? null,
      answers: {},
    };
    db.assets.push(stored);
    return HttpResponse.json(toAsset(stored, db.checklists, classTotal(stored.asset_class)), { status: 201 });
  }),

  http.patch("/api/v1/assets/:id", async ({ request, params }) => {
    const stored = db.assets.find((a) => a.id === params.id);
    if (!stored) return problem(404, "not_found", "Não encontrado");
    const body = (await request.json()) as UpdateAssetRequest;
    if (body.quantity !== undefined) stored.quantity = body.quantity;
    if (body.amount !== undefined) stored.quantity = body.amount;
    if (body.display_name !== undefined) stored.display_name = body.display_name;
    if (body.manual_price !== undefined) stored.manual_price = body.manual_price;
    if (body.score_override !== undefined) stored.score_override = body.score_override;
    if (body.answers) {
      for (const [id, answer] of Object.entries(body.answers)) {
        if (answer === null) delete stored.answers[id];
        else stored.answers[id] = answer;
      }
    }
    return HttpResponse.json(toAsset(stored, db.checklists, classTotal(stored.asset_class)));
  }),

  http.delete("/api/v1/assets/:id", ({ params }) => {
    const before = db.assets.length;
    db.assets = db.assets.filter((a) => a.id !== params.id);
    if (db.assets.length === before) return problem(404, "not_found", "Não encontrado");
    return new HttpResponse(null, { status: 204 });
  }),

  http.post("/api/v1/checklists/:kind/questions", async ({ request, params }) => {
    const kind = params.kind as ChecklistKind;
    const { prompt } = (await request.json()) as { prompt: string };
    const question = { id: `q-${db.nextId++}`, position: db.checklists[kind].length + 1, prompt };
    db.checklists[kind].push(question);
    return HttpResponse.json(question, { status: 201 });
  }),

  http.patch("/api/v1/checklist-questions/:id", async ({ request, params }) => {
    const { prompt } = (await request.json()) as { prompt: string };
    for (const kind of ["stocks", "real_estate"] as const) {
      const question = db.checklists[kind].find((q) => q.id === params.id);
      if (question) {
        question.prompt = prompt;
        return HttpResponse.json(question);
      }
    }
    return problem(404, "not_found", "Não encontrado");
  }),

  http.delete("/api/v1/checklist-questions/:id", ({ params }) => {
    for (const kind of ["stocks", "real_estate"] as const) {
      db.checklists[kind] = db.checklists[kind].filter((q) => q.id !== params.id);
    }
    return new HttpResponse(null, { status: 204 });
  }),

  http.post("/api/v1/checklists/:kind/reset", ({ params }) => {
    const kind = params.kind as ChecklistKind;
    db.checklists[kind] = defaultChecklists()[kind];
    return HttpResponse.json(db.checklists[kind]);
  }),

  http.post("/api/v1/plans", async ({ request }) => {
    const { amount } = (await request.json()) as { amount: number };
    return HttpResponse.json(samplePlan(amount));
  }),

  http.post("/api/v1/contributions", async ({ request }) => {
    const body = (await request.json()) as ContributionRequest;
    const items = body.items.map((item) => {
      const stored = db.assets.find((a) => a.id === item.asset_id);
      if (stored) stored.quantity += item.quantity;
      return {
        asset_id: item.asset_id,
        symbol: stored?.symbol ?? null,
        display_name: stored?.display_name ?? "",
        quantity: item.quantity,
        unit_price: item.unit_price,
        value: Math.round(item.quantity * item.unit_price * 100) / 100,
      };
    });
    const contribution: Contribution = {
      id: `c-${db.nextId++}`,
      created_at: "2026-11-25T12:00:00Z",
      amount: body.amount,
      invested: items.reduce((acc, i) => acc + i.value, 0),
      items,
    };
    db.contributions.unshift(contribution);
    return HttpResponse.json(contribution, { status: 201 });
  }),

  http.get("/api/v1/contributions", () => HttpResponse.json(db.contributions)),

  http.get("/api/v1/symbols", ({ request }) => {
    const query = (new URL(request.url).searchParams.get("query") ?? "").toUpperCase();
    return HttpResponse.json(
      SYMBOLS.filter((s) => s.symbol.includes(query) || s.display_name.toUpperCase().includes(query)),
    );
  }),

  http.post("/api/v1/quotes/refresh", () =>
    HttpResponse.json({ refreshed: db.assets.filter((a) => a.symbol).length, failed: [] }),
  ),
];

export const server = setupServer(...handlers);
