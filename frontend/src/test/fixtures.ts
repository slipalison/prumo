import { ASSET_CLASSES, type Asset, type AssetClass, type Checklists, type GoalTargets, type Plan, type Portfolio } from "../api/types";
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
};

const nativeB3 = (assetClass: AssetClass) => assetClass === "br_stocks" || assetClass === "br_reits";

export const QUOTES_AT = "2026-11-25T11:30:00Z";

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
    },
    fii("a-knri", "KNRI11", "Kinea Renda Imobiliária", 10, 142, allYes),
    fii("a-hglg", "HGLG11", "Pátria Log", 12, 140.8, allYes),
    fii("a-btlg", "BTLG11", "BTG Pactual Logística", 15, 94.55, { ...allYes, r6: false }),
    fii("a-trxf", "TRXF11", "TRX Real Estate", 20, 53.81, { ...allYes, r5: false }),
    fii("a-mxrf", "MXRF11", "Maxi Renda", 100, 8.37, { r1: false, r2: true, r3: false, r4: true, r5: true, r6: false }),
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

export function toAsset(stored: StoredAsset, checklists: Checklists, classTotal: number): Asset {
  const price = stored.manual_price ?? stored.quote_price;
  const value = price === null ? 0 : stored.quantity * price;
  const kind = stored.asset_class === "br_reits" || stored.asset_class === "us_reits"
    ? "real_estate"
    : stored.asset_class === "br_stocks" || stored.asset_class === "intl_stocks"
      ? "stocks"
      : null;
  const scoreSource = stored.score_override !== null ? "manual" : kind ? "checklist" : "none";
  const score =
    stored.score_override ?? (kind ? checklistScore(stored.answers, checklists[kind]) : 0);
  return {
    id: stored.id,
    asset_class: stored.asset_class,
    symbol: stored.symbol,
    display_name: stored.display_name,
    listed_on_b3: stored.listed_on_b3 ?? nativeB3(stored.asset_class),
    quantity: stored.quantity,
    unit_price: price,
    manual_price: stored.manual_price,
    currency: stored.currency,
    price_source: stored.manual_price !== null ? "manual" : price === null ? "missing" : "quote",
    quote_updated_at: stored.quote_price === null ? null : QUOTES_AT,
    current_value: round2(value),
    class_percent: classTotal > 0 ? round2((value / classTotal) * 100) : 0,
    score,
    score_source: scoreSource,
    score_override: stored.score_override,
    answers: { ...stored.answers },
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
  const mapped = assets.map((a) => toAsset(a, checklists, classTotals[a.asset_class]));
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
    items: [
      {
        asset_id: "a-itsa",
        asset_class: "br_stocks",
        symbol: "ITSA4",
        display_name: "Itaúsa",
        listed_on_b3: true,
        quantity: 243,
        unit_price: 11.2,
        value: 2721.6,
        divisible: false,
        class_percent_after: 100,
      },
      {
        asset_id: "a-btlg",
        asset_class: "br_reits",
        symbol: "BTLG11",
        display_name: "BTG Pactual Logística",
        listed_on_b3: true,
        quantity: 8,
        unit_price: 94.55,
        value: 756.4,
        divisible: false,
        class_percent_after: 26.8,
      },
      {
        asset_id: "a-trxf",
        asset_class: "br_reits",
        symbol: "TRXF11",
        display_name: "TRX Real Estate",
        listed_on_b3: true,
        quantity: 17,
        unit_price: 53.81,
        value: 914.77,
        divisible: false,
        class_percent_after: 24.6,
      },
      {
        asset_id: "a-btc",
        asset_class: "crypto",
        symbol: "BTC",
        display_name: "Bitcoin",
        listed_on_b3: false,
        quantity: 0.0045,
        unit_price: 345588.05,
        value: 1555.15,
        divisible: true,
        class_percent_after: 100,
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
