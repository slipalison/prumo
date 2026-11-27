import type { Ceiling } from "../api/ceilingTypes";
import type { Transaction } from "../api/ledgerTypes";
import {
  ASSET_CLASSES,
  type Asset,
  type AssetClass,
  type Checklists,
  type GoalTargets,
  type Plan,
  type Portfolio,
  type PriceSource,
  type Profit,
  type ScoreSource,
} from "../api/types";
import type { TaxInstrument } from "../api/taxTypes";
import { checklistScore } from "../lib/score";

/** Ativo como o banco falso guarda: o que o servidor calcula sai em `toPortfolio`. */
export type StoredAsset = {
  id: string;
  asset_class: AssetClass;
  symbol: string | null;
  display_name: string;
  quantity: number;
  quote_price: number | null;
  manual_price: number | null;
  currency: string;
  score_override: number | null;
  answers: Record<string, boolean>;
  /** Ausente = o mercado natural da classe (B3 em Ações BR e FIIs). */
  listed_on_b3?: boolean;
  /** Título do Tesouro Direto: `symbol` é a chave, o preço vem da tabela diária. */
  treasury?: boolean;
  /** De quando é a cotação, se não for `QUOTES_AT` (o Tesouro: a data-base). */
  quote_at?: string;
  /** Setor da fonte pública; ausente = sem setor. */
  sector?: string;
  subsector?: string;
  segment?: string;
  /** O teto de hoje, como o servidor devolve; ausente = sem teto. */
  ceiling?: Ceiling | null;
  /** Custo do livro, em reais. Ausente ou `null` = desconhecido (com quantidade). */
  cost_basis?: number | null;
  realized_gain?: number | null;
  /** Instrumento fiscal escolhido; ausente = o padrão. */
  tax_instrument?: TaxInstrument;
};

/** O padrão do instrumento fiscal, pela classe e pela B3 (o servidor também lê o nome). */
export function defaultTaxInstrument(stored: StoredAsset): TaxInstrument {
  const listed = stored.listed_on_b3 ?? nativeB3(stored.asset_class);
  switch (stored.asset_class) {
    case "br_stocks":
      return "stock";
    case "br_reits":
      return "real_estate_fund";
    case "crypto":
      return listed ? "etf" : "crypto";
    case "intl_stocks":
    case "us_reits":
      return listed ? "bdr" : "unsupported";
    default:
      return "unsupported";
  }
}

const nativeB3 = (assetClass: AssetClass) => assetClass === "br_stocks" || assetClass === "br_reits";

export const QUOTES_AT = "2026-11-25T11:30:00Z";
/** A data-base de um preço do Tesouro: a meia-noite de 19/11 em Brasília. */
export const TREASURY_AT = "2026-11-19T03:00:00Z";

export function defaultChecklists(): Checklists {
  return {
    stocks: [
      { id: "s1", position: 1, prompt: "Lucro líquido positivo em pelo menos 4 dos últimos 5 anos?" },
      { id: "s2", position: 2, prompt: "ROE médio acima de 10% nos últimos 5 anos?" },
      { id: "s3", position: 3, prompt: "Dívida líquida/EBITDA abaixo de 2?" },
    ],
    real_estate: [
      { id: "r1", position: 1, prompt: "Os imóveis ficam em regiões de alta demanda?" },
      { id: "r2", position: 2, prompt: "Paga rendimentos mensais há mais de 4 anos sem cortes bruscos?" },
      { id: "r3", position: 3, prompt: "Os imóveis são modernos e sem reformas pesadas pela frente?" },
      { id: "r4", position: 4, prompt: "O dividend yield está na média do segmento ou acima?" },
      { id: "r5", position: 5, prompt: "Nenhum inquilino responde por mais de 25% da receita?" },
      { id: "r6", position: 6, prompt: "Está negociado com P/VP abaixo de 1?" },
    ],
  };
}

const allYes = { r1: true, r2: true, r3: true, r4: true, r5: true, r6: true };

/**
 * Carteira FICTÍCIA de exemplo (~R$ 120 mil, números redondos). Tickers e
 * preços são públicos; nenhuma posição aqui é de alguém de verdade.
 */
export function defaultAssets(): StoredAsset[] {
  const fii = (id: string, symbol: string, name: string, quantity: number, price: number, answers: Record<string, boolean>): StoredAsset => ({
    id,
    asset_class: "br_reits",
    symbol,
    display_name: name,
    quantity,
    quote_price: price,
    manual_price: null,
    currency: "BRL",
    score_override: null,
    answers,
  });
  return [
    {
      id: "a-itsa",
      asset_class: "br_stocks",
      symbol: "ITSA4",
      display_name: "Itaúsa",
      quantity: 2500,
      quote_price: 11.2,
      manual_price: null,
      currency: "BRL",
      score_override: null,
      answers: { s1: true, s2: true, s3: true },
      sector: "Financeiro",
      subsector: "Holdings Diversificadas",
      segment: "Holdings Diversificadas",
      cost_basis: 25000,
    },
    { ...fii("a-knri", "KNRI11", "Kinea Renda Imobiliária", 10, 142, allYes), sector: "Tijolo", segment: "Multicategoria", cost_basis: 1300 },
    { ...fii("a-hglg", "HGLG11", "Pátria Log", 12, 140.8, allYes), sector: "Tijolo", segment: "Multicategoria" },
    { ...fii("a-btlg", "BTLG11", "BTG Pactual Logística", 15, 94.55, { ...allYes, r6: false }), sector: "Tijolo", segment: "Multicategoria" },
    // Fora da referência (fundo sem informe): fica "sem setor".
    fii("a-trxf", "TRXF11", "TRX Real Estate", 20, 53.81, { ...allYes, r5: false }),
    { ...fii("a-mxrf", "MXRF11", "Maxi Renda", 100, 8.37, { r1: false, r2: true, r3: false, r4: true, r5: true, r6: false }), sector: "Papel", segment: "Logística" },
    {
      id: "a-btc",
      asset_class: "crypto",
      symbol: "BTC",
      display_name: "Bitcoin",
      quantity: 0.002,
      quote_price: 345588.05,
      manual_price: null,
      currency: "BRL",
      score_override: 8,
      answers: {},
    },
    {
      id: "a-ipca",
      asset_class: "fixed_income",
      symbol: null,
      display_name: "Tesouro IPCA+ 2035",
      quantity: 85000,
      quote_price: 1,
      manual_price: 1,
      currency: "BRL",
      score_override: 7,
      answers: {},
    },
  ];
}

export function defaultGoals(): GoalTargets {
  return { br_stocks: 30, br_reits: 10, intl_stocks: 20, us_reits: 0, crypto: 5, fixed_income: 35, intl_fixed_income: 0 };
}

const round2 = (n: number) => Math.round(n * 100) / 100;

/**
 * O livro FICTÍCIO que bate com a carteira acima: Itaúsa e KNRI11 com custo,
 * HGLG11 com o saldo inicial sem preço (o "informe o preço médio").
 */
export function defaultTransactions(): Transaction[] {
  const base = {
    symbol: null,
    display_name: "",
    fees: 0,
    currency: "BRL",
    fx_rate: null,
    external_ref: null,
    contribution_id: null,
    created_at: "2026-01-01T12:00:00Z",
  } as const;
  return [
    { ...base, id: "t-itsa-1", asset_id: "a-itsa", asset_class: "br_stocks", symbol: "ITSA4", kind: "opening", trade_date: "2025-06-10", quantity: 2500, unit_price: 10, source: "migration" },
    { ...base, id: "t-knri-1", asset_id: "a-knri", asset_class: "br_reits", symbol: "KNRI11", kind: "opening", trade_date: "2025-03-03", quantity: 8, unit_price: 125, source: "manual" },
    { ...base, id: "t-knri-2", asset_id: "a-knri", asset_class: "br_reits", symbol: "KNRI11", kind: "buy", trade_date: "2026-02-10", quantity: 2, unit_price: 150, fees: 0, source: "plan", contribution_id: "c-0" },
    { ...base, id: "t-hglg-1", asset_id: "a-hglg", asset_class: "br_reits", symbol: "HGLG11", kind: "opening", trade_date: "2025-01-15", quantity: 12, unit_price: null, source: "migration" },
  ];
}

function checklistKind(assetClass: AssetClass): "real_estate" | "stocks" | null {
  if (assetClass === "br_reits" || assetClass === "us_reits") return "real_estate";
  if (assetClass === "br_stocks" || assetClass === "intl_stocks") return "stocks";
  return null;
}

function scoreOf(stored: StoredAsset, checklists: Checklists): number {
  const kind = checklistKind(stored.asset_class);
  return stored.score_override ?? (kind ? checklistScore(stored.answers, checklists[kind]) : 0);
}

/**
 * O % ideal de cada ativo, como o servidor calcula: nota ÷ soma das notas dos
 * que recebem aporte (nota > 0 e preço > 0) na classe. Quem não recebe fica em 0.
 */
export function idealPercents(assets: StoredAsset[], checklists: Checklists): Map<string, number> {
  const eligible = (a: StoredAsset) => scoreOf(a, checklists) > 0 && (a.manual_price ?? a.quote_price ?? 0) > 0;
  const sums = new Map<AssetClass, number>();
  for (const a of assets.filter(eligible)) {
    sums.set(a.asset_class, (sums.get(a.asset_class) ?? 0) + scoreOf(a, checklists));
  }
  return new Map(
    assets.map((a) => {
      const sum = sums.get(a.asset_class) ?? 0;
      return [a.id, eligible(a) && sum > 0 ? round2((scoreOf(a, checklists) / sum) * 100) : 0];
    }),
  );
}

/** De onde vem a nota: a manual vence; sem checklist na classe, não há nota. */
function scoreSourceOf(scoreOverride: number | null, hasChecklist: boolean): ScoreSource {
  if (scoreOverride !== null) return "manual";
  return hasChecklist ? "checklist" : "none";
}

/** De onde vem o preço: o manual vence a cotação. */
function priceSourceOf(manualPrice: number | null, price: number | null): PriceSource {
  if (manualPrice !== null) return "manual";
  return price === null ? "missing" : "quote";
}

export function toAsset(stored: StoredAsset, checklists: Checklists, classTotal: number, idealPercent = 0): Asset {
  const price = stored.manual_price ?? stored.quote_price;
  const value = price === null ? 0 : stored.quantity * price;
  const kind = checklistKind(stored.asset_class);
  const cost = stored.quantity === 0 ? 0 : (stored.cost_basis ?? null);
  const valueBased =
    (stored.asset_class === "fixed_income" || stored.asset_class === "intl_fixed_income") &&
    !stored.listed_on_b3 &&
    !stored.treasury;
  const unrealized = cost === null || price === null ? null : round2(value - cost);
  const scoreSource = scoreSourceOf(stored.score_override, kind !== null);
  const score = scoreOf(stored, checklists);
  return {
    id: stored.id,
    asset_class: stored.asset_class,
    symbol: stored.symbol,
    display_name: stored.display_name,
    listed_on_b3: stored.listed_on_b3 ?? nativeB3(stored.asset_class),
    treasury: stored.treasury ?? false,
    quantity: stored.quantity,
    unit_price: price,
    manual_price: stored.manual_price,
    ceiling: stored.ceiling ?? null,
    currency: stored.currency,
    price_source: priceSourceOf(stored.manual_price, price),
    quote_updated_at: stored.quote_price === null ? null : (stored.quote_at ?? QUOTES_AT),
    current_value: round2(value),
    class_percent: classTotal > 0 ? round2((value / classTotal) * 100) : 0,
    ideal_class_percent: idealPercent,
    score,
    score_source: scoreSource,
    score_override: stored.score_override,
    answers: { ...stored.answers },
    sector: stored.sector ?? null,
    subsector: stored.subsector ?? null,
    segment: stored.segment ?? null,
    average_price: cost === null || stored.quantity === 0 || valueBased ? null : cost / stored.quantity,
    cost_basis: cost,
    unrealized_gain: unrealized,
    realized_gain: stored.realized_gain === undefined ? 0 : stored.realized_gain,
    return_percent: unrealized === null || cost === null || cost <= 0 ? null : round2((unrealized / cost) * 100),
    tax_instrument: stored.tax_instrument ?? defaultTaxInstrument(stored),
    tax_instrument_default: defaultTaxInstrument(stored),
  };
}

/**
 * O lucro total como o servidor soma: ganho de capital dos ativos de custo e
 * preço conhecidos mais os proventos recebidos; ativo de custo desconhecido sai
 * da soma e vai para `partial_assets`.
 */
export function toProfit(assets: Asset[], incomeReceived = 0): Profit {
  const partial: Profit["partial_assets"] = [];
  let unrealized = 0;
  let realized = 0;
  for (const asset of assets) {
    if (asset.realized_gain !== null) realized += asset.realized_gain;
    if (asset.quantity > 0) {
      if (asset.cost_basis === null) partial.push({ asset_id: asset.id, label: asset.symbol ?? asset.display_name, reason: "unknown_cost" });
      else if (asset.unrealized_gain !== null) unrealized += asset.unrealized_gain;
    }
  }
  return {
    unrealized: round2(unrealized),
    realized: round2(realized),
    income_received: incomeReceived,
    total: round2(unrealized + realized + incomeReceived),
    partial: partial.length > 0,
    partial_assets: partial,
    removed_assets: false,
  };
}

export function toPortfolio(assets: StoredAsset[], goals: GoalTargets, checklists: Checklists): Portfolio {
  const valueOf = (a: StoredAsset) => {
    const price = a.manual_price ?? a.quote_price;
    return price === null ? 0 : a.quantity * price;
  };
  const classTotals = Object.fromEntries(
    ASSET_CLASSES.map((c) => [c, assets.filter((a) => a.asset_class === c).reduce((acc, a) => acc + valueOf(a), 0)]),
  ) as Record<AssetClass, number>;
  const total = Object.values(classTotals).reduce((acc, v) => acc + v, 0);
  const ideal = idealPercents(assets, checklists);
  const mapped = assets.map((a) => toAsset(a, checklists, classTotals[a.asset_class], ideal.get(a.id) ?? 0));
  return {
    total_value: round2(total),
    quotes_updated_at: QUOTES_AT,
    classes: ASSET_CLASSES.map((c) => ({
      asset_class: c,
      target_percent: goals[c],
      current_value: round2(classTotals[c]),
      current_percent: total > 0 ? round2((classTotals[c] / total) * 100) : 0,
      asset_count: assets.filter((a) => a.asset_class === c).length,
      eligible: mapped.some((a) => a.asset_class === c && a.score > 0 && (a.unit_price ?? 0) > 0),
    })),
    assets: mapped,
    profit: toProfit(mapped),
    checklists,
  };
}

/**
 * Um plano de R$ 6.000 coerente com a carteira fictícia acima: ações, dois FIIs
 * e BTC recebem; Ações Internacionais tem meta e nenhum ativo (bloqueada).
 */
export function samplePlan(amount = 6000): Plan {
  return {
    amount,
    invested: 5947.92,
    change: 52.08,
    unallocated: 0,
    total_before: 120132.23,
    total_after: 126080.15,
    income: null,
    items: [
      {
        asset_id: "a-itsa",
        asset_class: "br_stocks",
        symbol: "ITSA4",
        display_name: "Itaúsa",
        listed_on_b3: true,
        treasury: false,
        quantity: 243,
        unit_price: 11.2,
        value: 2721.6,
        divisible: false,
        class_percent_after: 100,
        estimated_monthly_income: null,
        price_references: null,
      },
      {
        asset_id: "a-btlg",
        asset_class: "br_reits",
        symbol: "BTLG11",
        display_name: "BTG Pactual Logística",
        listed_on_b3: true,
        treasury: false,
        quantity: 8,
        unit_price: 94.55,
        value: 756.4,
        divisible: false,
        class_percent_after: 26.8,
        estimated_monthly_income: null,
        price_references: null,
      },
      {
        asset_id: "a-trxf",
        asset_class: "br_reits",
        symbol: "TRXF11",
        display_name: "TRX Real Estate",
        listed_on_b3: true,
        treasury: false,
        quantity: 17,
        unit_price: 53.81,
        value: 914.77,
        divisible: false,
        class_percent_after: 24.6,
        estimated_monthly_income: null,
        price_references: null,
      },
      {
        asset_id: "a-btc",
        asset_class: "crypto",
        symbol: "BTC",
        display_name: "Bitcoin",
        listed_on_b3: false,
        treasury: false,
        quantity: 0.0045,
        unit_price: 345588.05,
        value: 1555.15,
        divisible: true,
        class_percent_after: 100,
        estimated_monthly_income: null,
        price_references: null,
      },
    ],
    classes: [
      { asset_class: "br_stocks", target_percent: 30, percent_before: 23.31, percent_after: 24.37, allocated: 2721.6, status: "receiving" },
      { asset_class: "br_reits", target_percent: 10, percent_before: 5.36, percent_after: 6.43, allocated: 1671.17, status: "receiving" },
      { asset_class: "intl_stocks", target_percent: 20, percent_before: 0, percent_after: 0, allocated: 0, status: "blocked" },
      { asset_class: "us_reits", target_percent: 0, percent_before: 0, percent_after: 0, allocated: 0, status: "no_target" },
      { asset_class: "crypto", target_percent: 5, percent_before: 0.58, percent_after: 1.78, allocated: 1555.15, status: "receiving" },
      { asset_class: "fixed_income", target_percent: 35, percent_before: 70.76, percent_after: 67.42, allocated: 0, status: "above_target" },
      { asset_class: "intl_fixed_income", target_percent: 0, percent_before: 0, percent_after: 0, allocated: 0, status: "no_target" },
    ],
    notices: [{ code: "class_blocked", asset_class: "intl_stocks", amount: 25226.45, reason: "no_assets" }],
  };
}
