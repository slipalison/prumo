import { describe, expect, it } from "vitest";
import type { HeatCell, HistoryBucket, HistoryGroup } from "../api/incomeHistoryTypes";
import { heatmapFrom, historyFrom, sampleBase, sampleIncomes } from "../test/incomeHistoryHandlers";
import {
  bucketLong,
  bucketShort,
  bucketsInPeriod,
  groupLabel,
  heatDetail,
  heatLevel,
  heatMax,
  heatMoney,
  heatPartial,
  heatText,
  heatValue,
  intervalStart,
  labelStep,
  monthName,
  OTHERS_GROUP,
  segmentsOf,
  seriesOf,
  seriesTotals,
  shiftMonth,
} from "./incomeHistory";
import { createMoney } from "./money";

const visible = createMoney(false);
/** O espaço inseparável que o `Intl` põe depois do "R$". */
const NBSP = String.fromCharCode(160);
const nbsp = (text: string) => text.split(NBSP).join(" ");
const hidden = createMoney(true);

const group = (fields: Partial<HistoryGroup> & Pick<HistoryGroup, "key" | "kind">): HistoryGroup => ({
  asset_class: null,
  segment: null,
  asset_id: null,
  label: null,
  total: 0,
  ...fields,
});

const bucket = (start: string, parts: [string, number, number?][]): HistoryBucket => ({
  start,
  received: parts.reduce((acc, [, r]) => acc + r, 0),
  upcoming: parts.reduce((acc, [, , u]) => acc + (u ?? 0), 0),
  groups: parts.map(([key, received, upcoming]) => ({ key, received, upcoming: upcoming ?? 0 })),
});

const cell = (fields: Partial<HeatCell> = {}): HeatCell => ({
  amount: 0,
  upcoming: 0,
  market_value: null,
  dy: null,
  dy_partial: false,
  cost: null,
  yield_on_cost: null,
  yield_on_cost_partial: false,
  ...fields,
});

describe("intervalos", () => {
  it("anda mês a mês pela virada do ano", () => {
    expect(shiftMonth("2026-11", 2)).toBe("2027-01");
    expect(shiftMonth("2026-01", -1)).toBe("2025-12");
    expect(shiftMonth("2026-05", -23)).toBe("2024-06");
  });

  it("acha o começo do trimestre, do semestre e do ano como o servidor", () => {
    expect(intervalStart("2026-05", "month")).toBe("2026-05");
    expect(intervalStart("2026-05", "quarter")).toBe("2026-04");
    expect(intervalStart("2026-12", "quarter")).toBe("2026-10");
    expect(intervalStart("2026-07", "half_year")).toBe("2026-07");
    expect(intervalStart("2026-06", "half_year")).toBe("2026-01");
    expect(intervalStart("2026-11", "year")).toBe("2026-01");
  });

  it("diz o intervalo curto no eixo e por extenso no balão", () => {
    expect(bucketShort("2026-11", "month")).toBe("nov/26");
    expect(bucketShort("2026-10", "quarter")).toBe("4T26");
    expect(bucketShort("2026-07", "half_year")).toBe("2S26");
    expect(bucketShort("2026-01", "year")).toBe("2026");
    expect(bucketLong("2026-03", "month")).toBe("março de 2026");
    expect(bucketLong("2026-04", "quarter")).toBe("2º trimestre de 2026");
    expect(bucketLong("2026-01", "half_year")).toBe("1º semestre de 2026");
    expect(bucketLong("2026-01", "year")).toBe("2026");
    expect(monthName(2026, 0)).toBe("janeiro de 2026");
  });

  it("recorta o período a partir do intervalo de hoje, com o que vem depois", () => {
    const months = ["2024-10", "2025-11", "2025-12", "2026-11", "2026-12"].map((start) => bucket(start, []));
    const starts = (list: HistoryBucket[]) => list.map((b) => b.start);
    expect(starts(bucketsInPeriod(months, "month", "12m", "2026-11-25"))).toEqual(["2025-12", "2026-11", "2026-12"]);
    expect(starts(bucketsInPeriod(months, "month", "24m", "2026-11-25"))).toEqual(["2025-11", "2025-12", "2026-11", "2026-12"]);
    expect(starts(bucketsInPeriod(months, "month", "all", "2026-11-25"))).toHaveLength(5);
    // Trimestre: 12 meses atrás é dezembro de 2025, do 4º trimestre de 2025.
    const quarters = ["2025-07", "2025-10", "2026-01"].map((start) => bucket(start, []));
    expect(starts(bucketsInPeriod(quarters, "quarter", "12m", "2026-11-25"))).toEqual(["2025-10", "2026-01"]);
  });

  it("espaça os rótulos para não se encostarem", () => {
    expect(labelStep(60)).toBe(1);
    expect(labelStep(20)).toBe(3);
    expect(labelStep(0)).toBe(1);
  });
});

describe("grupos", () => {
  it("dá nome a classe, segmento, ativo e ativo apagado", () => {
    expect(groupLabel(group({ key: "total", kind: "total" }))).toBe("Proventos");
    expect(groupLabel(group({ key: "class:br_reits", kind: "class", asset_class: "br_reits" }))).toBe("FIIs");
    expect(groupLabel(group({ key: "s", kind: "segment", asset_class: "br_stocks", segment: "Bancos" }))).toBe("Ações BR · Bancos");
    expect(groupLabel(group({ key: "s", kind: "segment", asset_class: "br_stocks" }))).toBe("Ações BR · sem segmento");
    expect(groupLabel(group({ key: "s", kind: "segment", asset_class: "crypto" }))).toBe("Cripto");
    expect(groupLabel(group({ key: "asset:x", kind: "asset", label: "ITSA4" }))).toBe("ITSA4");
    expect(groupLabel(group({ key: "asset:x", kind: "asset" }))).toBe("asset:x");
    expect(groupLabel(group({ key: "removed", kind: "removed" }))).toBe("Ativos removidos");
    expect(groupLabel(group({ key: "class:?", kind: "class" }))).toBe("class:?");
    expect(groupLabel(group({ key: "segment:?", kind: "segment" }))).toBe("segment:?");
  });

  it("pinta a classe com a cor dela e o ativo removido de neutro", () => {
    const groups = [
      group({ key: "class:br_stocks", kind: "class", asset_class: "br_stocks", total: 30 }),
      group({ key: "removed", kind: "removed", total: 20 }),
      group({ key: "total", kind: "total", total: 1 }),
    ];
    const series = seriesOf(groups, [bucket("2026-01", [["class:br_stocks", 30], ["removed", 20], ["total", 1]])]);
    expect(series.map((s) => s.color)).toEqual(["var(--c-br_stocks)", "var(--text-subtle)", "var(--gold-text)"]);
  });

  it("segue a cor do grupo pela ordem do histórico inteiro e junta o resto em Outros", () => {
    const groups = Array.from({ length: 9 }, (_, i) => group({ key: `asset:${i}`, kind: "asset", label: `ATV${i}`, total: 100 - i }));
    const all = [bucket("2026-01", groups.map((g) => [g.key, g.total] as [string, number]))];
    const series = seriesOf(groups, all);
    expect(series).toHaveLength(7);
    expect(series[6]?.key).toBe(OTHERS_GROUP);
    expect(series[6]?.label).toBe("Outros (3)");
    expect(series[6]?.members).toEqual(["asset:6", "asset:7", "asset:8"]);
    // Num período em que o primeiro não pagou, os outros não trocam de cor.
    const later = seriesOf(groups, [bucket("2026-02", [["asset:1", 10]])]);
    expect(later).toHaveLength(1);
    expect(later[0]?.color).toBe(series[1]?.color);
  });

  it("soma as partes de cada série e os totais do período", () => {
    const groups = Array.from({ length: 8 }, (_, i) => group({ key: `asset:${i}`, kind: "asset", label: `ATV${i}`, total: 100 - i }));
    const series = seriesOf(groups, [bucket("2026-01", groups.map((g) => [g.key, 1] as [string, number]))]);
    const b = bucket("2026-01", [
      ["asset:0", 10, 5],
      ["asset:6", 3],
      ["asset:7", 4, 1],
    ]);
    const segments = segmentsOf(b, series);
    expect(segments.map((s) => [s.series.key, s.received, s.upcoming])).toEqual([
      ["asset:0", 10, 5],
      [OTHERS_GROUP, 7, 1],
    ]);
    const totals = seriesTotals([b, bucket("2026-02", [["asset:0", 2]])], series);
    expect(totals.get("asset:0")).toBe(17);
    expect(totals.get(OTHERS_GROUP)).toBe(8);
  });
});

describe("mapa de calor", () => {
  it("lê a métrica escolhida e a marca de parcial", () => {
    const c = cell({ amount: 61.25, dy: 0.825, dy_partial: true, yield_on_cost: 0.9, yield_on_cost_partial: false });
    expect(heatValue(c, "amount")).toBe(61.25);
    expect(heatValue(c, "dy")).toBe(0.825);
    expect(heatValue(c, "yoc")).toBe(0.9);
    expect(heatPartial(c, "dy")).toBe(true);
    expect(heatPartial(c, "yoc")).toBe(false);
    expect(heatPartial(c, "amount")).toBe(false);
  });

  it("gradua a cor pelo maior mês, sem nível para o que não tem valor", () => {
    expect(heatLevel(100, 100)).toBe(5);
    expect(heatLevel(1, 100)).toBe(1);
    expect(heatLevel(41, 100)).toBe(3);
    expect(heatLevel(0, 100)).toBe(0);
    expect(heatLevel(null, 100)).toBe(0);
    expect(heatLevel(10, 0)).toBe(0);
    const years = heatmapFrom(sampleIncomes(), { basis: "payment", value: "net" }, "2026-11-25", sampleBase).years;
    expect(heatMax(years, "amount")).toBe(240);
    expect(heatMax(years, "dy")).toBe(1.2);
  });

  it("escreve o valor curto na célula e a conta por extenso", () => {
    const c = cell({ amount: 1500, upcoming: 90, market_value: 20000, dy: 0.75, cost: 16000, yield_on_cost: 0.9375, yield_on_cost_partial: true });
    expect(nbsp(heatText(c, "amount", visible))).toBe("R$ 1,5 mil");
    expect(nbsp(heatMoney(274.9, visible))).toBe("R$ 275");
    expect(nbsp(heatMoney(12.34, visible))).toBe("R$ 12,34");
    expect(heatMoney(274.9, hidden)).toBe("R$ ••••");
    expect(heatText(c, "dy", visible)).toBe("0,75%");
    expect(heatText(cell(), "amount", visible)).toBe("–");
    expect(heatText(cell({ amount: 10 }), "dy", visible)).toBe("–");
    expect(nbsp(heatDetail(c, "amount", visible))).toBe("R$ 1.500,00, dos quais R$ 90,00 a receber");
    expect(nbsp(heatDetail(c, "dy", visible))).toBe("0,75%: R$ 1.500,00, dos quais R$ 90,00 a receber do patrimônio médio de R$ 20.000,00");
    expect(nbsp(heatDetail(c, "yoc", visible))).toContain("0,94% (parcial: parte do provento fica fora da conta)");
    expect(nbsp(heatDetail(c, "yoc", visible))).toContain("do custo médio de R$ 16.000,00");
    expect(heatDetail(cell({ amount: 10 }), "dy", visible)).toContain("sem foto do patrimônio no mês");
    expect(heatDetail(cell({ amount: 10 }), "yoc", visible)).toContain("sem custo no mês");
    expect(heatDetail(cell(), "dy", visible)).toBe("sem provento");
    // Olho fechado: o percentual fica, o R$ sai.
    expect(heatText(c, "amount", hidden)).toBe("R$ ••••");
    expect(heatText(c, "dy", hidden)).toBe("0,75%");
    expect(heatDetail(c, "dy", hidden)).not.toMatch(/\d\.\d{3}|\d,\d{2} a receber/);
  });
});

describe("o banco falso soma como o servidor", () => {
  it("põe o provento no mês da data com e separa o a receber", () => {
    const params = { interval: "month", basis: "record", group: "class", value: "net" } as const;
    const history = historyFrom(sampleIncomes(), params, "2026-11-25");
    expect(history.buckets[0]?.start).toBe("2025-11");
    const november = history.buckets.find((b) => b.start === "2026-11");
    expect(november?.upcoming).toBe(90);
    expect(history.groups.map((g) => g.key)).toEqual(["class:br_reits", "class:br_stocks", "removed"]);
  });

  it("deixa o ativo apagado fora do DY e diz que é parcial", () => {
    const years = heatmapFrom(sampleIncomes(), { basis: "payment", value: "net" }, "2026-11-25", sampleBase).years;
    const june = years[0]?.months[5];
    expect(june?.amount).toBe(45);
    expect(june?.dy).toBe(0);
    expect(june?.dy_partial).toBe(true);
    expect(years[0]?.total.dy_partial).toBe(true);
    // 2025 tem custo e não tem foto do patrimônio.
    expect(years[1]?.months[11]?.dy).toBeNull();
    expect(years[1]?.months[11]?.yield_on_cost).toBe(0.625);
    expect(heatmapFrom([], { basis: "payment", value: "net" }, "2026-11-25", sampleBase).years).toEqual([]);
  });
});
