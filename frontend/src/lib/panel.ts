import type { DayReference } from "../api/panelTypes";
import type { Asset, AssetClass } from "../api/types";
import { assetName } from "./rating";

/** Diferença menor que um centavo é igual. */
const MONEY_EPSILON = 0.005;

export type Direction = "above" | "below" | "equal";

/** Se `value` está acima, abaixo ou igual a `base` (dinheiro: centavo; percentual: o que a tela mostra). */
export function compare(value: number, base: number, epsilon = MONEY_EPSILON): Direction {
  const diff = value - base;
  if (Math.abs(diff) < epsilon) return "equal";
  return diff > 0 ? "above" : "below";
}

const DIRECTION_WORD: Record<Direction, string> = { above: "acima", below: "abaixo", equal: "igual" };

/** A palavra que acompanha a seta: o sentido nunca depende só da cor ou do desenho. */
export function directionWord(direction: Direction): string {
  return DIRECTION_WORD[direction];
}

/** O sentido de um ganho ou perda (zero é "igual"). */
export function signDirection(value: number, epsilon = MONEY_EPSILON): Direction {
  return compare(value, 0, epsilon);
}

export type PatrimonyChange = { delta: number; percent: number };

/** Quanto o patrimônio andou desde o fim do mês anterior. `null` sem base (ou base zero). */
export function patrimonyChange(total: number, base: number | null): PatrimonyChange | null {
  if (base === null || base <= 0) return null;
  const delta = Math.round((total - base) * 100) / 100;
  return { delta, percent: (delta / base) * 100 };
}

/** `2026-11-01` → "novembro". */
export function monthName(iso: string): string {
  return new Date(`${iso}T12:00:00Z`).toLocaleDateString("pt-BR", { month: "long", timeZone: "UTC" });
}

/** O mês anterior ao de `iso` (primeiro dia do mês), no mesmo formato. */
export function previousMonthStart(iso: string): string {
  const [year = 0, month = 1] = iso.split("-").map(Number);
  return month === 1 ? `${year - 1}-12-01` : `${year}-${String(month - 1).padStart(2, "0")}-01`;
}

/** Um ativo e como ele andou no dia. */
export type DayMover = {
  id: string;
  label: string;
  assetClass: AssetClass;
  /**
   * Variação do preço contra o fechamento anterior, em %. `null` quando não há
   * o que mostrar nas listas de altas e baixas: o ativo comprado hoje pela
   * primeira vez (não tinha fechamento) e o que foi vendido inteiro.
   */
  changePercent: number | null;
  /**
   * O resultado do dia em reais: o valor de agora menos o do fechamento, sem o
   * dinheiro das compras e vendas depois dele. A compra de hoje ganha ou perde
   * a partir do preço pago (com as taxas); a venda, até o preço de venda.
   */
  delta: number;
  /** A base do percentual: a posição no fechamento mais o que se comprou depois. */
  base: number;
  /** O dia do fechamento anterior (`YYYY-MM-DD`). */
  since: string;
};

/**
 * O dia de cada ativo cotado com referência. Ativo sem cotação (preço manual,
 * renda fixa por valor) ou sem referência fica de fora: o dia dele não é
 * medido. Sem fechamento anterior, as cotas que já havia entram pelo preço de
 * agora (não há o que medir nelas) e só o negócio conta.
 */
export function dayMovers(assets: Asset[], references: DayReference[]): DayMover[] {
  const byAsset = new Map(references.map((r) => [r.asset_id, r]));
  const movers: DayMover[] = [];
  for (const asset of assets) {
    const reference = byAsset.get(asset.id);
    if (reference === undefined || asset.price_source !== "quote" || asset.unit_price === null) continue;
    const price = asset.unit_price;
    const close = reference.previous_close !== null && reference.previous_close > 0 ? reference.previous_close : null;
    const previousValue = reference.previous_quantity * (close ?? price);
    const base = previousValue + reference.bought;
    // Nada no fechamento e nada comprado: não há dia a medir.
    if (base <= 0) continue;
    movers.push({
      id: asset.id,
      label: assetName(asset),
      assetClass: asset.asset_class,
      changePercent: close !== null && asset.quantity > 0 ? (price / close - 1) * 100 : null,
      delta: asset.quantity * price - previousValue - reference.bought + reference.sold,
      base,
      since: reference.date,
    });
  }
  return movers;
}

export type DayTotal = {
  /** O resultado do dia da carteira, em reais. */
  delta: number;
  /** O mesmo sobre a posição no fechamento mais o que se comprou depois, em %. */
  percent: number;
  /** Quantos ativos foram medidos. */
  count: number;
  /** O fechamento mais recente contra o qual se mediu (`YYYY-MM-DD`). */
  since: string;
};

/** O dia da carteira inteira: a soma dos ativos medidos. `null` sem nenhum. */
export function dayTotal(movers: DayMover[]): DayTotal | null {
  if (movers.length === 0) return null;
  const delta = movers.reduce((sum, m) => sum + m.delta, 0);
  const base = movers.reduce((sum, m) => sum + m.base, 0);
  const since = movers.reduce((latest, m) => (m.since > latest ? m.since : latest), movers[0]?.since ?? "");
  return { delta, percent: base > 0 ? (delta / base) * 100 : 0, count: movers.length, since };
}

export type ClassDay = {
  assetClass: AssetClass;
  count: number;
  delta: number;
  /** Variação da classe no dia, em %: o resultado sobre a posição do fechamento mais o comprado depois. */
  percent: number;
};

/** O dia por classe, na ordem em que as classes aparecem em `order`. */
export function classDays(movers: DayMover[], order: readonly AssetClass[]): ClassDay[] {
  return order.flatMap((assetClass) => {
    const own = movers.filter((m) => m.assetClass === assetClass);
    if (own.length === 0) return [];
    const delta = own.reduce((sum, m) => sum + m.delta, 0);
    const base = own.reduce((sum, m) => sum + m.base, 0);
    return [{ assetClass, count: own.length, delta, percent: base > 0 ? (delta / base) * 100 : 0 }];
  });
}

/** O que a tela mostra (uma casa): "0,0%" não é alta nem baixa. */
const rounded = (percent: number) => Math.round(percent * 10) / 10;

/** Um ativo com variação de preço para as listas. */
export type PricedMover = DayMover & { changePercent: number };

/** As `limit` maiores altas (do maior para o menor) e baixas (da pior para a menos pior). */
export function topMovers(movers: DayMover[], limit: number): { gainers: PricedMover[]; losers: PricedMover[] } {
  const priced = movers.filter((m): m is PricedMover => m.changePercent !== null);
  const up = priced.filter((m) => rounded(m.changePercent) > 0);
  const down = priced.filter((m) => rounded(m.changePercent) < 0);
  return {
    gainers: [...up].sort((a, b) => b.changePercent - a.changePercent).slice(0, limit),
    losers: [...down].sort((a, b) => a.changePercent - b.changePercent).slice(0, limit),
  };
}
