import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { YieldReport } from "../../api/incomeTypes";
import { yieldText } from "../../lib/yieldOnCost";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { emptyYieldReport, knownYield, sampleEvent, sampleSummary, unknownCostYield } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

function openSummary() {
  const summary = sampleSummary();
  server.use(
    http.get("/api/v1/income/summary", () => HttpResponse.json(summary)),
    http.get("/api/v1/income", () => HttpResponse.json([sampleEvent()])),
  );
  return summary;
}

describe("yieldText", () => {
  it("diz o percentual, e o motivo quando não há custo", () => {
    expect(yieldText({ percent: 6.25, gap: null, basis: "received" })).toBe("6,3%");
    expect(yieldText({ percent: 9, gap: null, basis: "estimate" })).toBe("9,0% (estimado)");
    expect(yieldText({ percent: null, gap: "unknown_cost", basis: "received" })).toBe("informe o preço médio");
    expect(yieldText({ percent: null, gap: "zero_cost", basis: "received" })).toBe("custo zero: sem base");
    expect(yieldText({ percent: null, gap: "no_position", basis: "received" })).toBe("sem posição");
    expect(yieldText({ percent: null, gap: null, basis: "received" })).toBe("—");
  });
});

describe("Renda: yield on cost", () => {
  it("mostra o yield de cada pagador e pede o preço médio de quem não tem", async () => {
    openSummary();
    renderApp("renda");
    const payers = await screen.findByRole("region", { name: "Quem pagou" });
    const knri = within(payers).getByText("KNRI11").closest("tr") as HTMLElement;
    expect(knri).toHaveTextContent(/yield on cost 7,\d%/);
    const mxrf = within(payers).getByText("MXRF11").closest("tr") as HTMLElement;
    expect(mxrf).toHaveTextContent("yield on cost informe o preço médio");
  });

  /** O número do yield da carteira: um dos quatro do topo, com a explicação no ⓘ. */
  async function yieldStat(label: string) {
    const stats = await screen.findByRole("region", { name: "Resumo dos últimos 12 meses" });
    const stat = within(stats).getByText(label, { selector: "dt" }).closest("div") as HTMLElement;
    return { stats, stat };
  }

  it("mostra o yield da carteira e diz quem ficou de fora", async () => {
    openSummary();
    const { user } = renderApp("renda");
    // Com custo desconhecido, o próprio nome do número diz que é parcial.
    const { stat } = await yieldStat("Yield on cost (parcial)");
    expect(stat).toHaveTextContent(/\d,\d%/);
    await user.click(within(stat).getByRole("button", { name: "Sobre Yield on cost (parcial)" }));
    const tip = within(stat).getByRole("status");
    expect(tip).toHaveTextContent("recebida");
    expect(tip).toHaveTextContent("Parcial: fica de fora o que está sem preço médio (MXRF11)");
  });

  it("sem recebido, o yield vem da estimativa e a tela avisa", async () => {
    const summary = sampleSummary();
    summary.yield_on_cost = { ...summary.yield_on_cost, basis: "estimate", percent: 9, partial: false, unknown_cost: [] };
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)));
    const { user } = renderApp("renda");
    const { stat } = await yieldStat("Yield on cost");
    expect(stat).toHaveTextContent("9,0% (estimado)");
    await user.click(within(stat).getByRole("button", { name: "Sobre Yield on cost" }));
    const tip = within(stat).getByRole("status");
    expect(tip).toHaveTextContent("estimada, não é extrato");
    expect(tip).not.toHaveTextContent("Parcial");
  });

  it("carteira sem custo conhecido diz o motivo no lugar do número", async () => {
    const summary = sampleSummary();
    summary.yield_on_cost = { ...summary.yield_on_cost, percent: null, gap: "unknown_cost", message: "informe o preco medio", partial: true, unknown_cost: ["KNRI11"] };
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)));
    renderApp("renda");
    const { stat } = await yieldStat("Yield on cost (parcial)");
    expect(stat).toHaveTextContent("informe o preço médio");
  });

  it("com o olho fechado, o percentual fica e o dinheiro some", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    openSummary();
    const { user } = renderApp("renda");
    const { stats, stat } = await yieldStat("Yield on cost (parcial)");
    expect(stat).toHaveTextContent(/\d,\d%/);
    await user.click(within(stat).getByRole("button", { name: "Sobre Yield on cost (parcial)" }));
    expect(within(stat).getByRole("status")).toHaveTextContent(MASK_BRL);
    expect(stats.textContent ?? "").not.toMatch(/R\$\s*\d/);
    const payers = await screen.findByRole("region", { name: "Quem pagou" });
    expect(within(payers).getAllByText(/yield on cost/).length).toBeGreaterThan(0);
    expect(payers.textContent ?? "").not.toMatch(/R\$\s*\d/);
  });
});

describe("Folha do ativo: yield on cost", () => {
  async function openAsset(symbol: RegExp, report: YieldReport) {
    server.use(http.get("/api/v1/income/yield-on-cost", () => HttpResponse.json(report)));
    const rendered = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await rendered.user.click(screen.getByRole("button", { name: symbol }));
    const dialog = await screen.findByRole("dialog");
    return { ...rendered, ledger: within(dialog).getByRole("region", { name: "Lançamentos" }) };
  }

  const idOf = (symbol: string) => {
    const asset = getDb().assets.find((a) => a.symbol === symbol);
    if (!asset) throw new Error(`fixture sem ${symbol}`);
    return asset.id;
  };

  it("mostra o yield do ativo que rendeu provento", async () => {
    const report = emptyYieldReport();
    report.assets = [{ asset_id: idOf("KNRI11"), ...knownYield(120, 2000) }];
    const { ledger } = await openAsset(/KNRI11/, report);
    await waitFor(() => expect(ledger).toHaveTextContent(/Yield on cost \(12 meses\)\s*6,0%/));
  });

  it("sem custo, no lugar do número vem 'informe o preço médio'", async () => {
    const report = emptyYieldReport();
    report.assets = [{ asset_id: idOf("HGLG11"), ...unknownCostYield(80) }];
    const { ledger } = await openAsset(/HGLG11/, report);
    await waitFor(() => expect(ledger).toHaveTextContent(/Yield on cost \(12 meses\)\s*informe o preço médio/));
  });

  it("ativo que não rendeu nada não ganha uma linha de 0,0%", async () => {
    const report = emptyYieldReport();
    report.assets = [{ asset_id: idOf("KNRI11"), ...knownYield(0, 2000) }];
    const { ledger } = await openAsset(/KNRI11/, report);
    await within(ledger).findByText("Preço médio");
    expect(ledger).not.toHaveTextContent("Yield on cost");
  });
});
