import type { Rates } from "../../api/fundamentalsTypes";
import { formatPercent } from "../../lib/format";

/** Em palavras, onde a cotação está em relação ao valor patrimonial (fato, sem juízo). */
export function priceToBookText(value: number): string {
  const diff = Math.round((value - 1) * 100);
  if (diff === 0) return "no valor patrimonial";
  return diff < 0
    ? `${formatPercent(-diff, 0)} abaixo do valor patrimonial`
    : `${formatPercent(diff, 0)} acima do valor patrimonial`;
}

/** "CDI 12 meses líquido 12,1% · juro real 7,1%": o que houver das duas. */
export function ratesText(rates: Rates): string | null {
  const parts: string[] = [];
  if (rates.cdi_12m) parts.push(`CDI 12 meses líquido ${formatPercent(rates.cdi_12m.net_percent)}`);
  if (rates.real_rate) parts.push(`juro real ${formatPercent(rates.real_rate.percent)}`);
  return parts.length > 0 ? parts.join(" · ") : null;
}
