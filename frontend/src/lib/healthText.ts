import type { AssetClass } from "../api/types";
import type { GapPayer, HealthWarning, RuleId, RuleStatus, RuleUnit } from "../api/healthTypes";
import { CLASS_META } from "./assetClasses";
import { formatPercent, plural } from "./format";
import type { Money } from "./money";

/**
 * As regras como a pessoa as lê: o nome, onde o limite entra na frase e o
 * porquê. O servidor só manda fatos; o texto, com acento, nasce aqui.
 */
export type RuleText = {
  /** "Nenhum ativo acima de" — vem antes do campo do limite. */
  lead: string;
  /** "da carteira" — vem depois do campo do limite. */
  tail: string;
  /** Uma linha de por quê. */
  why: string;
};

export const RULE_TEXT: Record<RuleId, RuleText> = {
  max_asset: {
    lead: "Nenhum ativo acima de",
    tail: "da carteira",
    why: "Pega o ativo que virou aposta demais, mesmo dentro da meta da classe.",
  },
  top_five: {
    lead: "Os 5 maiores ativos somam até",
    tail: "da carteira",
    why: "Só vale com mais de 5 ativos. Mostra se a carteira depende de poucos nomes.",
  },
  max_sector: {
    lead: "Nenhum setor acima de",
    tail: "da classe",
    why: "Vale dentro de Ações Nacionais (setor da B3) e de Fundos Imobiliários (tijolo, papel, fundo de fundos ou híbrido).",
  },
  min_assets: {
    lead: "Pelo menos",
    tail: "ativos em cada classe que tem meta",
    why: "Só olha a classe que já tem algum ativo; a classe vazia é a regra de baixo.",
  },
  empty_class: {
    lead: "Toda classe com meta tem ativo",
    tail: "",
    why: "Avisa quando você definiu uma meta e ainda não comprou nada nela.",
  },
  income_every_month: {
    lead: "Provento recebido em pelo menos",
    tail: "dos últimos 12 meses fechados",
    why: "Conta os meses com provento recebido, sem o mês de hoje. Precisa de 3 meses de histórico na tela Renda.",
  },
  max_payer: {
    lead: "Nenhum pagador acima de",
    tail: "da renda de 12 meses",
    why: "Pega a renda que depende de um ativo só. Usa os proventos recebidos e precisa de 3 meses de histórico e de 2 pagadores.",
  },
};

/** Maior valor de cada limite em contagem; o padrão é 50 ativos. */
export const COUNT_MAX: Partial<Record<RuleId, number>> = { income_every_month: 12 };

export const STATUS_TEXT: Record<RuleStatus, string> = {
  off: "Desligada",
  not_applicable: "Sem o que medir",
  ok: "Dentro do limite",
  warning: "Passou do limite",
};

/** `31,04%` / `15%` para percentual, `3` para contagem. */
export function formatRuleValue(unit: RuleUnit, value: number): string {
  if (unit === "count") return String(Math.round(value));
  return formatPercent(value, Number.isInteger(value) ? 0 : 2);
}

const MONTH_NAMES = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

/** `2026-02` vira `fev/2026`. */
function monthLabel(yearMonth: string): string {
  const [year, month] = yearMonth.split("-");
  return `${MONTH_NAMES[Number(month) - 1] ?? month}/${year}`;
}

function monthNames(months: number[]): string {
  return months.map((m) => MONTH_NAMES[m - 1] ?? String(m)).join(", ");
}

function payerSentence(payers: GapPayer[]): string {
  return payers
    .map((p) => `${p.label} (${CLASS_META[p.asset_class].label}; já pagou em ${monthNames(p.months)})`)
    .join(", ");
}

export function warningTitle(warning: HealthWarning): string {
  const { subject } = warning;
  const pct = formatRuleValue("percent", warning.current);
  switch (subject.kind) {
    case "asset":
      return `${subject.label} é ${pct} da carteira`;
    case "top_assets":
      return `Os ${subject.labels.length} maiores ativos somam ${pct} da carteira`;
    case "sector":
      return `${subject.sector} é ${pct} de ${CLASS_META[subject.asset_class].label}`;
    case "class": {
      const label = CLASS_META[subject.asset_class].label;
      if (warning.rule === "empty_class") return `${label} tem meta e ainda nada comprado`;
      const count = Math.round(warning.current);
      return `${label} tem ${count} ${plural(count, ["ativo", "ativos"])}`;
    }
    case "income_months": {
      const months = Math.round(warning.current);
      return `Você recebeu provento em ${months} dos últimos 12 meses`;
    }
    case "payer":
      return `${subject.label} pagou ${pct} da renda de 12 meses`;
  }
}

/** O limite da pessoa, em uma frase. */
export function limitText(warning: HealthWarning): string {
  if (warning.rule === "empty_class") return "Mínimo: 1 ativo";
  if (warning.rule === "min_assets") return `Seu mínimo: ${formatRuleValue("count", warning.limit)} ativos`;
  if (warning.rule === "income_every_month") {
    return `Seu mínimo: ${formatRuleValue("count", warning.limit)} ${plural(warning.limit, ["mês", "meses"])}`;
  }
  return `Seu limite: ${formatRuleValue("percent", warning.limit)}`;
}

/** Só os ativos dos cinco maiores, para a linha de apoio. */
export function warningMembers(warning: HealthWarning): string | null {
  const { subject } = warning;
  if (subject.kind === "top_assets") return subject.labels.join(", ");
  if (subject.kind === "income_months") return `Meses sem provento: ${subject.empty_months.map(monthLabel).join(", ")}`;
  return null;
}

/**
 * Os ativos que o aviso cita, para o card mostrar o ícone de cada um (só
 * enfeite: os nomes já estão no texto). Setor e classe não citam ativo.
 */
export function warningAssets(warning: HealthWarning): Array<{ label: string; assetClass: AssetClass | null }> {
  const { subject } = warning;
  switch (subject.kind) {
    case "asset":
    case "payer":
      return [{ label: subject.label, assetClass: null }];
    case "top_assets":
      return subject.labels.map((label) => ({ label, assetClass: null }));
    case "income_months":
      return subject.payers.map((payer) => ({ label: payer.label, assetClass: payer.asset_class }));
    default:
      return [];
  }
}

export type WarningAction = { text: string; link?: { href: string; label: string } };

/**
 * O que fazer, sem mandar vender: aportar fora do que passou do limite. O valor
 * em reais passa por `money`, que o mascara com o olho fechado (percentuais e
 * limites não são valor em reais e ficam visíveis).
 */
export function warningAction(warning: HealthWarning, money: Money): WarningAction {
  const { subject } = warning;
  const needed = warning.needed_contribution;
  const toReach = needed === null ? "" : `cerca de ${money.compact(needed)}`;
  switch (subject.kind) {
    case "asset":
      return {
        text: `Para ${subject.label} voltar ao limite sem vender nada, aporte ${toReach} em outros ativos.`,
        link: { href: "#/aportar", label: "Calcular o próximo aporte" },
      };
    case "top_assets":
      return {
        text: `Para os maiores voltarem ao limite sem vender nada, aporte ${toReach} em ativos fora deles.`,
        link: { href: "#/aportar", label: "Calcular o próximo aporte" },
      };
    case "sector":
      return {
        text: `Para ${subject.sector} voltar ao limite sem vender nada, aporte ${toReach} em outros setores de ${CLASS_META[subject.asset_class].label}.`,
        link: { href: "#/aportar", label: "Calcular o próximo aporte" },
      };
    case "class": {
      const label = CLASS_META[subject.asset_class].label;
      if (warning.rule === "empty_class") {
        return {
          text: `Cadastre um ativo em ${label} (se ainda não tiver) e use o próximo aporte nele.`,
          link: { href: "#/carteira", label: "Abrir a Carteira" },
        };
      }
      const missing = Math.max(Math.round(warning.limit - warning.current), 1);
      return {
        text: `Adicione mais ${missing} ${plural(missing, ["ativo", "ativos"])} em ${label}.`,
        link: { href: "#/carteira", label: "Abrir a Carteira" },
      };
    }
    case "income_months":
      return {
        text:
          subject.payers.length > 0
            ? `Pelo histórico, já pagaram nesses meses: ${payerSentence(subject.payers)}. Aportar neles ajuda a cobrir os meses vazios.`
            : "Nenhum ativo da carteira pagou nesses meses até agora. Veja na tela Renda o calendário do que você já recebeu.",
        link: { href: "#/renda", label: "Abrir a Renda" },
      };
    case "payer":
      return {
        text: `${subject.label} pagou ${money.compact(subject.received)} dos ${money.compact(subject.total)} que você recebeu em 12 meses. Para espalhar, aporte em outros ativos da carteira que paguem renda.`,
        link: { href: "#/aportar", label: "Calcular o próximo aporte" },
      };
  }
}

/** O nome da regra sem o número ("Nenhum ativo acima de … da carteira"), para leitor de tela. */
export function ruleName(rule: RuleId): string {
  const { lead, tail } = RULE_TEXT[rule];
  return tail ? `${lead} … ${tail}` : lead;
}

/**
 * Valida o que a pessoa digitou no limite de uma regra. `null` quando está
 * bom; senão a mensagem. Espelha `RuleId::validate_threshold` do servidor.
 */
export function thresholdError(unit: RuleUnit, value: number | null, maxCount = 50): string | null {
  if (value === null) return "Digite um número.";
  if (unit === "count") {
    return Number.isInteger(value) && value >= 1 && value <= maxCount
      ? null
      : `Use um número inteiro de 1 a ${maxCount}.`;
  }
  const decimals = Math.round(value * 100) / 100 === value;
  return value > 0 && value <= 100 && decimals ? null : "Use um percentual maior que 0 e até 100, com até duas casas.";
}
