import type { Pool, TaxRules } from "../../api/taxTypes";
import { rateText } from "./taxText";

/** Uma linha do que o Prumo calcula: a regra do ano, com a fonte. */
export type CoveredItem = { id: string; title: string; detail: string; source: string };

/** Uma linha do que fica fora, e por quê. */
export type UncoveredItem = { id: string; title: string; why: string };

const poolRate = (rules: TaxRules, pool: Pool) => rules.pools.find((p) => p.pool === pool)?.rate;

/**
 * Limite da regra por extenso ("20 mil reais", "1 real"): é regra, não valor
 * da pessoa, e escrito assim não se confunde com o que o olho fechado mascara.
 */
export function reaisText(value: number): string {
  if (value >= 1000 && value % 1000 === 0) return `${(value / 1000).toLocaleString("pt-BR")} mil reais`;
  if (value === 1) return "1 real";
  return `${value.toLocaleString("pt-BR", { maximumFractionDigits: 2 })} reais`;
}

/**
 * O que o Prumo calcula no ano, montado da tabela de regras do servidor
 * (docs/ir.md § 2): cada número vem dela, e a fonte vai junto. Limite em
 * reais é regra, não dado da pessoa: não passa pelo olho fechado.
 */
export function coveredItems(rules: TaxRules): CoveredItem[] {
  const common = poolRate(rules, "common");
  const dayTrade = poolRate(rules, "day_trade");
  const fii = poolRate(rules, "real_estate_fund");
  const stocks = rules.exemptions.find((e) => e.id === "stock_sales");
  const crypto = rules.exemptions.find((e) => e.id === "crypto_sales");
  const instrument = (id: string) => rules.instruments.find((i) => i.instrument === id);
  const brackets = rules.capital_gain_brackets.map((b) => b.rate);
  const w = rules.withholding;
  const late = rules.late_payment;
  const items: CoveredItem[] = [];
  if (common && stocks) {
    items.push({
      id: "stocks",
      title: "Ações e units",
      detail: `${rateText(common.value)} sobre o lucro; isento no mês em que as vendas de ações somam até ${reaisText(stocks.limit.value)}. O prejuízo de mês isento continua compensável.`,
      source: `${common.source}; ${stocks.limit.source}`,
    });
  }
  if (dayTrade) {
    items.push({
      id: "day_trade",
      title: "Day trade",
      detail: `Compra e venda do mesmo ativo no mesmo dia: ${rateText(dayTrade.value)}, sem a isenção das ações, com prejuízo em conta separada e ${rateText(w.day_trade_rate.value)} retido sobre o lucro do dia.`,
      source: `${dayTrade.source}; ${w.day_trade_rate.source}`,
    });
  }
  const etf = instrument("etf");
  const bdr = instrument("bdr");
  if (common && etf && bdr) {
    items.push({
      id: "etf_bdr",
      title: "ETF de renda variável e BDR",
      detail: `${rateText(common.value)} sobre o lucro, no grupo das ações, sem a isenção das ações: as vendas não somam no isentômetro.`,
      source: `${etf.source}; ${bdr.source}`,
    });
  }
  if (fii) {
    items.push({
      id: "funds",
      title: "FII e Fiagro",
      detail: `${rateText(fii.value)} sobre o lucro da venda, sem isenção, cada espécie no seu grupo de prejuízo; o rendimento mensal entra como isento.`,
      source: fii.source,
    });
  }
  if (crypto && brackets.length > 0) {
    items.push({
      id: "crypto",
      title: "Criptoativos no Brasil",
      detail: `Ganho de capital de ${rateText(Math.min(...brackets))} a ${rateText(Math.max(...brackets))}; isento no mês com vendas até ${reaisText(crypto.limit.value)}; prejuízo não compensa.`,
      source: `${rules.capital_gain_brackets_source}; ${crypto.limit.source}`,
    });
  }
  const dividends = rules.dividend_withholding;
  const dividendsDetail = dividends
    ? `; dividendo de uma mesma empresa acima de ${reaisText(dividends.monthly_threshold)} no mês com retenção de ${rateText(dividends.rate)}`
    : "";
  const dividendsSource = dividends ? `; ${dividends.source}` : "";
  items.push(
    {
      id: "losses",
      title: "Prejuízo a compensar",
      detail: "Por grupo (ações, ETF e BDR; day trade; FII; Fiagro), mês a mês e de um ano para o outro, sem prazo.",
      source: "P&R 709 a 711; IN RFB 1.585/2015, arts. 37 e 64",
    },
    {
      id: "withholding",
      title: "IR retido na fonte",
      detail: `${rateText(w.common_rate.value)} das vendas comuns (dispensado até ${reaisText(w.common_dispensed_up_to.value)} no mês) e ${rateText(w.day_trade_rate.value)} do day trade, abatidos do DARF; a sobra passa para os meses seguintes do mesmo ano. Sem o valor da nota, vale a regra.`,
      source: `${w.common_rate.source}; ${w.common_dispensed_up_to.source}; ${w.carry_within_year_source}`,
    },
    {
      id: "darf",
      title: "DARF",
      detail: `${rules.exchange_darf_code.value} (bolsa) e ${rules.capital_gain_darf_code.value} (cripto), no último dia útil do mês seguinte; abaixo de ${reaisText(rules.darf_minimum.value)}, soma no mês seguinte.`,
      source: `${rules.exchange_darf_code.source}; ${rules.darf_minimum.source}`,
    },
    {
      id: "late",
      title: "DARF em atraso",
      detail: `Multa de ${rateText(late.daily_penalty.value)} ao dia, até ${rateText(late.penalty_cap.value)}, e juros da Selic de cada mês (série ${late.selic_series.value} do Banco Central) mais ${rateText(late.payment_month_interest.value)} no mês do pagamento.`,
      source: `${late.daily_penalty.source}; ${late.payment_month_interest.source}; ${late.selic_series.source}`,
    },
    {
      id: "income",
      title: "Proventos na declaração",
      detail: `Dividendos e rendimentos de FII como isentos; JCP com ${rateText(rules.interest_on_equity_rate.value)} na fonte, em tributação exclusiva${dividendsDetail}. Por fonte pagadora, com o CNPJ do cadastro da CVM.`,
      source: `${rules.interest_on_equity_rate.source}${dividendsSource}`,
    },
  );
  return items;
}

/** O que o Prumo não calcula (docs/ir.md § 3), cada um com o porquê. */
export const NOT_COVERED: readonly UncoveredItem[] = [
  {
    id: "derivatives",
    title: "Opções, termo e futuro",
    why: "O livro só registra compra e venda à vista. Essas operações têm ganho próprio (prêmio, ajuste diário, vencimento) e o Prumo não tem o lançamento delas.",
  },
  {
    id: "exercise",
    title: "Exercício de opções",
    why: "A venda por exercício não tem a isenção de 20 mil reais, e o livro não sabe que a venda veio de um exercício.",
  },
  {
    id: "lending",
    title: "Aluguel de ações (BTC) e ouro ativo financeiro",
    why: "Têm regra própria e não há lançamento deles no Prumo.",
  },
  {
    id: "fixed_income",
    title: "Renda fixa, Tesouro, ETF de renda fixa e fundos fora de bolsa",
    why: "O imposto é retido na fonte pelo banco ou pela corretora, pela tabela do prazo: não há DARF a apurar.",
  },
  {
    id: "abroad",
    title: "Investimentos no exterior",
    why: "Ações, REITs, ETFs e cripto fora do Brasil seguem a apuração anual da Lei 14.754/2023. Só o BDR, negociado na B3, entra.",
  },
  {
    id: "broker",
    title: "Corretora de cada negócio",
    why: "O livro não guarda a corretora: o day trade é casado e o IR retido pela regra é contado como se fosse uma corretora só. Informe o retido da nota para valer o dela.",
  },
  {
    id: "corporate",
    title: "Incorporação, cisão, troca de ticker, subscrição e amortização",
    why: "Mudam o custo de um jeito que o livro não registra; o ativo envolvido aparece em “O que o Prumo não calculou”.",
  },
  {
    id: "fund_conditions",
    title: "Condições da isenção do FII e do Fiagro",
    why: "Pelo menos 100 cotistas e menos de 10% das cotas: o Prumo não tem esse dado e supõe que valem.",
  },
  {
    id: "minimum",
    title: "IRPF mínimo (Lei 15.270/2025)",
    why: "Depende da renda total da pessoa no ano, que o Prumo não tem.",
  },
  {
    id: "crypto_swap",
    title: "Permuta de cripto por cripto",
    why: "É alienação, mas o livro só tem compra e venda em reais.",
  },
];
