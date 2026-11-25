import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  return rendered;
}

describe("Resumo de saúde na Carteira", () => {
  it("conta os avisos, mostra os dois primeiros e leva ao painel", async () => {
    const { user } = await openPortfolio();
    const summary = await screen.findByRole("region", { name: "Saúde da carteira" });
    expect(within(summary).getByRole("heading", { name: "Saúde da carteira: 8 avisos" })).toBeInTheDocument();
    const items = within(summary).getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual([
      "Tesouro IPCA+ 2035 é 70,76% da carteira",
      "ITSA4 é 23,31% da carteira",
      "e mais 6",
    ]);
    expect(within(summary).getByRole("link", { name: "Ver painel" })).toHaveAttribute("href", "#/ajustes/saude");
    await user.click(within(summary).getByRole("link", { name: "Ver painel" }));
    expect(await screen.findByRole("heading", { name: "Saúde da carteira", level: 1 })).toBeInTheDocument();
  });

  it("diz quando não há aviso, e quando ainda não há o que medir", async () => {
    getDb().warnings = [];
    const first = await openPortfolio();
    expect(await screen.findByRole("heading", { name: "Saúde da carteira: nenhum aviso" })).toBeInTheDocument();
    first.unmount();

    server.use(
      http.get("/api/v1/health", () =>
        HttpResponse.json({
          total_value: 0,
          warnings: [],
          rules: getDb().healthRules.map((r) => ({ ...r, status: "off" })),
          unclassified_count: 0,
          sources: [],
        }),
      ),
    );
    await openPortfolio();
    expect(await screen.findByRole("heading", { name: "Saúde da carteira: ainda sem o que medir" })).toBeInTheDocument();
  });

  it("um aviso só usa o singular", async () => {
    getDb().warnings = getDb().warnings.slice(0, 1);
    await openPortfolio();
    expect(await screen.findByRole("heading", { name: "Saúde da carteira: 1 aviso" })).toBeInTheDocument();
    expect(screen.queryByText(/^e mais/)).not.toBeInTheDocument();
  });

  it("não atrapalha a carteira quando os avisos não vêm", async () => {
    server.use(http.get("/api/v1/health", () => HttpResponse.error()));
    await openPortfolio();
    expect(screen.getByRole("region", { name: "Fundos Imobiliários" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Saúde da carteira" })).not.toBeInTheDocument();
  });
});
