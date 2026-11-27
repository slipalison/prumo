import type {
  AnalysisRuleId,
  AttentionPoint,
  Benchmark,
  DimensionId,
  RiskProfile,
  RuleConfig,
  RuleDetail,
  RuleResult,
  RuleStatus,
  Score,
} from "../api/analysisTypes";
import { CLASS_META } from "./assetClasses";
import { formatPercent, plural } from "./format";
import { formatRuleValue, type RuleText, type WarningAction } from "./healthText";
import type { Money } from "./money";

/**
 * O texto da Análise: o nome de cada regra, onde o limite entra na frase, o
 * porquê, o que fazer. O servidor só manda fatos (qual regra, o número, o
 * limite, o aporte); o texto, com acento, nasce aqui. Nada recomenda ativo de
 * fora da carteira e nada manda vender.
 */
export const RULE_TEXT: Record<AnalysisRuleId, RuleText> = {
  // Diversificação
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
  min_portfolio_assets: {
    lead: "Pelo menos",
    tail: "ativos na carteira",
    why: "Conta os ativos com valor. Poucos ativos deixam a carteira dependente de poucos nomes.",
  },
  min_classes: {
    lead: "Dinheiro em pelo menos",
    tail: "classes de ativos",
    why: "Conta as classes (ações, FIIs, exterior, cripto, renda fixa) que têm dinheiro de verdade dentro.",
  },
  min_sectors: {
    lead: "Ações em pelo menos",
    tail: "setores",
    why: "Setores da B3 distintos entre as suas Ações Nacionais. Ação que a fonte ainda não classificou não conta.",
  },
  min_abroad: {
    lead: "Pelo menos",
    tail: "da carteira no exterior",
    why: "Ações, REITs e renda fixa de fora, inclusive BDR e ETF de exterior (que ficam em Ações Internacionais).",
  },
  min_low_risk: {
    lead: "Pelo menos",
    tail: "da carteira em renda fixa",
    why: "A fatia de baixo risco: Renda Fixa e Renda Fixa Internacional.",
  },
  // Dividendos
  min_yield: {
    lead: "Dividend yield de 12 meses de pelo menos",
    tail: "",
    why: "O que você recebeu em 12 meses (líquido) sobre o valor de hoje das ações e dos FIIs, que são quem paga provento. Precisa de 12 meses de proventos na tela Renda.",
  },
  min_paying_share: {
    lead: "Pelo menos",
    tail: "dos ativos de renda variável pagaram provento em 12 meses",
    why: "Conta ações, FIIs e ativos de fora que pagaram algo nos últimos 12 meses. Ativo comprado há pouco ainda pode não ter pago. Precisa de 12 meses de proventos.",
  },
  income_growth: {
    lead: "Renda de 12 meses maior que a dos 12 meses anteriores",
    tail: "",
    why: "Compara as duas janelas da tela Renda. Precisa de 24 meses de proventos registrados.",
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
  brick_reit: {
    lead: "Se há FII na carteira, pelo menos um é de tijolo",
    tail: "",
    why: "FII de tijolo tem renda ligada ao aluguel, que costuma acompanhar a inflação. O tipo do fundo vem do informe da CVM.",
  },
  // Rentabilidade
  beat_cdi_12m: {
    lead: "A carteira rende ao menos o CDI em 12 meses",
    tail: "",
    why: "Compara a rentabilidade por cotas (a mesma do Desempenho) com o CDI na mesma janela.",
  },
  beat_cdi_5y: {
    lead: "A carteira rende ao menos o CDI em 5 anos",
    tail: "",
    why: "Precisa de 5 anos de série. Sem eles a regra fica sem dado, não reprovada.",
  },
  beat_ibov_12m: {
    lead: "A carteira rende ao menos o Ibovespa em 12 meses",
    tail: "",
    why: "Compara a rentabilidade por cotas com a variação do Ibovespa na mesma janela.",
  },
  beat_ibov_5y: {
    lead: "A carteira rende ao menos o Ibovespa em 5 anos",
    tail: "",
    why: "Precisa de 5 anos de série. Sem eles a regra fica sem dado, não reprovada.",
  },
  beat_ipca_12m: {
    lead: "A carteira rende ao menos o IPCA em 12 meses",
    tail: "",
    why: "O IPCA sai com atraso: a janela termina no último mês publicado.",
  },
  beat_ipca_5y: {
    lead: "A carteira rende ao menos o IPCA em 5 anos",
    tail: "",
    why: "Precisa de 5 anos de série. A janela termina no último mês de IPCA publicado.",
  },
  min_positive_years: {
    lead: "Pelo menos",
    tail: "dos anos civis fechados com retorno positivo",
    why: "Conta só anos inteiros, de janeiro a dezembro, já encerrados. Precisa de 2 deles.",
  },
  max_volatility: {
    lead: "Volatilidade anualizada de até",
    tail: "",
    why: "Desvio-padrão dos retornos mensais (até 36 meses), vezes a raiz de 12. Precisa de 12 meses de série.",
  },
  max_loss: {
    lead: "Nenhum ativo com perda maior que",
    tail: "desde a compra",
    why: "Valor de hoje contra o custo do livro (preço médio × quantidade). Ativo sem custo conhecido não entra.",
  },
  // Qualidade dos ativos: cada uma pesa pelo valor de quem tem o dado (balanços da CVM).
  quality_roe: {
    lead: "Pelo menos",
    tail: "do valor em ações vem de empresas com ROE de 10% ou mais",
    why: "O ROE é o lucro dos controladores sobre o patrimônio deles nos últimos 12 meses. A regra pesa pelo valor de cada empresa; quem não tem o dado fica fora da conta.",
  },
  quality_profit: {
    lead: "Pelo menos",
    tail: "do valor em ações vem de empresas com lucro em pelo menos 4 dos últimos 5 anos",
    why: "Conta os exercícios com lucro positivo nos balanços anuais. Precisa de 4 exercícios conhecidos.",
  },
  quality_leverage: {
    lead: "Pelo menos",
    tail: "do valor em ações vem de empresas com dívida líquida de até 3 vezes o EBITDA",
    why: "Não vale para banco nem seguradora (o plano de contas deles não tem dívida nem EBITDA) nem para holding (o EBITDA dela leva o lucro das participadas): eles ficam fora. O EBITDA é uma aproximação (resultado operacional mais depreciação).",
  },
  quality_margin: {
    lead: "Pelo menos",
    tail: "do valor em ações vem de empresas com margem EBITDA positiva",
    why: "Não vale para banco, seguradora nem holding (o lucro dela vem das participadas, não da receita). Pesa pelo valor de cada empresa.",
  },
  quality_cash: {
    lead: "Pelo menos",
    tail: "do valor em ações vem de empresas com caixa operacional positivo na média de 4 anos",
    why: "Média do caixa das atividades operacionais nos 4 últimos exercícios; precisa dos 4 conhecidos.",
  },
  quality_recovery: {
    lead: "Nenhuma empresa da carteira está em recuperação judicial",
    tail: "",
    why: "Vem do cadastro das companhias na CVM. Não tem limite: uma empresa nessa situação já é um ponto a ver.",
  },
  quality_fii_pvp: {
    lead: "Pelo menos",
    tail: "do valor em FIIs vem de fundos com P/VP de até 1,10",
    why: "O P/VP é a cotação do Prumo sobre o valor patrimonial por cota do informe mensal da CVM.",
  },
  quality_fii_vacancy: {
    lead: "Pelo menos",
    tail: "do valor em FIIs vem de fundos com vacância de até 15%",
    why: "Vacância física do informe trimestral. Só o FII de tijolo declara imóvel: o de papel fica sem dado.",
  },
};

/** Maior valor de cada limite em contagem; o padrão é 50 ativos. */
export const COUNT_MAX: Partial<Record<AnalysisRuleId, number>> = {
  min_assets: 50,
  min_portfolio_assets: 100,
  min_classes: 7,
  min_sectors: 30,
  income_every_month: 12,
};

export const STATUS_TEXT: Record<RuleStatus, string> = {
  off: "Desligada",
  no_data: "Sem dado",
  ok: "Ok",
  warning: "Ponto a ver",
};

/** Por que uma regra ligada ainda não tem o que medir. */
export const NO_DATA_TEXT: Record<AnalysisRuleId, string> = {
  max_asset: "Ainda não há ativos com valor.",
  top_five: "Só vale com mais de 5 ativos.",
  max_sector: "Precisa de 2 ou mais ativos com setor em Ações Nacionais ou em FIIs.",
  min_assets: "Precisa de uma classe com meta e algum ativo.",
  empty_class: "Defina as metas por classe para a regra ter o que olhar.",
  min_portfolio_assets: "Ainda não há ativos com valor.",
  min_classes: "Ainda não há ativos com valor.",
  min_sectors: "Precisa de ações com setor conhecido na fonte pública.",
  min_abroad: "Ainda não há ativos com valor.",
  min_low_risk: "Ainda não há ativos com valor.",
  min_yield: "Precisa de 12 meses de proventos e de ações ou FIIs com valor.",
  min_paying_share: "Precisa de 12 meses de proventos e de ações ou FIIs com valor.",
  income_growth: "Precisa de 24 meses de proventos registrados.",
  income_every_month: "Precisa de 3 meses de proventos registrados.",
  max_payer: "Precisa de 3 meses de proventos e de 2 pagadores.",
  brick_reit: "Precisa de FII com o tipo do fundo conhecido.",
  beat_cdi_12m: "A série da carteira ou do índice ainda não cobre 12 meses.",
  beat_cdi_5y: "A série da carteira ou do índice ainda não cobre 5 anos.",
  beat_ibov_12m: "A série da carteira ou do índice ainda não cobre 12 meses.",
  beat_ibov_5y: "A série da carteira ou do índice ainda não cobre 5 anos.",
  beat_ipca_12m: "A série da carteira ou do índice ainda não cobre 12 meses.",
  beat_ipca_5y: "A série da carteira ou do índice ainda não cobre 5 anos.",
  min_positive_years: "Precisa de 2 anos civis fechados na série.",
  max_volatility: "Precisa de 12 meses de série.",
  max_loss: "Nenhum ativo tem custo conhecido no livro.",
  quality_roe: "Precisa de empresas com fundamentos da CVM (a busca roda em segundo plano).",
  quality_profit: "Precisa de empresa com pelo menos 4 balanços anuais conhecidos.",
  quality_leverage: "Precisa de empresa que não seja banco, seguradora nem holding, com EBITDA positivo.",
  quality_margin: "Precisa de empresa que não seja banco, seguradora nem holding.",
  quality_cash: "Precisa de empresa com 4 exercícios de caixa operacional conhecidos.",
  quality_recovery: "Precisa de empresa com cadastro na CVM.",
  quality_fii_pvp: "Precisa de FII com informe mensal da CVM e cotação.",
  quality_fii_vacancy: "Precisa de FII de tijolo com informe trimestral da CVM.",
};

export const DIMENSION_TEXT: Record<DimensionId, { title: string; lead: string }> = {
  diversification: {
    title: "Diversificação",
    lead: "Quanto a carteira depende de poucos ativos, setores ou classes.",
  },
  dividends: {
    title: "Dividendos",
    lead: "Quanto a carteira paga de renda e se ela cai todo mês.",
  },
  returns: {
    title: "Rentabilidade",
    lead: "O que a carteira rendeu contra os índices e o quanto oscilou.",
  },
  quality: {
    title: "Qualidade dos ativos",
    lead: "Indicadores das empresas e dos fundos que você tem.",
  },
};

/** O `id` do painel de uma dimensão na Análise, para o cartão do topo levar até ele. */
export const dimensionAnchor = (id: DimensionId) => `analysis-${id}`;

export const BENCHMARK_LABEL: Record<Benchmark, string> = { cdi: "CDI", ibov: "Ibovespa", ipca: "IPCA" };

export const PROFILE_LABEL: Record<RiskProfile, string> = {
  conservative: "Conservador",
  moderate: "Moderado",
  aggressive: "Arrojado",
};

/** O nome da regra sem o número ("Nenhum ativo acima de … da carteira"), para leitor de tela. */
export function ruleName(rule: AnalysisRuleId): string {
  const { lead, tail } = RULE_TEXT[rule];
  return tail ? `${lead} … ${tail}` : lead;
}

/** A frase da regra com o limite da pessoa: "Nenhum ativo acima de 15% da carteira". */
export function ruleSentence(rule: RuleConfig): string {
  const { lead, tail } = RULE_TEXT[rule.rule];
  const limit = rule.threshold === null ? "" : ` ${formatRuleValue(rule.unit, rule.threshold)}`;
  return `${lead}${limit}${tail ? ` ${tail}` : ""}`;
}

/** O rótulo do segundo número ao lado do "Agora": o limite, o mínimo ou o índice. */
export function limitLabel(rule: RuleConfig): string | null {
  const benchmark = /^beat_(cdi|ibov|ipca)_/.exec(rule.rule)?.[1] as Benchmark | undefined;
  if (benchmark) return BENCHMARK_LABEL[benchmark];
  if (rule.direction === "at_least") return "Mínimo";
  if (rule.direction === "at_most") return "Limite";
  return null;
}

/**
 * O limite na célula da tabela, curto e com o sentido escrito: `máx. 15%`,
 * `mín. 10%` ou o índice (`CDI 13,20%`). `null` quando a regra não tem limite.
 */
export function limitCell(result: RuleResult): string | null {
  const label = limitLabel(result);
  if (label === null || result.limit === null) return null;
  const value = formatRuleValue(result.unit, result.limit);
  if (label === "Limite") return `máx. ${value}`;
  if (label === "Mínimo") return `mín. ${value}`;
  return `${label} ${value}`;
}

/** O que está recolhido no fim de cada dimensão: "3 regras ok · 1 sem dado · 1 desligada". */
export function restText(rules: readonly RuleResult[]): string {
  const count = (status: RuleStatus) => rules.filter((r) => r.status === status).length;
  const ok = count("ok");
  const noData = count("no_data");
  const off = count("off");
  return [
    ok > 0 ? `${ok} ${plural(ok, ["regra ok", "regras ok"])}` : null,
    noData > 0 ? `${noData} sem dado` : null,
    off > 0 ? `${off} ${plural(off, ["desligada", "desligadas"])}` : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** `5 de 8 regras ok (62,5%)`: o número exato por trás da pontuação. */
export function scoreText(score: Score): string {
  const rules = plural(score.measured, ["regra", "regras"]);
  return `${score.ok} de ${score.measured} ${rules} ok`;
}

/** O percentual da pontuação, `62,5%` ou `100%`. */
export function scorePercent(score: Score): string | null {
  if (score.percent === null) return null;
  return formatPercent(score.percent, Number.isInteger(score.percent) ? 0 : 1);
}

/** Quantas regras ligadas ficaram sem dado, em uma frase; `null` se nenhuma. */
export function noDataText(score: Score): string | null {
  if (score.no_data === 0) return null;
  return `${score.no_data} ${plural(score.no_data, ["regra sem dado", "regras sem dado"])}, fora da conta`;
}

const pct = (value: number) => formatRuleValue("percent", value);

type NotApplicable = Extract<RuleDetail, { kind: "quality" }>["not_applicable"];

/** O rótulo curto de por que a medida não se aplica ("ITSA4 (holding)"). */
const NOT_APPLICABLE_SHORT: Record<NotApplicable[number]["reason"], string> = {
  bank: "banco",
  insurer: "seguradora",
  holding: "holding",
};

/** "Entraram na conta 2 de 4 ativos. Não se aplica a A (holding) e B (banco). Abaixo do critério: C." */
function qualityCoverage(covered: number, total: number, failing: string[], label: string, outside: NotApplicable = []): string {
  const base = `Entraram na conta ${covered} de ${total} ${plural(total, ["ativo", "ativos"])}.`;
  const names = outside.map((o) => `${o.label} (${NOT_APPLICABLE_SHORT[o.reason]})`);
  const notApplicable =
    names.length > 0 ? ` Não se aplica a ${names.length > 1 ? `${names.slice(0, -1).join(", ")} e ${names.at(-1)}` : names[0]}.` : "";
  const below = failing.length > 0 ? ` ${label}: ${failing.join(", ")}.` : "";
  return `${base}${notApplicable}${below}`;
}

/** `+1,23%` / `−0,40%`: o sinal diz o sentido sem depender da cor. */
export function formatSignedPercent(value: number): string {
  const text = formatPercent(Math.abs(value), 2);
  if (Math.abs(value) < 0.005) return formatPercent(0, 2);
  return value > 0 ? `+${text}` : `−${text}`;
}

/** `25/11/2026` a partir de `2026-11-25`. */
function day(iso: string): string {
  const [year, month, date] = iso.split("-");
  return year && month && date ? `${date}/${month}/${year}` : iso;
}

/**
 * A frase com os números por trás do estado da regra. As regras originais da
 * Saúde (`findings`) têm cartão próprio e voltam `null` aqui. Os valores em
 * reais passam por `money`, que os mascara com o olho fechado.
 */
export function ruleExplanation(result: RuleResult, money: Money): string | null {
  const { detail } = result;
  const current = result.current;
  switch (result.rule) {
    case "min_portfolio_assets":
      return current === null ? null : `Você tem ${current} ${plural(current, ["ativo", "ativos"])} com valor.`;
    case "min_classes":
      return detail?.kind === "classes" && current !== null
        ? `Dinheiro em ${current} ${plural(current, ["classe", "classes"])}: ${detail.asset_classes.map((c) => CLASS_META[c].label).join(", ")}.`
        : null;
    case "min_sectors":
      return detail?.kind === "sectors" && current !== null
        ? `Suas ações estão em ${current} ${plural(current, ["setor", "setores"])}: ${detail.sectors.join(", ")}.`
        : null;
    case "min_abroad":
      return current === null ? null : `${pct(current)} da carteira está no exterior.`;
    case "min_low_risk":
      return current === null ? null : `${pct(current)} da carteira está em renda fixa.`;
    case "min_yield":
      return detail?.kind === "yield"
        ? `Você recebeu ${money.compact(detail.income_12m)} em 12 meses sobre ${money.compact(detail.base_value)} em ações e FIIs.`
        : null;
    case "min_paying_share":
      return detail?.kind === "paying"
        ? `${detail.paying} de ${detail.total} ${plural(detail.total, ["ativo pagou", "ativos pagaram"])} provento em 12 meses${
            detail.silent.length > 0 ? `. Sem provento: ${detail.silent.join(", ")}` : ""
          }.`
        : null;
    case "income_growth":
      return detail?.kind === "income_growth"
        ? `Renda de 12 meses: ${money.compact(detail.current_12m)}; dos 12 meses anteriores: ${money.compact(detail.previous_12m)}.`
        : null;
    case "brick_reit":
      return detail?.kind === "brick"
        ? `${detail.brick} de ${detail.reits} ${plural(detail.reits, ["FII é", "FIIs são"])} de tijolo.`
        : null;
    case "beat_cdi_12m":
    case "beat_cdi_5y":
    case "beat_ibov_12m":
    case "beat_ibov_5y":
    case "beat_ipca_12m":
    case "beat_ipca_5y":
      return detail?.kind === "benchmark"
        ? `De ${day(detail.from)} a ${day(detail.to)} a carteira rendeu ${formatSignedPercent(detail.portfolio_percent)}; o ${BENCHMARK_LABEL[detail.benchmark]}, ${formatSignedPercent(detail.benchmark_percent)}.`
        : null;
    case "min_positive_years":
      return detail?.kind === "years"
        ? `${detail.positive} de ${detail.total} ${plural(detail.total, ["ano civil fechado", "anos civis fechados"])} com retorno positivo.`
        : null;
    case "max_volatility":
      return current === null ? null : `Os retornos mensais oscilaram o equivalente a ${pct(current)} ao ano.`;
    case "quality_roe":
    case "quality_profit":
    case "quality_leverage":
    case "quality_margin":
    case "quality_cash":
    case "quality_fii_pvp":
    case "quality_fii_vacancy":
      return detail?.kind === "quality"
        ? qualityCoverage(detail.covered, detail.total, detail.failing, "Abaixo do critério", detail.not_applicable)
        : null;
    case "quality_recovery":
      if (detail?.kind !== "quality") return null;
      return detail.failing.length > 0
        ? `Em recuperação judicial: ${detail.failing.join(", ")}.`
        : `Nenhuma empresa está em recuperação judicial (${detail.covered} com cadastro na CVM).`;
    case "max_loss":
      if (detail?.kind === "losses") {
        return detail.assets.map((a) => `${a.label} está ${pct(a.loss_percent)} abaixo do que você pagou`).join("; ") + ".";
      }
      return current === null ? null : `A maior perda desde a compra é de ${pct(current)}.`;
    default:
      return null;
  }
}

/**
 * O que fazer num ponto a ver, sem mandar vender e sem citar ativo de fora da
 * carteira. As regras originais (`findings`) trazem o próprio "o que fazer" nos
 * cartões e voltam `null`.
 */
export function ruleAction(result: RuleResult, money: Money): WarningAction | null {
  const { current, limit } = result;
  const missing = current !== null && limit !== null ? Math.max(Math.ceil(limit - current), 1) : 1;
  const needed = result.needed_contribution;
  const toReach = needed === null ? "" : ` cerca de ${money.compact(needed)}`;
  const toPlan = { href: "#/aportar", label: "Calcular o próximo aporte" };
  const toIncome = { href: "#/renda", label: "Abrir a Renda" };
  const toPerformance = { href: "#/desempenho", label: "Abrir o Desempenho" };
  switch (result.rule) {
    case "min_portfolio_assets":
      return {
        text: `Adicione mais ${missing} ${plural(missing, ["ativo", "ativos"])}: o próximo aporte pode abrir posição em ativos novos das classes que você já tem.`,
        link: toPlan,
      };
    case "min_classes":
      return {
        text: `Abra posição em mais ${missing} ${plural(missing, ["classe", "classes"])}: use o próximo aporte numa classe que ainda está vazia.`,
        link: { href: "#/ajustes/metas", label: "Abrir as metas" },
      };
    case "min_sectors":
      return {
        text: `Para chegar a ${limit ?? 0} setores, leve o próximo aporte a ações de setores que você ainda não tem.`,
        link: toPlan,
      };
    case "min_abroad":
      return {
        text: `Para chegar ao seu mínimo no exterior sem vender nada, aporte${toReach} em ativos de fora.`,
        link: toPlan,
      };
    case "min_low_risk":
      return {
        text: `Para chegar ao seu mínimo em renda fixa sem vender nada, aporte${toReach} em renda fixa.`,
        link: toPlan,
      };
    case "min_yield":
      return {
        text: "Para subir o yield, direcione os próximos aportes a ativos que pagam provento. Quem já paga aparece na tela Renda.",
        link: toIncome,
      };
    case "min_paying_share":
      return {
        text: "Confira na tela Renda se os proventos desses ativos foram lançados; um ativo comprado há pouco ainda pode não ter pago.",
        link: toIncome,
      };
    case "income_growth":
      return { text: "A renda caiu ou ficou parada. Veja na tela Renda de onde ela veio e o que mudou.", link: toIncome };
    case "brick_reit":
      return {
        text: "Nenhum dos seus FIIs é de tijolo. O tijolo tem renda ligada ao aluguel, que costuma acompanhar a inflação; o próximo aporte em FII pode ir para um.",
        link: toPlan,
      };
    case "beat_cdi_12m":
    case "beat_cdi_5y":
    case "beat_ibov_12m":
    case "beat_ibov_5y":
    case "beat_ipca_12m":
    case "beat_ipca_5y":
      return {
        text: "Isto compara o que a carteira rendeu na janela; não manda trocar nada. A curva contra o índice está no Desempenho.",
        link: toPerformance,
      };
    case "min_positive_years":
      return { text: "Veja ano a ano, mês a mês, onde a carteira perdeu no Desempenho.", link: toPerformance };
    case "max_volatility":
      return { text: "A carteira oscilou mais que o seu limite. Veja o histórico no Desempenho.", link: toPerformance };
    case "max_loss":
      return {
        text: "Reveja se o motivo da compra ainda vale. O Prumo não manda vender: só mostra onde a perda passou do seu limite.",
        link: { href: "#/carteira", label: "Abrir a Carteira" },
      };
    case "quality_roe":
    case "quality_profit":
    case "quality_leverage":
    case "quality_margin":
    case "quality_cash":
    case "quality_fii_pvp":
    case "quality_fii_vacancy":
      return {
        text: "Isto mostra o que os balanços e informes da CVM dizem; não manda vender. Os números de cada ativo estão na folha dele, em Fundamentos.",
        link: { href: "#/carteira", label: "Abrir a Carteira" },
      };
    case "quality_recovery":
      return {
        text: "Veja na folha do ativo, em Fundamentos, a situação do emissor. O Prumo não manda vender.",
        link: { href: "#/carteira", label: "Abrir a Carteira" },
      };
    default:
      return null;
  }
}

/** Vezes, sem zeros à direita: `2,9 vezes`, `3 vezes`. */
function times(value: number): string {
  return `${value.toFixed(2).replace(".", ",").replace(/,?0+$/, "")} vezes`;
}

/** Um ponto de atenção em uma frase curta e factual; nunca recomenda. */
export function pointText(point: AttentionPoint): string {
  switch (point.kind) {
    case "income_drop":
      return `Provento de 12 meses caiu ${pct(point.percent)} em relação aos 12 meses anteriores`;
    case "low_revenue_growth":
      return `Receita ${point.percent >= 0 ? "cresceu" : "encolheu"} ${pct(Math.abs(point.percent))} ao ano em 5 anos, abaixo de 5%`;
    case "low_earnings_growth":
      return `Lucro ${point.percent >= 0 ? "cresceu" : "encolheu"} ${pct(Math.abs(point.percent))} ao ano em 5 anos, abaixo de 5%`;
    case "low_roe":
      return `ROE de ${pct(point.percent)}, abaixo de 10%`;
    case "low_net_margin":
      return `Margem líquida de ${pct(point.percent)}, abaixo de 10%`;
    case "debt_above_equity":
      return `Dívida líquida de ${times(point.times)} o patrimônio líquido`;
    case "judicial_recovery":
      return "Em recuperação judicial";
  }
}

/** Para a frase do resumo: "3 pontos a ver". */
export function attentionText(count: number): string {
  return `${count} ${plural(count, ["ponto", "pontos"])} a ver`;
}
