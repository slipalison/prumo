import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { analysisState, defaultAnalysis, result, scoreOf } from "../../test/analysisHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  return rendered;
}

/** A análise fictícia com só os avisos pedidos; o resto fica ok. */
function withWarnings(rules: string[], measured = true) {
  const base = defaultAnalysis();
  base.dimensions = base.dimensions.map((d) => {
    const next = d.rules.map((r) =>
      rules.includes(r.rule) ? r : result(r.rule, measured ? "ok" : "no_data", measured ? { current: 1 } : {}),
    );
    return { ...d, rules: next, score: scoreOf(next) };
  });
  base.attention_count = base.dimensions.flatMap((d) => d.rules).filter((r) => r.status === "warning").length;
  return base;
}

describe("Resumo da análise na Carteira", () => {
  it("conta os pontos a ver, mostra os dois primeiros e leva à tela", async () => {
    const { user } = await openPortfolio();
    const summary = await screen.findByRole("region", { name: "Análise da carteira" });
    expect(within(summary).getByRole("heading", { name: "Análise: 13 pontos a ver" })).toBeInTheDocument();
    const items = within(summary).getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual([
      "Tesouro IPCA+ 2035 é 70,76% da carteira",
      "Os 5 maiores ativos somam 97,84% da carteira",
      "e mais 11",
    ]);
    expect(within(summary).getByRole("link", { name: "Ver análise" })).toHaveAttribute("href", "#/analise");
    await user.click(within(summary).getByRole("link", { name: "Ver análise" }));
    expect(await screen.findByRole("heading", { name: "Análise da carteira", level: 1 })).toBeInTheDocument();
  });

  it("as regras novas entram no resumo com o número de agora", async () => {
    analysisState().analysis = withWarnings(["min_abroad", "max_loss", "min_paying_share"]);
    await openPortfolio();
    const summary = await screen.findByRole("region", { name: "Análise da carteira" });
    expect(within(summary).getByRole("heading", { name: "Análise: 3 pontos a ver" })).toBeInTheDocument();
    expect(within(summary).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Pelo menos 10% da carteira no exterior: agora 0%",
      "Pelo menos 70% dos ativos de renda variável pagaram provento em 12 meses: agora 50%",
      "e mais 1",
    ]);
  });

  it("diz quando não há ponto a ver, e quando ainda não há o que medir", async () => {
    analysisState().analysis = withWarnings([]);
    const first = await openPortfolio();
    expect(await screen.findByRole("heading", { name: "Análise: nenhum ponto a ver" })).toBeInTheDocument();
    first.unmount();

    analysisState().analysis = withWarnings([], false);
    await openPortfolio();
    expect(await screen.findByRole("heading", { name: "Análise: ainda sem o que medir" })).toBeInTheDocument();
  });

  it("um ponto só usa o singular e não tem 'e mais'", async () => {
    analysisState().analysis = withWarnings(["max_loss"]);
    await openPortfolio();
    expect(await screen.findByRole("heading", { name: "Análise: 1 ponto a ver" })).toBeInTheDocument();
    expect(screen.queryByText(/^e mais/)).not.toBeInTheDocument();
  });

  it("não atrapalha a carteira quando a análise não vem", async () => {
    server.use(http.get("/api/v1/analysis", () => HttpResponse.error()));
    await openPortfolio();
    expect(screen.getByRole("region", { name: "Fundos Imobiliários" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Análise da carteira" })).not.toBeInTheDocument();
  });
});
