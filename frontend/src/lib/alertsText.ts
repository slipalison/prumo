import type { AlertKind, AlertType } from "../api/alertsTypes";
import type { Money } from "./money";

type KindText = {
  /** O nome do tipo, também o rótulo do interruptor. */
  name: string;
  /** Em uma frase, quando avisa. */
  why: string;
  /** Antes do campo de limite, nos tipos que têm um. */
  lead?: string;
  /** Depois do campo de limite. */
  tail?: string;
  /** Por que o tipo não pode ser ligado, quando o servidor diz que está indisponível. */
  unavailable?: string;
};

export const KIND_TEXT: Record<AlertKind, KindText> = {
  income_tomorrow: {
    name: "Provento que cai amanhã",
    why: "No dia anterior ao pagamento de um provento lançado em Renda.",
  },
  class_drift: {
    name: "Classe fora da meta",
    lead: "Avisar quando uma classe ficar a",
    tail: "p.p. ou mais da meta",
    why: "Para cima ou para baixo, em pontos percentuais da carteira. Só avisa quando a classe passa a ficar fora, não todo dia.",
  },
  price_below_ceiling: {
    name: "Ativo voltou ao teto de preço",
    why: "Quando um ativo com teto de preço volta a custar o teto ou menos e passa a ser comprável de novo no aporte.",
  },
  objective_achieved: {
    name: "Objetivo atingido",
    why: "Quando um objetivo seu, como a meta de renda mensal, é atingido.",
  },
  tax_exemption: {
    name: "Isentômetro de ações",
    lead: "Avisar a partir de",
    tail: "% do limite de isenção de vendas no mês",
    why: "Quando as vendas de ações do mês chegam a esse percentual do limite de isenção e, de novo, quando passam dele. É a mesma conta da tela Impostos.",
    unavailable: "Indisponível: o Prumo ainda não tem a regra de IR deste ano, e não usa a de outro ano.",
  },
  health_warning: {
    name: "Aviso novo da análise da carteira",
    why: "Quando uma regra da Análise da carteira passa a avisar. O que já estava avisando não se repete.",
  },
};

export function kindName(kind: AlertKind): string {
  return KIND_TEXT[kind].name;
}

export type DigestSection = { title: string; lines: string[] };

/**
 * Como o resumo fica no Telegram, com dados FICTÍCIOS. Espelha o texto que o
 * servidor monta, só dos tipos ligados. `hideValues` é a escolha "sem valores" do resumo;
 * `money` é o olho da tela, que mascara o exemplo também.
 */
export function sampleDigest(types: AlertType[], hideValues: boolean, money: Money): DigestSection[] {
  const on = new Set(types.filter((t) => t.enabled && t.available).map((t) => t.kind));
  const sections: DigestSection[] = [];
  const add = (kind: AlertKind, title: string, line: string) => {
    if (on.has(kind)) sections.push({ title, lines: [line] });
  };
  add(
    "income_tomorrow",
    "Proventos amanhã",
    hideValues ? "- KNRI11 paga amanhã." : `- KNRI11 paga amanhã: ${money.brl(90)} líquidos.`,
  );
  add("class_drift", "Classes fora da meta", "- Fundos Imobiliários está com 28,4% da carteira (meta 20%), 8,4 p.p. acima.");
  add(
    "price_below_ceiling",
    "No seu teto de preço",
    hideValues
      ? "- ITSA4 voltou a ficar no seu teto de preço ou abaixo."
      : `- ITSA4 voltou a ficar no seu teto de preço: ${money.brl(9.5)} (teto ${money.brl(10)}).`,
  );
  add(
    "objective_achieved",
    "Objetivos atingidos",
    hideValues ? "- Objetivo de renda mensal atingido." : `- Objetivo de renda mensal atingido: ${money.brl(3000)}.`,
  );
  add(
    "tax_exemption",
    "Isentômetro de ações",
    hideValues
      ? "- Você já vendeu 82,5% do limite de isenção de ações do mês."
      : `- Você já vendeu 82,5% do limite de isenção de ações do mês. Faltam ${money.brl(3500)} para perder a isenção (${money.brl(16500)} de ${money.brl(20000)}).`,
  );
  add(
    "health_warning",
    "Análise da carteira",
    hideValues
      ? "- KNRI11 está com 23,5% da carteira (limite 15%)."
      : `- KNRI11 está com 23,5% da carteira (limite 15%). Um aporte de cerca de ${money.brl(1234.5)} em outros ativos volta ao limite.`,
  );
  return sections;
}
