import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";

const GOALS_HEADING = "Quanto você quer em cada classe?";
const CRITERIA_HEADING = "As perguntas que dão a nota";

describe("Navegação e Ajustes", () => {
  it("a navegação principal tem Aportar, Carteira, Desempenho e Ajustes", async () => {
    renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    const nav = screen.getByRole("navigation", { name: "Principal" });
    // O logo também é um link da barra, fora da lista de telas.
    expect(within(within(nav).getByRole("list")).getAllByRole("link").map((link) => link.textContent)).toEqual([
      "Aportar",
      "Carteira",
      "Desempenho",
      "Ajustes",
    ]);
    expect(within(nav).getByRole("link", { name: "Aportar" })).toHaveAttribute("aria-current", "page");
    expect(within(nav).getByRole("link", { name: "Ajustes" })).not.toHaveAttribute("aria-current");
  });

  it("Ajustes abre em Metas por classe e lista as seções, com a atual marcada", async () => {
    renderApp("ajustes");
    expect(await screen.findByRole("heading", { name: GOALS_HEADING })).toBeInTheDocument();
    const nav = screen.getByRole("navigation", { name: "Principal" });
    expect(within(nav).getByRole("link", { name: "Ajustes" })).toHaveAttribute("aria-current", "page");
    expect(document.title).toBe("Ajustes · Prumo");

    const sections = screen.getByRole("navigation", { name: "Seções de Ajustes" });
    expect(within(sections).getAllByRole("link").map((link) => link.textContent)).toEqual([
      "Metas por classe",
      "Critérios",
      "Saúde da carteira",
      "Importar da B3",
    ]);
    expect(within(sections).getByRole("link", { name: "Metas por classe" })).toHaveAttribute("aria-current", "page");
    expect(within(sections).getByRole("link", { name: "Critérios" })).not.toHaveAttribute("aria-current");
  });

  it("trocar de seção troca o conteúdo e o endereço, sem sair de Ajustes", async () => {
    const { user } = renderApp("ajustes");
    await screen.findByRole("heading", { name: GOALS_HEADING });
    const sections = screen.getByRole("navigation", { name: "Seções de Ajustes" });
    await user.click(within(sections).getByRole("link", { name: "Critérios" }));
    expect(await screen.findByRole("heading", { name: CRITERIA_HEADING })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/ajustes/criterios");
    expect(within(sections).getByRole("link", { name: "Critérios" })).toHaveAttribute("aria-current", "page");
    expect(within(sections).getByRole("link", { name: "Metas por classe" })).not.toHaveAttribute("aria-current");
    expect(
      within(screen.getByRole("navigation", { name: "Principal" })).getByRole("link", { name: "Ajustes" }),
    ).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("heading", { name: GOALS_HEADING })).not.toBeInTheDocument();
  });

  it("o link antigo #/metas leva a Metas por classe, dentro de Ajustes", async () => {
    renderApp("metas");
    expect(await screen.findByRole("heading", { name: GOALS_HEADING })).toBeInTheDocument();
    await waitFor(() => expect(window.location.hash).toBe("#/ajustes/metas"));
    expect(
      within(screen.getByRole("navigation", { name: "Principal" })).getByRole("link", { name: "Ajustes" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      within(screen.getByRole("navigation", { name: "Seções de Ajustes" })).getByRole("link", { name: "Metas por classe" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("o link antigo #/criterios leva a Critérios, dentro de Ajustes", async () => {
    renderApp("criterios");
    expect(await screen.findByRole("heading", { name: CRITERIA_HEADING })).toBeInTheDocument();
    await waitFor(() => expect(window.location.hash).toBe("#/ajustes/criterios"));
    expect(
      within(screen.getByRole("navigation", { name: "Seções de Ajustes" })).getByRole("link", { name: "Critérios" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("o redirecionamento não deixa entrada no histórico (Voltar não cai no link antigo)", async () => {
    window.location.hash = "#/metas";
    const before = window.history.length;
    renderApp("metas");
    await waitFor(() => expect(window.location.hash).toBe("#/ajustes/metas"));
    expect(window.history.length).toBe(before);
  });

  it("seção que não existe cai na primeira", async () => {
    renderApp("ajustes/nada");
    expect(await screen.findByRole("heading", { name: GOALS_HEADING })).toBeInTheDocument();
    expect(
      within(screen.getByRole("navigation", { name: "Seções de Ajustes" })).getByRole("link", { name: "Metas por classe" }),
    ).toHaveAttribute("aria-current", "page");
  });
});
