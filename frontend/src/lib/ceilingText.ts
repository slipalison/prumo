import type {
  Ceiling,
  CeilingChoice,
  CeilingMethod,
  FundReference,
  FundType,
  MarketRates,
  ReferenceGap,
  StockReference,
} from "../api/ceilingTypes";
import type { AssetClass } from "../api/types";
import { formatPercent } from "./format";
import type { Money } from "./money";

/** O nome curto de cada origem de teto, como a tela escreve. */
export const METHOD_LABEL: Record<CeilingMethod | "manual", string> = {
  bazin: "Bazin",
  graham: "Graham",
  combined: "Pelas duas",
  nav: "VP",
  fund_yield: "Rendimento",
  manual: "Valor próprio",
};

/** "pelo Graham", "pelo VP", "pelo Rendimento", "pelas duas referências". */
export function methodPhrase(method: CeilingMethod): string {
  return method === "combined" ? "pelas duas referências" : `pelo ${METHOD_LABEL[method]}`;
}

/** Por que a referência está sem valor, em uma frase curta (sem ponto final). */
export const GAP_TEXT: Record<ReferenceGap, string> = {
  no_dividends: "sem proventos no histórico",
  no_fundamentals: "sem os dados da CVM",
  loss: "prejuízo: lucro por ação zero ou negativo",
  negative_equity: "patrimônio zero ou negativo",
  unknown_fund_type: "tipo do FII ainda desconhecido",
  no_composition: "sem a composição da carteira do FII",
  no_market_rate: "sem a taxa de mercado do alvo",
  non_positive_target: "alvo de rendimento zero ou negativo",
  no_reference: "nenhuma das duas referências tem valor",
};

export const FUND_TYPE_LABEL: Record<FundType, string> = {
  brick: "Tijolo",
  paper: "Papel",
  hybrid: "Híbrido",
  fund_of_funds: "Fundo de fundos",
};

/** Que referências o ativo tem: as de ação ou as de FII (só na B3). */
export type ReferenceFamily = "stock" | "fund";

export function referenceFamily(asset: { asset_class: AssetClass; listed_on_b3: boolean; symbol: string | null }): ReferenceFamily | null {
  if (!asset.symbol || !asset.listed_on_b3) return null;
  if (asset.asset_class === "br_stocks") return "stock";
  if (asset.asset_class === "br_reits") return "fund";
  return null;
}

/** Os métodos de cada família, na ordem da escolha. */
export const FAMILY_METHODS: Record<ReferenceFamily, CeilingMethod[]> = {
  stock: ["bazin", "graham", "combined"],
  fund: ["nav", "fund_yield", "combined"],
};

/** `2026-11-19` → `19/11/2026` (a data como o servidor manda, sem fuso). */
export function formatIsoDay(day: string): string {
  const [year, month, date] = day.slice(0, 10).split("-");
  return year && month && date ? `${date}/${month}/${year}` : day;
}

/** Percentual com duas casas: "7,12%". */
const pct = (value: number) => formatPercent(value, 2);
/** Pontos percentuais com uma ou duas casas: "2,5 p.p.". */
const pp = (value: number) =>
  `${value.toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 2 })} p.p.`;

/** O valor de hoje de um método nas referências de um ativo. */
export function methodValue(item: StockReference | FundReference, method: CeilingMethod): { value: number | null; gap: ReferenceGap | null } {
  if ("bazin" in item) {
    if (method === "bazin") return item.bazin;
    if (method === "graham") return item.graham;
    if (method === "combined") return item.combined;
  } else {
    if (method === "nav") return item.nav;
    if (method === "fund_yield") return item.fund_yield;
    if (method === "combined") return item.combined;
  }
  return { value: null, gap: "no_reference" };
}

/** O que o teto escolhido é, em poucas palavras: "Graham", "Valor próprio", "Sem teto". */
export function ceilingLabel(ceiling: Ceiling | null): string {
  return ceiling ? METHOD_LABEL[ceiling.method] : "Sem teto";
}

/** A explicação do juro real e do CDI de hoje, com data e fonte. */
export function realRateText(market: MarketRates): string {
  const real = market.real_rate;
  return real
    ? `juro real ${pct(real.percent)} em ${formatIsoDay(real.base_date)} (${real.bond})`
    : "juro real do Tesouro ainda não lido";
}

export function cdiText(market: MarketRates): string {
  const cdi = market.cdi_12m;
  return cdi
    ? `CDI de 12 meses ${pct(cdi.percent)} até ${formatIsoDay(cdi.to)}, ${pct(cdi.net_percent)} depois dos 15% de IR`
    : "CDI de 12 meses ainda não guardado";
}

/** A conta do alvo do tijolo e a do papel, com os números de hoje. */
export function brickTargetMath(market: MarketRates): string | null {
  if (!market.real_rate || market.brick_target === null) return null;
  return `${pct(market.real_rate.percent)} + ${pp(market.brick_premium)} = ${pct(market.brick_target)}`;
}

export function paperTargetMath(market: MarketRates): string | null {
  if (!market.cdi_12m || market.paper_target === null) return null;
  return `${pct(market.cdi_12m.percent)} × 0,85 = ${pct(market.cdi_12m.net_percent)} + ${pp(market.paper_premium)} = ${pct(market.paper_target)}`;
}

/**
 * A conta inteira do Rendimento de um FII, com os números de verdade:
 * "alvo 7,12% + 2,5 p.p. = 9,62%; proventos 12m R$ 13,20 ÷ 9,62% = R$ 137,21".
 */
export function fundYieldMath(item: FundReference, market: MarketRates, money: Money): string | null {
  const found = item.fund_yield;
  const target = found.target;
  if (found.value === null || target === null || found.dividends_12m === null) return null;
  const brick = brickTargetMath(market);
  const paper = paperTargetMath(market);
  let alvo: string;
  switch (item.fund_type) {
    case "brick":
      alvo = `alvo do tijolo ${brick ?? pct(target.percent)}`;
      break;
    case "paper":
      alvo = `alvo do papel: CDI ${paper ?? pct(target.percent)}`;
      break;
    case "fund_of_funds":
      alvo = `média dos alvos de tijolo (${pct(target.brick ?? 0)}) e papel (${pct(target.paper ?? 0)}) = ${pct(target.percent)}, porque a carteira dos fundos de baixo não é conhecida`;
      break;
    default:
      alvo = `${formatPercent(target.brick_weight * 100, 0)} tijolo (${pct(target.brick ?? 0)}) + ${formatPercent(target.paper_weight * 100, 0)} papel (${pct(target.paper ?? 0)}) = ${pct(target.percent)}`;
  }
  return `${alvo}; proventos 12m ${money.brl(found.dividends_12m)} ÷ ${pct(target.percent)} = ${money.brl(found.value)}`;
}

/** A escolha em edição: sem teto, um método ou um valor próprio. */
export type CeilingDraft = { kind: "none" } | { kind: "method"; method: CeilingMethod } | { kind: "manual"; value: number };

export function draftOf(ceiling: Ceiling | null, fallbackValue: number): CeilingDraft {
  if (!ceiling) return { kind: "none" };
  if (ceiling.method === "manual") return { kind: "manual", value: ceiling.value ?? fallbackValue };
  return { kind: "method", method: ceiling.method };
}

/** O que vai no PATCH: `null` tira o teto. */
export function choiceOf(draft: CeilingDraft): CeilingChoice | null {
  if (draft.kind === "none") return null;
  if (draft.kind === "manual") return { method: "manual", value: draft.value };
  return { method: draft.method };
}

/** Se a escolha em edição muda o teto que está gravado. */
export function draftChanges(draft: CeilingDraft, ceiling: Ceiling | null): boolean {
  const before = draftOf(ceiling, 0);
  if (draft.kind !== before.kind) return true;
  if (draft.kind === "method" && before.kind === "method") return draft.method !== before.method;
  if (draft.kind === "manual" && before.kind === "manual") return draft.value !== before.value;
  return false;
}
