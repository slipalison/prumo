import { formatBRL, formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";

/** `+R$ 12,00` / `−R$ 3,00` / `R$ 0,00`: o sinal escrito, não só a cor. */
export function formatSignedBRL(value: number): string {
  if (value > 0) return `+${formatBRL(value)}`;
  if (value < 0) return `−${formatBRL(-value)}`;
  return formatBRL(0);
}

/**
 * O mesmo, pelos formatadores da tela: com os valores ocultos sai a máscara,
 * sem sinal — o sentido do ganho já está na rentabilidade em %, que fica à vista.
 */
export function signedBRL(money: Money, value: number): string {
  return money.hidden ? money.brl(value) : formatSignedBRL(value);
}

/** `+12,3%` / `−4,0%`. */
export function formatSignedPercent(value: number): string {
  if (value > 0) return `+${formatPercent(value)}`;
  if (value < 0) return `−${formatPercent(-value)}`;
  return formatPercent(0);
}

/** Número para um campo de texto em pt-BR, sem separador de milhar ("1234,5"). */
export function numberText(value: number): string {
  return value.toLocaleString("pt-BR", { maximumFractionDigits: 8, useGrouping: false });
}
