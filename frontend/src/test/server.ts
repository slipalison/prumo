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
import type { HealthRule, HealthWarning, RuleId, RulesRequest, SectorSourceInfo } from "../api/healthTypes";
import {
  defaultAssets,
  defaultChecklists,
  defaultGoals,
  defaultHealthRules,
  defaultSectorSources,
  defaultWarnings,
  samplePlan,
  toPortfolio,
  TREASURY_AT,
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
  /** Regras de saúde como a pessoa as deixou e os avisos que o servidor acha deles. */
  healthRules: HealthRule[];
  warnings: HealthWarning[];
  sectorSources: SectorSourceInfo[];
  unclassified: number;
};

function createDb(): FakeDb {
  return {
    assets: defaultAssets(),
    goals: defaultGoals(),
    checklists: defaultChecklists(),
    contributions: [],
    requests: [],
    nextId: 1,
    healthRules: defaultHealthRules(),
    warnings: defaultWarnings(),
    sectorSources: defaultSectorSources(),
    unclassified: 1,
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
  { symbol: "KNRI11", display_name: "Kinea Renda Imobiliária", exchange: "SAO", suggested_class: "br_reits", listed_on_b3: true, treasury: false },
  { symbol: "KNCR11", display_name: "Kinea Rendimentos Imobiliários", exchange: "SAO", suggested_class: "br_reits", listed_on_b3: true, treasury: false },
  { symbol: "ITSA4", display_name: "Itaúsa", exchange: "SAO", suggested_class: "br_stocks", listed_on_b3: true, treasury: false },
  { symbol: "VOO", display_name: "Vanguard S&P 500 ETF", exchange: "PCX", suggested_class: "intl_stocks", listed_on_b3: false, treasury: false },
  { symbol: "ETH", display_name: "Ethereum", exchange: "CCC", suggested_class: "crypto", listed_on_b3: false, treasury: false },
  { symbol: "QBTC11", display_name: "QR CME CF Bitcoin Reference Rate", exchange: "SAO", suggested_class: "crypto", listed_on_b3: true, treasury: false },
];

/** Os títulos do Tesouro que a busca devolve (nome e preço de resgate de 19/11/2026). */
export const BONDS = [
  { symbol: "TD-SELIC-20290301", display_name: "Tesouro Selic 2029", price: 20305.65 },
  { symbol: "TD-IPCA-20350515", display_name: "Tesouro IPCA+ 2035", price: 2500.95 },
];

const BOND_SUGGESTIONS: SymbolSuggestion[] = BONDS.map((b) => ({
  symbol: b.symbol,
  display_name: b.display_name,
  exchange: "Tesouro Direto",
  suggested_class: "fixed_income",
  listed_on_b3: false,
  treasury: true,
}));

const problem = (status: number, code: string, title: string, detail?: string) =>
  HttpResponse.json(
    { type: `urn:prumo:error:${code}`, title, status, code, ...(detail ? { detail } : {}) },
    { status, headers: { "Content-Type": "application/problem+json" } },
  );

/** O ativo como o servidor o devolve: com a carteira inteira em volta (% da classe, ideal). */
const assetView = (stored: StoredAsset) =>
  toPortfolio(db.assets, db.goals, db.checklists).assets.find((a) => a.id === stored.id);

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
const isNativeB3 = (c: AssetClass) => c === "br_stocks" || c === "br_reits";

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
    const listed = isNativeB3(body.asset_class) || body.listed_on_b3 === true;
    const bond = body.treasury === true ? BONDS.find((b) => b.symbol === symbol) : undefined;
    const valueBased = isValueClass(body.asset_class) && !listed && body.treasury !== true;
    const stored: StoredAsset = {
      id: `new-${db.nextId++}`,
      asset_class: body.asset_class,
      symbol: valueBased ? null : symbol,
      display_name: bond?.display_name ?? body.display_name ?? symbol ?? "Ativo",
      quantity: valueBased ? (body.amount ?? 0) : (body.quantity ?? 0),
      quote_price: bond ? bond.price : valueBased ? 1 : 100,
      manual_price: valueBased ? 1 : (body.manual_price ?? null),
      currency: "BRL",
      score_override: body.score_override ?? null,
      answers: {},
      listed_on_b3: listed,
      ...(body.treasury === true ? { treasury: true, quote_at: TREASURY_AT } : {}),
    };
    db.assets.push(stored);
    return HttpResponse.json(assetView(stored), { status: 201 });
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
    if (body.listed_on_b3 !== undefined) {
      stored.listed_on_b3 = body.listed_on_b3;
      // Na B3 o ativo tem cotação; fora dela, um ticker da B3 fica sem preço.
      stored.quote_price = body.listed_on_b3 ? 52 : null;
    }
    if (body.answers) {
      for (const [id, answer] of Object.entries(body.answers)) {
        if (answer === null) delete stored.answers[id];
        else stored.answers[id] = answer;
      }
    }
    return HttpResponse.json(assetView(stored));
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
      [...BOND_SUGGESTIONS, ...SYMBOLS].filter(
        (s) => s.symbol.includes(query) || s.display_name.toUpperCase().includes(query),
      ),
    );
  }),

  http.get("/api/v1/health", () => {
    const on = new Set<RuleId>(db.healthRules.filter((r) => r.enabled).map((r) => r.rule));
    const warnings = db.warnings.filter((w) => on.has(w.rule));
    return HttpResponse.json({
      total_value: toPortfolio(db.assets, db.goals, db.checklists).total_value,
      warnings,
      rules: db.healthRules.map((r) => ({
        ...r,
        status: !r.enabled ? "off" : warnings.some((w) => w.rule === r.rule) ? "warning" : "ok",
      })),
      unclassified_count: db.unclassified,
      sources: db.sectorSources,
    });
  }),

  http.get("/api/v1/health/rules", () => HttpResponse.json({ rules: db.healthRules })),

  http.put("/api/v1/health/rules", async ({ request }) => {
    const { rules } = (await request.json()) as RulesRequest;
    for (const [rule, change] of Object.entries(rules) as Array<[RuleId, { enabled?: boolean; threshold?: number }]>) {
      if (change.threshold !== undefined && change.threshold <= 0) {
        return problem(422, "validation_failed", "Dados inválidos", `${rule}: o limite precisa ser maior que 0.`);
      }
    }
    for (const [rule, change] of Object.entries(rules) as Array<[RuleId, { enabled?: boolean; threshold?: number }]>) {
      const stored = db.healthRules.find((r) => r.rule === rule);
      if (!stored) continue;
      if (change.enabled !== undefined) stored.enabled = change.enabled;
      if (change.threshold !== undefined) stored.threshold = change.threshold;
    }
    return HttpResponse.json({ rules: db.healthRules });
  }),

  http.post("/api/v1/quotes/refresh", () =>
    HttpResponse.json({ refreshed: db.assets.filter((a) => a.symbol).length, failed: [] }),
  ),
];

export const server = setupServer(...handlers);
