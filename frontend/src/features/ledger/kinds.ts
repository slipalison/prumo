import type { Transaction, TransactionKind } from "../../api/ledgerTypes";
import type { UnitKind } from "../../lib/assetClasses";

/** Nome de cada tipo de lançamento, como a pessoa lê. */
const LABEL: Record<TransactionKind, string> = {
  opening: "Saldo inicial",
  buy: "Compra",
  sell: "Venda",
  split: "Desdobramento",
  reverse_split: "Grupamento",
  bonus: "Bonificação",
  amortization: "Amortização",
  redemption: "Resgate",
  adjustment: "Ajuste",
};

/** Em renda fixa por valor, compra é aplicação e ajuste é a atualização do saldo. */
export function kindLabel(kind: TransactionKind, unit: UnitKind): string {
  if (unit === "value" && kind === "buy") return "Aplicação";
  if (unit === "value" && kind === "adjustment") return "Atualização do saldo";
  return LABEL[kind];
}

/** Os tipos que fazem sentido para a unidade do ativo (docs/api.md § Lançamentos). */
export function kindsFor(unit: UnitKind): TransactionKind[] {
  if (unit === "value") return ["buy", "redemption", "adjustment", "opening"];
  return ["buy", "sell", "split", "reverse_split", "bonus", "amortization", "redemption", "adjustment", "opening"];
}

export type PriceRule = "required" | "optional" | "forbidden";

export const PRICE_RULE: Record<TransactionKind, PriceRule> = {
  opening: "optional",
  buy: "required",
  sell: "required",
  split: "forbidden",
  reverse_split: "forbidden",
  bonus: "optional",
  amortization: "required",
  redemption: "required",
  adjustment: "forbidden",
};

export function takesFees(kind: TransactionKind): boolean {
  return kind === "opening" || kind === "buy" || kind === "sell" || kind === "redemption";
}

/** Rótulo do campo de preço, que muda de sentido conforme o tipo. */
export function priceLabel(kind: TransactionKind): string {
  if (kind === "opening") return "Preço médio (deixe vazio se não souber)";
  if (kind === "bonus") return "Custo atribuído por cota (opcional)";
  if (kind === "amortization") return "Valor recebido por cota";
  return "Preço por unidade";
}

/** Dica do campo de quantidade, onde o sentido não é óbvio. */
export function quantityHint(kind: TransactionKind, unit: UnitKind): string | undefined {
  if (kind === "split") return "As cotas novas que você recebeu (num desdobramento de 1 para 2, a mesma quantidade que já tinha).";
  if (kind === "reverse_split") return "As cotas que deixaram de existir. O custo total não muda.";
  if (kind === "amortization") return "Quantas cotas receberam a amortização. A quantidade não muda; o custo cai.";
  if (kind === "adjustment")
    return unit === "value"
      ? "Quanto o saldo mudou (rendimento). Use − para diminuir. O custo não muda."
      : "Quanto a quantidade muda. Use − para diminuir. O custo não muda.";
  return undefined;
}

/** O que o lançamento faz com a quantidade. */
export function quantityDelta(transaction: Pick<Transaction, "kind" | "quantity">): number {
  switch (transaction.kind) {
    case "sell":
    case "redemption":
    case "reverse_split":
      return -transaction.quantity;
    case "amortization":
      return 0;
    default:
      return transaction.quantity;
  }
}

/** Hoje, no fuso do navegador, como `AAAA-MM-DD`. */
export function todayIso(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** `2026-03-02` → `02/03/2026`. */
export function formatTradeDate(iso: string): string {
  const [year, month, day] = iso.split("-");
  return year && month && day ? `${day}/${month}/${year}` : iso;
}
