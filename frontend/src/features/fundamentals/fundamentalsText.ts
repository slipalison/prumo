import type { FundamentalsGap, UnsupportedReason } from "../../api/fundamentalsTypes";

/** O que a pessoa lê no lugar de um número que falta. */
export const GAP_TEXT: Record<FundamentalsGap, string> = {
  financial_template: "não se aplica a banco ou seguradora",
  unknown_template: "plano de contas que o Prumo não reconhece",
  not_reported: "a conta não veio no balanço",
  no_filing: "sem balanço do exercício na CVM",
  non_positive_base: "base zero ou negativa (patrimônio, EBITDA ou receita)",
  non_positive_endpoint: "prejuízo numa das pontas: a taxa não tem sentido",
  insufficient_history: "faltam exercícios para a conta",
  no_quote: "sem cotação",
  no_rental_property: "o fundo não declara imóvel para renda",
  no_quarterly_report: "o fundo não aparece no informe trimestral",
  no_shares: "sem o número de ações ou de cotas",
};

/** Por que o ativo não tem fundamentos. */
export const UNSUPPORTED_TEXT: Record<UnsupportedReason, string> = {
  not_listed_on_b3: "Fora da B3: a CVM não tem balanço deste ativo.",
  exchange_traded_product: "ETF ou BDR: não entrega balanço à CVM.",
  no_cvm_code: "A B3 não informa código CVM para este papel (um ETF, talvez): sem balanço para mostrar.",
  no_filings: "A CVM não tem DFP nem ITR desta companhia nos últimos anos.",
  fund_not_found: "O fundo não aparece no informe mensal da CVM.",
  pending: "Buscando os dados na CVM. Volte em alguns minutos.",
};
