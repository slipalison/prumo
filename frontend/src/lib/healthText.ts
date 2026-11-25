import type { HealthWarning, RuleId, RuleStatus, RuleUnit } from "../api/healthTypes";
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
};

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
  }
}

/** O limite da pessoa, em uma frase. */
export function limitText(warning: HealthWarning): string {
  if (warning.rule === "empty_class") return "Mínimo: 1 ativo";
  if (warning.rule === "min_assets") return `Seu mínimo: ${formatRuleValue("count", warning.limit)} ativos`;
  return `Seu limite: ${formatRuleValue("percent", warning.limit)}`;
}

/** Só os ativos dos cinco maiores, para a linha de apoio. */
export function warningMembers(warning: HealthWarning): string | null {
  return warning.subject.kind === "top_assets" ? warning.subject.labels.join(", ") : null;
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
export function thresholdError(unit: RuleUnit, value: number | null): string | null {
  if (value === null) return "Digite um número.";
  if (unit === "count") {
    return Number.isInteger(value) && value >= 1 && value <= 50 ? null : "Use um número inteiro de 1 a 50.";
  }
  const decimals = Math.round(value * 100) / 100 === value;
  return value > 0 && value <= 100 && decimals ? null : "Use um percentual maior que 0 e até 100, com até duas casas.";
}
