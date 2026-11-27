// Espelho exato do JSON de docs/api.md § Painel da Carteira.

/** O patrimônio no último dia do mês anterior. */
export type MonthEnd = { date: string; value: number };

/** Rentabilidade (TWR pela cota) de um mês, em %. */
export type MonthReturn = {
  from: string;
  to: string;
  return_percent: number;
  /** `false`: a série começou no meio do mês e a janela ficou mais curta. */
  full_window: boolean;
};

export type MonthTotals = {
  /** Proventos líquidos recebidos no mês. */
  income: number;
  /** Aportes menos retiradas do mês. */
  contribution: number;
};

/** `GET /portfolio/month`. */
export type PanelMonth = {
  /** O último dia da série; `null` quando a pessoa ainda não tem série. */
  as_of: string | null;
  /** O primeiro dia do mês corrente (`YYYY-MM-DD`). */
  month_starts_on: string;
  previous_month_end: MonthEnd | null;
  current_return: MonthReturn | null;
  previous_return: MonthReturn | null;
  current: MonthTotals;
  previous: MonthTotals;
};

/** O fechamento anterior de um ativo, em reais por unidade. */
export type DayReference = { asset_id: string; previous_close: number; date: string };

/** `GET /portfolio/day-reference`. */
export type DayReferences = { assets: DayReference[] };
