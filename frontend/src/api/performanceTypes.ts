/** Contrato de `GET /history/net-worth`, `GET /performance` e `GET /performance/classes` (docs/api.md). */

import type { AssetClass } from "./types";

export type PerformancePeriod = "month" | "ytd" | "12m" | "2y" | "5y" | "all";
export type BenchmarkCode = "cdi" | "selic" | "ipca" | "ibov" | "ifix";

export type NetWorthPoint = {
  /** `AAAA-MM-DD`, dia em Brasília. */
  date: string;
  market_value: number;
  /** O que a pessoa pôs menos o que tirou, desde o começo da série. */
  invested: number;
  /** Patrimônio − aplicado: o ganho de preço, sem os proventos. */
  gain: number;
  /** Proventos recebidos desde o começo da série. */
  income: number;
  /** Foto de hoje calculada na hora, antes do fechamento. */
  partial: boolean;
};

/** De onde veio a variação do patrimônio no período. */
export type NetWorthBridge = {
  /** O fechamento de base; `null`: a janela começa do zero (Tudo, ou série mais nova que o período). */
  from: string | null;
  to: string;
  start_value: number;
  /** Aportes menos retiradas no período. */
  contributions: number;
  /** O que o preço fez no período. */
  market: number;
  /** Proventos recebidos no período (saíram da carteira). */
  income: number;
  end_value: number;
  /** Desde o começo da série. */
  invested: number;
  gain: number;
  income_total: number;
};

export type NetWorth = {
  period: PerformancePeriod;
  starts_on: string | null;
  /** O livro mudou no passado e a série está sendo refeita desde esta data. */
  rebuilding_since: string | null;
  points: NetWorthPoint[];
  summary: NetWorthBridge | null;
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

/** Um índice numa linha da tabela mês × ano, nas janelas da carteira. */
export type YearIndex = {
  months: Array<number | null>;
  /** Desde o começo da série até o fim de cada mês. */
  accumulated: Array<number | null>;
  return_percent: number | null;
  accumulated_percent: number | null;
  /** Último dia coberto por dado publicado no ano. */
  as_of: string | null;
};

export type PerformanceYear = {
  year: number;
  /** Janeiro a dezembro, em %. */
  months: Array<number | null>;
  return_percent: number;
  cdi_percent: number | null;
  /** Desde o começo da série até o fim de cada mês, em %. */
  accumulated: Array<number | null>;
  /** Desde o começo da série até o fim do ano. */
  accumulated_percent: number;
  benchmarks: Partial<Record<BenchmarkCode, YearIndex>>;
};

export type MonthMark = { year: number; /** 1 a 12. */ month: number; return_percent: number };

export type MonthsSummary = {
  best: MonthMark | null;
  worst: MonthMark | null;
  positive: number;
  negative: number;
  flat: number;
  /** Em quantos meses a carteira rendeu mais que cada índice (entre os que dava para comparar). */
  versus: Array<{ code: BenchmarkCode; label: string; above: number; compared: number }>;
};

export type Performance = {
  period: PerformancePeriod;
  starts_on: string | null;
  rebuilding_since: string | null;
  window: { from: string; to: string } | null;
  return_percent: number | null;
  benchmarks: Benchmark[];
  points: CurvePoint[];
  summary: { twelve_months: VersusCdi | null; since_start: VersusCdi | null };
  years: PerformanceYear[];
  months_summary: MonthsSummary;
};

export type ClassPerformance = {
  asset_class: AssetClass;
  /** Patrimônio da classe no fim do período. */
  market_value: number;
  weight_percent: number;
  /** Método de cotas, na janela da classe. */
  return_percent: number;
  from: string;
  to: string;
  /** A classe mede o período inteiro (não entrou nem zerou no meio). */
  full_window: boolean;
  /** O índice de referência da classe na mesma janela. */
  benchmark: Benchmark | null;
};

export type ClassesPerformance = {
  period: PerformancePeriod;
  rebuilding_since: string | null;
  window: { from: string; to: string } | null;
  classes: ClassPerformance[];
};
