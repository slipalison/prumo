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

/**
 * IR retido na fonte do mês. Sem `informed`, vale a regra: 0,005% das vendas
 * comuns (dispensado até 1 real no mês) mais 1% do lucro de cada dia de day trade.
 */
export type Withholding = {
  common_sales: number;
  common_rule: number;
  day_trade_gains: number;
  day_trade_rule: number;
  /** O que a pessoa leu na nota; `null` = pela regra. */
  informed: number | null;
  /** O que entra na conta. */
  withheld: number;
  carried_in: number;
  used: number;
  carried_out: number;
};

export type Notice = "capital_gain_above_first_bracket" | "withholding_by_rule";

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
  notices: Notice[];
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

/** Um mês em que a mesma empresa pagou mais de 50 mil reais em dividendos (2026 em diante). */
export type ThresholdMonth = { month: number; gross: number; withholding_by_rule: number; withheld: number };

/** Uma fonte pagadora numa linha de rendimento, com o CNPJ do cadastro da CVM. */
export type IncomePayer = {
  /** ON e PN da mesma empresa vão juntos. */
  symbols: string[];
  /** `null`: o Prumo não achou na CVM. */
  cnpj: string | null;
  name: string | null;
  payer_source: "cvm_company" | "cvm_fund" | null;
  gross: number;
  withheld: number;
  net: number;
  above_threshold: ThresholdMonth[];
};

export type IrpfIncome = {
  group: string;
  code: string;
  label: string;
  code_source: string;
  gross: number;
  withheld: number;
  net: number;
  by_payer: IncomePayer[];
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

/** Um número da regra e a fonte dele. */
export type Sourced<T = number> = { value: T; source: string };

export type LatePaymentRule = {
  /** 0,0033 = 0,33% ao dia. */
  daily_penalty: Sourced;
  penalty_cap: Sourced;
  payment_month_interest: Sourced;
  selic_series: Sourced<string>;
};

/** A tabela de regras de um ano (`GET /tax/rules/{year}`). */
export type TaxRules = {
  year: number;
  reviewed_on: string;
  covered_years: number[];
  instruments: Array<{
    instrument: Instrument;
    regime: Regime;
    common_pool: Pool | null;
    day_trade_pool: Pool | null;
    exemption: ExemptionId | null;
    source: string;
  }>;
  pools: Array<{ pool: Pool; rate: Sourced }>;
  exemptions: Array<{
    id: ExemptionId;
    limit: Sourced;
    counted: Instrument[];
    counts_day_trade: boolean;
    counts_day_trade_source: string;
  }>;
  capital_gain_brackets: Array<{ up_to: number | null; rate: number }>;
  capital_gain_brackets_source: string;
  exchange_darf_code: Sourced<string>;
  capital_gain_darf_code: Sourced<string>;
  darf_minimum: Sourced;
  withholding: {
    common_rate: Sourced;
    common_dispensed_up_to: Sourced;
    day_trade_rate: Sourced;
    carry_within_year_source: string;
  };
  late_payment: LatePaymentRule;
  interest_on_equity_rate: Sourced;
  dividend_withholding: { monthly_threshold: number; rate: number; source: string } | null;
};

export type SelicMonth = { year: number; month: number; rate: number | null };

/** A multa e os juros de um DARF pago em `pay_date` (`GET /tax/darfs/{year}/{month}/late`). */
export type LateCharge = {
  code: DarfCode;
  year: number;
  month: number;
  principal: number;
  due_date: string;
  pay_date: string;
  days_late: number;
  /** 0,2 = 20%. */
  penalty_rate: number;
  penalty: number;
  selic_months: SelicMonth[];
  payment_month_rate: number;
  /** `null` quando falta a Selic de algum mês. */
  interest_rate: number | null;
  interest: number | null;
  total: number | null;
  missing_selic: SelicMonth[];
  rule: LatePaymentRule;
};

export type InformedWithholding = { year: number; month: number; amount: number };
