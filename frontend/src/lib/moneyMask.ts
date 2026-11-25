import { formatBRL } from "./format";

/** Teto de dígitos: R$ 99.999.999.999,99 — além disso é engano de digitação. */
const MAX_DIGITS = 13;

/**
 * Máscara de caixa eletrônico: cada dígito entra pela direita, como centavo.
 * Digitar `600000` dá R$ 6.000,00. Qualquer outro caractere é ignorado.
 */
export function centsFromInput(text: string): number {
  const digits = text.replace(/\D/g, "").replace(/^0+/, "").slice(0, MAX_DIGITS);
  return digits ? Number(digits) : 0;
}

export function reaisFromInput(text: string): number {
  return centsFromInput(text) / 100;
}

export function formatMoneyInput(reais: number): string {
  return formatBRL(Math.round(reais * 100) / 100);
}
