import type { Change, CompanyResults, Comparison, Quarter, TopLine } from "../../api/resultsTypes";
import { formatPercent } from "../../lib/format";
import { GAP_TEXT } from "../fundamentals/fundamentalsText";

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

const parts = (iso: string) => iso.split("-").map(Number) as [number, number, number];

type QuarterDates = Pick<Quarter, "fiscal_quarter" | "period_start" | "period_end">;

/** O exercício é o ano civil quando o trimestre termina no mês 3 × o número dele. */
function calendar(q: QuarterDates): boolean {
  return parts(q.period_end)[1] === q.fiscal_quarter * 3;
}

/** "3º tri 2026"; com exercício fora do ano civil, os meses: "jul–set/2026". */
export function quarterLabel(q: QuarterDates): string {
  const [year, month] = parts(q.period_end);
  if (calendar(q)) return `${q.fiscal_quarter}º tri ${year}`;
  const start = parts(q.period_start)[1];
  return `${MONTHS[start - 1]}–${MONTHS[month - 1]}/${year}`;
}

/** "3T26" no eixo; fora do ano civil, o mês do fim: "set/26". */
export function quarterShort(q: QuarterDates): string {
  const [year, month] = parts(q.period_end);
  const yy = String(year).slice(2);
  return calendar(q) ? `${q.fiscal_quarter}T${yy}` : `${MONTHS[month - 1]}/${yy}`;
}

/** O nome da linha de cima, pelo plano de contas. */
export const TOP_LINE_LABEL: Record<TopLine, string> = {
  revenue: "Receita líquida",
  intermediation_result: "Resultado bruto da intermediação",
  insurance_result: "Resultado bruto de seguros",
  unknown: "Linha de cima",
};

/** A linha de cima em minúsculas, para o meio da frase. */
const TOP_LINE_WORD: Record<TopLine, string> = {
  revenue: "receita",
  intermediation_result: "resultado da intermediação",
  insurance_result: "resultado de seguros",
  unknown: "linha de cima",
};

export type Tone = "up" | "down" | "neutral" | "missing";

/** O que a tela escreve para uma variação: o texto (sempre), o tom (só reforça) e o porquê de faltar. */
export type ChangeView = { text: string; tone: Tone; hint: string | null };

/** `+25,0%` / `−3,2%`, de uma fração. */
export function signedPercent(fraction: number): string {
  const text = formatPercent(Math.abs(fraction) * 100, 1);
  if (Math.abs(fraction) < 0.0005) return formatPercent(0, 1);
  return fraction > 0 ? `+${text}` : `−${text}`;
}

const PROFIT_LABEL = {
  left_loss: "saiu do prejuízo",
  entered_loss: "entrou no prejuízo",
  smaller_loss: "prejuízo menor",
  larger_loss: "prejuízo maior",
} as const;

const TOP_LABEL = {
  left_loss: "saiu do negativo",
  entered_loss: "ficou negativo",
  smaller_loss: "negativo menor",
  larger_loss: "negativo maior",
} as const;

/**
 * A variação em palavras. Com a base positiva, o percentual; com prejuízo numa
 * das pontas, o rótulo ("saiu do prejuízo", "prejuízo menor"...), nunca um
 * percentual sobre base negativa.
 */
export function changeView(change: Change, line: "profit" | "top"): ChangeView {
  switch (change.kind) {
    case "percent": {
      const value = change.value ?? 0;
      return { text: signedPercent(value), tone: value > 0 ? "up" : value < 0 ? "down" : "neutral", hint: null };
    }
    case "left_loss":
    case "smaller_loss":
      return { text: (line === "profit" ? PROFIT_LABEL : TOP_LABEL)[change.kind], tone: "up", hint: null };
    case "entered_loss":
    case "larger_loss":
      return { text: (line === "profit" ? PROFIT_LABEL : TOP_LABEL)[change.kind], tone: "down", hint: null };
    case "unchanged":
      return { text: "igual", tone: "neutral", hint: null };
    case "no_base":
      return { text: "sem base", tone: "neutral", hint: "O trimestre comparado tem zero: não há variação a dizer." };
    case "missing":
      return { text: "—", tone: "missing", hint: change.gap ? GAP_TEXT[change.gap] : "sem número" };
  }
}

/** A diferença de margem em pontos: `+1,2 p.p.`. */
export function pointsText(fraction: number): string {
  const points = fraction * 100;
  const text = Math.abs(points).toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  if (Math.abs(points) < 0.05) return "0,0 p.p.";
  return `${points > 0 ? "+" : "−"}${text} p.p.`;
}

export type BubbleMode = "year_ago" | "previous_quarter";

export const MODE_LABEL: Record<BubbleMode, string> = {
  year_ago: "contra o mesmo trimestre do ano anterior",
  previous_quarter: "contra o trimestre anterior",
};

/** A comparação do modo escolhido. */
export function comparisonOf(company: CompanyResults, mode: BubbleMode): Comparison | null {
  return mode === "year_ago" ? company.year_ago : company.previous_quarter;
}

const percentOf = (change: Change): number | null =>
  change.kind === "percent" ? change.value : change.kind === "unchanged" ? 0 : null;

/** As duas variações em fração, quando as duas têm percentual; senão, por que a empresa fica fora do gráfico. */
export function bubblePoint(
  company: CompanyResults,
  mode: BubbleMode,
): { x: number; y: number } | { out: string } {
  const comparison = comparisonOf(company, mode);
  if (!comparison) {
    return { out: mode === "year_ago" ? "sem o trimestre de um ano antes" : "sem o trimestre anterior" };
  }
  const x = percentOf(comparison.net_income);
  const y = percentOf(comparison.top_line);
  if (x === null) return { out: `lucro: ${changeView(comparison.net_income, "profit").text.replace("—", "sem número")}` };
  if (y === null) {
    return {
      out: `${TOP_LINE_WORD[company.top_line]}: ${changeView(comparison.top_line, "top").text.replace("—", "sem número")}`,
    };
  }
  return { x, y };
}

/**
 * O eixo de uma variação: de −`cap` a `cap` no máximo (200%), sempre com o
 * zero, e com folga. Quem passa do teto fica na borda, com o número escrito.
 */
export function axisDomain(values: number[], cap = 2): [number, number] {
  const lo = Math.max(-cap, Math.min(-0.1, ...values));
  const hi = Math.min(cap, Math.max(0.1, ...values));
  const pad = (hi - lo) * 0.08;
  return [lo - pad, hi + pad];
}
