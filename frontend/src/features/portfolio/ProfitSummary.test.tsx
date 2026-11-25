import { act, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { LivePrices, Portfolio, Profit } from "../../api/types";
import { mergePrices } from "../../lib/livePrices";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { FakeEventSource } from "../../test/eventSource";
import { toPortfolio } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

/** Carteira FICTÍCIA: 3.000 de ganho não realizado, 100 realizado, 50 de proventos. */
const whole: Profit = {
  unrealized: 3000,
  realized: 100,
  income_received: 50,
  total: 3150,
  partial: false,
  partial_assets: [],
  removed_assets: false,
};

function withProfit(profit: Profit) {
  const db = getDb();
  server.use(http.get("/api/v1/portfolio", () => HttpResponse.json({ ...toPortfolio(db.assets, db.goals, db.checklists), profit })));
}

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  const region = await screen.findByRole("region", { name: "Lucro total" });
  return { ...rendered, region };
}

describe("Carteira: lucro total", () => {
  it("soma ganho de capital (realizado e não realizado) e proventos recebidos", async () => {
    withProfit(whole);
    const { region } = await openPortfolio();
    expect(region).toHaveTextContent(/Lucro total\s*\+R\$\s3\.150,00/);
    expect(region).not.toHaveTextContent("parcial");
    expect(region).toHaveTextContent(/Ganho de capital\s*\+R\$\s3\.100,00/);
    expect(region).toHaveTextContent(/não realizado\s*\+R\$\s3\.000,00/);
    expect(region).toHaveTextContent(/realizado \(vendas\)\s*\+R\$\s100,00/);
    expect(region).toHaveTextContent(/Proventos recebidos\s*\+R\$\s50,00/);
    expect(within(region).queryByText(/Parcial:/)).not.toBeInTheDocument();
  });

  it("prejuízo aparece com o sinal escrito", async () => {
    withProfit({ ...whole, unrealized: -4000, realized: 0, income_received: 50, total: -3950 });
    const { region } = await openPortfolio();
    expect(region).toHaveTextContent(/−R\$\s3\.950,00/);
  });

  it("custo desconhecido: diz que é parcial e quem faltou, sem inventar o número", async () => {
    withProfit({
      ...whole,
      partial: true,
      partial_assets: [
        { asset_id: "a-hglg", label: "HGLG11", reason: "unknown_cost" },
        { asset_id: "a-cdb", label: "CDB", reason: "unknown_realized" },
        { asset_id: "a-btc", label: "BTC", reason: "missing_price" },
      ],
    });
    const { region } = await openPortfolio();
    expect(region).toHaveTextContent("Lucro total (parcial)");
    // O total é só do que se sabe: o mesmo número do servidor, sem ajuste.
    expect(region).toHaveTextContent(/\+R\$\s3\.150,00/);
    expect(region).toHaveTextContent(
      "Parcial: falta o preço médio de HGLG11; falta o preço médio da venda de CDB; sem cotação: BTC.",
    );
  });

  it("avisa quando há venda de ativo que já saiu da carteira", async () => {
    withProfit({ ...whole, removed_assets: true });
    const { region } = await openPortfolio();
    expect(region).toHaveTextContent(/não inclui o resultado de vendas de ativos que já saíram da carteira/i);
  });

  it("o evento ao vivo troca o lucro junto com o preço", () => {
    const db = getDb();
    const portfolio: Portfolio = { ...toPortfolio(db.assets, db.goals, db.checklists), profit: whole };
    const live: LivePrices = {
      total_value: portfolio.total_value,
      quotes_updated_at: portfolio.quotes_updated_at,
      classes: portfolio.classes,
      assets: [],
      profit: { ...whole, unrealized: 3500, total: 3650 },
    };
    expect(mergePrices(portfolio, live).profit.total).toBe(3650);
  });

  it("na tela, o stream atualiza o lucro sem recarregar", async () => {
    withProfit(whole);
    const { region } = await openPortfolio();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const db = getDb();
    const portfolio = toPortfolio(db.assets, db.goals, db.checklists);
    const event: LivePrices = {
      total_value: portfolio.total_value,
      quotes_updated_at: portfolio.quotes_updated_at,
      classes: portfolio.classes,
      assets: [],
      profit: { ...whole, unrealized: 3500, total: 3650 },
    };
    act(() => FakeEventSource.latest().emit("prices", event));
    await waitFor(() => expect(region).toHaveTextContent(/\+R\$\s3\.650,00/));
  });
});

describe("Carteira com o olho fechado: lucro total", () => {
  it("nenhum valor aparece, nem o sinal do lucro, nem para o leitor de tela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    withProfit({
      ...whole,
      total: -3950,
      unrealized: -4000,
      partial: true,
      partial_assets: [{ asset_id: "a-hglg", label: "HGLG11", reason: "unknown_cost" }],
    });
    const { region } = await openPortfolio();
    expect(region).toHaveTextContent(MASK_BRL);
    expect(region).not.toHaveTextContent("+");
    expect(region).not.toHaveTextContent("−");
    const walker = document.createTreeWalker(region, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      expect(node.textContent ?? "").not.toMatch(/R\$\s*\d/);
    }
    for (const element of region.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "title", "placeholder"]) {
        expect(element.getAttribute(attribute) ?? "").not.toMatch(/R\$\s*\d/);
      }
    }
    // O aviso de parcial é texto, não valor: continua à vista.
    expect(region).toHaveTextContent("Lucro total (parcial)");
    expect(region).toHaveTextContent("falta o preço médio de HGLG11");
  });
});
