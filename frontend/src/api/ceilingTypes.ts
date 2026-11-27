// O teto de preço e as referências de preço, espelho de docs/api.md
// (§ Teto de preço e § Referências de preço).

import type { AssetClass } from "./types";

/** Um método de referência que pode ser o teto. Ações: Bazin, Graham, Pelas duas; FIIs: VP, Rendimento, Pelas duas. */
export type CeilingMethod = "bazin" | "graham" | "combined" | "nav" | "fund_yield";

/** Por que uma referência (ou o teto que segue um método) está sem valor hoje. */
export type ReferenceGap =
  | "no_dividends"
  | "no_fundamentals"
  | "loss"
  | "negative_equity"
  | "unknown_fund_type"
  | "no_composition"
  | "no_market_rate"
  | "non_positive_target"
  | "no_reference";

/**
 * O teto que a pessoa escolheu, com o valor de hoje. `method: "manual"` é o
 * valor próprio. Método sem valor hoje: `value` nulo e `unavailable_reason`
 * dizendo por quê — o ativo fica fora do aporte. `null` no ativo = sem teto.
 */
export type Ceiling = {
  method: CeilingMethod | "manual";
  value: number | null;
  unavailable_reason: ReferenceGap | null;
};

/** A escolha que o PATCH grava; `null` tira o teto. */
export type CeilingChoice = { method: CeilingMethod } | { method: "manual"; value: number };

/** Uma referência: o valor, ou o motivo de não haver. */
export type RefValue = { value: number | null; gap: ReferenceGap | null };

export type BazinRef = RefValue & {
  /** Dividendo anual médio por cota. */
  annual_average: number | null;
  /** Quantos anos de 12 meses entraram na média. */
  years: number | null;
};

/** O rendimento-alvo de um FII e de onde ele saiu. */
export type TargetYield = {
  percent: number;
  /** O alvo do tijolo (juro real + prêmio) e o do papel (CDI líquido + prêmio), quando entram. */
  brick: number | null;
  paper: number | null;
  brick_weight: number;
  paper_weight: number;
};

export type FundYieldRef = RefValue & {
  /** Proventos por cota dos últimos 12 meses. */
  dividends_12m: number | null;
  target: TargetYield | null;
};

export type FundType = "brick" | "paper" | "hybrid" | "fund_of_funds";

/** O que a carteira do FII tem, em frações que somam 1. */
export type FundComposition = { brick: number; paper: number; funds: number };

type ReferenceBase = {
  asset_id: string;
  label: string;
  asset_class: AssetClass;
  unit_price: number | null;
  ceiling: Ceiling | null;
};

export type StockReference = ReferenceBase & {
  bazin: BazinRef;
  graham: RefValue;
  combined: RefValue;
};

export type FundReference = ReferenceBase & {
  fund_type: FundType | null;
  composition: FundComposition | null;
  nav: RefValue;
  fund_yield: FundYieldRef;
  combined: RefValue;
};

export type RealRate = {
  /** % ao ano acima do IPCA (taxa de compra do Tesouro). */
  percent: number;
  base_date: string;
  /** O nome do título: "Tesouro IPCA+ com Juros Semestrais 2037". */
  bond: string;
  bond_symbol: string;
  source: "tesouro_transparente";
};

export type Cdi12m = {
  percent: number;
  /** Depois do IR de 15%. */
  net_percent: number;
  from: string;
  to: string;
  source: "bcb_sgs_12";
};

/** As taxas de mercado, os prêmios da pessoa e os alvos de hoje. */
export type MarketRates = {
  real_rate: RealRate | null;
  cdi_12m: Cdi12m | null;
  brick_premium: number;
  paper_premium: number;
  default_brick_premium: number;
  default_paper_premium: number;
  brick_target: number | null;
  paper_target: number | null;
};

/** As referências de preço (a Análise e `GET /assets/{id}/references`). */
export type References = {
  /** O rendimento-alvo do Bazin, em % ao ano. */
  bazin_yield: number;
  market: MarketRates;
  stocks: StockReference[];
  funds: FundReference[];
};

/** Um chip do plano para ativo sem teto. */
export type PriceReference = { method: CeilingMethod; value: number };
