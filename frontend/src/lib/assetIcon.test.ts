/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ASSET_CLASSES } from "./assetClasses";
import { iconClass, iconKind, iconSubjectOfLabel, initialsOf } from "./assetIcon";

describe("initialsOf", () => {
  it("usa as duas primeiras letras do ticker", () => {
    expect(initialsOf("ITSA4")).toBe("IT");
    expect(initialsOf("knri11")).toBe("KN");
    expect(initialsOf("BRK.B")).toBe("BR");
    expect(initialsOf("3M")).toBe("3M");
  });

  it("sem ticker, usa as iniciais do nome, sem acento nem símbolo", () => {
    expect(initialsOf(null, "Caixa Reserva")).toBe("CR");
    expect(initialsOf(null, "Ótimo CDB do Banco")).toBe("OC");
    expect(initialsOf(null, "Poupança")).toBe("PO");
    expect(initialsOf(undefined, "  ")).toBe("•");
    expect(initialsOf(null, null)).toBe("•");
  });

  it("a chave de um título do Tesouro não é ticker: vale o nome", () => {
    expect(initialsOf("TD-IPCA-20350515", "Tesouro IPCA+ 2035")).toBe("TI");
  });
});

describe("iconKind e iconClass", () => {
  it("Tesouro tem ícone próprio, pela marca ou pela chave", () => {
    expect(iconKind({ symbol: "TD-SELIC-20290301", treasury: true })).toBe("treasury");
    expect(iconKind({ symbol: "TD-IPCA-20350515" })).toBe("treasury");
    expect(iconClass({ treasury: true })).toBe("fixed_income");
  });

  it("bitcoin e as outras criptos têm o deles; o resto cai nas iniciais", () => {
    expect(iconKind({ symbol: "BTC", assetClass: "crypto" })).toBe("bitcoin");
    expect(iconKind({ symbol: "btc" })).toBe("bitcoin");
    expect(iconKind({ symbol: "ETH", assetClass: "crypto" })).toBe("crypto");
    expect(iconKind({ symbol: "SOL" })).toBe("crypto");
    expect(iconKind({ symbol: "ITSA4", assetClass: "br_stocks" })).toBe("initials");
    expect(iconKind({ symbol: null, assetClass: "fixed_income" })).toBe("initials");
  });

  it("a classe dita pela tela vence o palpite pelo ticker", () => {
    expect(iconClass({ symbol: "KNRI11", assetClass: "br_stocks" })).toBe("br_stocks");
    expect(iconClass({ symbol: "KNRI11" })).toBe("br_reits");
    expect(iconClass({ symbol: "ITSA4" })).toBe("br_stocks");
    expect(iconClass({ symbol: "VOO" })).toBeNull();
    expect(iconClass({})).toBeNull();
  });

  it("um rótulo é ticker ou nome", () => {
    expect(iconSubjectOfLabel("ITSA4")).toEqual({ symbol: "ITSA4", name: "ITSA4" });
    expect(iconSubjectOfLabel("Tesouro Selic 2029")).toEqual({ symbol: null, name: "Tesouro Selic 2029" });
  });
});

// Lê o CSS de verdade (o vitest roda com `css: false`, que esvaziaria um `?raw`).
const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const tokens = read("../styles/tokens.css");
const iconsCss = read("../styles/icons.css");

/** Luminância relativa e razão de contraste (WCAG 2.x), para `#rrggbb`. */
function luminance(hex: string): number {
  const [r = 0, g = 0, b = 0] = [1, 3, 5].map((i) => {
    const channel = parseInt(hex.slice(i, i + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
}

/** O corpo da primeira regra que começa em `selector {`. */
function block(css: string, selector: string): string {
  const start = css.indexOf(`${selector} {`);
  expect(start, selector).toBeGreaterThanOrEqual(0);
  return css.slice(start, css.indexOf("}", start));
}

function variable(body: string, name: string): string {
  const found = new RegExp(`${name}:\\s*(#[0-9a-fA-F]{6})`).exec(body);
  expect(found, name).not.toBeNull();
  return found?.[1] ?? "#000000";
}

describe("contraste das iniciais (AA, nos dois temas)", () => {
  const themes = [
    { name: "escuro", tokens: block(tokens, ":root"), ink: variable(block(iconsCss, ":root"), "--icon-ink") },
    {
      name: "claro",
      tokens: block(tokens, '[data-theme="light"]'),
      ink: variable(block(iconsCss, '[data-theme="light"]'), "--icon-ink"),
    },
  ];

  for (const theme of themes) {
    it(`a tinta do tema ${theme.name} tem 4,5:1 sobre a cor de cada classe e sobre o neutro`, () => {
      const backgrounds = [
        ...ASSET_CLASSES.map((c) => [c, variable(theme.tokens, `--c-${c}`)] as const),
        ["neutro", variable(theme.tokens, "--text-muted")] as const,
      ];
      expect(backgrounds).toHaveLength(8);
      for (const [name, background] of backgrounds) {
        expect(contrast(theme.ink, background), `${name} ${background} sobre ${theme.ink}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }

  it("o tema do sistema claro usa a mesma tinta do tema claro", () => {
    const media = iconsCss.slice(iconsCss.indexOf("@media (prefers-color-scheme: light)"));
    expect(variable(media, "--icon-ink")).toBe(variable(block(iconsCss, '[data-theme="light"]'), "--icon-ink"));
  });
});
