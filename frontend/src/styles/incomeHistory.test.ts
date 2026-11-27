import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Lê o CSS de verdade (o vitest roda com `css: false`). O caminho passa por
// uma função para o Vite não trocar o `new URL` literal pelo endereço do asset.
const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const css = read("./incomeHistory.css");

/** O corpo da primeira regra que começa em `selector {`. */
function block(selector: string, from = 0): string {
  const start = css.indexOf(`${selector} {`, from);
  expect(start, selector).toBeGreaterThanOrEqual(0);
  return css.slice(start, css.indexOf("}", start));
}

function hex(body: string, name: string): [number, number, number] {
  const found = new RegExp(`${name}:\\s*#([0-9a-f]{6});`, "i").exec(body);
  expect(found, name).not.toBeNull();
  const h = found?.[1] ?? "000000";
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}

function luminance([r, g, b]: [number, number, number]): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(a: [number, number, number], b: [number, number, number]): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
}

const LEVELS = [1, 2, 3, 4, 5];
const dark = block(":root");
const light = block('[data-theme="light"]');
const system = block(":root:not([data-theme])", css.indexOf("@media (prefers-color-scheme: light)"));

describe("escala do mapa de calor", () => {
  it.each([
    ["escuro", dark],
    ["claro", light],
  ])("o texto de cada passo passa de 4,5:1 no tema %s", (_, body) => {
    for (const level of LEVELS) {
      const ratio = contrast(hex(body, `--heat-${level}`), hex(body, `--heat-ink-${level}`));
      expect(ratio, `passo ${level}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("é um tom só, que clareia no escuro e escurece no claro", () => {
    const darkL = LEVELS.map((level) => luminance(hex(dark, `--heat-${level}`)));
    const lightL = LEVELS.map((level) => luminance(hex(light, `--heat-${level}`)));
    for (let i = 1; i < LEVELS.length; i += 1) {
      expect(darkL[i]).toBeGreaterThan(darkL[i - 1] ?? 0);
      expect(lightL[i]).toBeLessThan(lightL[i - 1] ?? 1);
    }
  });

  it("o claro do sistema é o mesmo do claro escolhido", () => {
    for (const level of LEVELS) {
      expect(hex(system, `--heat-${level}`)).toEqual(hex(light, `--heat-${level}`));
      expect(hex(system, `--heat-ink-${level}`)).toEqual(hex(light, `--heat-ink-${level}`));
    }
  });
});
