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

/** A palavra que acompanha a seta: o sentido nunca depende só da cor ou do desenho. */
export function directionWord(direction: Direction): string {
  return direction === "above" ? "acima" : direction === "below" ? "abaixo" : "igual";
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

/** Um ativo e como o preço dele andou no dia. */
export type DayMover = {
  id: string;
  label: string;
  assetClass: AssetClass;
  /** Variação do preço contra o fechamento anterior, em %. */
  changePercent: number;
  /** Quanto isso vale na posição de hoje (quantidade × diferença de preço), em reais. */
  delta: number;
  /** A posição ao preço do fechamento anterior (a base do percentual da classe). */
  previousValue: number;
};

/**
 * A variação do dia de cada ativo com posição, cotação e referência. Ativo sem
 * cotação (preço manual, renda fixa por valor) ou sem fechamento anterior fica
 * de fora: o dia dele não é medido.
 */
export function dayMovers(assets: Asset[], references: DayReference[]): DayMover[] {
  const byAsset = new Map(references.map((r) => [r.asset_id, r.previous_close]));
  const movers: DayMover[] = [];
  for (const asset of assets) {
    const previous = byAsset.get(asset.id);
    if (previous === undefined || previous <= 0) continue;
    if (asset.quantity <= 0 || asset.price_source !== "quote" || asset.unit_price === null) continue;
    movers.push({
      id: asset.id,
      label: assetName(asset),
      assetClass: asset.asset_class,
      changePercent: (asset.unit_price / previous - 1) * 100,
      delta: asset.quantity * (asset.unit_price - previous),
      previousValue: asset.quantity * previous,
    });
  }
  return movers;
}

export type ClassDay = {
  assetClass: AssetClass;
  count: number;
  delta: number;
  /** Variação da classe no dia, em %: o ganho sobre a posição do fechamento anterior. */
  percent: number;
};

/** O dia por classe, na ordem em que as classes aparecem em `order`. */
export function classDays(movers: DayMover[], order: readonly AssetClass[]): ClassDay[] {
  return order.flatMap((assetClass) => {
    const own = movers.filter((m) => m.assetClass === assetClass);
    if (own.length === 0) return [];
    const delta = own.reduce((sum, m) => sum + m.delta, 0);
    const base = own.reduce((sum, m) => sum + m.previousValue, 0);
    return [{ assetClass, count: own.length, delta, percent: base > 0 ? (delta / base) * 100 : 0 }];
  });
}

/** O que a tela mostra (uma casa): "0,0%" não é alta nem baixa. */
const rounded = (percent: number) => Math.round(percent * 10) / 10;

/** As `limit` maiores altas (do maior para o menor) e baixas (da pior para a menos pior). */
export function topMovers(movers: DayMover[], limit: number): { gainers: DayMover[]; losers: DayMover[] } {
  const up = movers.filter((m) => rounded(m.changePercent) > 0);
  const down = movers.filter((m) => rounded(m.changePercent) < 0);
  return {
    gainers: [...up].sort((a, b) => b.changePercent - a.changePercent).slice(0, limit),
    losers: [...down].sort((a, b) => a.changePercent - b.changePercent).slice(0, limit),
  };
}
