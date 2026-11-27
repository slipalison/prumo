import type { Asset, AssetClass } from "../api/types";
import { ASSET_CLASSES, CLASS_META } from "./assetClasses";
import { assetName } from "./rating";

/**
 * O último nível da composição:
 * - `sector`: carteira → classe → setor (em FIIs, o tipo do fundo);
 * - `segment`: carteira → classe → segmento;
 * - `asset`: carteira → classe → setor → ativo.
 */
export type CompositionLevel = "sector" | "segment" | "asset";

export const LEVEL_LABEL: Record<CompositionLevel, string> = {
  sector: "Setor",
  segment: "Segmento",
  asset: "Ativo",
};

export const NO_SECTOR = "Sem setor";
export const NO_SEGMENT = "Sem segmento";

/** Fatias abaixo disto (da carteira), em % , juntam-se em "Outros" no diagrama. */
export const MINOR_PERCENT = 1;

export type CompositionNode = {
  id: string;
  label: string;
  value: number;
  /** Fatia da carteira, em %. */
  percent: number;
  /** 0 = carteira, 1 = classe, 2 e 3 = os níveis de baixo. */
  column: number;
  parent: string | null;
  /** A classe do nó (a raiz não tem). */
  assetClass: AssetClass | null;
  kind: "root" | "class" | "group" | "asset" | "others";
  /** Quantos ativos o nó reúne. */
  assets: number;
};

export type Composition = {
  total: number;
  /** Nomes das colunas, da esquerda para a direita. */
  headings: string[];
  /** Em ordem de desenho: cada nó seguido da descendência dele. */
  nodes: CompositionNode[];
};

const TREASURY_TYPES: Record<string, string> = {
  SELIC: "Tesouro Selic",
  PRE: "Tesouro Prefixado",
  PREJS: "Tesouro Prefixado com juros",
  IPCA: "Tesouro IPCA+",
  IPCAJS: "Tesouro IPCA+ com juros",
  RENDA: "Tesouro Renda+",
};

/**
 * O agrupamento do ativo no nível de setor ou segmento. Ações e FIIs da B3 usam
 * o que a fonte pública trouxe ("Sem setor" só quando ela não classificou); as
 * demais classes não têm setor da B3 e usam o agrupamento natural delas: o tipo
 * do título na renda fixa, "Cripto", e o tipo do papel no exterior.
 */
export function groupLabel(asset: Asset, level: "sector" | "segment"): string {
  const own = level === "sector" ? asset.sector : asset.segment;
  switch (asset.asset_class) {
    case "br_stocks":
    case "br_reits":
      return own ?? (level === "sector" ? NO_SECTOR : NO_SEGMENT);
    case "crypto":
      return "Cripto";
    case "fixed_income":
    case "intl_fixed_income": {
      if (asset.treasury) {
        const kind = /^TD-([A-Z]+)-/.exec(asset.symbol ?? "")?.[1] ?? "";
        return TREASURY_TYPES[kind] ?? "Tesouro Direto";
      }
      if (asset.listed_on_b3) return "ETF de renda fixa";
      return asset.asset_class === "intl_fixed_income" ? "Renda fixa internacional" : "Renda fixa por valor";
    }
    case "us_reits":
      return own ?? "REITs";
    case "intl_stocks":
      return own ?? (asset.listed_on_b3 ? "BDRs e ETFs na B3" : "Ações e ETFs do exterior");
  }
}

type Entry = { key: string; label: string; value: number; assets: Asset[] };

const byValueThenLabel = (a: Entry, b: Entry) => b.value - a.value || a.label.localeCompare(b.label, "pt-BR");

function group(assets: Asset[], keyOf: (asset: Asset) => string, none: string): Entry[] {
  const map = new Map<string, Entry>();
  for (const asset of assets) {
    const label = keyOf(asset);
    const entry = map.get(label) ?? { key: label, label, value: 0, assets: [] };
    entry.value += asset.current_value;
    entry.assets.push(asset);
    map.set(label, entry);
  }
  // "Sem setor" por último, mesmo quando é a maior fatia: é o que a fonte não disse.
  return [...map.values()].sort((a, b) => (a.label === none ? 1 : b.label === none ? -1 : byValueThenLabel(a, b)));
}

function headingsOf(level: CompositionLevel): string[] {
  const base = ["Carteira", "Classe"];
  if (level === "sector") return [...base, "Setor"];
  if (level === "segment") return [...base, "Segmento"];
  return [...base, "Setor", "Ativo"];
}

/**
 * A árvore carteira → classe → … → ativo, só com o que tem valor. `collapse`
 * junta as fatias pequenas do último nível em "Outros" (o diagrama precisa; a
 * tabela e as barras mostram tudo).
 */
export function buildComposition(assets: Asset[], level: CompositionLevel, collapse: boolean): Composition {
  const held = assets.filter((a) => a.current_value > 0);
  const total = held.reduce((sum, a) => sum + a.current_value, 0);
  const headings = headingsOf(level);
  if (total <= 0) return { total: 0, headings, nodes: [] };

  const percent = (value: number) => (value / total) * 100;
  const nodes: CompositionNode[] = [
    { id: "root", label: "Carteira", value: total, percent: 100, column: 0, parent: null, assetClass: null, kind: "root", assets: held.length },
  ];
  const push = (node: Omit<CompositionNode, "percent">) => nodes.push({ ...node, percent: percent(node.value) });

  /** Folhas de um pai; juntas em "Outros" as pequenas, se pedido e se forem duas ou mais. */
  const leaves = (parentId: string, assetClass: AssetClass, column: number, entries: Entry[], kind: "group" | "asset") => {
    const minor = collapse ? entries.filter((e) => percent(e.value) < MINOR_PERCENT) : [];
    const merged = minor.length >= 2;
    for (const entry of entries) {
      if (merged && minor.includes(entry)) continue;
      push({ id: `${parentId}/${entry.key}`, label: entry.label, value: entry.value, column, parent: parentId, assetClass, kind, assets: entry.assets.length });
    }
    if (merged) {
      const count = minor.reduce((sum, e) => sum + e.assets.length, 0);
      push({
        id: `${parentId}/others`,
        label: `Outros (${minor.length})`,
        value: minor.reduce((sum, e) => sum + e.value, 0),
        column,
        parent: parentId,
        assetClass,
        kind: "others",
        assets: count,
      });
    }
  };

  const classes = ASSET_CLASSES.map((assetClass) => {
    const own = held.filter((a) => a.asset_class === assetClass);
    return { key: assetClass, label: CLASS_META[assetClass].label, value: own.reduce((s, a) => s + a.current_value, 0), assets: own };
  })
    .filter((c) => c.assets.length > 0)
    .sort(byValueThenLabel);

  for (const entry of classes) {
    const assetClass = entry.key;
    const classId = `class:${assetClass}`;
    push({ id: classId, label: entry.label, value: entry.value, column: 1, parent: "root", assetClass, kind: "class", assets: entry.assets.length });
    if (level === "sector") {
      leaves(classId, assetClass, 2, group(entry.assets, (a) => groupLabel(a, "sector"), NO_SECTOR), "group");
    } else if (level === "segment") {
      leaves(classId, assetClass, 2, group(entry.assets, (a) => groupLabel(a, "segment"), NO_SEGMENT), "group");
    } else {
      for (const sector of group(entry.assets, (a) => groupLabel(a, "sector"), NO_SECTOR)) {
        const sectorId = `${classId}/${sector.key}`;
        push({ id: sectorId, label: sector.label, value: sector.value, column: 2, parent: classId, assetClass, kind: "group", assets: sector.assets.length });
        const items: Entry[] = sector.assets
          .map((a) => ({ key: a.id, label: assetName(a), value: a.current_value, assets: [a] }))
          .sort(byValueThenLabel);
        leaves(sectorId, assetClass, 3, items, "asset");
      }
    }
  }
  return { total, headings, nodes };
}

/** As folhas (o último nível) de cada classe, para as barras empilhadas. */
export function leavesByClass(composition: Composition): Array<{ node: CompositionNode; leaves: CompositionNode[] }> {
  const last = composition.headings.length - 1;
  const classes = composition.nodes.filter((n) => n.kind === "class");
  return classes.map((node) => ({
    node,
    leaves: composition.nodes.filter((n) => n.column === last && n.assetClass === node.assetClass),
  }));
}
