import { formatBRL, formatCompactBRL, formatQuantity } from "./format";

/** O que aparece no lugar de um valor em reais quando os valores estão ocultos. */
export const MASK_BRL = "R$ ••••";
/** O que aparece no lugar de uma quantidade. */
export const MASK_QUANTITY = "••";

/**
 * Os formatadores de dinheiro e quantidade da tela. Toda tela formata valor
 * por aqui, e nenhuma decide sozinha se mascara: com `hidden` ligado, o texto
 * que sai é a máscara — o mesmo que o olho vê e o leitor de tela lê.
 */
export type Money = {
  hidden: boolean;
  /** `R$ 1.234,56`. */
  brl: (value: number) => string;
  /** `R$ 136 mil`. */
  compact: (value: number) => string;
  /** `12` ou `0,0045`. */
  quantity: (value: number, fractional: boolean) => string;
};

const VISIBLE: Money = {
  hidden: false,
  brl: formatBRL,
  compact: formatCompactBRL,
  quantity: formatQuantity,
};

const HIDDEN: Money = {
  hidden: true,
  brl: () => MASK_BRL,
  compact: () => MASK_BRL,
  quantity: () => MASK_QUANTITY,
};

export function createMoney(hidden: boolean): Money {
  return hidden ? HIDDEN : VISIBLE;
}
