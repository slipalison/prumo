import type { PortfolioYield, YieldGap, YieldOnCost } from "../api/incomeTypes";
import { formatPercent } from "./format";

const GAP_TEXT: Record<YieldGap, string> = {
  unknown_cost: "informe o preço médio",
  zero_cost: "custo zero: sem base",
  no_position: "sem posição",
};

/**
 * O yield on cost como a tela o diz: `6,2%`, ou — quando o custo não existe —
 * o motivo, no lugar do número. Sem recebido nos 12 meses a renda vem da
 * estimativa, e o texto avisa. Percentual não é patrimônio: fica à vista com o
 * olho fechado.
 */
export function yieldText(y: Pick<YieldOnCost, "percent" | "gap" | "basis"> | PortfolioYield): string {
  if (y.percent === null) return y.gap ? GAP_TEXT[y.gap] : "—";
  return `${formatPercent(y.percent)}${y.basis === "estimate" ? " (estimado)" : ""}`;
}
