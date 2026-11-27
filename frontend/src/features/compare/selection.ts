import type { Asset, AssetClass, SymbolSuggestion } from "../../api/types";
import { acceptsB3Listing } from "../../lib/assetClasses";

/** Quantos ativos cabem numa comparação. */
export const MAX_ASSETS = 5;

/** O endereço da tela. */
export const COMPARE_PATH = "analise/comparar";

/** `#/analise/comparar?ativos=PETR4,VALE3` → `["PETR4", "VALE3"]`; sem repetição, no limite. */
export function readSelection(hash: string): string[] {
  const query = hash.split("?")[1] ?? "";
  const raw = new URLSearchParams(query).get("ativos") ?? "";
  const keys = raw
    .split(",")
    .map((key) => key.trim())
    .filter(Boolean);
  return [...new Set(keys)].slice(0, MAX_ASSETS);
}

/** O endereço com a escolha: dá para voltar a ele, e o link vale para outra pessoa abrir. */
export function selectionHash(keys: readonly string[]): string {
  return keys.length > 0 ? `#/${COMPARE_PATH}?ativos=${keys.join(",")}` : `#/${COMPARE_PATH}`;
}

/** O ticker de uma escolha (`KNRI11:br_reits` → `KNRI11`). */
export function symbolOf(key: string): string {
  return key.split(":")[0] ?? key;
}

/** Ativo da carteira que o comparador cobre: tem cotação em bolsa (fora renda fixa e Tesouro). */
export function comparable(asset: Asset): asset is Asset & { symbol: string } {
  return (
    asset.symbol !== null &&
    !asset.treasury &&
    asset.asset_class !== "fixed_income" &&
    asset.asset_class !== "intl_fixed_income"
  );
}

/** Ativo de busca que o comparador cobre. */
export function suggestionComparable(suggestion: SymbolSuggestion): boolean {
  return !suggestion.treasury && suggestion.suggested_class !== "fixed_income" && suggestion.suggested_class !== "intl_fixed_income";
}

/**
 * A escolha como o servidor lê: o ticker sozinho para o ativo da carteira; para
 * o de fora, com a classe (e `:b3` para BDR/ETF numa classe de fora da B3).
 * Segue `Token::key` do servidor.
 */
export function tokenFor(symbol: string, assetClass: AssetClass, listedOnB3: boolean): string {
  const upper = symbol.toUpperCase();
  return listedOnB3 && acceptsB3Listing(assetClass) ? `${upper}:${assetClass}:b3` : `${upper}:${assetClass}`;
}

/** A escolha que a busca devolve: o ticker puro se o ativo já está na carteira. */
export function keyForSuggestion(suggestion: SymbolSuggestion, portfolio: readonly Asset[]): string {
  const held = portfolio.some((a) => a.symbol?.toUpperCase() === suggestion.symbol.toUpperCase());
  return held ? suggestion.symbol.toUpperCase() : tokenFor(suggestion.symbol, suggestion.suggested_class, suggestion.listed_on_b3);
}
