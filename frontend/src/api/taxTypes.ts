// Espelho do JSON de `/tax/*` (docs/api.md § Impostos).

/** O que o ativo é para o IR; `unsupported` fica fora da conta. */
export const TAX_INSTRUMENTS = [
  "stock",
  "etf",
  "bdr",
  "real_estate_fund",
  "agribusiness_fund",
  "crypto",
  "unsupported",
] as const;
export type TaxInstrument = (typeof TAX_INSTRUMENTS)[number];

/** Os instrumentos que o IR do Prumo apura. */
export type Instrument = Exclude<TaxInstrument, "unsupported">;

export type Pool = "common" | "day_trade" | "real_estate_fund" | "agribusiness_fund";
export type Regime = "exchange_net_gain" | "capital_gain";
export type ExemptionId = "stock_sales" | "crypto_sales";
export type DarfCode = "6015" | "4600";

export type SkipReason =
  | "unsupported_instrument"
  | "unknown_opening_cost"
  | "redemption"
  | "amortization"
  | "adjustment"
  | "adjustment_closes_position"
  | "event_without_position"
  | "conflicting_instruments"
  | "income_amortization"
  | "income_other"
  | "income_from_abroad"
  | "income_out_of_scope";

/** Os pontos de docs/ir.md § 5 que mexem num número. */
export type Doubt =
  | "day_trade_in_stock_exemption"
  | "agribusiness_separate_from_real_estate"
  | "irpf_codes"
  | "crypto_per_sale"
  | "year_end"
  | "withholding_leftover"
  | "dividend_withholding"
  | "interest_on_equity_net";

export type Skipped = {
  subject: string;
  /** `2025-03-10`; `null` quando é o ativo inteiro. */
  date: string | null;
  reason: SkipReason;
  amount: number | null;
};

export type InstrumentSales = {
  instrument: Instrument;
  common_sales: number;
  day_trade_sales: number;
  common_result: number;
  day_trade_result: number;
};

export type Exemption = {
  id: ExemptionId;
  sales: number;
  limit: number;
  remaining: number;
  exempt: boolean;
  exempt_gain: number;
};

export type PoolResult = {
  pool: Pool;
  result: number;
  loss_before: number;
  loss_used: number;
  base: number;
  /** 0,15 = 15%. */
  rate: number;
  tax: number;
  loss_after: number;
};

export type CapitalGain = { gains: number; losses: number; exempt: boolean; base: number; tax: number };

export type Withholding = { withheld: number; carried_in: number; used: number; carried_out: number };

export type Darf = {
  code: DarfCode;
  regime: Regime;
  tax: number;
  pending_before: number;
  total: number;
  /** Há DARF a pagar neste mês (R$ 10 ou mais). */
  payable: boolean;
  amount: number;
  due_date: string;
  pending_after: number;
  paid: { amount: number; paid_at: string; same_amount: boolean } | null;
};

export type TaxMonth = {
  year: number;
  month: number;
  sales: InstrumentSales[];
  exemptions: Exemption[];
  pools: PoolResult[];
  capital_gain: CapitalGain;
  withholding: Withholding;
  darfs: Darf[];
  notices: string[];
  doubts: Doubt[];
};

export type Balance = {
  losses: Partial<Record<Pool, number>>;
  pending_darf: Partial<Record<Regime, number>>;
};

export type TaxMonths = {
  year: number;
  since_year: number;
  covered_years: number[];
  /** Hoje em Brasília, `2026-11-25`. */
  today: string;
  months: TaxMonth[];
  closing: Balance;
  skipped: Skipped[];
};

export type IrpfAsset = {
  symbol: string;
  instrument: Instrument;
  quantity: number;
  total_cost: number;
  average_price: number;
  group: string;
  code: string;
  label: string;
  code_source: string;
};

export type IrpfIncome = {
  group: string;
  code: string;
  label: string;
  code_source: string;
  gross: number;
  withheld: number;
  net: number;
  by_payer: Array<{ symbol: string; gross: number; withheld: number }>;
};

export type Irpf = {
  year: number;
  since_year: number;
  covered_years: number[];
  assets: IrpfAsset[];
  exempt_income: IrpfIncome[];
  exclusive_income: IrpfIncome[];
  year_end: Balance;
  withholding_left: number;
  darf_total: number;
  doubts: Doubt[];
  skipped: Skipped[];
};

export type CarryforwardValues = {
  common_loss: number;
  day_trade_loss: number;
  real_estate_fund_loss: number;
  agribusiness_fund_loss: number;
  pending_exchange_darf: number;
  pending_capital_gain_darf: number;
};

export type Carryforward = CarryforwardValues & {
  /** O saldo vale em 1º de janeiro deste ano. */
  year: number;
  informed: boolean;
  covered_years: number[];
};

export type CarryforwardRequest = CarryforwardValues & { year: number };
