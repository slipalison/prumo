// Mede a altura de cada tela e de cada folha do Prumo no modo de demonstração
// e tira as capturas (docs/layout.md, "Como medir").
//
// O Playwright NÃO é dependência do projeto: este script só serve para medir.
// Rode-o de uma pasta onde o Playwright esteja instalado (fora do repositório):
//
//   cd frontend && npm run dev:demo                    # num terminal
//   mkdir /tmp/pw && cd /tmp/pw && npm i playwright@1.65.1 && npx playwright install chromium
//   node <repo>/frontend/scripts/measure-layout.mjs <pasta-das-capturas>
//
// `MEASURE_URL` troca o endereço (padrão http://localhost:5180),
// `MEASURE_VIEWPORTS` escolhe as larguras (veja `ALL_VIEWPORTS`) e
// `MEASURE_THEME=light` mede no tema claro (padrão: escuro). Escreve
// `medidas.json` e `medidas.md` na pasta das capturas, mais um PNG de página
// inteira por tela, folha e largura. Também aponta rolagem lateral: da página,
// da folha e de qualquer tabela ou bloco que rola de lado por dentro.

import { mkdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join, resolve } from "node:path";

// O Playwright vem da pasta de onde se roda, não do repositório.
const { chromium } = createRequire(join(process.cwd(), "noop.js"))("playwright");

const BASE = process.env.MEASURE_URL ?? "http://localhost:5180";
const OUT = resolve(process.argv[2] ?? "layout-shots");
mkdirSync(OUT, { recursive: true });

const ALL_VIEWPORTS = [
  { name: "1440x900", width: 1440, height: 900 },
  { name: "1920x1080", width: 1920, height: 1080 },
  { name: "ipad-1366x1024", width: 1366, height: 1024 },
  { name: "ipad-1024x1366", width: 1024, height: 1366 },
  { name: "celular-390x844", width: 390, height: 844 },
];

// `MEASURE_VIEWPORTS=1440x900,1920x1080` mede só essas larguras.
const only = process.env.MEASURE_VIEWPORTS?.split(",");
const THEME = process.env.MEASURE_THEME === "light" ? "light" : "dark";
const VIEWPORTS = only ? ALL_VIEWPORTS.filter((v) => only.includes(v.name)) : ALL_VIEWPORTS;

/** Espera a tela assentar: rede parada e as entradas animadas terminadas. */
async function settle(page) {
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(1200);
}

async function calculatePlan(page) {
  const input = page.locator("input[data-screen-focus]").first();
  await input.click();
  await input.pressSequentially("600000");
  await page.getByRole("button", { name: /Calcular/ }).click();
  await page.getByRole("button", { name: /Comprei todos como sugerido/ }).waitFor();
}

const SCREENS = [
  { id: "aportar", path: "aportar" },
  { id: "aportar-plano", path: "aportar", setup: calculatePlan },
  { id: "carteira", path: "carteira" },
  { id: "renda", path: "renda" },
  { id: "agenda", path: "agenda" },
  { id: "agenda-calendario", path: "agenda", setup: (page) => page.getByRole("button", { name: "Calendário" }).click() },
  { id: "desempenho", path: "desempenho" },
  { id: "desempenho-risco", path: "desempenho/risco" },
  { id: "analise", path: "analise" },
  { id: "analise-resultados", path: "analise/resultados" },
  { id: "ajustes-metas", path: "ajustes/metas" },
  { id: "ajustes-criterios", path: "ajustes/criterios" },
  { id: "ajustes-objetivos", path: "ajustes/objetivos" },
  { id: "ajustes-analise", path: "ajustes/analise" },
  { id: "ajustes-importar-b3", path: "ajustes/importar-b3" },
  { id: "ajustes-impostos", path: "ajustes/impostos" },
  { id: "ajustes-alertas", path: "ajustes/alertas" },
  { id: "ajustes-proventos-automaticos", path: "ajustes/proventos-automaticos" },
];

const SHEETS = [
  {
    id: "folha-ativo-acao",
    path: "carteira",
    open: (page) => page.getByRole("button", { name: /ITSA4/ }).first().click(),
  },
  {
    id: "folha-ativo-fii",
    path: "carteira",
    open: (page) => page.getByRole("button", { name: /KNRI11/ }).first().click(),
  },
  { id: "folha-novo-ativo", path: "carteira", open: (page) => page.getByRole("button", { name: "Adicionar ativo" }).click() },
  {
    id: "folha-checklist",
    path: "aportar",
    open: async (page) => {
      await calculatePlan(page);
      await page.getByRole("button", { name: /Abrir HGLG11/ }).click();
    },
  },
  {
    id: "folha-registrar-aporte",
    path: "aportar",
    open: async (page) => {
      await calculatePlan(page);
      await page.getByRole("button", { name: /Comprei todos como sugerido/ }).click();
      await page.getByRole("button", { name: /Registrar \d+ comprados/ }).click();
    },
  },
  { id: "folha-provento", path: "renda", open: (page) => page.getByRole("button", { name: /Lançar provento/ }).first().click() },
  { id: "folha-meta-renda", path: "renda", open: (page) => page.getByRole("button", { name: /Editar meta|Definir meta/ }).click() },
  { id: "folha-objetivo", path: "ajustes/objetivos", open: (page) => page.getByRole("button", { name: "Novo objetivo" }).click() },
  { id: "folha-teto", path: "analise", open: (page) => page.getByRole("button", { name: /Escolher o teto de ITSA4/ }).click() },
  { id: "folha-teto-fii", path: "analise", open: (page) => page.getByRole("button", { name: /Escolher o teto de KNRI11/ }).click() },
];

/** Blocos que rolam de lado por dentro (tabela larga, lista de pílulas): quais e quanto passa. */
function innerScrollers() {
  return [...document.querySelectorAll("main *, [role='dialog'] *")]
    .filter((el) => {
      if (el.scrollWidth <= el.clientWidth + 1 || el.clientWidth === 0) return false;
      const overflowX = getComputedStyle(el).overflowX;
      return overflowX === "auto" || overflowX === "scroll";
    })
    .map((el) => `${el.tagName.toLowerCase()}.${String(el.className).split(" ")[0]} +${el.scrollWidth - el.clientWidth}px`);
}

/** Altura, largura útil e o que cabe acima da dobra. */
function measurePage() {
  const vh = window.innerHeight;
  const vw = window.innerWidth;
  const height = document.scrollingElement.scrollHeight;
  const main = document.querySelector("main");
  const box = main?.getBoundingClientRect();
  const nav = document.querySelector("nav.nav")?.getBoundingClientRect();
  const titles = [...document.querySelectorAll("main h1, main h2, main h3")]
    .filter((el) => el.getClientRects().length > 0)
    .map((el) => ({ text: el.textContent.trim().replace(/\s+/g, " ").slice(0, 60), top: Math.round(el.getBoundingClientRect().top + window.scrollY) }));
  const root = document.documentElement;
  return {
    viewport: `${vw}x${vh}`,
    height,
    hscroll: root.scrollWidth > root.clientWidth ? root.scrollWidth - root.clientWidth : 0,
    innerScroll: window.innerScrollers(),
    screens: Math.round((height / vh) * 100) / 100,
    mainWidth: box ? Math.round(box.width) : null,
    navWidth: nav && nav.width < vw ? Math.round(nav.width) : 0,
    unusedWidth: box ? Math.round(vw - box.width - (nav && nav.width < vw ? nav.width : 0)) : null,
    aboveFold: titles.filter((t) => t.top < vh).map((t) => t.text),
    belowFold: titles.filter((t) => t.top >= vh).map((t) => `${t.text} @${t.top}`),
  };
}

function measureSheet() {
  const dialog = document.querySelector('[role="dialog"]');
  const body = dialog?.querySelector(".sheet__body");
  if (!dialog || !body) return null;
  const rect = dialog.getBoundingClientRect();
  const chrome = rect.height - body.clientHeight;
  const content = chrome + body.scrollHeight;
  return {
    viewport: `${window.innerWidth}x${window.innerHeight}`,
    dialogHeight: Math.round(rect.height),
    dialogWidth: Math.round(rect.width),
    contentHeight: Math.round(content),
    hscroll: body.scrollWidth > body.clientWidth ? body.scrollWidth - body.clientWidth : 0,
    innerScroll: window.innerScrollers(),
    screens: Math.round((content / rect.height) * 100) / 100,
    title: dialog.querySelector("h2")?.textContent.trim(),
  };
}

const results = { screens: {}, sheets: {} };
const browser = await chromium.launch();

for (const viewport of VIEWPORTS) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    // `MEASURE_REDUCED_MOTION=1`: sem animação, para comparar capturas pixel a pixel.
    reducedMotion: process.env.MEASURE_REDUCED_MOTION === "1" ? "reduce" : "no-preference",
  });
  await context.addInitScript(`window.innerScrollers = ${innerScrollers}`);
  await context.addInitScript((theme) => window.localStorage.setItem("prumo.theme", theme), THEME);
  const page = await context.newPage();
  page.on("pageerror", (error) => console.error(`[${viewport.name}] erro na página:`, error.message));

  for (const screen of SCREENS) {
    await page.goto(`${BASE}/#/${screen.path}`);
    await page.reload();
    await settle(page);
    if (screen.setup) {
      await screen.setup(page);
      await settle(page);
    }
    const data = await page.evaluate(measurePage);
    (results.screens[screen.id] ??= {})[viewport.name] = data;
    await page.screenshot({ path: join(OUT, `${screen.id}--${viewport.name}.png`), fullPage: true });
    const side = data.hscroll || data.innerScroll.length ? `  ROLAGEM LATERAL: página +${data.hscroll}px ${data.innerScroll.join(", ")}` : "";
    console.log(`${viewport.name} ${screen.id}: ${data.height}px = ${data.screens} telas${side}`);
  }

  for (const sheet of SHEETS) {
    await page.goto(`${BASE}/#/${sheet.path}`);
    await page.reload();
    await settle(page);
    try {
      await sheet.open(page);
      await page.locator('[role="dialog"]').waitFor();
      await settle(page);
      const data = await page.evaluate(measureSheet);
      (results.sheets[sheet.id] ??= {})[viewport.name] = data;
      // A folha rola por dentro: a captura mostra a primeira "tela" dela.
      await page.screenshot({ path: join(OUT, `${sheet.id}--${viewport.name}.png`) });
      const side = data?.hscroll || data?.innerScroll.length ? `  ROLAGEM LATERAL: folha +${data.hscroll}px ${data.innerScroll.join(", ")}` : "";
      console.log(`${viewport.name} ${sheet.id}: ${data?.contentHeight}px em ${data?.dialogHeight}px = ${data?.screens}${side}`);
    } catch (error) {
      console.error(`${viewport.name} ${sheet.id}: não abriu (${error.message.split("\n")[0]})`);
    }
    await page.keyboard.press("Escape");
  }
  await context.close();
}
await browser.close();

writeFileSync(join(OUT, "medidas.json"), JSON.stringify(results, null, 2));

const lines = ["# Medidas", "", "## Telas (altura da página ÷ altura da janela)", ""];
const names = VIEWPORTS.map((v) => v.name);
lines.push(`| Tela | ${names.join(" | ")} |`, `|---|${names.map(() => "---").join("|")}|`);
for (const [id, byViewport] of Object.entries(results.screens)) {
  lines.push(`| ${id} | ${names.map((n) => (byViewport[n] ? `${byViewport[n].screens} (${byViewport[n].height}px)` : "—")).join(" | ")} |`);
}
lines.push("", "## Folhas (conteúdo ÷ altura da folha)", "");
lines.push(`| Folha | ${names.join(" | ")} |`, `|---|${names.map(() => "---").join("|")}|`);
for (const [id, byViewport] of Object.entries(results.sheets)) {
  lines.push(`| ${id} | ${names.map((n) => (byViewport[n] ? `${byViewport[n].screens} (${byViewport[n].contentHeight}px)` : "—")).join(" | ")} |`);
}
lines.push("", "## Rolagem lateral (nenhuma é o esperado)", "");
const scrolls = [];
for (const kind of ["screens", "sheets"]) {
  for (const [id, byViewport] of Object.entries(results[kind])) {
    for (const [name, data] of Object.entries(byViewport)) {
      if (data && (data.hscroll || data.innerScroll.length)) {
        const own = data.hscroll ? `${kind === "sheets" ? "folha" : "página"} +${data.hscroll}px` : "";
        scrolls.push(`- ${id} em ${name}: ${[own, ...data.innerScroll].filter(Boolean).join(", ")}`);
      }
    }
  }
}
lines.push(...(scrolls.length ? scrolls : ["Nenhuma."]));
writeFileSync(join(OUT, "medidas.md"), `${lines.join("\n")}\n`);
console.log(`Medidas em ${OUT}`);
