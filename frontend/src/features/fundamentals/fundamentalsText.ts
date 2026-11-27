import type { AccountTemplate, FundamentalsGap, UnsupportedReason } from "../../api/fundamentalsTypes";

/** O que a pessoa lê no lugar de um número que falta. */
export const GAP_TEXT: Record<FundamentalsGap, string> = {
  financial_template: "não se aplica a banco ou seguradora",
  unknown_template: "plano de contas que o Prumo não reconhece",
  not_reported: "a conta não veio no balanço",
  no_filing: "sem balanço do exercício na CVM",
  non_positive_base: "base zero ou negativa (patrimônio, EBITDA ou receita)",
  non_positive_endpoint: "zero ou negativo numa das pontas (prejuízo, ou nenhum provento): a taxa não tem sentido",
  insufficient_history: "falta histórico para a conta (exercícios, pregões ou anos de proventos)",
  no_quote: "sem cotação",
  no_rental_property: "o fundo não declara imóvel para renda",
  no_quarterly_report: "o fundo não aparece no informe trimestral",
  no_shares: "sem o número de ações ou de cotas",
  holding: "não se aplica a holding (o lucro vem das participações)",
  implausible: "valor informado fora do possível (erro de preenchimento)",
};

/** Por que um indicador de empresa não se aplica. */
export type NotApplicableReason = "bank" | "insurer" | "holding";

/** Por que receita, margens, dívida e EBITDA não se aplicam à companhia; `null` se se aplicam. */
export function notApplicableReason(template: AccountTemplate, holding: boolean): NotApplicableReason | null {
  if (template === "bank") return "bank";
  if (template === "insurer") return "insurer";
  return holding ? "holding" : null;
}

/** O motivo diz "não se aplica" (banco, seguradora, holding), e não "número que falta". */
export function isNotApplicable(gap: FundamentalsGap | null): boolean {
  return gap === "financial_template" || gap === "holding";
}

/** A dica de "não se aplica": por que o número enganaria. */
export const NOT_APPLICABLE_TEXT: Record<NotApplicableReason, string> = {
  bank: "Banco: o plano de contas da CVM para instituição financeira não tem receita líquida, dívida nem EBITDA. O que vale para ele é lucro, patrimônio e ROE.",
  insurer:
    "Seguradora: o plano de contas da CVM para seguradora não tem receita líquida, dívida nem EBITDA. O que vale para ela é lucro, patrimônio e ROE.",
  holding:
    "Holding: o lucro vem das empresas em que ela tem participação (equivalência patrimonial) e não passa pela receita. Margem sobre a receita passaria de 100% sem dizer nada; dívida sobre EBITDA e crescimento da receita também não se aplicam.",
};

/** A dica quando só se sabe que é instituição financeira (o motivo `financial_template`). */
export const FINANCIAL_TEMPLATE_TEXT =
  "Banco ou seguradora: o plano de contas da CVM para instituição financeira não tem receita líquida, dívida nem EBITDA. O que vale é lucro, patrimônio e ROE.";

/** Por que o ativo não tem fundamentos. */
export const UNSUPPORTED_TEXT: Record<UnsupportedReason, string> = {
  not_listed_on_b3: "Fora da B3: a CVM não tem balanço deste ativo.",
  exchange_traded_product: "ETF ou BDR: não entrega balanço à CVM.",
  no_cvm_code: "A B3 não informa código CVM para este papel (um ETF, talvez): sem balanço para mostrar.",
  no_filings: "A CVM não tem DFP nem ITR desta companhia nos últimos anos.",
  fund_not_found: "O fundo não aparece no informe mensal da CVM.",
  pending: "Buscando os dados na CVM. Volte em alguns minutos.",
};
