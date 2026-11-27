import { describe, expect, it } from "vitest";
import type { FundReference } from "../api/ceilingTypes";
import { defaultFunds, defaultMarket, defaultStocks } from "../test/referencesFixtures";
import {
  cdiText,
  ceilingLabel,
  choiceOf,
  draftChanges,
  draftOf,
  formatIsoDay,
  fundYieldMath,
  methodPhrase,
  methodValue,
  realRateText,
  referenceFamily,
} from "./ceilingText";
import { createMoney, MASK_BRL } from "./money";

// O Intl separa "R$" do número com espaço que não quebra (U+00A0).
const nbsp = (text: string | null) => (text ?? "").replace(new RegExp(String.fromCharCode(160), "g"), " ");
const shown = createMoney(false);

const fund = (label: string): FundReference => {
  const found = defaultFunds().find((f) => f.label === label);
  if (!found) throw new Error(label);
  return found;
};

describe("ceilingText", () => {
  it("a data do servidor vira dd/mm/aaaa, sem fuso", () => {
    expect(formatIsoDay("2026-11-19")).toBe("19/11/2026");
    expect(formatIsoDay("2026-01-01T00:00:00Z")).toBe("01/01/2026");
    expect(formatIsoDay("ontem")).toBe("ontem");
  });

  it("só ações e FIIs da B3, com ticker, têm referências", () => {
    expect(referenceFamily({ asset_class: "br_stocks", listed_on_b3: true, symbol: "ITSA4" })).toBe("stock");
    expect(referenceFamily({ asset_class: "br_reits", listed_on_b3: true, symbol: "KNRI11" })).toBe("fund");
    expect(referenceFamily({ asset_class: "crypto", listed_on_b3: true, symbol: "QBTC11" })).toBeNull();
    expect(referenceFamily({ asset_class: "intl_stocks", listed_on_b3: false, symbol: "VOO" })).toBeNull();
    expect(referenceFamily({ asset_class: "fixed_income", listed_on_b3: false, symbol: null })).toBeNull();
  });

  it("o valor de um método é o da família do ativo; outro método não tem valor", () => {
    const [itsa] = defaultStocks();
    if (!itsa) throw new Error("ITSA4");
    expect(methodValue(itsa, "graham")).toEqual({ value: 9.5, gap: null });
    expect(methodValue(itsa, "nav")).toEqual({ value: null, gap: "no_reference" });
    expect(methodValue(fund("KNRI11"), "fund_yield").value).toBe(137.21);
    expect(methodValue(fund("KNRI11"), "combined").value).toBe(98.4);
    expect(methodValue(fund("KNRI11"), "bazin")).toEqual({ value: null, gap: "no_reference" });
  });

  it("a escolha em edição vai e volta do teto gravado", () => {
    expect(draftOf(null, 11)).toEqual({ kind: "none" });
    expect(draftOf({ method: "manual", value: 30, unavailable_reason: null }, 11)).toEqual({ kind: "manual", value: 30 });
    expect(draftOf({ method: "graham", value: null, unavailable_reason: "loss" }, 11)).toEqual({ kind: "method", method: "graham" });
    expect(choiceOf({ kind: "none" })).toBeNull();
    expect(choiceOf({ kind: "manual", value: 30 })).toEqual({ method: "manual", value: 30 });
    expect(choiceOf({ kind: "method", method: "nav" })).toEqual({ method: "nav" });
    const graham = { method: "graham" as const, value: 9.5, unavailable_reason: null };
    expect(draftChanges({ kind: "method", method: "graham" }, graham)).toBe(false);
    expect(draftChanges({ kind: "method", method: "bazin" }, graham)).toBe(true);
    expect(draftChanges({ kind: "none" }, graham)).toBe(true);
    expect(draftChanges({ kind: "none" }, null)).toBe(false);
    const manual = { method: "manual" as const, value: 30, unavailable_reason: null };
    expect(draftChanges({ kind: "manual", value: 30 }, manual)).toBe(false);
    expect(draftChanges({ kind: "manual", value: 31 }, manual)).toBe(true);
    expect(ceilingLabel(null)).toBe("Sem teto");
    expect(ceilingLabel(graham)).toBe("Graham");
    expect(ceilingLabel(manual)).toBe("Valor próprio");
    expect(methodPhrase("combined")).toBe("pelas duas referências");
    expect(methodPhrase("fund_yield")).toBe("pelo Rendimento");
  });

  it("a conta do Rendimento usa os números de verdade de cada tipo", () => {
    const market = defaultMarket();
    expect(nbsp(fundYieldMath(fund("KNRI11"), market, shown))).toBe(
      "alvo do tijolo 7,12% + 2,5 p.p. = 9,62%; proventos 12m R$ 13,20 ÷ 9,62% = R$ 137,21",
    );
    expect(nbsp(fundYieldMath(fund("MXRF11"), market, shown))).toBe(
      "alvo do papel: CDI 14,20% × 0,85 = 12,07% + 1,0 p.p. = 13,07%; proventos 12m R$ 1,20 ÷ 13,07% = R$ 9,18",
    );
    expect(fundYieldMath(fund("HIBR11"), market, shown)).toContain("60% tijolo (9,62%) + 40% papel (13,07%) = 11,00%");
    expect(fundYieldMath(fund("FOFF11"), market, shown)).toContain(
      "média dos alvos de tijolo (9,62%) e papel (13,07%) = 11,35%, porque a carteira dos fundos de baixo não é conhecida",
    );
    // Sem Rendimento, sem conta.
    expect(fundYieldMath(fund("TRXF11"), market, shown)).toBeNull();
    // Sem a taxa, o alvo aparece sozinho.
    const noRate = { ...market, real_rate: null, brick_target: null };
    expect(fundYieldMath(fund("KNRI11"), noRate, shown)).toContain("alvo do tijolo 9,62%");
    // Olho fechado: os percentuais ficam, os reais somem.
    const hidden = fundYieldMath(fund("KNRI11"), market, createMoney(true));
    expect(hidden).toContain(`proventos 12m ${MASK_BRL} ÷ 9,62% = ${MASK_BRL}`);
  });

  it("as taxas dizem o valor, a data e de onde vêm; sem elas, que ainda não chegaram", () => {
    const market = defaultMarket();
    expect(realRateText(market)).toBe("juro real 7,12% em 19/11/2026 (Tesouro IPCA+ com Juros Semestrais 2037)");
    expect(cdiText(market)).toBe("CDI de 12 meses 14,20% até 18/11/2026, 12,07% depois dos 15% de IR");
    expect(realRateText({ ...market, real_rate: null })).toBe("juro real do Tesouro ainda não lido");
    expect(cdiText({ ...market, cdi_12m: null })).toBe("CDI de 12 meses ainda não guardado");
  });
});
