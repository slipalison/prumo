import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { IncomeSummary } from "../../api/incomeTypes";
import { emptySummary, estimateAsset, estimateOf } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/**
 * Carteira FICTÍCIA: uma ação pela B3 com JCP (líquido ≠ bruto) e acima do
 * habitual, um FII pela B3 com 3 anos de histórico e uma ação pelo Yahoo, que
 * a B3 não tinha.
 */
function netSummary(): IncomeSummary {
  const summary = emptySummary();
  summary.estimate = estimateOf([
    estimateAsset({
      asset_id: "a-abcd",
      symbol: "ABCD4",
      display_name: "Empresa Fictícia",
      quantity: 120,
      per_unit_12m: 1.825,
      per_unit_12m_gross: 2,
      monthly: 18.25,
      monthly_gross: 20,
      usual: { years: 5, per_unit_year: 1.095, per_unit_year_gross: 1.2, monthly: 10.95, monthly_gross: 12 },
      above_usual: true,
    }),
    estimateAsset({
      asset_id: "a-fict",
      symbol: "FICT11",
      display_name: "Fundo Fictício",
      quantity: 10,
      per_unit_12m: 12,
      monthly: 10,
      usual: { years: 3, per_unit_year: 11.5, per_unit_year_gross: 11.5, monthly: 9.58, monthly_gross: 9.58 },
    }),
    estimateAsset({
      asset_id: "a-wxyz",
      symbol: "WXYZ3",
      display_name: "Outra Fictícia",
      quantity: 100,
      per_unit_12m: 2.4,
      monthly: 20,
      source: "yahoo",
    }),
  ]);
  return summary;
}

async function openEstimate(summary: IncomeSummary) {
  server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)));
  const rendered = renderApp("renda");
  const panel = await screen.findByRole("region", { name: "Estimativa da carteira de hoje" });
  return { ...rendered, panel };
}

/** A linha de um ativo na tabela (o cabeçalho da linha tem o código). */
function rowOf(root: HTMLElement, symbol: string): HTMLElement {
  const header = within(root)
    .getAllByRole("rowheader")
    .find((th) => th.textContent?.includes(symbol));
  if (!header?.parentElement) throw new Error(`sem linha de ${symbol}`);
  return header.parentElement;
}

describe("Estimativa líquida", () => {
  it("o líquido em destaque, o bruto menor ao lado, no total e em cada ativo", async () => {
    const { panel } = await openEstimate(netSummary());
    expect(panel).toHaveTextContent(/por mês, líquido/);
    expect(within(panel).getByText(/^bruto ~R\$\s50,00 por mês$/)).toBeInTheDocument();
    const stock = rowOf(panel, "ABCD4");
    expect(within(stock).getByText(/^~R\$\s18,25$/).tagName).toBe("STRONG");
    expect(within(stock).getByText(/^bruto R\$\s20,00$/)).toBeInTheDocument();
    // Sem JCP, líquido = bruto: o bruto não se repete.
    expect(within(rowOf(panel, "FICT11")).queryByText(/^bruto/)).not.toBeInTheDocument();
  });

  it("cada ativo diz a fonte num rótulo pequeno, com o significado para o leitor de tela", async () => {
    const { panel } = await openEstimate(netSummary());
    expect(rowOf(panel, "ABCD4")).toHaveTextContent(/B3, fonte: anúncios da B3/);
    expect(rowOf(panel, "WXYZ3")).toHaveTextContent(/Yahoo, fonte: Yahoo, valor bruto/);
  });

  it("a média de até 5 anos e a marca acima do habitual, com texto, ícone e o total marcado", async () => {
    const { panel, user } = await openEstimate(netSummary());
    const stock = rowOf(panel, "ABCD4");
    expect(within(stock).getByText(/^~R\$\s10,95$/)).toBeInTheDocument();
    // No painel, estreito, a marca diz "acima" com o ícone; o leitor de tela ouve "acima do habitual".
    const tag = stock.querySelector(".estimate__above-tag");
    expect(tag).toHaveTextContent(/^acima do habitual$/);
    expect(tag?.querySelector("svg")).not.toBeNull();
    expect(rowOf(panel, "FICT11").querySelector(".estimate__above-tag")).toBeNull();
    // Menos de cinco anos de histórico: a média diz quantos.
    expect(within(rowOf(panel, "FICT11")).getByText("3 anos")).toBeInTheDocument();
    expect(within(rowOf(panel, "WXYZ3")).getByText("histórico curto")).toBeInTheDocument();

    expect(panel).toHaveTextContent(/~R\$\s18,25\/mês de um ativo acima do habitual/);
    await user.click(within(panel).getByRole("button", { name: "O que é acima do habitual" }));
    const tips = within(panel)
      .getAllByRole("status")
      .map((tip) => tip.textContent)
      .join(" ");
    expect(tips).toMatch(/antecipações.*a estimativa pode cair quando eles saírem da janela/i);
  });

  it("a folha tem por cota líquido e bruto e o total com o que vem de ativos acima do habitual", async () => {
    const summary = netSummary();
    // Mais de quatro ativos para o "Ver todos" aparecer.
    summary.estimate = estimateOf([
      ...summary.estimate.assets,
      ...[1, 2, 3].map((i) =>
        estimateAsset({ asset_id: `a-x${i}`, symbol: `XTRA${i}`, display_name: `Extra ${i}`, quantity: 1, per_unit_12m: 12, monthly: i }),
      ),
    ]);
    const { panel, user } = await openEstimate(summary);
    await user.click(within(panel).getByRole("button", { name: "Ver todos os 6 ativos" }));
    const dialog = await screen.findByRole("dialog", { name: "Estimativa por ativo" });
    const stock = rowOf(dialog, "ABCD4");
    expect(within(stock).getByText(/^~R\$\s1,8\d$/)).toBeInTheDocument();
    expect(within(stock).getByText(/^bruto R\$\s2,00$/)).toBeInTheDocument();
    expect(dialog).toHaveTextContent(/~R\$\s54,25 por mês, líquido \(bruto ~R\$\s56,00\), 6 ativos/);
    expect(dialog).toHaveTextContent(/~R\$\s18,25\/mês de um ativo acima do habitual/);
    // Na folha a marca vem por extenso.
    expect(within(stock).getByText("acima do habitual")).toBeInTheDocument();
  });

  it("sem ninguém acima do habitual, a linha do total não aparece", async () => {
    const summary = netSummary();
    summary.estimate = estimateOf(summary.estimate.assets.map((a) => ({ ...a, above_usual: false })));
    const { panel } = await openEstimate(summary);
    expect(panel).not.toHaveTextContent(/acima/);
    expect(within(panel).queryByRole("button", { name: "O que é acima do habitual" })).not.toBeInTheDocument();
  });

  it("com dois ou mais marcados, o total diz quantos", async () => {
    const summary = netSummary();
    summary.estimate = estimateOf(summary.estimate.assets.map((a) => ({ ...a, above_usual: a.symbol !== "WXYZ3" })));
    const { panel } = await openEstimate(summary);
    expect(panel).toHaveTextContent(/~R\$\s28,25\/mês de 2 ativos acima do habitual/);
  });
});
