import type {
  Assumptions,
  ContributionSource,
  InflationSource,
  Objective,
  ObjectivesDefaults,
  Projection,
  ReturnSource,
  Scenarios,
} from "../api/objectivesTypes";
import { CLASS_META } from "./assetClasses";
import { formatPercent, plural } from "./format";
import { formatIsoDate } from "./income";
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

type Named = { kind: Objective["kind"]; asset_class: Objective["asset_class"]; income_mode?: Objective["income_mode"] };

/** O nome do objetivo, sem o valor da meta. */
export function objectiveName(objective: Named): string {
  if (objective.kind === "class_value" && objective.asset_class) return `Valor em ${CLASS_META[objective.asset_class].label}`;
  if (objective.kind === "monthly_income") {
    return objective.income_mode === "floor" ? "Renda mensal (todo mês)" : "Renda mensal (média)";
  }
  return "Patrimônio total";
}

/** A meta escrita: "R$ 1 mi" ou "R$ 3.000 por mês". */
export function targetText(objective: Pick<Objective, "kind" | "target">, money: Money): string {
  return objective.kind === "monthly_income" ? `${money.brl(objective.target)} por mês` : money.brl(objective.target);
}

/** A frase da data estimada (cenário base, meta em reais da época). */
export function projectionText(projection: Projection): string {
  switch (projection.state) {
    case "reached":
      return "A meta já foi atingida.";
    case "eta":
      return `No ritmo das premissas, a meta chega em ${formatMonth(projection.month)} (daqui a ${formatDuration(projection.months ?? 0)}).`;
    case "beyond":
      return "No ritmo das premissas, a meta leva mais de 100 anos. Revise o aporte ou a rentabilidade.";
    case "unreachable":
      return "Com estas premissas nada leva a conta até a meta. Informe um aporte mensal ou revise as premissas.";
  }
}

/** A frase do cenário base na tela de objetivos. */
export function baseText(projection: Projection): string {
  switch (projection.state) {
    case "eta":
      return `Cenário base: a meta chega em ${formatMonth(projection.month)} (daqui a ${formatDuration(projection.months ?? 0)}).`;
    case "beyond":
      return "Cenário base: mais de 100 anos. Revise o aporte ou as premissas.";
    default:
      return projectionText(projection);
  }
}

/** Uma data curta: `mar/2031`, "já", "mais de 100 anos" ou "não chega". */
export function shortDate(projection: Projection): string {
  switch (projection.state) {
    case "reached":
      return "já";
    case "eta":
      return formatMonth(projection.month);
    case "beyond":
      return "mais de 100 anos";
    case "unreachable":
      return "não chega";
  }
}

/** A faixa de datas entre o otimista e o pessimista, na leitura pedida. */
export function rangeText(scenarios: Scenarios, basis: "nominal" | "today"): string {
  const early = shortDate(scenarios.optimistic[basis]);
  const late = shortDate(scenarios.pessimistic[basis]);
  if (early === late) return `Nos três cenários: ${early}.`;
  return `Faixa: ${early} no otimista, ${late} no pessimista.`;
}

const RETURN_SOURCE_TEXT: Record<ReturnSource, string> = {
  informed: "informada por você",
  performance: "a da sua carteira em Desempenho nos últimos 12 meses, menos o IPCA do período",
  cdi_net: "o CDI de hoje menos 15% de IR e a inflação, porque a carteira ainda não tem 6 meses de série",
  fallback: "o padrão do Prumo, porque ainda não há série da carteira nem CDI",
};

/** O padrão de rentabilidade, dito na folha antes de a pessoa escolher o seu. */
export const RETURN_DEFAULT_TEXT: Record<ReturnSource, string> = {
  informed: "o seu",
  performance: "a da sua carteira em Desempenho nos últimos 12 meses, menos o IPCA do período",
  cdi_net: "o CDI de hoje menos 15% de IR e a inflação",
  fallback: "o padrão do Prumo, sem série da carteira nem CDI",
};

const CONTRIBUTION_SOURCE_TEXT: Record<ContributionSource, string> = {
  informed: "informado por você",
  ledger: "a média das suas compras nos últimos 12 meses, do livro de lançamentos",
};

/** De onde veio a inflação, com os anos e a data da pesquisa do Focus quando há. */
export function inflationSourceText(
  source: InflationSource,
  defaults: Pick<ObjectivesDefaults, "inflation_years" | "focus"> | null,
): string {
  switch (source) {
    case "informed":
      return "informada por você";
    case "focus": {
      const years = defaults?.inflation_years;
      const span = years ? (years[0] === years[1] ? ` para ${years[0]}` : ` de ${years[0]} a ${years[1]}`) : "";
      const day = defaults?.focus ? `, pesquisa de ${formatIsoDate(defaults.focus.survey_date)}` : "";
      return `o IPCA esperado pelo Focus do Banco Central${span}${day}`;
    }
    case "ipca_12m":
      return "o IPCA dos últimos 12 meses, porque ainda não há Focus no banco";
    case "target":
      return "a meta de inflação (3%), porque não há Focus nem IPCA no banco";
  }
}

/** As duas premissas principais em frase, para o ⓘ do Aportar. */
export function assumptionLines(assumptions: Assumptions, money: Money): { contribution: string; rate: string } {
  return {
    contribution: `Aporte de ${money.brl(assumptions.monthly_contribution)} por mês, em reais de hoje: ${CONTRIBUTION_SOURCE_TEXT[assumptions.contribution_source]}.`,
    rate: `Rentabilidade de ${formatPercent(assumptions.real_return_percent, 2)} ao ano acima da inflação: ${RETURN_SOURCE_TEXT[assumptions.return_source]}.`,
  };
}

export type PremiseRow = {
  /** Chave estável. */
  id: string;
  label: string;
  /** O número (ou o sim/não), sempre à vista. */
  value: string;
  /** De onde veio, em uma frase. */
  source: string;
  /** A pessoa informou (os outros são padrão). */
  informed: boolean;
};

/**
 * Cada premissa da conta: o que é, o número e de onde veio. A tela mostra o
 * número sempre e a origem ao lado; nada fica escondido.
 */
export function premiseRows(
  objective: Pick<Objective, "kind">,
  assumptions: Assumptions,
  money: Money,
  defaults: Pick<ObjectivesDefaults, "inflation_years" | "focus"> | null,
): PremiseRow[] {
  const income = objective.kind === "monthly_income";
  const rows: PremiseRow[] = [
    {
      id: "contribution",
      label: "Aporte",
      value: `${money.brl(assumptions.monthly_contribution)} por mês`,
      source: CONTRIBUTION_SOURCE_TEXT[assumptions.contribution_source],
      informed: assumptions.contribution_source === "informed",
    },
  ];
  if (!income) {
    rows.push({
      id: "return",
      label: "Rentabilidade real",
      value: `${formatPercent(assumptions.real_return_percent, 2)} ao ano`,
      source: RETURN_SOURCE_TEXT[assumptions.return_source],
      informed: assumptions.return_source === "informed",
    });
  }
  rows.push(
    {
      id: "inflation",
      label: "Inflação",
      value: `${formatPercent(assumptions.inflation_percent, 2)} ao ano`,
      source: inflationSourceText(assumptions.inflation_source, defaults),
      informed: assumptions.inflation_source === "informed",
    },
    {
      id: "reinvest",
      label: "Proventos",
      value: assumptions.reinvest_income ? "reinvestidos" : "retirados todo mês",
      source: assumptions.reinvest_source === "informed" ? "escolhido por você" : "o padrão: reinvestir",
      informed: assumptions.reinvest_source === "informed",
    },
  );
  if (income) {
    rows.push({
      id: "growth",
      label: "Crescimento dos proventos",
      value: `${formatPercent(assumptions.income_growth_percent, 2)} ao ano, acima da inflação`,
      source: assumptions.income_growth_source === "informed" ? "informado por você" : "o padrão: sem crescimento real",
      informed: assumptions.income_growth_source === "informed",
    });
  }
  rows.push(
    {
      id: "yield",
      label: "Rendimento do dinheiro novo",
      value: `${formatPercent(assumptions.income_yield_percent, 2)} ao ano`,
      source:
        assumptions.income_yield_source === "informed"
          ? "informado por você"
          : `o da carteira de hoje: ${money.brl(assumptions.starting_income)} por mês de renda estimada sobre ${money.brl(assumptions.starting_value)}`,
      informed: assumptions.income_yield_source === "informed",
    },
    {
      id: "spread",
      label: "Cenários",
      value: `${formatPoints(assumptions.scenario_spread_percent)} ao ano para cada lado`,
      source:
        (assumptions.spread_source === "informed" ? "informado por você" : "o padrão") +
        (income ? ", no crescimento dos proventos" : ", na rentabilidade real"),
      informed: assumptions.spread_source === "informed",
    },
  );
  return rows;
}

/** `2` → `2 pontos`; `1,5` → `1,5 ponto` (pontos percentuais). */
export function formatPoints(value: number): string {
  const text = value.toLocaleString("pt-BR", { maximumFractionDigits: 2 });
  return `${text} ${Math.abs(value) >= 2 ? "pontos" : "ponto"}`;
}

/** `12.5` → `"12,5"`; os campos de taxa são texto em pt-BR. */
export function percentToField(value: number | null): string {
  return value === null ? "" : String(value).replace(".", ",");
}
