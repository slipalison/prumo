import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import type { IncomeProjection } from "../../api/projectionTypes";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { sampleSummary } from "../../test/incomeHandlers";
import { fakeObjectives, sampleIncomeObjective } from "../../test/objectivesHandlers";
import { emptyProjection, sampleProjection, sampleProjectionHandler } from "../../test/projectionHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

async function openIncome() {
  const rendered = renderApp("renda");
  await screen.findByRole("heading", { name: "Quanto sua carteira paga?" });
  const panel = await screen.findByRole("region", { name: "Renda projetada" });
  return { ...rendered, panel };
}

/** As consultas que a tela fez à projeção, com os parâmetros. */
function trackProjection(body: (url: URL) => IncomeProjection = (url) => sampleProjection(url.searchParams.get("months") === "24" ? 24 : 12)) {
  const urls: URL[] = [];
  server.use(
    http.get("/api/v1/income/projection", ({ request }) => {
      const url = new URL(request.url);
      urls.push(url);
      return HttpResponse.json(body(url));
    }),
  );
  return urls;
}

const box = { left: 0, width: 240, top: 0, height: 150, right: 240, bottom: 150, x: 0, y: 0, toJSON: () => ({}) };

describe("Renda: renda projetada", () => {
  it("mostra os três cenários, o mês a mês, os tipos e as premissas com a data do Focus", async () => {
    server.use(sampleProjectionHandler);
    const { user, panel } = await openIncome();
    const stats = await within(panel).findByText("Base, 12 meses");
    expect(stats.closest(".stat")).toHaveTextContent(/R\$\s[\d.]+,\d{2}/);
    expect(within(panel).getByText("Pessimista", { selector: "dt" })).toBeInTheDocument();
    expect(within(panel).getByText("Otimista", { selector: "dt" })).toBeInTheDocument();

    const types = within(panel).getByRole("table", { name: "Renda projetada por tipo" });
    expect(within(types).getByRole("rowheader", { name: "FII de papel" })).toBeInTheDocument();
    expect(within(types).getByText("acompanha o CDI esperado")).toBeInTheDocument();
    expect(within(types).getByText("reajuste pelo IPCA esperado")).toBeInTheDocument();
    expect(within(types).getByText("a média, sem crescimento")).toBeInTheDocument();

    const premises = within(panel).getByRole("list", { name: "Premissas da projeção" });
    expect(premises).toHaveTextContent("CDI de 14,90% hoje a 13,40% em nov/2027 (meta Selic do Focus menos 0,10 ponto).");
    expect(premises).toHaveTextContent("Base do FII de papel: o CDI médio dos 12 meses que passaram, 14,60%.");
    expect(premises).toHaveTextContent("IPCA esperado de 4,25% ao ano (Focus).");
    expect(premises).toHaveTextContent("Cenários: 2 pontos ao ano para cada lado (padrão).");
    expect(premises).toHaveTextContent("Focus do Banco Central, pesquisa de 19/11/2026.");

    // Por ativo, com a base de cada um.
    await user.click(within(panel).getByText("Por ativo"));
    const assets = within(panel).getByRole("table", { name: "Renda projetada por ativo" });
    expect(within(assets).getByRole("rowheader", { name: "ACAO3" })).toBeInTheDocument();
    expect(within(assets).getByText("média de 5 anos")).toBeInTheDocument();
    expect(within(assets).getAllByText("12 meses")).toHaveLength(2);

    // O leitor de tela tem a tabela de cada mês; o desenho tem o resumo.
    expect(within(panel).getByRole("table", { name: "Renda projetada por mês" })).toBeInTheDocument();
    expect(within(panel).getByRole("img").getAttribute("aria-label")).toMatch(/de dez\/2026 a nov\/2027/);

    // O ponteiro sobre as barras mostra o mês, a faixa e cada tipo.
    const plot = within(panel).getByTestId("projection-bars");
    vi.spyOn(plot, "getBoundingClientRect").mockReturnValue(box);
    fireEvent.pointerMove(plot, { clientX: 230 });
    expect(within(panel).getByText("Faixa", { exact: false })).toBeInTheDocument();
    expect(within(panel).getAllByText("FII de tijolo", { exact: false }).length).toBeGreaterThan(1);
    fireEvent.pointerLeave(plot);
  });

  it("troca o horizonte para 24 meses", async () => {
    const urls = trackProjection();
    const { user, panel } = await openIncome();
    await within(panel).findByText("Base, 12 meses");
    await user.click(within(panel).getByRole("button", { name: "24 meses" }));
    expect(await within(panel).findByText("Base, 24 meses")).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "24 meses" })).toHaveAttribute("aria-pressed", "true");
    expect(urls.at(-1)?.searchParams.get("months")).toBe("24");
  });

  it("refaz a conta com o CDI, o IPCA e a amplitude da pessoa, e volta ao Focus", async () => {
    const urls = trackProjection();
    const { user, panel } = await openIncome();
    await within(panel).findByText("Base, 12 meses");
    await user.click(within(panel).getByText("Trocar premissas"));
    const cdi = within(panel).getByRole("textbox", { name: "CDI (% ao ano)" });
    await user.click(cdi);
    await user.paste("abc");
    expect(within(panel).getByRole("alert")).toHaveTextContent("Use um número de 0 a 50.");
    expect(within(panel).getByRole("button", { name: "Refazer a conta" })).toBeDisabled();
    await user.clear(cdi);
    await user.paste("10,5");
    await user.click(within(panel).getByRole("textbox", { name: "IPCA (% ao ano)" }));
    await user.paste("5");
    await user.click(within(panel).getByRole("textbox", { name: "Amplitude (pontos ao ano)" }));
    await user.paste("3");
    await user.click(within(panel).getByRole("button", { name: "Refazer a conta" }));
    await waitFor(() => expect(urls.at(-1)?.search).toBe("?months=12&spread=3&cdi=10.5&ipca=5"));
    expect(within(panel).getByText("trocadas")).toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "Voltar ao Focus" }));
    await waitFor(() => expect(urls.at(-1)?.search).toBe("?months=12"));
  });

  it("diz de onde vem cada premissa quando não é o Focus", async () => {
    trackProjection(() => {
      const p = sampleProjection();
      return {
        ...p,
        premises: {
          ...p.premises,
          spread_source: "informed",
          scenario_spread_percent: 1,
          cdi: { source: "missing", now_percent: null, trailing_percent: null, end_percent: null },
          ipca: { source: "fallback", annual_percent: 3 },
          focus: null,
        },
        missing: ["XPTO11"],
      };
    });
    const { panel } = await openIncome();
    const premises = await within(panel).findByRole("list", { name: "Premissas da projeção" });
    expect(premises).toHaveTextContent("Sem CDI no banco: a renda de papel fica parada.");
    expect(premises).not.toHaveTextContent("Base do FII de papel");
    expect(premises).toHaveTextContent("IPCA na meta de inflação: 3,00% ao ano");
    expect(premises).toHaveTextContent("1 ponto ao ano para cada lado (seu)");
    expect(premises).toHaveTextContent("Ainda sem Focus no banco: a busca é diária.");
    expect(within(panel).getByText(/Sem estimativa, fora da conta: XPTO11/)).toBeInTheDocument();
  });

  it("sem ativo com estimativa, explica que não há o que projetar", async () => {
    server.use(http.get("/api/v1/income/projection", () => HttpResponse.json(emptyProjection())));
    const { panel } = await openIncome();
    expect(await within(panel).findByText(/não há o que projetar/)).toBeInTheDocument();
  });

  it("se a projeção falha, o painel diz e a Renda segue", async () => {
    server.use(http.get("/api/v1/income/projection", () => new HttpResponse(null, { status: 500 })));
    const { panel } = await openIncome();
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Não consegui carregar a projeção agora.");
    expect(screen.getByRole("heading", { name: "Quanto sua carteira paga?" })).toBeInTheDocument();
  });

  it("com o olho fechado, nenhum valor aparece, nem no balão nem na tabela do leitor de tela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(sampleProjectionHandler);
    const { user, panel } = await openIncome();
    await within(panel).findByText("Base, 12 meses");
    await user.click(within(panel).getByText("Por ativo"));
    const plot = within(panel).getByTestId("projection-bars");
    vi.spyOn(plot, "getBoundingClientRect").mockReturnValue(box);
    fireEvent.pointerMove(plot, { clientX: 100 });
    expect(panel.textContent).not.toMatch(MONEY);
    for (const element of panel.querySelectorAll("[aria-label]")) {
      expect(element.getAttribute("aria-label")).not.toMatch(MONEY);
    }
    // Percentuais e datas continuam.
    expect(panel).toHaveTextContent("14,90%");
  });
});

describe("Renda: a data da meta de renda", () => {
  it("o cartão da meta mostra o cenário base e leva às premissas", async () => {
    const summary = sampleSummary();
    server.use(
      http.get("/api/v1/income/summary", () => HttpResponse.json(summary)),
      ...fakeObjectives([sampleIncomeObjective({ id: summary.goal?.id ?? "g-1" })]).handlers,
    );
    await openIncome();
    const card = screen.getByRole("region", { name: "Meta de renda mensal" });
    expect(await within(card).findByText(/Cenário base: mar\/2031\. Faixa: mar\/2030 no otimista, set\/2032 no pessimista\./)).toBeInTheDocument();
    expect(within(card).getByRole("link", { name: "Ver projeção e premissas" })).toHaveAttribute("href", "#/ajustes/objetivos");
  });

  it("no piso, o cartão diz que é o menor mês", async () => {
    const summary = sampleSummary();
    if (summary.goal) summary.goal = { ...summary.goal, mode: "floor" };
    server.use(
      http.get("/api/v1/income/summary", () => HttpResponse.json(summary)),
      ...fakeObjectives([
        sampleIncomeObjective({ id: "g-1", income_mode: "floor", projection: { state: "reached", months: null, month: null } }),
      ]).handlers,
    );
    const { user } = await openIncome();
    const card = screen.getByRole("region", { name: "Meta de renda mensal" });
    expect(within(card).getByText("no menor mês", { exact: false })).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Como a renda da meta é medida" }));
    expect(within(card).getByRole("status")).toHaveTextContent(/O menor mês recebido nos 12 meses fechados/);
  });
});
