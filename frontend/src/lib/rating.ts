import type { Asset } from "../api/types";
import { CLASS_META, isTreasuryKey } from "./assetClasses";

/** Ativo de classe com checklist que ainda não tem nota — e por isso não recebe aporte. */
export function needsRating(asset: Pick<Asset, "asset_class" | "score">): boolean {
  return CLASS_META[asset.asset_class].checklist !== null && asset.score <= 0;
}

/** Como a pessoa chama o ativo: o ticker, ou o nome quando não há ticker (título do Tesouro tem chave, não ticker). */
export function assetName(asset: Pick<Asset, "symbol" | "display_name">): string {
  return asset.symbol && !isTreasuryKey(asset.symbol) ? asset.symbol : asset.display_name;
}

/** A dica do topo da folha quando ela abre para dar nota. */
export function ratingIntro(asset: Pick<Asset, "symbol" | "display_name">): string {
  return `Responda o checklist para o ${assetName(asset)} ganhar nota — cada sim soma 1.`;
}

/** Acha o ativo de um aviso pelo que o servidor manda em `symbols`. */
export function findAssetByLabel(assets: Asset[], label: string): Asset | undefined {
  return assets.find((a) => a.symbol === label) ?? assets.find((a) => a.display_name === label);
}

/** O preço de hoje passou do teto da pessoa: o ativo fica fora do aporte. */
export function isAboveCeiling(asset: Pick<Asset, "unit_price" | "ceiling" | "price_source">): boolean {
  const max = asset.ceiling?.value ?? null;
  return max !== null && asset.unit_price !== null && asset.price_source !== "missing" && asset.unit_price > max;
}

/** O teto pelo método que a pessoa escolheu está sem valor hoje: o ativo fica fora do aporte. */
export function isCeilingUnavailable(asset: Pick<Asset, "ceiling">): boolean {
  return asset.ceiling !== null && asset.ceiling.unavailable_reason !== null;
}
