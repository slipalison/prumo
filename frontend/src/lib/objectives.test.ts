import { describe, expect, it } from "vitest";
import { createMoney } from "./money";
import { assumptionLines, formatDuration, formatMonth, objectiveName, percentToField, projectionText } from "./objectives";

describe("lib/objectives", () => {
  it("escreve o mês estimado como mês/ano", () => {
    expect(formatMonth("2031-03")).toBe("mar/2031");
    expect(formatMonth("2026-12")).toBe("dez/2026");
    expect(formatMonth("2031-13")).toBe("");
    expect(formatMonth("depois")).toBe("");
    expect(formatMonth(null)).toBe("");
  });

  it("escreve a duração em anos e meses, no singular quando é um", () => {
    expect(formatDuration(1)).toBe("1 mês");
    expect(formatDuration(2)).toBe("2 meses");
    expect(formatDuration(12)).toBe("1 ano");
    expect(formatDuration(13)).toBe("1 ano e 1 mês");
    expect(formatDuration(52)).toBe("4 anos e 4 meses");
    expect(formatDuration(0)).toBe("0 meses");
  });

  it("nomeia o objetivo pelo tipo e pela classe", () => {
    expect(objectiveName({ kind: "net_worth", asset_class: null })).toBe("Patrimônio total");
    expect(objectiveName({ kind: "class_value", asset_class: "us_reits" })).toBe("Valor em REITs");
  });

  it("a frase da projeção cobre os quatro estados", () => {
    expect(projectionText({ state: "reached", months: null, month: null })).toMatch(/já foi atingida/);
    expect(projectionText({ state: "eta", months: 1, month: "2026-12" })).toMatch(/chega em dez\/2026 \(daqui a 1 mês\)/);
    expect(projectionText({ state: "beyond", months: null, month: null })).toMatch(/mais de 100 anos/);
    expect(projectionText({ state: "unreachable", months: null, month: null })).toMatch(/Informe um aporte/);
  });

  it("cada premissa diz o número e de onde veio, com o valor mascarado quando o olho está fechado", () => {
    const assumptions = {
      monthly_contribution: 1200,
      contribution_source: "ledger",
      annual_return_percent: 8,
      return_source: "fallback",
    } as const;
    const open = assumptionLines(assumptions, createMoney(false));
    expect(open.contribution).toMatch(/R\$\s1\.200,00 por mês: a média das suas compras/);
    expect(open.rate).toBe("Rentabilidade de 8,00% ao ano: o padrão do Prumo, porque ainda não há série da carteira nem CDI.");
    expect(assumptionLines(assumptions, createMoney(true)).contribution).toContain("R$ ••••");
    expect(assumptionLines({ ...assumptions, return_source: "cdi_net" }, createMoney(false)).rate).toMatch(/CDI de hoje menos 15% de IR/);
  });

  it("o campo de rentabilidade usa vírgula", () => {
    expect(percentToField(12.5)).toBe("12,5");
    expect(percentToField(8)).toBe("8");
    expect(percentToField(null)).toBe("");
  });
});
