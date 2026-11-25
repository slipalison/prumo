import { ASSET_CLASSES, type AssetClass, type ChecklistKind } from "../api/types";

export { ASSET_CLASSES };

/** Como a classe é medida: cota inteira, fração (cripto) ou valor (renda fixa). */
export type UnitKind = "unit" | "fraction" | "value";

export type ClassMeta = {
  label: string;
  shortLabel: string;
  checklist: ChecklistKind | null;
  unit: UnitKind;
  /** Substantivo da unidade, singular e plural ("cota", "cotas"). */
  noun: [string, string];
};

export const CLASS_META: Record<AssetClass, ClassMeta> = {
  br_stocks: { label: "Ações Nacionais", shortLabel: "Ações BR", checklist: "stocks", unit: "unit", noun: ["ação", "ações"] },
  br_reits: { label: "Fundos Imobiliários", shortLabel: "FIIs", checklist: "real_estate", unit: "unit", noun: ["cota", "cotas"] },
  intl_stocks: { label: "Ações Internacionais", shortLabel: "Ações EUA", checklist: "stocks", unit: "unit", noun: ["ação", "ações"] },
  us_reits: { label: "REITs", shortLabel: "REITs", checklist: "real_estate", unit: "unit", noun: ["cota", "cotas"] },
  crypto: { label: "Criptomoedas", shortLabel: "Cripto", checklist: null, unit: "fraction", noun: ["unidade", "unidades"] },
  fixed_income: { label: "Renda Fixa", shortLabel: "Renda Fixa", checklist: null, unit: "value", noun: ["real", "reais"] },
  intl_fixed_income: {
    label: "Renda Fixa Internacional",
    shortLabel: "RF Intl.",
    checklist: null,
    unit: "value",
    noun: ["real", "reais"],
  },
};

/** Ações Nacionais e FIIs: o mercado delas já é a B3. */
export function isNativeB3(assetClass: AssetClass): boolean {
  return assetClass === "br_stocks" || assetClass === "br_reits";
}

/**
 * Se a classe aceita um ativo negociado na B3 fora do seu mercado natural —
 * ETF de bitcoin em Cripto, BDR em Ações EUA, ETF de IMA-B em Renda Fixa.
 */
export function acceptsB3Listing(assetClass: AssetClass): boolean {
  return !isNativeB3(assetClass);
}

/** A unidade de um ativo: na B3 é cota inteira, seja qual for a classe. */
export function unitOf(assetClass: AssetClass, listedOnB3: boolean): UnitKind {
  return listedOnB3 ? "unit" : CLASS_META[assetClass].unit;
}

/** O substantivo da unidade: ETF e BDR da B3 fora de Ações BR e FIIs são "cotas". */
export function nounOf(assetClass: AssetClass, listedOnB3: boolean): [string, string] {
  return listedOnB3 && acceptsB3Listing(assetClass) ? ["cota", "cotas"] : CLASS_META[assetClass].noun;
}

/**
 * Formato de ticker da B3 (4 letras e 1–2 dígitos: PETR4, IVVB11, AAPL34).
 * Cripto (BTC) e bolsa americana (VOO, AAPL) não têm dígito no fim.
 */
export function looksLikeB3Ticker(raw: string): boolean {
  return /^[A-Z0-9]{4}\d{1,2}F?$/.test(raw.trim().toUpperCase().replace(/\.SA$/, ""));
}

/** Cor da classe como variável CSS — a mesma nos dois temas, com tons próprios. */
export function classColor(assetClass: AssetClass): string {
  return `var(--c-${assetClass})`;
}

export function isValueBased(assetClass: AssetClass): boolean {
  return CLASS_META[assetClass].unit === "value";
}

/**
 * Palpite local para quando a pessoa digita o ticker sem escolher sugestão:
 * B3 terminando em 11 costuma ser FII; 4 letras + 1–2 dígitos, ação da B3.
 * `null` quando não dá para dizer — a classe escolhida continua valendo.
 */
export function guessClassFromSymbol(raw: string): AssetClass | null {
  const symbol = raw.trim().toUpperCase().replace(/\.SA$/, "");
  if (/^[A-Z]{4}11$/.test(symbol)) return "br_reits";
  if (/^[A-Z]{4}\d{1,2}F?$/.test(symbol)) return "br_stocks";
  if (/^(BTC|ETH|SOL|USDT|USDC|BNB|XRP|ADA|DOGE)$/.test(symbol)) return "crypto";
  return null;
}

export const CHECKLIST_LABEL: Record<ChecklistKind, string> = {
  stocks: "Ações",
  real_estate: "Imobiliário",
};

export const CHECKLIST_CLASSES: Record<ChecklistKind, AssetClass[]> = {
  stocks: ["br_stocks", "intl_stocks"],
  real_estate: ["br_reits", "us_reits"],
};
