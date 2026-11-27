import type { ContributionRequest, Plan, PlanItem } from "../../api/types";

/**
 * O que a pessoa de fato comprou de um ativo do plano: a quantidade e o preço
 * pagos (vêm preenchidos com os do plano e podem ser outros).
 */
export type Mark = { quantity: number; unit_price: number };

/** Os ativos marcados como comprados, por `asset_id`. Quem não está aqui ficou de fora. */
export type Marks = Record<string, Mark>;

const cents = (value: number) => Math.round(value * 100) / 100;

/** A marca "como sugerido": a quantidade e o preço do plano. */
export function suggested(item: PlanItem): Mark {
  return { quantity: item.quantity, unit_price: item.unit_price };
}

/** Quanto vale a compra marcada, em centavos redondos (o que o aporte soma). */
export function markValue(mark: Mark): number {
  return cents(mark.quantity * mark.unit_price);
}

/** A marca difere do que o plano sugeria (na quantidade ou no preço)? */
export function differsFromPlan(item: PlanItem, mark: Mark): boolean {
  return mark.quantity !== item.quantity || mark.unit_price !== item.unit_price;
}

/** Os itens do plano que estão marcados, na ordem do plano. */
export function markedItems(plan: Plan, marks: Marks): PlanItem[] {
  return plan.items.filter((item) => marks[item.asset_id] !== undefined);
}

/** A soma dos marcados: o valor do aporte que será registrado. */
export function marksTotal(plan: Plan, marks: Marks): number {
  return cents(markedItems(plan, marks).reduce((sum, item) => sum + markValue(marks[item.asset_id] as Mark), 0));
}

/**
 * Quanto do aporte registrado sai do caixa. Regra do caixa (docs/api.md,
 * `cash_used`): o caixa incluído no valor digitado é gasto primeiro, e nunca
 * passa do que foi registrado — `cash_used` ≤ `amount`. Registrando só parte do
 * plano, o caixa cobre o que couber; o resto, se houver, é dinheiro de fora.
 */
export function cashUsedFor(cashIncluded: number, total: number): number {
  return Math.min(cents(cashIncluded), total);
}

/** O corpo do `POST /contributions` só com os marcados. */
export function contributionBody(plan: Plan, marks: Marks, cashIncluded: number): ContributionRequest {
  const amount = marksTotal(plan, marks);
  const cashUsed = cashUsedFor(cashIncluded, amount);
  return {
    amount,
    ...(cashUsed > 0 ? { cash_used: cashUsed } : {}),
    items: markedItems(plan, marks).map((item) => {
      const mark = marks[item.asset_id] as Mark;
      return { asset_id: item.asset_id, quantity: mark.quantity, unit_price: mark.unit_price };
    }),
  };
}
