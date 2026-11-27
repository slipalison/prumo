import type { IncomeKind, IncomeSource } from "./incomeTypes";
import type { AssetClass } from "./types";

/** O tipo de um evento de `GET /agenda` (docs/api.md § Agenda). */
export type AgendaEventType =
  | "market_closed"
  | "income_record"
  | "income_payment"
  | "unusual_income"
  | "results"
  | "material_fact"
  | "notice";

export type AgendaAsset = {
  asset_id: string;
  symbol: string | null;
  display_name: string;
  asset_class: AssetClass;
};

export type UnusualDirection = "increase" | "cut";

/** O habitual de um período: a média dos pagamentos parecidos, por cota de hoje. */
export type UsualReference = {
  usual: number;
  /** `valor ÷ habitual − 1` (`-0.25` é 25% abaixo). */
  change: number;
  payments: number;
};

export type UnusualMark = {
  direction: UnusualDirection;
  /** Os 12 meses antes da data ex. */
  recent: UsualReference;
  /** Até 5 anos antes; `null` com menos de 2 anos de histórico. */
  long: UsualReference | null;
  years: number;
};

/** `undated`: a empresa anunciou, mas ainda não definiu a data de pagamento. */
export type AgendaIncomeState = "received" | "upcoming" | "undated";

export type AgendaIncome = {
  income_id: string | null;
  kind: IncomeKind;
  record_date: string | null;
  ex_date: string | null;
  pay_date: string | null;
  amount_per_unit: number;
  quantity: number;
  gross: number;
  net: number;
  state: AgendaIncomeState;
  source: IncomeSource;
  unusual: UnusualMark | null;
};

export type AgendaUnusual = {
  direction: UnusualDirection;
  /** O que a data com pagou por cota de hoje, bruto. */
  per_unit: number;
  record_date: string;
  ex_date: string;
  approved_on: string | null;
  pay_dates: string[];
  recent: UsualReference;
  long: UsualReference | null;
  years: number;
};

export type DocumentCategory =
  | "material_fact"
  | "market_notice"
  | "shareholder_notice"
  | "meeting"
  | "results"
  | "management_report";

export type DocumentSource = "cvm_ipe" | "cvm_filing" | "fundos_net";

export type AgendaDocument = {
  category: DocumentCategory;
  source: DocumentSource;
  title: string | null;
  subject: string | null;
  reference_date: string | null;
  version: number;
  url: string | null;
  /** `annual` (DFP) ou `quarterly` (ITR), no resultado da companhia. */
  filing: "annual" | "quarterly" | null;
};

export type MarketClosure =
  | "new_year"
  | "carnival"
  | "good_friday"
  | "tiradentes"
  | "labour_day"
  | "corpus_christi"
  | "independence"
  | "aparecida"
  | "all_souls"
  | "republic"
  | "black_consciousness"
  | "christmas_eve"
  | "christmas"
  | "year_end";

export type AgendaEvent = {
  date: string;
  type: AgendaEventType;
  assets: AgendaAsset[];
  income: AgendaIncome | null;
  unusual: AgendaUnusual | null;
  document: AgendaDocument | null;
  closure: MarketClosure | null;
};

export type Agenda = {
  today: string;
  from: string;
  to: string;
  /** O último ano com calendário de pregões cadastrado. */
  calendar_until: number | null;
  updated: { cvm_documents: string | null; fund_documents: string | null };
  events: AgendaEvent[];
};

/** `GET /agenda/unusual-income`: os proventos a receber fora do padrão. */
export type UnusualIncome = UnusualMark & {
  income_id: string;
  asset_id: string;
  record_date: string;
  ex_date: string;
  per_unit: number;
};

export type UnusualIncomeList = { items: UnusualIncome[] };
