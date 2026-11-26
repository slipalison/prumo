import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { setIcons } from "../../test/iconsHandlers";
import { TREASURY_AT } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";
import { LOGOS, img, kind, logoOf } from "../../test/icons";

/* O ícone do ativo nas listas desta área (ajuda em `test/icons.ts`). */

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
