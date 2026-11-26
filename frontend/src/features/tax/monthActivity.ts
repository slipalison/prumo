import type { TaxMonth } from "../../api/taxTypes";

/** Mês com algo a mostrar na apuração; os outros ficam só como linha da tabela. */
export function hasActivity(month: TaxMonth): boolean {
  return (
    month.sales.some((s) => s.common_sales > 0 || s.day_trade_sales > 0) ||
    month.pools.some((p) => p.result !== 0) ||
    month.darfs.some((d) => d.total > 0) ||
    month.doubts.length > 0
  );
}
