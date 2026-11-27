import { describe, expect, it } from "vitest";
import type { Asset } from "../api/types";
import { buildComposition, groupLabel, leavesByClass, NO_SECTOR, NO_SEGMENT } from "./composition";

let n = 0;
const asset = (over: Partial<Asset>): Asset => {
  n += 1;
  return {
    id: `a${n}`,
    asset_class: "br_stocks",
    symbol: `TST${n}`,
    display_name: `Teste ${n}`,
    current_value: 100,
    sector: null,
    subsector: null,
    segment: null,
    ...over,
  } as Asset;
};

const assets = [
  asset({ id: "wege", symbol: "WEGE3", current_value: 400, sector: "Industrial", segment: "Máquinas" }),
  asset({ id: "itsa", symbol: "ITSA4", current_value: 300, sector: "Financeiro", segment: "Holdings" }),
  asset({ id: "bbas", symbol: "BBAS3", current_value: 100, sector: "Financeiro", segment: "Bancos" }),
  asset({ id: "knri", symbol: "KNRI11", asset_class: "br_reits", current_value: 150, sector: "Tijolo", segment: "Multicategoria" }),
  asset({ id: "btc", symbol: "BTC", asset_class: "crypto", current_value: 50 }),
  asset({ id: "zero", symbol: "ZERO3", current_value: 0 }),
];

describe("composição da carteira", () => {
  it("monta carteira → classe → setor, da maior fatia para a menor, ignorando o que não tem valor", () => {
    const c = buildComposition(assets, "sector", false);
    expect(c.total).toBe(1000);
    expect(c.headings).toEqual(["Carteira", "Classe", "Setor"]);
    expect(c.nodes.map((x) => x.label)).toEqual([
      "Carteira",
      "Ações Nacionais",
      "Financeiro",
      "Industrial",
      "Fundos Imobiliários",
      "Tijolo",
      "Criptomoedas",
      "Cripto",
    ]);
    expect(c.nodes[1]?.percent).toBe(80);
    const financial = c.nodes.find((x) => x.label === "Financeiro");
    expect(financial?.value).toBe(400);
    expect(financial?.parent).toBe("class:br_stocks");
    expect(financial?.assets).toBe(2);
  });

  it("no nível segmento pula o setor; no de ativo, desce até o papel", () => {
    const segment = buildComposition(assets, "segment", false);
    expect(segment.headings).toEqual(["Carteira", "Classe", "Segmento"]);
    expect(segment.nodes.filter((x) => x.column === 2).map((x) => x.label)).toEqual(["Máquinas", "Holdings", "Bancos", "Multicategoria", "Cripto"]);

    const deep = buildComposition(assets, "asset", false);
    expect(deep.headings).toEqual(["Carteira", "Classe", "Setor", "Ativo"]);
    expect(deep.nodes.filter((x) => x.column === 3).map((x) => x.label)).toEqual(["ITSA4", "BBAS3", "WEGE3", "KNRI11", "BTC"]);
    // Cada coluna soma o total da carteira.
    for (const column of [0, 1, 2, 3]) {
      expect(deep.nodes.filter((x) => x.column === column).reduce((s, x) => s + x.value, 0)).toBe(1000);
    }
  });

  it("junta as fatias pequenas do último nível em Outros, só se forem duas ou mais", () => {
    const many = [
      asset({ id: "big", symbol: "BIG3", current_value: 1000, sector: "S" }),
      asset({ id: "p1", symbol: "P1", current_value: 4, sector: "S" }),
      asset({ id: "p2", symbol: "P2", current_value: 3, sector: "S" }),
      asset({ id: "p3", symbol: "P3", current_value: 5, asset_class: "crypto" }),
    ];
    const collapsed = buildComposition(many, "asset", true);
    const others = collapsed.nodes.find((x) => x.kind === "others");
    expect(others?.label).toBe("Outros (2)");
    expect(others?.value).toBe(7);
    expect(others?.assets).toBe(2);
    expect(collapsed.nodes.some((x) => x.label === "P1")).toBe(false);
    // Um papel pequeno sozinho continua com o nome dele.
    expect(collapsed.nodes.some((x) => x.label === "P3")).toBe(true);
    // Sem juntar, tudo aparece.
    expect(buildComposition(many, "asset", false).nodes.some((x) => x.label === "P1")).toBe(true);
  });

  it("sem posição com valor, a composição é vazia", () => {
    expect(buildComposition([asset({ current_value: 0 })], "asset", true)).toEqual({
      total: 0,
      headings: ["Carteira", "Classe", "Setor", "Ativo"],
      nodes: [],
    });
  });

  it("agrupa as folhas de cada classe para as barras", () => {
    const groups = leavesByClass(buildComposition(assets, "sector", false));
    expect(groups.map((g) => g.node.label)).toEqual(["Ações Nacionais", "Fundos Imobiliários", "Criptomoedas"]);
    expect(groups[0]?.leaves.map((l) => l.label)).toEqual(["Financeiro", "Industrial"]);
  });
});

describe("agrupamento das classes sem setor da B3", () => {
  it("usa o tipo do título na renda fixa e o tipo do papel no exterior", () => {
    const bond = (symbol: string) => asset({ asset_class: "fixed_income", treasury: true, symbol });
    expect(groupLabel(bond("TD-SELIC-20290301"), "sector")).toBe("Tesouro Selic");
    expect(groupLabel(bond("TD-PRE-20290101"), "sector")).toBe("Tesouro Prefixado");
    expect(groupLabel(bond("TD-IPCA-20350515"), "segment")).toBe("Tesouro IPCA+");
    expect(groupLabel(bond("TD-IPCAJS-20600815"), "sector")).toBe("Tesouro IPCA+ com juros");
    expect(groupLabel(bond("TD-XX"), "sector")).toBe("Tesouro Direto");
    expect(groupLabel(asset({ asset_class: "fixed_income", symbol: null }), "sector")).toBe("Renda fixa por valor");
    expect(groupLabel(asset({ asset_class: "fixed_income", listed_on_b3: true }), "sector")).toBe("ETF de renda fixa");
    expect(groupLabel(asset({ asset_class: "intl_fixed_income" }), "sector")).toBe("Renda fixa internacional");
    expect(groupLabel(asset({ asset_class: "crypto" }), "segment")).toBe("Cripto");
    expect(groupLabel(asset({ asset_class: "us_reits" }), "sector")).toBe("REITs");
    expect(groupLabel(asset({ asset_class: "intl_stocks", sector: "Tecnologia" }), "sector")).toBe("Tecnologia");
    expect(groupLabel(asset({ asset_class: "intl_stocks" }), "sector")).toBe("Ações e ETFs do exterior");
    expect(groupLabel(asset({ asset_class: "intl_stocks", listed_on_b3: true }), "sector")).toBe("BDRs e ETFs na B3");
  });

  it("Sem setor e Sem segmento ficam só para ação e FII sem classificação", () => {
    expect(groupLabel(asset({ asset_class: "br_stocks" }), "sector")).toBe(NO_SECTOR);
    expect(groupLabel(asset({ asset_class: "br_reits" }), "segment")).toBe(NO_SEGMENT);
  });
});
