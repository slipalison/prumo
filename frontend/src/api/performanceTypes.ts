/** Contrato de `GET /history/net-worth` e `GET /performance` (docs/api.md). */

export type PerformancePeriod = "month" | "ytd" | "12m" | "all";
export type BenchmarkCode = "cdi" | "selic" | "ipca" | "ibov";

export type NetWorthPoint = {
  /** `AAAA-MM-DD`, dia em Brasília. */
  date: string;
  market_value: number;
  invested: number;
  gain: number;
  /** Foto de hoje calculada na hora, antes do fechamento. */
  partial: boolean;
};

export type NetWorth = {
  period: PerformancePeriod;
  starts_on: string | null;
  points: NetWorthPoint[];
};

export type Benchmark = {
  code: BenchmarkCode;
  label: string;
  return_percent: number | null;
  as_of: string | null;
};

export type CurvePoint = {
  date: string;
  portfolio: number;
  benchmarks: Partial<Record<BenchmarkCode, number | null>>;
};

export type VersusCdi = {
  from: string;
  to: string;
  return_percent: number;
  cdi_percent: number | null;
  cdi_as_of: string | null;
  excess_over_cdi: number | null;
  percent_of_cdi: number | null;
  full_window: boolean;
};

export type PerformanceYear = {
  year: number;
  /** Janeiro a dezembro, em %. */
  months: Array<number | null>;
  return_percent: number;
  cdi_percent: number | null;
};

export type Performance = {
  period: PerformancePeriod;
  starts_on: string | null;
  window: { from: string; to: string } | null;
  return_percent: number | null;
  benchmarks: Benchmark[];
  points: CurvePoint[];
  summary: { twelve_months: VersusCdi | null; since_start: VersusCdi | null };
  years: PerformanceYear[];
};
