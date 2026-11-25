import { describe, expect, it } from "vitest";
import type { HealthWarning } from "../api/healthTypes";
import type { Asset } from "../api/types";
import { defaultAssets, defaultChecklists, toAsset } from "../test/fixtures";
import {
  COUNT_MAX,
  formatRuleValue,
  limitText,
  ruleName,
  thresholdError,
  warningAction,
  warningMembers,
  warningTitle,
} from "./healthText";
import { createMoney } from "./money";
import { hasSectorSource, sectorHeading, sectorSlices } from "./sectors";

const warning = (patch: Partial<HealthWarning> & Pick<HealthWarning, "rule" | "subject">): HealthWarning => ({
  unit: "percent",
  current: 31.04,
  limit: 15,
  needed_contribution: 10_335,
  ...patch,
});

const act = (w: HealthWarning, hidden = false) => warningAction(w, createMoney(hidden));

const classWarning = (rule: "min_assets" | "empty_class", assetClass: "br_stocks" | "br_reits" | "crypto" | "intl_stocks", current: number, limit: number) =>
  warning({ rule, unit: "count", current, limit, needed_contribution: null, subject: { kind: "class", asset_class: assetClass } });

describe("texto dos avisos", () => {
  it("formata percentual com as casas que importam e contagem inteira", () => {
    expect(formatRuleValue("percent", 15)).toBe("15%");
    expect(formatRuleValue("percent", 31.04)).toBe("31,04%");
    expect(formatRuleValue("count", 3)).toBe("3");
  });

  it("diz o que é cada aviso", () => {
    expect(warningTitle(warning({ rule: "max_asset", subject: { kind: "asset", asset_id: "a", label: "ITUB4" } }))).toBe(
      "ITUB4 é 31,04% da carteira",
    );
    expect(
      warningTitle(warning({ rule: "top_five", subject: { kind: "top_assets", labels: ["A", "B", "C", "D", "E"] } })),
    ).toBe("Os 5 maiores ativos somam 31,04% da carteira");
    expect(
      warningTitle(warning({ rule: "max_sector", subject: { kind: "sector", asset_class: "br_stocks", sector: "Financeiro" } })),
    ).toBe("Financeiro é 31,04% de Ações Nacionais");
    expect(warningTitle(classWarning("min_assets", "br_reits", 1, 3))).toBe("Fundos Imobiliários tem 1 ativo");
    expect(warningTitle(classWarning("min_assets", "crypto", 2, 3))).toBe("Criptomoedas tem 2 ativos");
    expect(warningTitle(classWarning("empty_class", "intl_stocks", 0, 1))).toBe(
      "Ações Internacionais tem meta e ainda nada comprado",
    );
  });

  it("diz o limite e lista os cinco maiores", () => {
    const asset = warning({ rule: "max_asset", subject: { kind: "asset", asset_id: "a", label: "X" } });
    expect(limitText(asset)).toBe("Seu limite: 15%");
    expect(limitText(classWarning("min_assets", "crypto", 1, 3))).toBe("Seu mínimo: 3 ativos");
    expect(limitText(classWarning("empty_class", "crypto", 0, 1))).toBe("Mínimo: 1 ativo");
    expect(warningMembers(asset)).toBeNull();
    expect(warningMembers(warning({ rule: "top_five", subject: { kind: "top_assets", labels: ["A", "B"] } }))).toBe("A, B");
  });

  it("diz o que fazer sem mandar vender", () => {
    const asset = act(warning({ rule: "max_asset", subject: { kind: "asset", asset_id: "a", label: "ITUB4" } }));
    expect(asset.text).toMatch(/ITUB4 voltar ao limite sem vender nada, aporte cerca de R\$\s10,3\smil em outros ativos/);
    expect(asset.link).toEqual({ href: "#/aportar", label: "Calcular o próximo aporte" });
    expect(act(warning({ rule: "top_five", subject: { kind: "top_assets", labels: ["A"] } })).text).toMatch(
      /em ativos fora deles/,
    );
    const sector = act(
      warning({ rule: "max_sector", subject: { kind: "sector", asset_class: "br_reits", sector: "Tijolo" } }),
    );
    expect(sector.text).toMatch(/Tijolo voltar ao limite.*outros setores de Fundos Imobiliários/);
    const shallow = act(classWarning("min_assets", "br_stocks", 1, 3));
    expect(shallow.text).toBe("Adicione mais 2 ativos em Ações Nacionais.");
    expect(shallow.link?.href).toBe("#/carteira");
    expect(act(classWarning("min_assets", "br_stocks", 2, 3)).text).toBe("Adicione mais 1 ativo em Ações Nacionais.");
    expect(act(classWarning("empty_class", "crypto", 0, 1)).text).toMatch(/Cadastre um ativo em Criptomoedas/);
  });

  it("com o olho fechado, o valor em reais some do texto e o resto fica", () => {
    const asset = act(warning({ rule: "max_asset", subject: { kind: "asset", asset_id: "a", label: "ITUB4" } }), true);
    expect(asset.text).toBe("Para ITUB4 voltar ao limite sem vender nada, aporte cerca de R$ •••• em outros ativos.");
    expect(asset.text).not.toMatch(/10,3/);
  });

  it("nomeia a regra para leitor de tela", () => {
    expect(ruleName("max_asset")).toBe("Nenhum ativo acima de … da carteira");
    expect(ruleName("empty_class")).toBe("Toda classe com meta tem ativo");
  });

  it("valida o limite como o servidor", () => {
    expect(thresholdError("percent", 15)).toBeNull();
    expect(thresholdError("percent", 12.5)).toBeNull();
    expect(thresholdError("percent", 100)).toBeNull();
    expect(thresholdError("percent", 0)).not.toBeNull();
    expect(thresholdError("percent", 100.5)).not.toBeNull();
    expect(thresholdError("percent", 10.123)).not.toBeNull();
    expect(thresholdError("percent", null)).toBe("Digite um número.");
    expect(thresholdError("count", 3)).toBeNull();
    expect(thresholdError("count", 2.5)).not.toBeNull();
    expect(thresholdError("count", 0)).not.toBeNull();
    expect(thresholdError("count", 51)).not.toBeNull();
    expect(thresholdError("count", 12, COUNT_MAX.income_every_month)).toBeNull();
    expect(thresholdError("count", 13, COUNT_MAX.income_every_month)).toBe("Use um número inteiro de 1 a 12.");
  });

  it("diz as regras de renda: título, limite, meses vazios e o que fazer", () => {
    const gaps = warning({
      rule: "income_every_month",
      unit: "count",
      current: 1,
      limit: 12,
      needed_contribution: null,
      subject: {
        kind: "income_months",
        empty_months: ["2025-12", "2026-01"],
        payers: [{ asset_id: "a", label: "KNRI11", asset_class: "br_reits", months: [12] }],
      },
    });
    expect(warningTitle(gaps)).toBe("Você recebeu provento em 1 dos últimos 12 meses");
    expect(limitText(gaps)).toBe("Seu mínimo: 12 meses");
    expect(limitText({ ...gaps, limit: 1 })).toBe("Seu mínimo: 1 mês");
    expect(warningMembers(gaps)).toBe("Meses sem provento: dez/2025, jan/2026");
    expect(act(gaps).text).toBe(
      "Pelo histórico, já pagaram nesses meses: KNRI11 (Fundos Imobiliários; já pagou em dez). Aportar neles ajuda a cobrir os meses vazios.",
    );
    expect(act(gaps).link).toEqual({ href: "#/renda", label: "Abrir a Renda" });

    const payer = warning({
      rule: "max_payer",
      current: 80,
      limit: 25,
      needed_contribution: null,
      subject: { kind: "payer", asset_id: null, label: "OLD3", received: 800, total: 1000 },
    });
    expect(warningTitle(payer)).toBe("OLD3 pagou 80% da renda de 12 meses");
    expect(limitText(payer)).toBe("Seu limite: 25%");
    expect(warningMembers(payer)).toBeNull();
    expect(act(payer).text).toMatch(/OLD3 pagou R\$\s800,00 dos R\$\s1\smil que você recebeu em 12 meses/);
    expect(act(payer, true).text).toBe(
      "OLD3 pagou R$ •••• dos R$ •••• que você recebeu em 12 meses. Para espalhar, aporte em outros ativos da carteira que paguem renda.",
    );
  });
});

describe("setor na carteira", () => {
  const assets = (): Asset[] => {
    const checklists = defaultChecklists();
    return defaultAssets().map((a) => toAsset(a, checklists, 1));
  };

  it("só Ações Nacionais e FIIs têm fonte de setor", () => {
    expect(hasSectorSource("br_stocks")).toBe(true);
    expect(hasSectorSource("br_reits")).toBe(true);
    expect(hasSectorSource("intl_stocks")).toBe(false);
    expect(sectorHeading("br_reits")).toBe("Por tipo de fundo");
    expect(sectorHeading("br_stocks")).toBe("Por setor");
  });

  it("agrupa os FIIs por tipo e separa os que ficaram sem setor", () => {
    const slices = sectorSlices(assets(), "br_reits");
    expect(slices.map((s) => [s.label, s.kind])).toEqual([
      ["Tijolo", "sector"],
      ["Papel", "sector"],
      ["Sem setor", "none"],
    ]);
    expect(slices[0]?.color).toBe("var(--sector-1)");
    expect(slices[2]?.color).toBe("var(--sector-none)");
    // Tijolo: KNRI 1420 + HGLG 1689,6 + BTLG 1418,25 de 6441,05 (com Papel 837 e TRXF 1076,2).
    expect(slices[0]?.percent).toBeCloseTo(70.3, 1);
    expect(slices.reduce((acc, s) => acc + s.percent, 0)).toBeCloseTo(100, 1);
  });

  it("junta o que passa de sete setores em Outros setores", () => {
    const base = assets().find((a) => a.asset_class === "br_stocks");
    if (!base) throw new Error("sem ação na carteira de teste");
    const many = Array.from({ length: 9 }, (_, i) => ({
      ...base,
      id: `s${i}`,
      sector: `Setor ${i}`,
      current_value: 1000 - i * 10,
    }));
    const slices = sectorSlices(many, "br_stocks");
    expect(slices).toHaveLength(8);
    expect(slices.at(-1)).toMatchObject({ label: "Outros setores", kind: "other", color: "var(--sector-other)" });
    expect(slices.filter((s) => s.kind === "sector")).toHaveLength(7);
  });

  it("ignora ativo sem valor e classe sem nada comprado", () => {
    const base = assets().find((a) => a.asset_class === "br_stocks");
    if (!base) throw new Error("sem ação na carteira de teste");
    expect(sectorSlices([{ ...base, current_value: 0 }], "br_stocks")).toEqual([]);
    expect(sectorSlices(assets(), "crypto")).toEqual([]);
  });
});
