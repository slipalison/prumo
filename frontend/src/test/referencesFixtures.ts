import type {
  Ceiling,
  CeilingChoice,
  FundReference,
  FundType,
  MarketRates,
  References,
  StockReference,
} from "../api/ceilingTypes";
import { methodValue } from "../lib/ceilingText";

/*
 * As referências de preço FICTÍCIAS dos testes e da demonstração: uma ação e
 * um FII de cada tipo (tijolo, papel, híbrido, fundo de fundos e um sem tipo),
 * com as contas batendo com as taxas de `defaultMarket`. Tickers públicos,
 * números inventados.
 */

/** Juro real de 7,12% e CDI de 14,20% (12,07% líquido): alvos 9,62% (tijolo) e 13,07% (papel). */
export function defaultMarket(): MarketRates {
  return {
    real_rate: {
      percent: 7.12,
      base_date: "2026-11-19",
      bond: "Tesouro IPCA+ com Juros Semestrais 2037",
      bond_symbol: "TD-IPCAJS-20370515",
      source: "tesouro_transparente",
    },
    cdi_12m: { percent: 14.2, net_percent: 12.07, from: "2025-11-19", to: "2026-11-18", source: "bcb_sgs_12" },
    brick_premium: 2.5,
    paper_premium: 1,
    default_brick_premium: 2.5,
    default_paper_premium: 1,
    brick_target: 9.62,
    paper_target: 13.07,
  };
}

const ok = (value: number) => ({ value, gap: null });

function fund(
  asset_id: string,
  label: string,
  unit_price: number,
  fund_type: FundType | null,
  nav: number | null,
  dividends: number,
  target: { percent: number; brick: number | null; paper: number | null; brick_weight: number; paper_weight: number } | null,
  composition: FundReference["composition"] = null,
): FundReference {
  const yieldValue = target ? Math.round((dividends / target.percent) * 10000) / 100 : null;
  const values = [nav, yieldValue].filter((v): v is number => v !== null);
  return {
    asset_id,
    label,
    asset_class: "br_reits",
    unit_price,
    ceiling: null,
    fund_type,
    composition,
    nav: nav === null ? { value: null, gap: "no_fundamentals" } : ok(nav),
    fund_yield:
      yieldValue === null
        ? { value: null, gap: fund_type === null ? "unknown_fund_type" : "no_market_rate", dividends_12m: null, target: null }
        : { value: yieldValue, gap: null, dividends_12m: dividends, target },
    combined: values.length > 0 ? ok(Math.min(...values)) : { value: null, gap: "no_reference" },
  };
}

const BRICK = { percent: 9.62, brick: 9.62, paper: null, brick_weight: 1, paper_weight: 0 };
const PAPER = { percent: 13.07, brick: null, paper: 13.07, brick_weight: 0, paper_weight: 1 };

export function defaultStocks(): StockReference[] {
  return [
    {
      asset_id: "a-itsa",
      label: "ITSA4",
      asset_class: "br_stocks",
      unit_price: 11.2,
      ceiling: { method: "manual", value: 10, unavailable_reason: null },
      bazin: { value: 12.5, gap: null, annual_average: 0.75, years: 4 },
      graham: ok(9.5),
      combined: ok(9.5),
    },
  ];
}

export function defaultFunds(): FundReference[] {
  return [
    // Tijolo: 7,12% + 2,5 p.p. = 9,62%; R$ 13,20 ÷ 9,62% = R$ 137,21.
    fund("a-knri", "KNRI11", 142, "brick", 98.4, 13.2, BRICK),
    fund("a-hglg", "HGLG11", 140.8, "brick", 161.3, 13.8, BRICK),
    // Papel: 14,20% × 0,85 = 12,07% + 1 p.p. = 13,07%; R$ 1,20 ÷ 13,07% = R$ 9,18.
    fund("a-mxrf", "MXRF11", 8.37, "paper", 9.71, 1.2, PAPER),
    // Híbrido: 50% imóveis, 30% recebíveis e 20% cotas => 60% tijolo e 40% papel = 11,00%.
    fund(
      "a-hibr",
      "HIBR11",
      88,
      "hybrid",
      95.4,
      10.2,
      { percent: 11, brick: 9.62, paper: 13.07, brick_weight: 0.6, paper_weight: 0.4 },
      { brick: 0.5, paper: 0.3, funds: 0.2 },
    ),
    // Fundo de fundos: a média de 9,62% e 13,07% = 11,35%.
    fund("a-foff", "FOFF11", 70.5, "fund_of_funds", 81.2, 8.4, {
      percent: 11.35,
      brick: 9.62,
      paper: 13.07,
      brick_weight: 0.5,
      paper_weight: 0.5,
    }),
    // Sem informe da CVM: sem tipo, sem Rendimento.
    fund("a-trxf", "TRXF11", 53.81, null, 110.2, 6.5, null),
  ];
}

export function defaultReferences(): References {
  return { bazin_yield: 6, market: defaultMarket(), stocks: defaultStocks(), funds: defaultFunds() };
}

/** As referências de um ativo só, como `GET /assets/{id}/references` devolve. */
export function referencesOf(all: References, id: string): References {
  return {
    ...all,
    stocks: all.stocks.filter((s) => s.asset_id === id),
    funds: all.funds.filter((f) => f.asset_id === id),
  };
}

/** O teto de hoje para uma escolha, pelas referências do ativo (o que o servidor calcula). */
export function effectiveCeiling(all: References, id: string, choice: CeilingChoice | null | undefined): Ceiling | null {
  if (!choice) return null;
  if (choice.method === "manual") return { method: "manual", value: choice.value, unavailable_reason: null };
  const item = all.stocks.find((s) => s.asset_id === id) ?? all.funds.find((f) => f.asset_id === id);
  const found = item ? methodValue(item, choice.method) : { value: null, gap: "no_reference" as const };
  return found.value === null
    ? { method: choice.method, value: null, unavailable_reason: found.gap ?? "no_reference" }
    : { method: choice.method, value: found.value, unavailable_reason: null };
}
