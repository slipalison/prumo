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

/** O fechamento anterior de um ativo e o que os negócios mudaram depois dele. */
export type DayReference = {
  asset_id: string;
  /** Reais por unidade; `null` no ativo que não estava no fechamento e teve compra ou venda depois. */
  previous_close: number | null;
  date: string;
  /** As cotas que havia no fechamento. */
  previous_quantity: number;
  /** O que as compras depois do fechamento custaram, com taxas, em reais. */
  bought: number;
  /** O líquido das vendas e resgates depois do fechamento, em reais. */
  sold: number;
};

/** `GET /portfolio/day-reference`. */
export type DayReferences = { assets: DayReference[] };
