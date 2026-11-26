/**
 * `GET /api/v1/fundamentals` (docs/api.md § Fundamentos): os indicadores da
 * CVM de cada ativo da carteira. Toda medida é `{ value, gap }`: o número ou
 * o motivo de não haver número — nunca um zero no lugar.
 */

/** Por que uma medida não tem número. */
export type FundamentalsGap =
  | "financial_template"
  | "unknown_template"
  | "not_reported"
  | "no_filing"
  | "non_positive_base"
  | "non_positive_endpoint"
  | "insufficient_history"
  | "no_quote"
  | "no_rental_property"
  | "no_quarterly_report"
  | "no_shares";

export type Figure = { value: number | null; gap: FundamentalsGap | null };

/** Plano de contas: o comercial tem todas as contas; banco e seguradora, só lucro e patrimônio. */
export type AccountTemplate = "commercial" | "bank" | "insurer" | "unknown";

export type FilingRef = {
  kind: "annual" | "quarterly";
  reference_date: string;
  version: number;
  scope: "consolidated" | "individual";
  column: "current" | "previous";
};

export type PeriodFigures = {
  period_start: string;
  period_end: string;
  template: AccountTemplate;
  sources: FilingRef[];
  revenue: Figure;
  net_income: Figure;
  equity: Figure;
  gross_debt: Figure;
  cash: Figure;
  net_debt: Figure;
  ebit: Figure;
  depreciation_amortization: Figure;
  ebitda: Figure;
  operating_cash_flow: Figure;
  roe: Figure;
  net_margin: Figure;
  ebitda_margin: Figure;
  net_debt_to_ebitda: Figure;
  net_debt_to_equity: Figure;
};

export type YearCount = { positive: number; known: number; window: number; gap: FundamentalsGap | null };

export type CompanyFundamentals = {
  cvm_code: number;
  name: string;
  template: AccountTemplate;
  judicial_recovery: boolean | null;
  issuer_status: string | null;
  registration_status: string | null;
  latest_filing: FilingRef | null;
  synced_at: string | null;
  trailing: PeriodFigures | null;
  years: Array<{ year: number; figures: PeriodFigures | null }>;
  profitable_years: YearCount;
  positive_operating_cash_years: YearCount;
  revenue_cagr: Figure;
  earnings_cagr: Figure;
};

export type FundFundamentals = {
  cnpj: string;
  name: string;
  reference_date: string;
  net_assets: Figure;
  shares: Figure;
  nav_per_share: Figure;
  price: number | null;
  price_to_book: Figure;
  vacancy: Figure;
  vacancy_reference_date: string | null;
  synced_at: string | null;
};

export type UnsupportedReason =
  | "not_listed_on_b3"
  | "exchange_traded_product"
  | "no_cvm_code"
  | "no_filings"
  | "fund_not_found"
  | "pending";

type Base = { asset_id: string; symbol: string };

export type AssetFundamentals =
  | (Base & { kind: "company"; company: CompanyFundamentals; fund: null; reason: null })
  | (Base & { kind: "fund"; company: null; fund: FundFundamentals; reason: null })
  | (Base & { kind: "unsupported"; company: null; fund: null; reason: UnsupportedReason });

export type Fundamentals = { assets: AssetFundamentals[] };
