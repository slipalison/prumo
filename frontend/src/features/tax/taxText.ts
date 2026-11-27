import type { Doubt, Instrument, Pool, SkipReason, TaxInstrument } from "../../api/taxTypes";

/**
 * O aviso de docs/ir.md, sempre à vista e em uma linha: o que ele é e de quem é
 * a responsabilidade. O resto vai no ⓘ ao lado.
 */
export const DISCLAIMER =
  "O Prumo estima o imposto a partir do que você lançou: não substitui o contador, o programa da Receita nem o informe da corretora. A responsabilidade pelo que vai para a Receita é de quem declara.";

export const DISCLAIMER_MORE =
  "Confira cada DARF e cada linha da declaração antes de pagar ou entregar. Erro de imposto gera multa e juros.";

export const INSTRUMENT_LABEL: Record<TaxInstrument, string> = {
  stock: "Ação ou unit",
  etf: "ETF de renda variável",
  bdr: "BDR",
  real_estate_fund: "Fundo imobiliário (FII)",
  agribusiness_fund: "Fiagro",
  crypto: "Criptoativo no Brasil",
  unsupported: "Fora do IR do Prumo",
};

/** O que cada escolha muda na conta (sem valor em reais: é regra, não dado). */
export const INSTRUMENT_HINT: Record<TaxInstrument, string> = {
  stock: "15% sobre o lucro; isento no mês em que as vendas de ações somam até 20 mil reais.",
  etf: "15% sobre o lucro, sem a isenção das ações. Vale para o ETF de bitcoin da B3.",
  bdr: "15% sobre o lucro, sem a isenção das ações; o dividendo é rendimento do exterior.",
  real_estate_fund: "20% sobre o lucro da venda; o rendimento mensal é isento.",
  agribusiness_fund: "20% sobre o lucro da venda; prejuízo só compensa com Fiagro.",
  crypto: "Ganho de capital de 15% a 22,5%; isento no mês com vendas até 35 mil reais.",
  unsupported: "Renda fixa, Tesouro, ETF de renda fixa e exterior fora de BDR: o Prumo não calcula.",
};

export const SHORT_INSTRUMENT: Record<Instrument, string> = {
  stock: "Ações",
  etf: "ETF",
  bdr: "BDR",
  real_estate_fund: "FII",
  agribusiness_fund: "Fiagro",
  crypto: "Cripto",
};

export const POOL_LABEL: Record<Pool, string> = {
  common: "Ações, ETF e BDR",
  day_trade: "Day trade",
  real_estate_fund: "FII",
  agribusiness_fund: "Fiagro",
};

export const SKIP_TEXT: Record<SkipReason, string> = {
  unsupported_instrument: "Fora do IR do Prumo (renda fixa, Tesouro, exterior ou ETF de renda fixa): nada deste ativo entra na conta.",
  unknown_opening_cost: "O saldo inicial está sem preço. Sem o custo não há preço médio: informe o preço do saldo inicial em Lançamentos.",
  redemption: "Resgate é regra de renda fixa, que o Prumo não calcula: o ativo ficou fora da conta.",
  amortization: "Amortização não abate do custo no IR do Prumo: o custo (e o lucro de vendas depois) sai maior que o real.",
  adjustment: "Ajuste manual de quantidade: entrou como no livro (o custo total fica). Se foi compra ou venda, lance com preço.",
  adjustment_closes_position: "Um ajuste manual zerou a posição. Se foi uma venda, lance a venda com preço: sem ela, o ativo ficou fora da conta.",
  event_without_position: "Um desdobramento, grupamento, bonificação ou ajuste não fecha com a posição do dia: o ativo ficou fora da conta.",
  conflicting_instruments: "O mesmo ticker está com dois instrumentos fiscais diferentes: escolha um só na folha do ativo.",
  income_amortization: "Amortização é devolução de capital, não rendimento: não entra na declaração como provento.",
  income_other: "Provento do tipo “outro”: o Prumo não sabe em que ficha ele entra.",
  income_from_abroad: "Dividendo de BDR é rendimento do exterior (Lei 14.754/2023), que o Prumo não calcula.",
  income_out_of_scope: "Provento de ETF, cripto ou ativo fora do IR do Prumo: fica fora da declaração.",
};

/** As dúvidas de docs/ir.md § 5, como "ponto a confirmar com o contador". */
export const DOUBT_TEXT: Record<Doubt, string> = {
  day_trade_in_stock_exemption:
    "As vendas em day trade entraram no total das vendas de ações. É a leitura conservadora; em outra leitura, o mês poderia ser isento.",
  agribusiness_separate_from_real_estate:
    "O prejuízo de Fiagro só compensa com lucro de Fiagro, separado do FII (a leitura conservadora de “fundo da mesma espécie”).",
  irpf_codes:
    "Os códigos de Bens e Direitos e de rendimentos vieram de informes de corretoras, não da Receita: confira no programa do IRPF.",
  crypto_per_sale:
    "Cripto foi apurada por venda: o prejuízo de uma não abateu o lucro de outra no mesmo mês.",
  year_end: "O Prumo trata 31/12 como dia sem expediente bancário: confira a Agenda Tributária de dezembro.",
  withholding_leftover: "Sobrou IR retido na fonte no fim do ano: confirme onde informar na declaração.",
  dividend_withholding:
    "Dividendo com retenção de 10% (desde 2026): confirme a ficha do dividendo e do imposto retido na declaração.",
  interest_on_equity_net: "O JCP foi levado pelo valor líquido: confirme no programa do IRPF.",
};

/** O nome curto de cada dúvida: dá nome ao ⓘ (o texto inteiro abre nele). */
export const DOUBT_SHORT: Record<Doubt, string> = {
  day_trade_in_stock_exemption: "vendas em day trade no isentômetro",
  agribusiness_separate_from_real_estate: "prejuízo de Fiagro separado do FII",
  irpf_codes: "códigos de Bens e Direitos e de rendimentos",
  crypto_per_sale: "cripto apurada por venda",
  year_end: "31/12 sem expediente bancário",
  withholding_leftover: "IR retido que sobrou no fim do ano",
  dividend_withholding: "dividendo com retenção de 10%",
  interest_on_equity_net: "JCP levado pelo valor líquido",
};

export const MONTH_NAMES = [
  "janeiro",
  "fevereiro",
  "março",
  "abril",
  "maio",
  "junho",
  "julho",
  "agosto",
  "setembro",
  "outubro",
  "novembro",
  "dezembro",
] as const;

export function monthName(month: number): string {
  return MONTH_NAMES[month - 1] ?? String(month);
}

/** `2025-06-30` → `30/06/2025`, sem fuso (a data já é o dia). */
export function isoDay(iso: string): string {
  const [year, month, day] = iso.split("-");
  return year && month && day ? `${day}/${month}/${year}` : iso;
}

/** `0.15` → `15%`; `0.175` → `17,5%`; `0.00005` → `0,005%`. */
export function rateText(rate: number): string {
  return `${(rate * 100).toLocaleString("pt-BR", { maximumFractionDigits: 3 })}%`;
}

/** `{ year: 2026, month: 6 }` → `junho de 2026`. */
export function periodName(year: number, month: number): string {
  return `${monthName(month)} de ${year}`;
}

/** Rótulo com acento de cada linha da declaração (o servidor manda sem acento). */
const IRPF_LABEL: Record<string, string> = {
  "03/01": "Ações (inclusive as listadas em bolsa)",
  "07/09": "Demais fundos de índice (ETF)",
  "04/04": "Ativos negociados em bolsa no Brasil (BDR, opções e outros)",
  "07/03": "Fundos de Investimento Imobiliário (FII)",
  "07/02": "Fundos de Investimento nas Cadeias Produtivas Agroindustriais (Fiagro)",
  "08/01": "Criptomoeda Bitcoin (BTC)",
  "08/02": "Outras criptomoedas (altcoins)",
  "08/03": "Stablecoins",
  "isentos/09": "Lucros e dividendos recebidos",
  "isentos/99": "Outros: rendimentos de FII e Fiagro",
  "isentos/20": "Ganhos líquidos em operações no mercado à vista de ações até 20 mil reais no mês",
  "isentos/05": "Ganho de capital em alienações de cripto até 35 mil reais no mês",
  "exclusiva/10": "Juros sobre capital próprio",
};

export function irpfLabel(group: string, code: string, fallback: string): string {
  return IRPF_LABEL[`${group}/${code}`] ?? fallback;
}
