import { describe, expect, it } from "vitest";
import type { Change } from "../../api/resultsTypes";
import { label, percent, sampleCompany } from "../../test/resultsHandlers";
import { axisDomain, bubblePoint, changeView, pointsText, quarterLabel, quarterShort, signedPercent } from "./resultsText";

describe("o texto dos resultados", () => {
  it("o trimestre pelo nome; fora do ano civil, pelos meses", () => {
    const calendar = { fiscal_quarter: 3, period_start: "2026-07-01", period_end: "2026-09-30" };
    expect(quarterLabel(calendar)).toBe("3º tri 2026");
    expect(quarterShort(calendar)).toBe("3T26");
    // Exercício de abril a março: o 2º trimestre vai de julho a setembro.
    const offset = { fiscal_quarter: 2, period_start: "2026-07-01", period_end: "2026-09-30" };
    expect(quarterLabel(offset)).toBe("jul–set/2026");
    expect(quarterShort(offset)).toBe("set/26");
  });

  it("cada variação tem texto próprio; prejuízo nunca vira percentual", () => {
    expect(changeView(percent(0.25), "profit")).toEqual({ text: "+25,0%", tone: "up", hint: null });
    expect(changeView(percent(-0.032), "top")).toEqual({ text: "−3,2%", tone: "down", hint: null });
    expect(changeView(percent(0), "top").tone).toBe("neutral");
    const kinds: Array<[Change["kind"], string, string]> = [
      ["left_loss", "saiu do prejuízo", "saiu do negativo"],
      ["entered_loss", "entrou no prejuízo", "ficou negativo"],
      ["smaller_loss", "prejuízo menor", "negativo menor"],
      ["larger_loss", "prejuízo maior", "negativo maior"],
    ];
    for (const [kind, profit, top] of kinds) {
      expect(changeView(label(kind), "profit").text).toBe(profit);
      expect(changeView(label(kind), "top").text).toBe(top);
    }
    expect(changeView(label("left_loss"), "profit").tone).toBe("up");
    expect(changeView(label("larger_loss"), "profit").tone).toBe("down");
    expect(changeView(label("unchanged"), "profit")).toEqual({ text: "igual", tone: "neutral", hint: null });
    expect(changeView(label("no_base"), "profit").hint).toMatch(/zero/);
    expect(changeView({ kind: "missing", value: null, gap: "holding" }, "profit")).toEqual({
      text: "—",
      tone: "missing",
      hint: "não se aplica a holding (o lucro vem das participações)",
    });
    expect(changeView({ kind: "missing", value: null, gap: null }, "profit").hint).toBe("sem número");
  });

  it("percentual com sinal e pontos de margem", () => {
    expect(signedPercent(0.0001)).toBe("0,0%");
    expect(signedPercent(5)).toBe("+500,0%");
    expect(pointsText(0.012)).toBe("+1,2 p.p.");
    expect(pointsText(-0.0371)).toBe("−3,7 p.p.");
    expect(pointsText(0.0001)).toBe("0,0 p.p.");
  });

  it("a bolha precisa das duas variações em percentual", () => {
    const company = sampleCompany();
    expect(bubblePoint(company, "year_ago")).toEqual({ x: 0.25, y: 0.25 });
    expect(bubblePoint({ ...company, year_ago: null }, "year_ago")).toEqual({ out: "sem o trimestre de um ano antes" });
    expect(bubblePoint({ ...company, previous_quarter: null }, "previous_quarter")).toEqual({ out: "sem o trimestre anterior" });
    const comparison = { period_end: "2025-09-30", top_line: percent(0.1), net_income: label("entered_loss"), net_margin_points: { value: null, gap: null } };
    expect(bubblePoint({ ...company, year_ago: comparison }, "year_ago")).toEqual({ out: "lucro: entrou no prejuízo" });
    expect(
      bubblePoint(
        { ...company, top_line: "intermediation_result", year_ago: { ...comparison, net_income: label("unchanged"), top_line: { kind: "missing", value: null, gap: null } } },
        "year_ago",
      ),
    ).toEqual({ out: "resultado da intermediação: sem número" });
  });

  it("o eixo sempre tem o zero, folga, e um teto de 200%", () => {
    const [lo, hi] = axisDomain([0.05, 0.2]);
    expect(lo).toBeLessThan(-0.1);
    expect(hi).toBeGreaterThan(0.2);
    const [capLo, capHi] = axisDomain([-9, 9]);
    expect(capLo).toBeGreaterThanOrEqual(-2.5);
    expect(capHi).toBeLessThanOrEqual(2.5);
    expect(axisDomain([])[0]).toBeLessThan(0);
  });
});
