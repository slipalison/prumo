import { describe, expect, it } from "vitest";
import type { Asset } from "../api/types";
import { buildComposition } from "./composition";
import { estimateTextWidth, fitLabel, labelledIds, layoutSankey } from "./sankey";

const asset = (id: string, value: number, over: Partial<Asset> = {}): Asset =>
  ({ id, symbol: id.toUpperCase(), display_name: id, asset_class: "br_stocks", current_value: value, sector: "S", segment: "G", ...over }) as Asset;

const options = { width: 760, height: 300, nodeWidth: 10, gap: 5, labelRoom: 190 };

describe("layout do Sankey", () => {
  const composition = buildComposition([asset("a", 600), asset("b", 300), asset("c", 100, { asset_class: "crypto" })], "asset", false);
  const layout = layoutSankey(composition.nodes, options);

  it("dá a cada nó uma altura proporcional ao valor, na mesma escala", () => {
    const box = (id: string) => layout.boxes.find((b) => b.id === id);
    const a = box("class:br_stocks/S/a");
    const b = box("class:br_stocks/S/b");
    expect(a && b && a.height / b.height).toBeCloseTo(2);
    // A raiz é a soma das classes.
    const root = box("root");
    const classes = ["class:br_stocks", "class:crypto"].map((id) => box(id)?.height ?? 0);
    expect(root?.height).toBeCloseTo(classes.reduce((s, h) => s + h, 0), 1);
  });

  it("põe as colunas da esquerda para a direita, com folga para os rótulos", () => {
    const xs = [...new Set(layout.boxes.map((b) => b.x))].sort((p, q) => p - q);
    expect(xs).toHaveLength(4);
    expect(xs[0]).toBe(0);
    expect((xs.at(-1) ?? 0) + 10 + 190).toBeLessThanOrEqual(760);
    for (const box of layout.boxes) {
      expect(box.y).toBeGreaterThanOrEqual(-0.01);
      expect(box.y + box.height).toBeLessThanOrEqual(300.01);
    }
  });

  it("liga cada nó ao pai por uma faixa fechada, sem faixa para a raiz", () => {
    expect(layout.links).toHaveLength(composition.nodes.length - 1);
    expect(layout.links.every((l) => l.path.startsWith("M") && l.path.endsWith("Z"))).toBe(true);
    expect(layout.links.find((l) => l.to === "root")).toBeUndefined();
  });

  it("a faixa de cada filho sai de cima para baixo no pai, sem sobrepor", () => {
    const children = layout.links.filter((l) => l.from === "root");
    const tops = children.map((l) => Number(l.path.split(/[MC ]/)[2]));
    expect(tops).toHaveLength(2);
    expect(tops).toEqual([...tops].sort((p, q) => p - q));
  });

  it("sem nó, desenha nada", () => {
    expect(layoutSankey([], options)).toEqual({ width: 760, height: 300, step: 0, boxes: [], links: [] });
  });

  it("com uma coluna só, não quebra", () => {
    const only = layoutSankey(composition.nodes.slice(0, 1), options);
    expect(only.boxes).toHaveLength(1);
    expect(only.links).toEqual([]);
  });
});

describe("rótulos do Sankey", () => {
  it("corta o nome com reticências para caber e nunca corta a fatia", () => {
    const long = "Petróleo, Gás e Biocombustíveis";
    const fit = fitLabel(long, "0,5%", 150);
    expect(fit.truncated).toBe(true);
    expect(fit.text.endsWith("…")).toBe(true);
    expect(estimateTextWidth(`${fit.text} 0,5%`)).toBeLessThanOrEqual(150);
    expect(fitLabel("TAEE4", "0,9%", 150)).toEqual({ text: "TAEE4", truncated: false });
    expect(fitLabel("Nome", "100,0%", 10).text).toBe("N…");
  });

  it("em cada coluna fica o rótulo do maior e cai o que colidiria na vertical", () => {
    const many = [asset("a", 500), asset("b", 8), asset("c", 7), asset("d", 6), asset("e", 300)];
    const composition = buildComposition(many, "asset", false);
    const laid = layoutSankey(composition.nodes, options);
    const shown = labelledIds(composition.nodes, laid.boxes, 14);
    for (const column of [0, 1, 2, 3]) {
      const centers = composition.nodes
        .filter((n) => n.column === column && shown.has(n.id))
        .map((n) => {
          const box = laid.boxes.find((b) => b.id === n.id);
          return (box?.y ?? 0) + (box?.height ?? 0) / 2;
        })
        .sort((p, q) => p - q);
      for (let i = 1; i < centers.length; i += 1) expect((centers[i] ?? 0) - (centers[i - 1] ?? 0)).toBeGreaterThanOrEqual(14);
    }
    expect(shown.has("class:br_stocks/S/a")).toBe(true);
  });
});
