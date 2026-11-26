import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { sampleEvent, sampleSummary } from "../../test/incomeHandlers";
import { setIcons } from "../../test/iconsHandlers";
import { TREASURY_AT } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

/*
 * O ícone do ativo em cada lista onde um ativo aparece: com logo no que o
 * servidor tem guardado, iniciais (ou o desenho próprio) no resto. O
 * componente em si está em `components/AssetIcon.test.tsx`.
 */

const LOGOS = [
  { symbol: "ITSA4", version: "v-itsa" },
  { symbol: "KNRI11", version: "v-knri" },
];

const img = (root: HTMLElement) => root.querySelector<HTMLImageElement>("img.asset-icon__img");
const kind = (root: HTMLElement) => root.querySelector<HTMLElement>(".asset-icon")?.dataset.kind;

/** O logo da linha, quando o índice de logos chega (ele vem depois da tela). */
async function logoOf(root: HTMLElement): Promise<string | null> {
  await waitFor(() => expect(img(root)).not.toBeNull());
  return img(root)?.getAttribute("src") ?? null;
}

describe("Carteira", () => {
  it("cada linha tem o logo, as iniciais, ou o desenho próprio de bitcoin, Tesouro e renda fixa", async () => {
    setIcons(LOGOS);
    getDb().assets.push({
      id: "a-selic",
      asset_class: "fixed_income",
      symbol: "TD-SELIC-20290301",
      display_name: "Tesouro Selic 2029",
      quantity: 0.5,
      quote_price: 20305.65,
      manual_price: null,
      currency: "BRL",
      score_override: 6,
      answers: {},
      treasury: true,
      quote_at: TREASURY_AT,
    });
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });

    const row = (name: RegExp) => screen.getAllByRole("button").find((b) => name.test(b.textContent ?? ""));
    const itsa = row(/ITSA4/) as HTMLElement;
    expect(await logoOf(itsa)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    expect(await logoOf(row(/KNRI11/) as HTMLElement)).toBe("/api/v1/icons/KNRI11?v=v-knri");

    // Sem logo guardado: iniciais do ticker, na cor da classe.
    const hglg = row(/HGLG11/) as HTMLElement;
    expect(img(hglg)).toBeNull();
    expect(hglg.querySelector(".asset-icon__initials")).toHaveTextContent("HG");
    // Cripto, Tesouro e renda fixa por valor.
    expect(kind(row(/BTC/) as HTMLElement)).toBe("bitcoin");
    expect(kind(row(/Tesouro Selic 2029/) as HTMLElement)).toBe("treasury");
    // Renda fixa por valor não tem ticker: iniciais do nome.
    expect((row(/Tesouro IPCA\+ 2035/) as HTMLElement).querySelector(".asset-icon__initials")).toHaveTextContent("TI");
    // Nunca vira texto lido: o nome do ativo continua sendo a única pista.
    expect(within(itsa).queryByRole("img")).toBeNull();
  });

  it("sem logo nenhum no servidor, a carteira inteira usa as iniciais", async () => {
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    expect(document.querySelectorAll("img.asset-icon__img")).toHaveLength(0);
    expect(document.querySelectorAll(".asset-icon").length).toBeGreaterThan(3);
  });
});

describe("Aportar", () => {
  it("o plano e a folha de registrar o aporte mostram o ícone de cada ativo", async () => {
    setIcons(LOGOS);
    const { user } = renderApp("aportar");
    await user.type(await screen.findByRole("textbox", { name: "Quanto você vai aportar?" }), "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));

    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const cards = within(result).getAllByRole("listitem").filter((li) => li.classList.length === 0 || li.querySelector(".plan-card"));
    const card = (symbol: string) => cards.find((c) => c.querySelector(".plan-card__symbol")?.textContent === symbol) as HTMLElement;
    expect(await logoOf(card("ITSA4"))).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    expect(card("BTLG11").querySelector(".asset-icon__initials")).toHaveTextContent("BT");
    expect(kind(card("BTC"))).toBe("bitcoin");

    await user.click(await screen.findByRole("button", { name: /Já comprei/ }));
    const dialog = await screen.findByRole("dialog", { name: "Registrar aporte" });
    const rows = within(dialog).getAllByRole("listitem");
    expect(await logoOf(rows[0] as HTMLElement)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    expect(rows.every((r) => r.querySelector(".asset-icon"))).toBe(true);
  });
});

describe("Renda", () => {
  it("a lista de proventos, a estimativa e os pagadores trazem o ícone", async () => {
    setIcons(LOGOS);
    server.use(
      http.get("/api/v1/income/summary", () => HttpResponse.json(sampleSummary())),
      http.get("/api/v1/income", () =>
        HttpResponse.json([
          sampleEvent(),
          sampleEvent({ id: "e-2", asset_id: "a-itsa", symbol: "ITSA4", display_name: "Itaúsa", kind: "dividend" }),
          sampleEvent({ id: "e-3", asset_id: null, symbol: null, display_name: "CDB do Banco Exemplo", kind: "other" }),
        ]),
      ),
    );
    renderApp("renda");
    await screen.findByRole("heading", { name: "Quanto sua carteira paga?" });

    // Estimativa: uma linha por pagador estimado.
    const estimate = (await screen.findAllByText(/por cota em 12 meses/))[0]?.closest("li") as HTMLElement;
    expect(estimate.querySelector(".asset-icon")).not.toBeNull();

    // Pagadores: um ícone por pagador, e "Outros" não tem.
    const payers = document.querySelector(".payers__list") as HTMLElement;
    expect(payers.querySelectorAll(".asset-icon")).toHaveLength(3);
    expect(await logoOf(within(payers).getByText("KNRI11").closest("li") as HTMLElement)).toBe("/api/v1/icons/KNRI11?v=v-knri");

    // Lista de lançados: o sem ticker usa as iniciais do nome.
    const cdb = (await screen.findAllByText("CDB do Banco Exemplo"))[0]?.closest("li") as HTMLElement;
    expect(cdb.querySelector(".asset-icon__initials")).toHaveTextContent("CD");
    const itsa = screen.getAllByText("ITSA4").map((el) => el.closest("li")).find((li) => li?.classList.contains("income-row--event"));
    expect(await logoOf(itsa as HTMLElement)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
  });
});

describe("Busca de símbolo", () => {
  it("cada sugestão tem o ícone: logo, iniciais, cripto e Tesouro", async () => {
    setIcons(LOGOS);
    const { user } = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    const combobox = within(dialog).getByRole("combobox", { name: "Ticker ou nome" });

    await user.type(combobox, "itsa");
    const itsa = await within(dialog).findByRole("option", { name: /ITSA4/ });
    expect(await logoOf(itsa)).toBe("/api/v1/icons/ITSA4?v=v-itsa");

    await user.clear(combobox);
    await user.type(combobox, "kncr");
    expect((await within(dialog).findByRole("option", { name: /KNCR11/ })).querySelector(".asset-icon__initials")).toHaveTextContent("KN");

    await user.clear(combobox);
    await user.type(combobox, "eth");
    expect(kind(await within(dialog).findByRole("option", { name: /ETH/ }))).toBe("crypto");

    await user.clear(combobox);
    await user.type(combobox, "tesouro ipca");
    expect(kind(await within(dialog).findByRole("option", { name: /Tesouro IPCA/ }))).toBe("treasury");
  });
});

describe("Saúde", () => {
  it("os avisos mostram o ícone dos ativos que citam, e os de setor e classe não", async () => {
    setIcons(LOGOS);
    renderApp("ajustes/saude");
    await screen.findByRole("heading", { name: "Saúde da carteira", level: 1 });
    const warnings = within(await screen.findByRole("list", { name: "Avisos" }));

    const itsa = warnings.getByRole("article", { name: "ITSA4 é 23,31% da carteira" });
    expect(await logoOf(itsa)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    const topFive = warnings.getAllByRole("article").find((a) => a.querySelectorAll(".asset-icon").length === 5) as HTMLElement;
    expect(topFive).toBeDefined();
    // O Tesouro do aviso é citado por nome (não há chave): iniciais do nome.
    const ipca = warnings.getByRole("article", { name: "Tesouro IPCA+ 2035 é 70,76% da carteira" });
    expect(ipca.querySelector(".asset-icon__initials")).toHaveTextContent("TI");
    // Setor e classe não citam ativo.
    const sector = warnings.getAllByRole("article").find((a) => /Tijolo/.test(a.textContent ?? "")) as HTMLElement;
    expect(sector.querySelector(".asset-icon")).toBeNull();
  });
});

describe("Impostos", () => {
  it("cada linha de Bens e Direitos tem o ícone do ativo", async () => {
    setIcons(LOGOS);
    renderApp("ajustes/impostos");
    await screen.findByRole("heading", { name: "Imposto de renda" });
    const goods = await screen.findByRole("list", { name: "Bens e Direitos" });
    const rows = within(goods).getAllByRole("listitem");
    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) expect(row.querySelector(".asset-icon")).not.toBeNull();
    expect(await logoOf(rows.find((r) => /ITSA4/.test(r.textContent ?? "")) as HTMLElement)).toBe(
      "/api/v1/icons/ITSA4?v=v-itsa",
    );
    // O bitcoin da declaração usa o desenho próprio.
    expect(kind(rows.find((r) => /BTC/.test(r.textContent ?? "")) as HTMLElement)).toBe("bitcoin");
  });
});

describe("Lista de logos", () => {
  it("a lista de logos vem uma vez só para a tela toda", async () => {
    setIcons(LOGOS);
    let calls = 0;
    server.use(
      http.get("/api/v1/icons", () => {
        calls += 1;
        return HttpResponse.json({ icons: LOGOS });
      }),
    );
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await waitFor(() => expect(document.querySelectorAll("img.asset-icon__img").length).toBeGreaterThan(0));
    expect(document.querySelectorAll(".asset-icon").length).toBeGreaterThan(5);
    expect(calls).toBe(1);
  });
});
