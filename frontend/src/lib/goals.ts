import { ASSET_CLASSES, type AssetClass, type ClassSummary, type GoalTargets } from "../api/types";

export type PresetId = "conservative" | "moderate" | "aggressive";

export type Preset = {
  id: PresetId;
  label: string;
  description: string;
  targets: GoalTargets;
};

const zero = (): GoalTargets =>
  Object.fromEntries(ASSET_CLASSES.map((c) => [c, 0])) as GoalTargets;

export const PRESETS: Preset[] = [
  {
    id: "conservative",
    label: "Conservador",
    description: "Proteção e liquidez primeiro. A maior parte em renda fixa.",
    targets: { ...zero(), fixed_income: 60, intl_fixed_income: 10, br_stocks: 10, br_reits: 10, intl_stocks: 10 },
  },
  {
    id: "moderate",
    label: "Moderado",
    description: "Equilíbrio entre segurança e crescimento.",
    targets: { ...zero(), fixed_income: 35, br_stocks: 25, br_reits: 15, intl_stocks: 15, us_reits: 5, crypto: 5 },
  },
  {
    id: "aggressive",
    label: "Arrojado",
    description: "Crescimento no longo prazo, aceitando mais oscilação.",
    targets: { ...zero(), br_stocks: 35, intl_stocks: 25, br_reits: 10, us_reits: 5, crypto: 5, fixed_income: 20 },
  },
];

export function targetsFromClasses(classes: ClassSummary[]): GoalTargets {
  const targets = zero();
  for (const c of classes) targets[c.asset_class] = c.target_percent;
  return targets;
}

/** Soma com duas casas, para 33,33 + 33,33 + 33,34 dar 100 e não 99,99999. */
export function sumTargets(targets: GoalTargets): number {
  const cents = ASSET_CLASSES.reduce((acc, c) => acc + Math.round(targets[c] * 100), 0);
  return cents / 100;
}

export function isComplete(targets: GoalTargets): boolean {
  return Math.abs(sumTargets(targets) - 100) < 0.005;
}

/**
 * Leva a soma a 100 mantendo as proporções entre as classes que já têm meta.
 * Inteiros, pelo maior resto: a soma fecha em 100 exatos. Sem nenhuma meta, não
 * há proporção para manter — devolve o que recebeu.
 */
export function balanceTargets(targets: GoalTargets): GoalTargets {
  const total = sumTargets(targets);
  if (total <= 0) return { ...targets };
  const raw = ASSET_CLASSES.map((c) => ({ c, exact: (targets[c] / total) * 100 }));
  const floored = raw.map((r) => ({ ...r, value: Math.floor(r.exact), rest: r.exact - Math.floor(r.exact) }));
  let missing = 100 - floored.reduce((acc, r) => acc + r.value, 0);
  const byRest = [...floored].sort((a, b) => b.rest - a.rest);
  for (const r of byRest) {
    if (missing <= 0) break;
    if (r.exact > 0) {
      r.value += 1;
      missing -= 1;
    }
  }
  const result = zero();
  for (const r of floored) result[r.c] = r.value;
  return result;
}

export function sameTargets(a: GoalTargets, b: GoalTargets): boolean {
  return ASSET_CLASSES.every((c: AssetClass) => Math.abs(a[c] - b[c]) < 0.005);
}
