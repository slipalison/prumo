import { formatPercent } from "../../lib/format";

/** `2026-11-25` → `25/11` (ou `25/11/2026`). */
export function formatDay(iso: string, withYear = false): string {
  const [year, month, day] = iso.split("-");
  if (!year || !month || !day) return iso;
  return withYear ? `${day}/${month}/${year}` : `${day}/${month}`;
}

/** As duas datas caem em anos diferentes: aí "05/03" sozinho fica ambíguo e a data leva o ano. */
export function spansYears(first: string | undefined, last: string | undefined): boolean {
  if (!first || !last) return false;
  return first.slice(0, 4) !== last.slice(0, 4);
}

/** `+1,23%` / `−0,40%`: o sinal diz o sentido sem depender da cor. */
export function formatSignedPercent(value: number, digits = 2): string {
  if (Math.abs(value) < 0.5 * 10 ** -digits) return formatPercent(0, digits);
  const text = formatPercent(Math.abs(value), digits);
  return value > 0 ? `+${text}` : `−${text}`;
}

/** Classe do tom (alta, baixa ou neutro) de uma variação. */
export function trend(value: number | null | undefined): "up" | "down" | "flat" {
  if (value === null || value === undefined || Math.abs(value) < 0.005) return "flat";
  return value > 0 ? "up" : "down";
}

/** "1,80% acima do CDI" / "0,52% abaixo do CDI" / "igual ao CDI". */
export function versusText(excess: number): string {
  if (Math.abs(excess) < 0.005) return "igual ao CDI";
  return `${formatPercent(Math.abs(excess), 2)} ${excess > 0 ? "acima" : "abaixo"} do CDI`;
}

export const MONTHS = ["Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez"] as const;
export const MONTH_NAMES = [
  "janeiro",
  "fevereiro",
  "março",
  "abril",
  "maio",
  "junho",
  "julho",
  "agosto",
  "setembro",
  "outubro",
  "novembro",
  "dezembro",
] as const;
