import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { DURATION, SPRING } from "../lib/motion";

// Lê o CSS de verdade (o vitest roda com `css: false`).
const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const tokens = read("./tokens.css");

/** O corpo da primeira regra que começa em `selector {` (até a primeira `}`). */
function block(css: string, selector: string): string {
  const start = css.indexOf(`${selector} {`);
  expect(start, selector).toBeGreaterThanOrEqual(0);
  return css.slice(start, css.indexOf("}", start));
}

function value(body: string, name: string): string {
  const found = new RegExp(`${name}:\\s*([^;]+);`).exec(body);
  expect(found, name).not.toBeNull();
  return found?.[1]?.trim() ?? "";
}

type Rgb = [number, number, number];

function parseColor(color: string): { rgb: Rgb; alpha: number } {
  const hex = /^#([0-9a-f]{6})$/i.exec(color);
  if (hex?.[1]) {
    const h = hex[1];
    return { rgb: [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb, alpha: 1 };
  }
  const rgba = /^rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)$/.exec(color);
  if (rgba) {
    return { rgb: [Number(rgba[1]), Number(rgba[2]), Number(rgba[3])], alpha: Number(rgba[4]) };
  }
  throw new Error(`cor que o teste não lê: ${color}`);
}

/** Uma cor translúcida pousada sobre um fundo opaco. */
function over(top: string, bottom: string): Rgb {
  const { rgb, alpha } = parseColor(top);
  const base = parseColor(bottom).rgb;
  return rgb.map((c, i) => c * alpha + (base[i] ?? 0) * (1 - alpha)) as Rgb;
}

function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(a: Rgb, b: Rgb): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
}

const THEMES = [
  { name: "escuro", body: block(tokens, ":root") },
  { name: "claro", body: block(tokens, '[data-theme="light"]') },
];

/** As cores de texto que a interface usa sobre os painéis. */
const TEXT = ["--text", "--text-muted", "--text-subtle", "--gold-text", "--success", "--danger", "--info"];

describe("contraste do texto sobre o painel (AA, 4,5:1, nos dois temas)", () => {
  for (const theme of THEMES) {
    for (const name of TEXT) {
      it(`${name} no tema ${theme.name}`, () => {
        const text = parseColor(value(theme.body, name)).rgb;
        // O painel opaco (a faixa de números) e o balão do toggletip, sobre o fundo da página.
        for (const surface of ["--surface-strong", "--surface-popover"]) {
          const panel = over(value(theme.body, surface), value(theme.body, "--bg"));
          expect(contrast(text, panel), surface).toBeGreaterThanOrEqual(4.5);
        }
      });
    }
  }

  it('o tema "do sistema" claro repete o claro, cor por cor', () => {
    const system = tokens.slice(tokens.indexOf(":root:not([data-theme]) {"));
    for (const name of [...TEXT, "--bg", "--surface-strong", "--surface-popover", "--surface-sunken", "--border"]) {
      expect(value(block(system, ":root:not([data-theme])"), name), name).toBe(value(THEMES[1]?.body ?? "", name));
    }
  });
});

describe("movimento", () => {
  it("o CSS e o motion usam as mesmas durações", () => {
    const root = THEMES[0]?.body ?? "";
    for (const [key, seconds] of Object.entries(DURATION)) {
      expect(value(root, `--dur-${key}`), key).toBe(`${Math.round(seconds * 1000)}ms`);
    }
  });

  it("nada que a pessoa espera passa de 300 ms", () => {
    for (const seconds of Object.values(DURATION)) expect(seconds).toBeLessThanOrEqual(0.3);
  });

  it("toda mola assenta (2%) em até 0,35 s, sem quicar à toa", () => {
    for (const [key, spring] of Object.entries(SPRING)) {
      const omega = Math.sqrt(spring.stiffness);
      const zeta = spring.damping / (2 * omega);
      // Taxa do polo mais lento: ζω (subamortecida) ou ω(ζ − √(ζ² − 1)) (superamortecida).
      const rate = zeta < 1 ? zeta * omega : omega * (zeta - Math.sqrt(zeta * zeta - 1));
      expect(4 / rate, key).toBeLessThanOrEqual(0.35);
      expect(zeta, key).toBeGreaterThanOrEqual(0.65);
    }
  });
});
