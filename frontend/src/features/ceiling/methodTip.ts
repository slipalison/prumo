import type { CeilingMethod, FundReference, MarketRates, StockReference } from "../../api/ceilingTypes";
import { FUND_TYPE_LABEL, fundYieldMath, GAP_TEXT, methodValue } from "../../lib/ceilingText";
import { formatPercent, plural } from "../../lib/format";
import type { Money } from "../../lib/money";

const isStock = (item: StockReference | FundReference): item is StockReference => "bazin" in item;

/**
 * A explicação de um método com a conta de verdade do ativo, para o ⓘ: a média
 * e o rendimento-alvo do Bazin, a fórmula do Graham, a fonte do VP e a conta
 * inteira do Rendimento ("alvo 7,12% + 2,5 p.p. = 9,62%; proventos 12m R$ 13,20
 * ÷ 9,62% = R$ 137,21"). Sem valor, o motivo.
 */
export function methodTip(
  item: StockReference | FundReference,
  method: CeilingMethod,
  market: MarketRates,
  bazinYield: number | null,
  money: Money,
): string {
  const found = methodValue(item, method);
  if (found.value === null) {
    const why = GAP_TEXT[found.gap ?? "no_reference"];
    if (method === "fund_yield" && found.gap === "no_market_rate") {
      return `Sem valor hoje: ${why}. O alvo do tijolo usa o juro real do Tesouro IPCA+ e o do papel, o CDI de 12 meses.`;
    }
    return `Sem valor hoje: ${why}.`;
  }
  const yieldText =
    bazinYield === null ? "seu rendimento-alvo" : `${formatPercent(bazinYield, Number.isInteger(bazinYield) ? 0 : 2)} ao ano`;
  if (isStock(item)) {
    switch (method) {
      case "bazin":
        return item.bazin.years !== null && item.bazin.annual_average !== null
          ? `Média de ${item.bazin.years} ${plural(item.bazin.years, ["ano", "anos"])} de dividendos: ${money.brl(item.bazin.annual_average)} por cota ao ano, dividida por ${yieldText}.`
          : `A média de dividendos por cota dividida por ${yieldText}.`;
      case "graham":
        return "A raiz de 22,5 × lucro por ação × patrimônio por ação, dos balanços da CVM e das ações da B3.";
      default:
        return "O menor valor entre Bazin e Graham: abaixo dele o preço passa nos dois. Com uma referência só, é ela.";
    }
  }
  switch (method) {
    case "nav":
      return "Patrimônio líquido ÷ cotas, do informe mensal da CVM: o preço de P/VP 1.";
    case "fund_yield": {
      const math = fundYieldMath(item, market, money);
      const kind = item.fund_type ? `${FUND_TYPE_LABEL[item.fund_type]}: ` : "";
      return math ? `${kind}${math}.` : "Os proventos de 12 meses divididos pelo rendimento-alvo do tipo do fundo.";
    }
    default:
      return "O menor valor entre VP e Rendimento: abaixo dele o preço passa nos dois. Com uma referência só, é ela.";
  }
}
