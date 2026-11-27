import { describe, expect, it } from "vitest";
import { createMoney } from "./money";
import { sampleAssumptions, sampleDefaults, sampleScenarios } from "../test/objectivesHandlers";
import {
  assumptionLines,
  baseText,
  formatDuration,
  formatMonth,
  inflationSourceText,
  objectiveName,
  percentToField,
  premiseRows,
  projectionText,
  rangeText,
  shortDate,
  targetText,
} from "./objectives";

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
    expect(baseText({ state: "eta", months: 13, month: "2027-12" })).toBe("Cenário base: a meta chega em dez/2027 (daqui a 1 ano e 1 mês).");
    expect(baseText({ state: "beyond", months: null, month: null })).toMatch(/^Cenário base: mais de 100 anos/);
    expect(baseText({ state: "reached", months: null, month: null })).toMatch(/já foi atingida/);
  });

  it("a faixa junta o otimista e o pessimista, e diz quando os três coincidem", () => {
    expect(rangeText(sampleScenarios, "nominal")).toBe("Faixa: mar/2030 no otimista, set/2032 no pessimista.");
    expect(rangeText(sampleScenarios, "today")).toBe("Faixa: jan/2031 no otimista, nov/2034 no pessimista.");
    const same = { state: "unreachable", months: null, month: null } as const;
    const flat = { pessimistic: { nominal: same, today: same }, base: { nominal: same, today: same }, optimistic: { nominal: same, today: same } };
    expect(rangeText(flat, "nominal")).toBe("Nos três cenários: não chega.");
    expect(shortDate({ state: "reached", months: null, month: null })).toBe("já");
    expect(shortDate({ state: "beyond", months: null, month: null })).toBe("mais de 100 anos");
  });

  it("nomeia a meta de renda pelo modo e escreve a meta por mês", () => {
    expect(objectiveName({ kind: "monthly_income", asset_class: null, income_mode: "floor" })).toBe("Renda mensal (todo mês)");
    expect(objectiveName({ kind: "monthly_income", asset_class: null, income_mode: "average" })).toBe("Renda mensal (média)");
    expect(targetText({ kind: "monthly_income", target: 500 }, createMoney(false))).toMatch(/R\$\s500,00 por mês/);
    expect(targetText({ kind: "net_worth", target: 500 }, createMoney(true))).toBe("R$ ••••");
  });

  it("as duas premissas principais em frase, com o valor mascarado quando o olho está fechado", () => {
    const assumptions = { ...sampleAssumptions, monthly_contribution: 1200, real_return_percent: 4, return_source: "fallback" } as const;
    const open = assumptionLines(assumptions, createMoney(false));
    expect(open.contribution).toMatch(/R\$\s1\.200,00 por mês, em reais de hoje: a média das suas compras/);
    expect(open.rate).toBe("Rentabilidade de 4,00% ao ano acima da inflação: o padrão do Prumo, porque ainda não há série da carteira nem CDI.");
    expect(assumptionLines(assumptions, createMoney(true)).contribution).toContain("R$ ••••");
    expect(assumptionLines({ ...assumptions, return_source: "cdi_net" }, createMoney(false)).rate).toMatch(/CDI de hoje menos 15% de IR e a inflação/);
  });

  it("cada premissa da tabela diz o número, a origem e se é sua", () => {
    const rows = premiseRows({ kind: "net_worth" }, sampleAssumptions, createMoney(false), sampleDefaults);
    expect(rows.map((r) => r.id)).toEqual(["contribution", "return", "inflation", "reinvest", "yield", "spread"]);
    expect(rows.find((r) => r.id === "inflation")?.source).toBe(
      "o IPCA esperado pelo Focus do Banco Central de 2027 a 2030, pesquisa de 19/11/2026",
    );
    expect(rows.find((r) => r.id === "spread")?.value).toBe("2 pontos ao ano para cada lado");
    expect(rows.find((r) => r.id === "spread")?.source).toBe("o padrão, na rentabilidade real");
    const income = premiseRows(
      { kind: "monthly_income" },
      {
        ...sampleAssumptions,
        income_growth_source: "informed",
        income_growth_percent: 1.5,
        income_yield_source: "informed",
        scenario_spread_percent: 1,
        spread_source: "informed",
      },
      createMoney(false),
      null,
    );
    expect(income.map((r) => r.id)).toEqual(["contribution", "inflation", "reinvest", "growth", "yield", "spread"]);
    expect(income.find((r) => r.id === "growth")).toMatchObject({ informed: true, value: "1,50% ao ano, acima da inflação" });
    expect(income.find((r) => r.id === "yield")?.source).toBe("informado por você");
    expect(income.find((r) => r.id === "spread")).toMatchObject({
      value: "1 ponto ao ano para cada lado",
      source: "informado por você, no crescimento dos proventos",
    });
  });

  it("a origem da inflação cobre as quatro fontes", () => {
    expect(inflationSourceText("informed", null)).toBe("informada por você");
    expect(inflationSourceText("focus", null)).toBe("o IPCA esperado pelo Focus do Banco Central");
    expect(inflationSourceText("focus", { inflation_years: [2027, 2027], focus: null })).toBe(
      "o IPCA esperado pelo Focus do Banco Central para 2027",
    );
    expect(inflationSourceText("ipca_12m", null)).toMatch(/últimos 12 meses/);
    expect(inflationSourceText("target", null)).toMatch(/meta de inflação \(3%\)/);
  });

  it("o campo de rentabilidade usa vírgula", () => {
    expect(percentToField(12.5)).toBe("12,5");
    expect(percentToField(8)).toBe("8");
    expect(percentToField(null)).toBe("");
  });
});
