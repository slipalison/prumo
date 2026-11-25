import type { Asset, AssetClass } from "../api/types";

/** Quantos setores têm cor própria na rosca; o resto vira "Outros setores". */
export const SECTOR_COLORS = 7;

export type SectorSlice = {
  key: string;
  label: string;
  value: number;
  /** Fatia do valor da classe (0–100). */
  percent: number;
  /** Cor da rosca: `var(--sector-N)` ou um dos tons neutros. */
  color: string;
  kind: "sector" | "other" | "none";
};

/** Classes cujo setor vem de fonte pública: o que a rosca por setor cobre. */
export function hasSectorSource(assetClass: AssetClass): boolean {
  return assetClass === "br_stocks" || assetClass === "br_reits";
}

/** Em FIIs o "setor" é o tipo do fundo (tijolo, papel...); o nome da seção diz isso. */
export function sectorHeading(assetClass: AssetClass): string {
  return assetClass === "br_reits" ? "Por tipo de fundo" : "Por setor";
}

/**
 * Os setores da classe, do maior para o menor, mais "Outros setores" (o que
 * passa de sete) e "Sem setor" (o que a fonte não classificou). Só entra ativo
 * com valor. Vazio quando a classe não tem nada comprado.
 */
export function sectorSlices(assets: Asset[], assetClass: AssetClass): SectorSlice[] {
  if (!hasSectorSource(assetClass)) return [];
  const owned = assets.filter((a) => a.asset_class === assetClass && a.current_value > 0);
  const total = owned.reduce((acc, a) => acc + a.current_value, 0);
  if (total <= 0) return [];

  const bySector = new Map<string, number>();
  let unclassified = 0;
  for (const asset of owned) {
    if (asset.sector === null) unclassified += asset.current_value;
    else bySector.set(asset.sector, (bySector.get(asset.sector) ?? 0) + asset.current_value);
  }
  const sorted = [...bySector.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], "pt-BR"));
  const share = (value: number) => Math.round((value / total) * 10000) / 100;

  const slices: SectorSlice[] = sorted.slice(0, SECTOR_COLORS).map(([label, value], index) => ({
    key: `sector-${label}`,
    label,
    value,
    percent: share(value),
    color: `var(--sector-${index + 1})`,
    kind: "sector",
  }));
  const rest = sorted.slice(SECTOR_COLORS).reduce((acc, [, value]) => acc + value, 0);
  if (rest > 0) {
    slices.push({ key: "other", label: "Outros setores", value: rest, percent: share(rest), color: "var(--sector-other)", kind: "other" });
  }
  if (unclassified > 0) {
    slices.push({ key: "none", label: "Sem setor", value: unclassified, percent: share(unclassified), color: "var(--sector-none)", kind: "none" });
  }
  return slices;
}
