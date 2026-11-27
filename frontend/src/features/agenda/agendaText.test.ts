import { describe, expect, it } from "vitest";
import type { AgendaDocument, AgendaEvent } from "../../api/agendaTypes";
import { createMoney, MASK_BRL } from "../../lib/money";
import { ITSA, KNRI, sampleMonth } from "../../test/agendaHandlers";
import {
  addMonths,
  ALL_TYPES,
  assetNames,
  badgeLabel,
  byDay,
  calendarWeeks,
  categoryLabel,
  changePercent,
  dayHeading,
  dayLong,
  daysIn,
  formatUnit,
  fullDate,
  monthOf,
  monthRange,
  monthTitle,
  passes,
  resultsTitle,
  shortDate,
  todayInBrazil,
  unitText,
  unusualTitle,
  usualText,
} from "./agendaText";

const doc = (fields: Partial<AgendaDocument>): AgendaDocument => ({
  category: "results",
  source: "cvm_filing",
  title: null,
  subject: null,
  reference_date: null,
  version: 1,
  url: null,
  filing: null,
  ...fields,
});

describe("datas da agenda", () => {
  it("monta o mês de domingo a sábado sem passar pelo fuso", () => {
    // Novembro de 2026 começa num domingo; dezembro, numa terça.
    const november = calendarWeeks({ year: 2026, month: 11 });
    expect(november[0]?.[0]).toBe("2026-11-01");
    expect(november).toHaveLength(5);
    const december = calendarWeeks({ year: 2026, month: 12 });
    expect(december[0]?.slice(0, 3)).toEqual([null, null, "2026-12-01"]);
    expect(december.at(-1)?.at(-1)).toBeNull();
    expect(daysIn({ year: 2028, month: 2 })).toBe(29);
    expect(monthRange({ year: 2026, month: 2 })).toEqual({ from: "2026-02-01", to: "2026-02-28" });
  });

  it("anda de mês em mês, inclusive na virada do ano", () => {
    expect(addMonths({ year: 2026, month: 12 }, 1)).toEqual({ year: 2027, month: 1 });
    expect(addMonths({ year: 2026, month: 1 }, -1)).toEqual({ year: 2025, month: 12 });
    expect(addMonths({ year: 2026, month: 11 }, 14)).toEqual({ year: 2028, month: 1 });
    expect(monthOf("2026-11-27")).toEqual({ year: 2026, month: 11 });
  });

  it("escreve as datas em português", () => {
    expect(monthTitle({ year: 2026, month: 11 })).toBe("novembro de 2026");
    expect(dayHeading("2026-12-02")).toMatch(/qua.*2.*dez/);
    expect(dayLong("2026-12-02")).toBe("quarta-feira, 2 de dezembro");
    expect(shortDate("2026-12-02")).toBe("02/12");
    expect(fullDate("2026-12-02")).toBe("02/12/2026");
    // 01h de 28/11 em UTC ainda é 27/11 em Brasília.
    expect(todayInBrazil(new Date("2026-11-28T01:00:00Z"))).toBe("2026-11-27");
  });
});

describe("textos dos eventos", () => {
  it("diz o corte e o aumento pelo valor absoluto", () => {
    expect(unusualTitle("cut", -0.25)).toBe("Corte de 25%");
    expect(unusualTitle("increase", 0.714)).toBe("Aumento de 71%");
    expect(changePercent(-0.3333)).toBe("33%");
  });

  it("mostra o valor por cota com até quatro casas e o esconde com o olho fechado", () => {
    expect(formatUnit(0.0182)).toMatch(/0,0182/);
    expect(formatUnit(0.6)).toMatch(/0,60/);
    const visible = createMoney(false);
    const hidden = createMoney(true);
    expect(unitText(hidden, 0.6)).toBe(MASK_BRL);
    const mark = { recent: { usual: 0.8, change: -0.25, payments: 12 }, long: { usual: 0.79, change: -0.24, payments: 58 }, years: 5 };
    expect(usualText(visible, mark)).toMatch(/0,80.*12 meses e .*0,79 em 5 anos/);
    expect(usualText(hidden, { ...mark, long: null })).toBe(`${MASK_BRL} nos 12 meses`);
  });

  it("nomeia o resultado e o documento pela fonte", () => {
    expect(resultsTitle(doc({ filing: "quarterly", reference_date: "2026-09-30" }))).toBe("ITR do trimestre até 30/09/2026");
    expect(resultsTitle(doc({ filing: "annual", reference_date: "2025-12-31" }))).toBe("DFP de 2025");
    expect(resultsTitle(doc({ filing: "annual" }))).toBe("DFP");
    expect(resultsTitle(doc({ filing: "quarterly" }))).toBe("ITR");
    expect(resultsTitle(doc({ source: "fundos_net", title: "Demonstrações Financeiras" }))).toBe("Demonstrações Financeiras");
    expect(resultsTitle(doc({ source: "fundos_net" }))).toBe("Demonstrações do fundo");
    expect(categoryLabel(doc({ category: "shareholder_notice", source: "fundos_net" }))).toBe("Aviso aos cotistas");
    expect(categoryLabel(doc({ category: "shareholder_notice", source: "cvm_ipe" }))).toBe("Aviso aos acionistas");
  });
});

describe("filtros", () => {
  const events = sampleMonth("2026-11-01");

  it("filtra por tipo, classe e ativo; o dia sem pregão só sai pelo tipo", () => {
    const all = { types: ALL_TYPES, assetClass: null, assetId: null };
    expect(events.filter((e) => passes(e, all))).toHaveLength(events.length);
    const stocks = events.filter((e) => passes(e, { ...all, assetClass: "br_stocks" }));
    expect(stocks.every((e) => e.type === "market_closed" || e.assets.some((a) => a.asset_id === ITSA.asset_id))).toBe(true);
    const knri = events.filter((e) => passes(e, { ...all, assetId: KNRI.asset_id }));
    expect(knri.map((e) => e.type)).toEqual(["market_closed", "income_payment", "notice"]);
    const noClosed = events.filter((e) => passes(e, { ...all, types: new Set(["notice" as const]) }));
    expect(noClosed.map((e) => e.type)).toEqual(["notice"]);
  });

  it("agrupa por dia e escreve quem o evento cita", () => {
    const days = byDay([...events].reverse());
    expect([...days.keys()][0]).toBe("2026-11-02");
    const [, second] = events;
    expect(second).toBeDefined();
    const two: AgendaEvent = { ...(second as AgendaEvent), assets: [ITSA, { ...ITSA, asset_id: "a-itsa3", symbol: "ITSA3" }] };
    expect(assetNames(two)).toBe("ITSA4, ITSA3");
    const labels = events.map(badgeLabel);
    expect(labels).toContain("Assembleia");
    expect(labels).toContain("Fora do padrão");
    expect(labels).not.toContain("Comunicados e avisos");
  });
});
