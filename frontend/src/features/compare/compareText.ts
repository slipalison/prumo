import type { CompareDirection, CompareReason, CompareRowId, CurveReason } from "../../api/compareTypes";
import { GAP_TEXT, UNSUPPORTED_TEXT } from "../fundamentals/fundamentalsText";

/** O nome de cada linha e a dica que diz o que ela mede. */
export const ROW_TEXT: Record<CompareRowId, { label: string; hint: string }> = {
  price: { label: "Preço", hint: "A última cotação, na moeda do ativo." },
  return_12m: {
    label: "Retorno em 12 meses",
    hint: "Variação do preço em 12 meses, na moeda do ativo e sem contar proventos. Desdobramentos e bonificações já estão ajustados.",
  },
  dividend_yield_12m: {
    label: "Rendimento em 12 meses (DY)",
    hint: "Proventos brutos com data ex nos últimos 12 meses, divididos pelo preço de hoje. Em ações da B3, o JCP vem junto com o dividendo.",
  },
  price_to_book: {
    label: "P/VP",
    hint: "Preço dividido pelo patrimônio por ação (empresa) ou por cota (fundo). Em 1, o preço iguala o patrimônio contábil.",
  },
  segment: {
    label: "Setor ou tipo",
    hint: "Setor pela Classificação Setorial da B3 (ações) ou tipo do fundo pelo balanço do informe da CVM: tijolo, papel, fundo de fundos ou híbrido.",
  },
  price_to_earnings: {
    label: "P/L",
    hint: "Preço dividido pelo lucro por ação dos últimos 12 meses (LPA). Só existe com lucro positivo.",
  },
  roe: { label: "ROE", hint: "Lucro líquido dos controladores sobre o patrimônio dos controladores, nos últimos 12 meses." },
  net_margin: { label: "Margem líquida", hint: "Lucro líquido sobre a receita líquida, nos últimos 12 meses." },
  ebitda_margin: {
    label: "Margem EBITDA",
    hint: "EBITDA aproximado (EBIT + depreciação e amortização) sobre a receita líquida, nos últimos 12 meses.",
  },
  net_debt_to_ebitda: {
    label: "Dív. líquida / EBITDA",
    hint: "Quantos anos de EBITDA a dívida líquida representa. Negativo quer dizer mais caixa que dívida.",
  },
  net_debt_to_equity: { label: "Dív. líquida / patrimônio", hint: "Dívida líquida sobre o patrimônio líquido." },
  revenue_cagr: {
    label: "Receita, CAGR 5 anos",
    hint: "Crescimento anual composto da receita entre as pontas da janela de cinco exercícios.",
  },
  earnings_cagr: {
    label: "Lucro, CAGR 5 anos",
    hint: "Crescimento anual composto do lucro entre as pontas da janela de cinco exercícios. Sem sentido com prejuízo numa das pontas.",
  },
  earnings_per_share: { label: "LPA", hint: "Lucro por ação dos últimos 12 meses." },
  book_value_per_share: { label: "VPA", hint: "Patrimônio dos controladores por ação." },
  market_cap: { label: "Valor de mercado", hint: "Total de ações da B3 vezes o preço de hoje." },
  nav_per_share: { label: "Valor patrimonial por cota", hint: "Patrimônio líquido do fundo dividido pelas cotas emitidas, pelo último informe mensal." },
  vacancy: {
    label: "Vacância física",
    hint: "Área vaga sobre a área dos imóveis para renda, pelo informe trimestral mais novo. Fundo de papel não declara imóvel.",
  },
  net_assets: { label: "Patrimônio líquido do fundo", hint: "Pelo último informe mensal da CVM." },
  cash_share: {
    label: "Caixa",
    hint: "Disponibilidades, títulos públicos e privados e fundos de renda fixa sobre o ativo total, pelo último informe mensal da CVM.",
  },
  liquidity_30d: {
    label: "Liquidez média, 30 pregões",
    hint: "Média do volume financeiro negociado por pregão (cotas × fechamento) nos últimos 30 pregões.",
  },
  liquidity_90d: {
    label: "Liquidez média, 90 pregões",
    hint: "Média do volume financeiro negociado por pregão nos últimos 90 pregões.",
  },
  dividend_cagr_3y: {
    label: "Proventos por cota, CAGR 3 anos",
    hint: "Crescimento anual composto da soma dos proventos por cota de 12 meses, de três anos atrás até hoje (desdobramentos ajustados).",
  },
  dividend_cagr_5y: {
    label: "Proventos por cota, CAGR 5 anos",
    hint: "O mesmo crescimento em cinco anos. Só com cinco anos inteiros de proventos conhecidos.",
  },
  price_cagr_3y: {
    label: "Cota, CAGR 3 anos",
    hint: "Crescimento anual composto da cota em três anos, sem os proventos, com desdobramentos e bonificações ajustados.",
  },
  price_cagr_5y: { label: "Cota, CAGR 5 anos", hint: "O mesmo crescimento da cota em cinco anos." },
};

/** Para onde o destaque olha, dito na dica de cada linha. */
export const DIRECTION_TEXT: Record<CompareDirection, string> = {
  higher: "O destaque marca o maior valor da linha.",
  lower: "O destaque marca o menor valor da linha.",
  none: "Esta linha não tem destaque: o número não tem lado.",
};

/** Por que não há número, em palavras da pessoa. */
export const REASON_TEXT: Record<CompareReason, string> = {
  ...GAP_TEXT,
  ...UNSUPPORTED_TEXT,
  not_applicable: "Não se aplica a este tipo de ativo.",
  no_price: "Sem cotação.",
  no_price_history: "A fonte não tem histórico de preço deste ativo.",
  short_history: "O ativo tem menos de 12 meses de histórico.",
  no_dividend_history: "A fonte não tem histórico de proventos deste ativo.",
  no_sector: "A fonte não classifica este papel.",
  no_sector_source: "Não há fonte pública de setor para esta classe.",
  not_tracked:
    "Sem fundamentos: o Prumo só baixa os balanços da CVM dos ativos da carteira, e este não está nela.",
};

export const CURVE_REASON_TEXT: Record<CurveReason, string> = {
  no_price_history: "A fonte não tem histórico de preço deste ativo.",
  no_series: "A carteira ainda não tem série de cotas.",
  no_index_data: "O índice ainda não tem dado neste período.",
};
