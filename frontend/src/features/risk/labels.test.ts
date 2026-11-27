import { describe, expect, it } from "vitest";
import { placeLabels, type Mark } from "./labels";

const mark = (key: string, cx: number, cy: number, extra: Partial<Mark> = {}): Mark => ({
  key,
  label: key,
  kind: "asset",
  cx,
  cy,
  r: 6,
  ...extra,
});

describe("nomes das marcas do risco × retorno", () => {
  it("põe à direita quando cabe e vira para o outro lado na borda", () => {
    const sides = placeLabels([mark("ITSA4", 50, 50), mark("BTC", 395, 50)], 400, 200);
    expect(sides.get("ITSA4")).toBe("right");
    expect(sides.get("BTC")).toBe("left");
  });

  it("a carteira escolhe primeiro; quem não cabe fica sem nome à vista", () => {
    const marks = [
      mark("KNRI11", 100, 100, { weight: 1 }),
      mark("Sua carteira", 102, 100, { kind: "portfolio" }),
      mark("HGLG11", 100, 103, { weight: 2 }),
      mark("BTLG11", 101, 101, { weight: 0.5 }),
      mark("MXRF11", 99, 102, { weight: 0.4 }),
      mark("TRXF11", 100, 99, { weight: 0.3 }),
    ];
    const sides = placeLabels(marks, 400, 200);
    expect(sides.get("Sua carteira")).toBe("right");
    expect(sides.has("HGLG11")).toBe(true);
    expect(sides.size).toBeLessThan(marks.length);
  });

  it("em cima e embaixo quando os lados estão tomados", () => {
    const wall = [mark("A", 20, 100), mark("B", 380, 100)];
    const sides = placeLabels([...wall, mark("LONGO-DEMAIS-PARA-O-LADO", 200, 100, { weight: 9 })], 400, 200);
    expect(sides.get("LONGO-DEMAIS-PARA-O-LADO")).toBe("top");
    const bottom = placeLabels([mark("X", 120, 4, { weight: 1, label: "LONGO-DEMAIS-PARA-QUALQUER-LADO" })], 240, 200);
    expect(bottom.get("X")).toBe("bottom");
  });

  it("a carteira leva o nome mesmo por cima de uma bolha", () => {
    const marks = [
      mark("Sua carteira", 30, 100, { kind: "portfolio" }),
      mark("MXRF11", 46, 100, { weight: 1, r: 9 }),
      mark("BTLG11", 30, 82, { weight: 0.5 }),
      mark("TRXF11", 30, 118, { weight: 0.4 }),
    ];
    const sides = placeLabels(marks, 300, 200);
    expect(sides.get("Sua carteira")).toBe("right");
  });
});
