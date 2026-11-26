import type { Assumptions, Objective, Projection, ReturnSource } from "../api/objectivesTypes";
import { CLASS_META } from "./assetClasses";
import { formatPercent, plural } from "./format";
import type { Money } from "./money";

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"] as const;

/** `2031-03` → `mar/2031`. Vazio se não for um mês. */
export function formatMonth(value: string | null): string {
  const match = /^(\d{4})-(\d{2})$/.exec(value ?? "");
  const name = match ? MONTHS[Number(match[2]) - 1] : undefined;
  return match && name ? `${name}/${match[1]}` : "";
}

/** `52` → `4 anos e 4 meses`; `1` → `1 mês`. */
export function formatDuration(months: number): string {
  const years = Math.floor(months / 12);
  const rest = months % 12;
  const parts: string[] = [];
  if (years > 0) parts.push(`${years} ${plural(years, ["ano", "anos"])}`);
  if (rest > 0 || years === 0) parts.push(`${rest} ${plural(rest, ["mês", "meses"])}`);
  return parts.join(" e ");
}

/** O nome do objetivo, sem o valor da meta. */
export function objectiveName(objective: { kind: Objective["kind"]; asset_class: Objective["asset_class"] }): string {
  if (objective.kind === "class_value" && objective.asset_class) return `Valor em ${CLASS_META[objective.asset_class].label}`;
  return "Patrimônio total";
}

/** A frase da data estimada. */
export function projectionText(projection: Projection): string {
  switch (projection.state) {
    case "reached":
      return "A meta já foi atingida.";
    case "eta":
      return `No ritmo das premissas, a meta chega em ${formatMonth(projection.month)} (daqui a ${formatDuration(projection.months ?? 0)}).`;
    case "beyond":
      return "No ritmo das premissas, a meta leva mais de 100 anos. Revise o aporte ou a rentabilidade.";
    case "unreachable":
      return "Sem aporte e sem rentabilidade positiva, a meta não chega. Informe um aporte mensal.";
  }
}

const RETURN_SOURCE_TEXT: Record<Assumptions["return_source"], string> = {
  informed: "informada por você",
  performance: "a da sua carteira em Desempenho, dos últimos 12 meses, ao ano e antes do IR",
  cdi_net: "o CDI de hoje menos 15% de IR, porque a carteira ainda não tem 6 meses de série",
  fallback: "o padrão do Prumo, porque ainda não há série da carteira nem CDI",
};

/** O padrão de rentabilidade, dito na folha antes de a pessoa escolher o seu. */
export const RETURN_DEFAULT_TEXT: Record<ReturnSource, string> = {
  informed: "o seu",
  performance: "a rentabilidade da sua carteira em Desempenho, dos últimos 12 meses, antes do IR",
  cdi_net: "o CDI de hoje menos 15% de IR",
  fallback: "o padrão do Prumo, sem série da carteira nem CDI",
};

const CONTRIBUTION_SOURCE_TEXT: Record<Assumptions["contribution_source"], string> = {
  informed: "informado por você",
  ledger: "a média das suas compras nos últimos 12 meses, do livro de lançamentos",
};

/** A explicação de cada premissa da conta: o que é, em número, e de onde veio. */
export function assumptionLines(assumptions: Assumptions, money: Money): { contribution: string; rate: string } {
  return {
    contribution: `Aporte de ${money.brl(assumptions.monthly_contribution)} por mês: ${CONTRIBUTION_SOURCE_TEXT[assumptions.contribution_source]}.`,
    rate: `Rentabilidade de ${formatPercent(assumptions.annual_return_percent, 2)} ao ano: ${RETURN_SOURCE_TEXT[assumptions.return_source]}.`,
  };
}

/** `12.5` → `"12,5"`; o campo de rentabilidade é texto em pt-BR. */
export function percentToField(value: number | null): string {
  return value === null ? "" : String(value).replace(".", ",");
}
