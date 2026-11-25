import { describe, expect, it } from "vitest";
import type { IncomeMonth, IncomePayer } from "../api/incomeTypes";
import {
  barGeometry,
  barsSummary,
  formatDay,
  formatIsoDate,
  formatIsoDay,
  monthLabel,
  monthShort,
  niceCeiling,
  payerSlices,
  previewAmounts,
} from "./income";

const money = (value: number) => `R$ ${value.toFixed(2)}`;

describe("previewAmounts", () => {
  it("tira 15% de IR do JCP pago até 2025 e 17,5% desde 2026", () => {
    expect(previewAmounts("jcp", 0.5, 1000, "2025-12-31")).toEqual({ gross: 500, withholding: 75, net: 425 });
    expect(previewAmounts("jcp", 0.5, 1000, "2026-01-02")).toEqual({ gross: 500, withholding: 87.5, net: 412.5 });
  });

  it("dividendo e rendimento de FII não têm IR retido", () => {
    expect(previewAmounts("dividend", 1.1, 10, "2026-03-02")).toEqual({ gross: 11, withholding: 0, net: 11 });
    expect(previewAmounts("reit_income", 0.93, 7, "2026-03-02")).toEqual({ gross: 6.51, withholding: 0, net: 6.51 });
  });

  it("o IR informado vale mais que a regra do tipo", () => {
    expect(previewAmounts("dividend", 2, 10, "2026-03-02", 6)).toEqual({ gross: 20, withholding: 6, net: 14 });
  });
});

describe("datas e meses", () => {
  it("escreve o mês e o dia sem passar por fuso", () => {
    expect(monthShort("2026-11")).toBe("nov");
    expect(monthLabel("2026-01")).toBe("jan/26");
    expect(monthShort("2026-13")).toBe("2026-13");
    expect(formatIsoDate("2026-11-25")).toBe("25/11/2026");
    expect(formatIsoDay("2026-11-05")).toBe("05/11");
    expect(formatIsoDate("ontem")).toBe("ontem");
  });

  it("formata um instante como dia e ignora lixo", () => {
    expect(formatDay("2026-11-25T15:00:00Z")).toMatch(/2026/);
    expect(formatDay("nao-e-data")).toBe("");
  });
});

describe("payerSlices", () => {
  const payer = (label: string, total: number, percent: number): IncomePayer => ({
    asset_id: null,
    label,
    total,
    percent,
    yield_on_cost: { income_12m: total, basis: "received", cost: 1000, percent: total / 10, gap: null, message: null },
  });

  it("mostra todos quando são poucos", () => {
    const slices = payerSlices([payer("A", 70, 70), payer("B", 30, 30)]);
    expect(slices.map((s) => s.label)).toEqual(["A", "B"]);
    expect(new Set(slices.map((s) => s.color)).size).toBe(2);
  });

  it("junta o que passa de seis em Outros", () => {
    const many = Array.from({ length: 9 }, (_, i) => payer(`P${i}`, 10, 11.1));
    const slices = payerSlices(many);
    expect(slices).toHaveLength(7);
    expect(slices[6]).toMatchObject({ key: "others", label: "Outros (3)", total: 30, percent: 33.3, yield: null });
    // Cada pagador mostrado leva o próprio yield; o grupo "Outros" não tem um só.
    expect(slices[0]?.yield?.percent).toBe(1);
  });

  it("usa o id do ativo como chave quando há", () => {
    expect(payerSlices([{ ...payer("A", 1, 100), asset_id: "a-1" }])[0]?.key).toBe("a-1");
  });
});

describe("gráfico de barras", () => {
  it("arredonda o teto do eixo para um número redondo", () => {
    expect(niceCeiling(0)).toBe(1);
    expect(niceCeiling(-4)).toBe(1);
    expect(niceCeiling(0.7)).toBe(1);
    expect(niceCeiling(37)).toBe(50);
    expect(niceCeiling(210)).toBe(250);
    expect(niceCeiling(1000)).toBe(1000);
    expect(niceCeiling(1001)).toBe(2000);
    expect(niceCeiling(7600)).toBe(10000);
  });

  const months: IncomeMonth[] = [
    { month: "2026-10", received: 50, upcoming: 0 },
    { month: "2026-11", received: 100, upcoming: 50 },
  ];

  it("empilha o recebido e o a receber na mesma coluna", () => {
    const [first, second] = barGeometry(months, 200, 100, 200);
    expect(first).toMatchObject({ month: "2026-10", receivedHeight: 25, upcomingHeight: 0 });
    expect(second).toMatchObject({ month: "2026-11", receivedHeight: 50, upcomingHeight: 25 });
    expect(second && first && second.x > first.x).toBe(true);
    expect(barGeometry([], 200, 100, 200)).toEqual([]);
  });

  it("descreve o gráfico para o leitor de tela", () => {
    expect(barsSummary(months, money)).toBe("Renda dos últimos 12 meses: R$ 150.00 recebidos. Maior mês: nov/26, R$ 150.00.");
    expect(barsSummary([{ month: "2026-11", received: 0, upcoming: 0 }], money)).toMatch(/nenhum provento/);
    expect(barsSummary([], money)).toMatch(/nenhum provento/);
  });
});
