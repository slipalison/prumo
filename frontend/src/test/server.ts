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
import type { AllocationSettings } from "../api/allocationTypes";
import type { CreateTransactionRequest, Transaction, UpdateTransactionRequest } from "../api/ledgerTypes";
import { analysisHandlers } from "./analysisHandlers";
import { fundamentalsHandlers } from "./fundamentalsHandlers";
import { iconsHandlers } from "./iconsHandlers";
import { defaultIncomeAutoHandlers } from "./incomeAutoHandlers";
import { incomeHandlers } from "./incomeHandlers";
import { objectivesHandlers } from "./objectivesHandlers";
import { taxHandlers } from "./taxHandlers";
import {
  defaultAssets,
  defaultChecklists,
  defaultGoals,
  defaultTransactions,
  samplePlan,
  toPortfolio,
  TREASURY_AT,
  type StoredAsset,
} from "./fixtures";

/** Banco em memória que segue docs/api.md; cada teste começa do zero. */
export type FakeDb = {
  assets: StoredAsset[];
  /** `GET/PUT /settings/allocation`. */
  maxAssetPercent: number | null;
  goals: GoalTargets;
  checklists: Checklists;
  contributions: Contribution[];
  transactions: Transaction[];
  requests: Array<{ method: string; path: string; body: unknown }>;
  nextId: number;
};

function createDb(): FakeDb {
  return {
    assets: defaultAssets(),
    maxAssetPercent: null,
    goals: defaultGoals(),
    checklists: defaultChecklists(),
    contributions: [],
    transactions: defaultTransactions(),
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

/**
 * O livro de um ativo, refeito como o servidor faz (docs/api.md § Lançamentos):
 * custo médio, venda pelo custo médio, eventos que preservam o custo. Devolve
 * `null` se alguma data ficaria com posição negativa.
 */
function replay(list: Transaction[]): { quantity: number; cost: number | null; realized: number | null } | null {
  const sorted = [...list].sort((a, b) => (a.trade_date === b.trade_date ? 0 : a.trade_date < b.trade_date ? -1 : 1));
  let quantity = 0;
  let cost: number | null = 0;
  let realized: number | null = 0;
  for (const t of sorted) {
    const fx = t.fx_rate ?? 1;
    const gross = t.quantity * (t.unit_price ?? 0) * fx;
    switch (t.kind) {
      case "opening":
      case "buy":
      case "bonus":
        quantity += t.quantity;
        cost = cost === null || (t.kind === "opening" && t.unit_price === null) ? null : cost + gross + t.fees * fx;
        break;
      case "split":
        quantity += t.quantity;
        break;
      case "reverse_split":
        if (t.quantity >= quantity) return null;
        quantity -= t.quantity;
        break;
      case "sell":
      case "redemption": {
        if (t.quantity > quantity) return null;
        const removed: number | null = cost === null ? null : (cost * t.quantity) / quantity;
        realized = realized === null || removed === null ? null : realized + gross - t.fees * fx - removed;
        quantity -= t.quantity;
        cost = quantity === 0 ? 0 : cost === null || removed === null ? null : cost - removed;
        break;
      }
      case "amortization":
        if (t.quantity > quantity) return null;
        cost = cost === null ? null : Math.max(0, cost - gross);
        break;
      case "adjustment":
        if (quantity + t.quantity < 0) return null;
        quantity += t.quantity;
        if (quantity === 0) cost = 0;
        break;
    }
  }
  return { quantity, cost, realized };
}

/** Grava o livro do ativo se ele fecha; senão, nada muda e responde 422. */
function commitLedger(assetId: string, next: Transaction[]) {
  const stored = db.assets.find((a) => a.id === assetId);
  const position = replay(next.filter((t) => t.asset_id === assetId));
  if (!stored || !position) {
    return problem(422, "exceeds_position", "O lancamento passa da posicao", "A venda passa da posicao naquela data.");
  }
  db.transactions = next;
  stored.quantity = position.quantity;
  stored.cost_basis = position.cost;
  stored.realized_gain = position.realized;
  return null;
}
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
      max_price: body.max_price ?? null,
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
    if (body.max_price !== undefined) stored.max_price = body.max_price;
    if (body.tax_instrument === null) delete stored.tax_instrument;
    else if (body.tax_instrument !== undefined) stored.tax_instrument = body.tax_instrument;
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
      cash_used: body.cash_used ?? 0,
      invested: items.reduce((acc, i) => acc + i.value, 0),
      items,
    };
    db.contributions.unshift(contribution);
    return HttpResponse.json(contribution, { status: 201 });
  }),

  http.get("/api/v1/contributions", () => HttpResponse.json(db.contributions)),

  // As cotas do livro na data com: na carteira falsa, as de hoje (o fake não guarda datas).
  http.get("/api/v1/income/entitlement", ({ request }) => {
    const asset = new URL(request.url).searchParams.get("asset");
    const stored = db.assets.find((a) => a.id === asset);
    if (!stored) return HttpResponse.json({ code: "not_found", status: 404 }, { status: 404 });
    return HttpResponse.json({ quantity: stored.quantity, record_date: "2026-11-24" });
  }),

  http.get("/api/v1/transactions", ({ request }) => {
    const assetId = new URL(request.url).searchParams.get("asset_id");
    const list = db.transactions
      .filter((t) => !assetId || t.asset_id === assetId)
      .sort((a, b) => (a.trade_date === b.trade_date ? 0 : a.trade_date < b.trade_date ? 1 : -1));
    return HttpResponse.json(list);
  }),

  http.post("/api/v1/transactions", async ({ request }) => {
    const body = (await request.json()) as CreateTransactionRequest;
    const asset = db.assets.find((a) => a.id === body.asset_id);
    if (!asset) return problem(404, "not_found", "Não encontrado");
    const transaction: Transaction = {
      id: `t-${db.nextId++}`,
      asset_id: asset.id,
      symbol: asset.symbol,
      display_name: asset.display_name,
      asset_class: asset.asset_class,
      kind: body.kind,
      trade_date: body.trade_date,
      quantity: body.quantity,
      unit_price: body.unit_price ?? (isValueClass(asset.asset_class) && body.kind !== "opening" ? 1 : null),
      fees: body.fees ?? 0,
      currency: body.currency ?? "BRL",
      fx_rate: body.fx_rate ?? null,
      source: "manual",
      external_ref: null,
      contribution_id: null,
      created_at: "2026-11-25T12:00:00Z",
    };
    const refused = commitLedger(asset.id, [...db.transactions, transaction]);
    return refused ?? HttpResponse.json(transaction, { status: 201 });
  }),

  http.patch("/api/v1/transactions/:id", async ({ request, params }) => {
    const current = db.transactions.find((t) => t.id === params.id);
    if (!current?.asset_id) return problem(404, "not_found", "Não encontrado");
    const body = (await request.json()) as UpdateTransactionRequest;
    const updated: Transaction = { ...current, ...body };
    const refused = commitLedger(
      current.asset_id,
      db.transactions.map((t) => (t.id === current.id ? updated : t)),
    );
    return refused ?? HttpResponse.json(updated);
  }),

  http.delete("/api/v1/transactions/:id", ({ params }) => {
    const current = db.transactions.find((t) => t.id === params.id);
    if (!current?.asset_id) return problem(404, "not_found", "Não encontrado");
    const refused = commitLedger(
      current.asset_id,
      db.transactions.filter((t) => t.id !== current.id),
    );
    return refused ?? new HttpResponse(null, { status: 204 });
  }),

  http.get("/api/v1/symbols", ({ request }) => {
    const query = (new URL(request.url).searchParams.get("query") ?? "").toUpperCase();
    return HttpResponse.json(
      [...BOND_SUGGESTIONS, ...SYMBOLS].filter(
        (s) => s.symbol.includes(query) || s.display_name.toUpperCase().includes(query),
      ),
    );
  }),

  http.get("/api/v1/settings/allocation", () => HttpResponse.json({ max_asset_percent: db.maxAssetPercent })),

  http.put("/api/v1/settings/allocation", async ({ request }) => {
    const body = (await request.json()) as AllocationSettings;
    const percent = body.max_asset_percent;
    if (percent !== null && (percent <= 0 || percent > 100)) {
      return problem(422, "validation_failed", "Dado inválido", "O limite por ativo vai de 0,01% a 100%.");
    }
    db.maxAssetPercent = percent;
    return HttpResponse.json({ max_asset_percent: percent });
  }),

  http.post("/api/v1/quotes/refresh", () =>
    HttpResponse.json({ refreshed: db.assets.filter((a) => a.symbol).length, failed: [] }),
  ),
];

export const server = setupServer(
  ...handlers,
  ...analysisHandlers,
  ...iconsHandlers,
  ...fundamentalsHandlers,
  ...incomeHandlers,
  ...defaultIncomeAutoHandlers,
  ...objectivesHandlers,
  ...taxHandlers,
);
