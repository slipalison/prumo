import { describe, expect, it } from "vitest";
import type { Asset } from "../api/types";
import { ASSET_CLASSES } from "./assetClasses";
import { classDays, compare, dayMovers, directionWord, monthName, patrimonyChange, previousMonthStart, signDirection, topMovers } from "./panel";

const asset = (over: Partial<Asset>): Asset =>
  ({
    id: "a",
    asset_class: "br_stocks",
    symbol: "AAA3",
    display_name: "Aaa",
    quantity: 10,
    unit_price: 11,
    price_source: "quote",
    ...over,
  }) as Asset;

describe("sentido", () => {
  it("compara com tolerância e escreve a palavra", () => {
    expect(compare(10, 9)).toBe("above");
    expect(compare(9, 10)).toBe("below");
    expect(compare(10.001, 10)).toBe("equal");
    expect(directionWord("above")).toBe("acima");
    expect(directionWord("below")).toBe("abaixo");
    expect(directionWord("equal")).toBe("igual");
    expect(signDirection(-3)).toBe("below");
    expect(signDirection(0)).toBe("equal");
  });
});

describe("patrimônio no mês", () => {
  it("é a diferença e o percentual sobre o fim do mês anterior", () => {
    expect(patrimonyChange(1100, 1000)).toEqual({ delta: 100, percent: 10 });
    expect(patrimonyChange(900, 1000)).toEqual({ delta: -100, percent: -10 });
  });

  it("sem base (ou base zero) não há comparação", () => {
    expect(patrimonyChange(1000, null)).toBeNull();
    expect(patrimonyChange(1000, 0)).toBeNull();
  });
});

describe("meses", () => {
  it("dá o nome do mês e o anterior, virando o ano", () => {
    expect(monthName("2026-11-01")).toBe("novembro");
    expect(monthName("2026-03-01")).toBe("março");
    expect(previousMonthStart("2026-11-01")).toBe("2026-10-01");
    expect(previousMonthStart("2027-01-01")).toBe("2026-12-01");
  });
});

describe("o dia dos ativos", () => {
  const refs = [
    { asset_id: "up", previous_close: 10, date: "2026-11-24" },
    { asset_id: "down", previous_close: 20, date: "2026-11-24" },
    { asset_id: "flat", previous_close: 5, date: "2026-11-24" },
    { asset_id: "manual", previous_close: 5, date: "2026-11-24" },
    { asset_id: "empty", previous_close: 5, date: "2026-11-24" },
  ];
  const assets = [
    asset({ id: "up", symbol: "UP3", quantity: 10, unit_price: 11 }),
    asset({ id: "down", symbol: "DN3", asset_class: "br_reits", quantity: 5, unit_price: 19 }),
    asset({ id: "flat", symbol: "FL3", quantity: 4, unit_price: 5.001 }),
    asset({ id: "manual", price_source: "manual", unit_price: 6 }),
    asset({ id: "empty", quantity: 0, unit_price: 6 }),
    asset({ id: "norefs", unit_price: 6 }),
  ];

  it("mede só ativo com posição, cotação e referência", () => {
    const movers = dayMovers(assets, refs);
    expect(movers.map((m) => m.id)).toEqual(["up", "down", "flat"]);
    const up = movers[0];
    expect(up?.changePercent).toBeCloseTo(10);
    expect(up?.delta).toBeCloseTo(10);
    expect(up?.previousValue).toBe(100);
    expect(movers[1]?.changePercent).toBeCloseTo(-5);
    expect(movers[1]?.delta).toBeCloseTo(-5);
  });

  it("ordena altas e baixas e deixa de fora o que a tela mostra como 0,0%", () => {
    const movers = dayMovers(assets, refs);
    const { gainers, losers } = topMovers(movers, 5);
    expect(gainers.map((m) => m.label)).toEqual(["UP3"]);
    expect(losers.map((m) => m.label)).toEqual(["DN3"]);
    expect(topMovers(movers, 0)).toEqual({ gainers: [], losers: [] });
  });

  it("soma o dia por classe sobre a posição do fechamento anterior", () => {
    const days = classDays(dayMovers(assets, refs), ASSET_CLASSES);
    expect(days.map((d) => d.assetClass)).toEqual(["br_stocks", "br_reits"]);
    // Ações: +10,00 (UP) +0,004 (FL) sobre 100 + 20.
    expect(days[0]?.delta).toBeCloseTo(10.004);
    expect(days[0]?.percent).toBeCloseTo((10.004 / 120) * 100);
    expect(days[1]?.percent).toBeCloseTo(-5);
    expect(days[1]?.count).toBe(1);
  });
});
