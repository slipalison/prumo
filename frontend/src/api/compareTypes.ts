import type { AssetClass } from "./types";

/**
 * `GET /api/v1/compare` e `GET /api/v1/compare/returns` (docs/api.md §
 * Comparador). Texto de tela (rótulos, dicas, motivos) é da tela: o servidor
 * manda ids e códigos estáveis.
 */

export type CompareFamily = "company" | "fund" | "other";
export type CompareDirection = "higher" | "lower" | "none";
export type CompareUnit = "multiple" | "percent" | "money" | "money_compact" | "text";
export type CompareGroup = "general" | "company" | "fund";
export type CompareMark = "max" | "min";

export type CompareRowId =
  | "price"
  | "return_12m"
  | "dividend_yield_12m"
  | "price_to_book"
  | "segment"
  | "price_to_earnings"
  | "roe"
  | "net_margin"
  | "ebitda_margin"
  | "net_debt_to_ebitda"
  | "net_debt_to_equity"
  | "revenue_cagr"
  | "earnings_cagr"
  | "earnings_per_share"
  | "book_value_per_share"
  | "market_cap"
  | "nav_per_share"
  | "vacancy"
  | "net_assets";

/** Por que uma célula não tem número: os motivos dos fundamentos e os do comparador. */
export type CompareReason =
  | "not_applicable"
  | "no_price"
  | "no_price_history"
  | "short_history"
  | "no_dividend_history"
  | "no_sector"
  | "no_sector_source"
  | "not_tracked"
  | "no_filing"
  | "no_shares"
  | "not_reported"
  | "financial_template"
  | "unknown_template"
  | "non_positive_base"
  | "non_positive_endpoint"
  | "insufficient_history"
  | "no_quote"
  | "no_rental_property"
  | "no_quarterly_report"
  /** Dos fundamentos: holding de participações (margem e dívida ÷ EBITDA não se aplicam). */
  | "holding"
  | "implausible"
  | "not_listed_on_b3"
  | "exchange_traded_product"
  | "no_cvm_code"
  | "no_filings"
  | "fund_not_found"
  | "pending";

export type CompareAsset = {
  /** A escolha como a tela mandou (`PETR4`, `KNRI11:br_reits`). */
  key: string;
  symbol: string;
  name: string;
  asset_class: AssetClass;
  in_portfolio: boolean;
  family: CompareFamily;
  currency: string;
  price: number | null;
};

export type CompareCell = {
  value: number | null;
  text: string | null;
  reason: CompareReason | null;
  /** O extremo da linha entre ativos do mesmo tipo. */
  mark: CompareMark | null;
};

export type CompareRow = {
  id: CompareRowId;
  group: CompareGroup;
  unit: CompareUnit;
  direction: CompareDirection;
  /** Uma célula por ativo, na ordem de `assets`. */
  cells: CompareCell[];
};

export type CompareTable = {
  as_of: string;
  assets: CompareAsset[];
  rows: CompareRow[];
};

export type ComparePeriod = "1m" | "3m" | "6m" | "ytd" | "12m" | "3y" | "5y" | "max";
export type CompareIndex = "cdi" | "ibov";

export type CurveReason = "no_price_history" | "no_series" | "no_index_data";

export type CompareCurve = {
  key: string;
  kind: "asset" | "portfolio" | "index";
  name: string;
  /** Em %, uma por data de `dates`; `null` antes de a curva existir. */
  values: Array<number | null>;
  return_percent: number | null;
  starts_on: string | null;
  reason: CurveReason | null;
  includes_income: boolean;
  converted_to_brl: boolean;
};

export type CompareReturns = {
  period: ComparePeriod;
  total_return: boolean;
  align_start: boolean;
  from: string;
  to: string;
  dates: string[];
  series: CompareCurve[];
};

export type ReturnsOptions = {
  period: ComparePeriod;
  totalReturn: boolean;
  alignStart: boolean;
  portfolio: boolean;
  benchmarks: readonly CompareIndex[];
};
