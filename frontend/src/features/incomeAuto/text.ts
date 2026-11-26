import type { PendingStockEvent, StockEventKind } from "../../api/incomeAutoTypes";
import { formatIsoDate } from "../../lib/income";

/** O nome do evento em ações, como a frase usa. */
export const STOCK_EVENT_NAME: Record<StockEventKind, string> = {
  bonus: "bonificação",
  split: "desdobramento",
  reverse_split: "grupamento",
};

const number = (value: number) => value.toLocaleString("pt-BR", { maximumFractionDigits: 8 });

/** Inteiro, a menos de arredondamento de ponto flutuante. */
function whole(value: number): number | null {
  const rounded = Math.round(value);
  return rounded > 0 && Math.abs(value - rounded) < 1e-6 ? rounded : null;
}

/**
 * O evento com o fator como a B3 publica: percentual na bonificação e no
 * desdobramento, multiplicador no grupamento.
 */
export function stockEventText(event: Pick<PendingStockEvent, "kind" | "factor">): string {
  const name = STOCK_EVENT_NAME[event.kind];
  switch (event.kind) {
    case "bonus":
      return `${name} de ${number(event.factor)}%`;
    case "split": {
      const each = whole(1 + event.factor / 100);
      return each ? `${name} de ${number(event.factor)}% (cada ação vira ${each})` : `${name} de ${number(event.factor)}%`;
    }
    case "reverse_split": {
      const from = event.factor > 0 ? whole(1 / event.factor) : null;
      return from ? `${name} de ${number(from)} para 1` : `${name} (fator ${number(event.factor)})`;
    }
  }
}

/** "A B3 anunciou bonificação de 2% em ITSA4 com data com 18/12/2025; lance no livro." */
export function pendingSentence(event: PendingStockEvent): string {
  return `A B3 anunciou ${stockEventText(event)} em ${event.symbol} com data com ${formatIsoDate(event.record_date)}; lance no livro.`;
}
