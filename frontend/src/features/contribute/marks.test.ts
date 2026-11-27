import { describe, expect, it } from "vitest";
import { samplePlan } from "../../test/fixtures";
import { cashUsedFor, contributionBody, differsFromPlan, markValue, marksTotal, suggested, type Marks } from "./marks";

const plan = samplePlan(6000);
const [itsa, btlg, trxf] = plan.items;
if (!itsa || !btlg || !trxf) throw new Error("fixture sem itens");

describe("marcas de compra", () => {
  it("o valor é quantidade × preço, em centavos redondos", () => {
    expect(markValue({ quantity: 3, unit_price: 0.1 })).toBe(0.3);
    expect(markValue({ quantity: 0.0045, unit_price: 345588.05 })).toBe(1555.15);
  });

  it("a sugestão é o plano, e só quantidade ou preço diferentes acusam diferença", () => {
    expect(differsFromPlan(btlg, suggested(btlg))).toBe(false);
    expect(differsFromPlan(btlg, { ...suggested(btlg), quantity: 7 })).toBe(true);
    expect(differsFromPlan(btlg, { ...suggested(btlg), unit_price: 90 })).toBe(true);
  });

  it("o aporte soma só os marcados, na ordem do plano", () => {
    const marks: Marks = { [trxf.asset_id]: { quantity: 10, unit_price: 50 }, [itsa.asset_id]: suggested(itsa) };
    expect(marksTotal(plan, marks)).toBe(3221.6);
    expect(contributionBody(plan, marks, 0).items.map((i) => i.asset_id)).toEqual([itsa.asset_id, trxf.asset_id]);
  });

  describe("regra do caixa com aporte parcial", () => {
    it("o caixa incluído é gasto primeiro e nunca passa do registrado", () => {
      expect(cashUsedFor(812, 5947.92)).toBe(812);
      expect(cashUsedFor(812, 500)).toBe(500);
      expect(cashUsedFor(0, 500)).toBe(0);
    });

    it("sem caixa incluído o corpo não leva `cash_used`", () => {
      const body = contributionBody(plan, { [btlg.asset_id]: suggested(btlg) }, 0);
      expect(body).toEqual({ amount: 756.4, items: [{ asset_id: btlg.asset_id, quantity: 8, unit_price: 94.55 }] });
      expect(body).not.toHaveProperty("cash_used");
    });

    it("com caixa, `cash_used` é o menor entre o caixa e a soma dos marcados", () => {
      const only = { [btlg.asset_id]: suggested(btlg) };
      expect(contributionBody(plan, only, 812).cash_used).toBe(756.4);
      expect(contributionBody(plan, { ...only, [itsa.asset_id]: suggested(itsa) }, 812).cash_used).toBe(812);
    });
  });
});
