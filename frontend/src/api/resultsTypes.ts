/**
 * `GET /api/v1/results` (docs/api.md § Resultados, docs/resultados.md): os
 * trimestres das empresas da carteira e a série mensal dos FIIs. Toda medida é
 * `{ value, gap }`, como nos fundamentos; a variação diz o tipo (`kind`) e só
 * tem percentual com a base positiva.
 */
import type { AccountTemplate, Figure, FilingRef, FundamentalsGap, UnsupportedReason } from "./fundamentalsTypes";

/** O que a linha de cima é: a receita, ou o resultado bruto de banco e seguradora. */
export type TopLine = "revenue" | "intermediation_result" | "insurance_result" | "unknown";

export type QuarterMethod = "first_quarter" | "year_to_date_difference" | "annual_less_nine_months";

export type ChangeKind =
  | "percent"
  | "left_loss"
  | "entered_loss"
  | "smaller_loss"
  | "larger_loss"
  | "unchanged"
  | "no_base"
  | "missing";

/** `value` (fração) só em `percent`; `gap` só em `missing`. */
export type Change = { kind: ChangeKind; value: number | null; gap: FundamentalsGap | null };

export type Quarter = {
  fiscal_quarter: number;
  period_start: string;
  period_end: string;
  method: QuarterMethod;
  top_line: Figure;
  net_income: Figure;
  net_margin: Figure;
  source: FilingRef;
  /** Quando a CVM recebeu o documento que fecha o trimestre. */
  received_on: string | null;
};

export type Comparison = {
  /** O fim do trimestre comparado. */
  period_end: string;
  top_line: Change;
  net_income: Change;
  /** Margem de agora − a de antes, em fração (0,012 = 1,2 ponto). */
  net_margin_points: Figure;
};

export type CompanyResults = {
  name: string;
  template: AccountTemplate;
  holding: boolean;
  top_line: TopLine;
  synced_at: string | null;
  /** Até oito trimestres, do mais velho ao mais novo. */
  quarters: Quarter[];
  year_ago: Comparison | null;
  previous_quarter: Comparison | null;
};

export type FundMonth = { reference_date: string; nav_per_share: Figure; dividend_yield: Figure };

export type FundResults = {
  name: string;
  synced_at: string | null;
  /** Até doze meses, do mais velho ao mais novo. */
  months: FundMonth[];
};

type Base = { asset_id: string; symbol: string; /** Fração do valor da carteira. */ weight: number | null };

export type AssetResults =
  | (Base & { kind: "company"; company: CompanyResults; fund: null; reason: null })
  | (Base & { kind: "fund"; company: null; fund: FundResults; reason: null })
  | (Base & { kind: "unsupported"; company: null; fund: null; reason: UnsupportedReason });

export type Results = { assets: AssetResults[] };
