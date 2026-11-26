import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { getDb } from "../../test/server";

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  return rendered;
}

/* A rosca por setor mora no painel lateral da Carteira, um painel por classe. */
describe("Rosca por setor na Carteira", () => {
  it("mostra os FIIs por tipo, com nome e fatia escritos", async () => {
    await openPortfolio();
    const breakdown = screen.getByRole("region", { name: "Por tipo de fundo em Fundos Imobiliários" });
    const rows = within(breakdown).getAllByRole("listitem");
    expect(rows.map((li) => li.textContent)).toEqual(["Tijolo70,3%", "Papel13,0%", "Sem setor16,7%"]);
    expect(within(breakdown).getByRole("img", { name: /Por tipo de fundo: Tijolo 70,3%/ })).toBeInTheDocument();
  });

  it("mostra as ações por setor", async () => {
    await openPortfolio();
    const breakdown = screen.getByRole("region", { name: "Por setor em Ações Nacionais" });
    expect(within(breakdown).getByText("Financeiro")).toBeInTheDocument();
    expect(within(breakdown).getByText("100,0%")).toBeInTheDocument();
    expect(within(breakdown).getByRole("heading", { name: "Por setor" })).toBeInTheDocument();
    // O painel diz de qual classe é, por escrito.
    expect(within(breakdown).getByText("Ações Nacionais")).toBeInTheDocument();
  });

  it("só aparece onde há fonte de setor", async () => {
    await openPortfolio();
    expect(screen.getAllByRole("heading", { name: /^Por (setor|tipo de fundo)$/ })).toHaveLength(2);
    for (const name of ["Criptomoedas", "Renda Fixa", "Ações Internacionais"]) {
      expect(screen.queryByRole("region", { name: new RegExp(`^Por (setor|tipo de fundo) em ${name}$`) })).not.toBeInTheDocument();
    }
  });

  it("diz que o setor está indisponível quando a fonte ainda não respondeu", async () => {
    for (const asset of getDb().assets) {
      delete asset.sector;
      delete asset.subsector;
      delete asset.segment;
    }
    await openPortfolio();
    const stocks = screen.getByRole("region", { name: "Por setor em Ações Nacionais" });
    expect(within(stocks).getByText(/Setor indisponível por enquanto: a B3 ainda não respondeu/)).toBeInTheDocument();
    const fiis = screen.getByRole("region", { name: "Por tipo de fundo em Fundos Imobiliários" });
    expect(within(fiis).getByText(/Setor indisponível por enquanto: a CVM ainda não respondeu/)).toBeInTheDocument();
    expect(within(fiis).queryByRole("img", { name: /Por tipo de fundo/ })).not.toBeInTheDocument();
  });
});
