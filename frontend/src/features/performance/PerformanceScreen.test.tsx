import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import { REBUILD_POLL_MS, whileRebuilding } from "../../api/performance";
import type { ClassesPerformance, NetWorth, Performance, PerformanceYear } from "../../api/performanceTypes";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import {
  emptyClasses,
  sampleClasses,
  sampleNetWorth as worthFixture,
  samplePerformance as perfFixture,
} from "../../test/performanceHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { formatDay, formatSignedPercent, spansYears, trend, versusText } from "./performanceFormat";

/** Os caminhos pedidos, na ordem. */
let asked: string[] = [];

function serve(worth: NetWorth, perf: Performance, classes: ClassesPerformance = sampleClasses()) {
  asked = [];
  const path = (request: Request) => {
    const url = new URL(request.url);
    asked.push(url.pathname + url.search);
  };
  server.use(
    http.get("/api/v1/history/net-worth", ({ request }) => {
      path(request);
      return HttpResponse.json(worth);
    }),
    http.get("/api/v1/performance", ({ request }) => {
      path(request);
      return HttpResponse.json(perf);
    }),
    http.get("/api/v1/performance/classes", ({ request }) => {
      path(request);
      return HttpResponse.json(classes);
    }),
  );
}

async function open() {
  const rendered = renderApp("desempenho");
  await screen.findByRole("heading", { name: "Você está ganhando do CDI?" });
  return rendered;
}

/** Um dos números do topo (o rótulo é o `dt`; o número, o detalhe e o ⓘ ficam na mesma caixa). */
async function stat(label: string) {
  const summary = await screen.findByRole("region", { name: "Resumo do desempenho" });
  return within(summary).getByText(label, { selector: "dt" }).closest("div") as HTMLElement;
}

/** Abre o ⓘ de nome `name` dentro de `root` e devolve o texto que ele mostra. */
async function openTip(user: ReturnType<typeof userEvent.setup>, root: HTMLElement, name: string) {
  const button = within(root).getByRole("button", { name });
  await user.click(button);
  // Cada ⓘ tem a própria região de estado; a deste botão é a que ele controla.
  return document.getElementById(button.getAttribute("aria-controls") ?? "") as HTMLElement;
}

afterEach(() => vi.restoreAllMocks());

describe("Desempenho", () => {
  it("está na navegação e abre pela rota", async () => {
    serve(worthFixture(), perfFixture());
    await open();
    const nav = screen.getByRole("navigation", { name: "Principal" });
    expect(within(nav).getByRole("link", { name: "Desempenho" })).toHaveAttribute("aria-current", "page");
    expect(document.title).toBe("Desempenho · Prumo");
  });

  it("sem patrimônio, explica de onde a série começa", async () => {
    serve(
      worthFixture({ starts_on: null, points: [], summary: null }),
      perfFixture({ starts_on: null, window: null, return_percent: null, benchmarks: [], points: [], years: [], summary: { twelve_months: null, since_start: null } }),
      emptyClasses(),
    );
    await open();
    const heading = await screen.findByRole("heading", { name: "Ainda não há série" });
    const card = heading.closest("section") as HTMLElement;
    expect(within(card).getByRole("link", { name: "Carteira" })).toHaveAttribute("href", "#/carteira");
  });

  it("série nova: diz a data de início em vez de inventar número", async () => {
    const day = { from: "2026-11-25", to: "2026-11-25", return_percent: 0, cdi_percent: null, cdi_as_of: null, excess_over_cdi: null, percent_of_cdi: null, full_window: false };
    serve(
      worthFixture({
        starts_on: "2026-11-25",
        points: [{ date: "2026-11-25", market_value: 837, invested: 837, gain: 0, income: 0, partial: true }],
        summary: { from: null, to: "2026-11-25", start_value: 0, contributions: 837, market: 0, income: 0, end_value: 837, invested: 837, gain: 0, income_total: 0 },
      }),
      perfFixture({
        starts_on: "2026-11-25",
        window: { from: "2026-11-25", to: "2026-11-25" },
        return_percent: 0,
        points: [{ date: "2026-11-25", portfolio: 0, benchmarks: { cdi: 0, ipca: 0, ibov: 0 } }],
        summary: { twelve_months: day, since_start: { ...day, full_window: true } },
        years: [],
      }),
    );
    await open();
    expect(await screen.findByRole("note")).toHaveTextContent("A série começa em 25/11/2026.");
    expect(screen.getAllByText("Ainda não há dois fechamentos para comparar.")).toHaveLength(2);
    expect(screen.getByText("A linha aparece com dois fechamentos no período.")).toBeInTheDocument();
    expect(screen.getByText("A tabela aparece quando um mês tiver dois fechamentos.")).toBeInTheDocument();
    expect(screen.getByText("Hoje é parcial")).toBeInTheDocument();
  });

  it("mostra os números contra o CDI, o patrimônio e a tabela", async () => {
    serve(worthFixture(), perfFixture());
    const { user } = await open();
    const year = await stat("Últimos 12 meses");
    expect(within(year).getByText("+1,23%")).toBeInTheDocument();
    expect(year).toHaveTextContent("1,13% acima do CDI");
    expect(year).toHaveTextContent("(1.215% do CDI)");
    expect(year).toHaveTextContent("a série ainda não tem 12 meses");
    expect(await openTip(user, year, "Sobre Últimos 12 meses")).toHaveTextContent(/De 23\/11\/2026 a 25\/11\/2026\. O CDI rendeu \+0,10% no período\./);
    const since = await stat("Desde o início");
    expect(within(since).getByText("−0,20%")).toBeInTheDocument();
    expect(since).toHaveTextContent("0,30% abaixo do CDI");

    expect(await stat("Aplicado")).toHaveTextContent("R$ 1.500,00");
    expect(await stat("Proventos")).toHaveTextContent("R$ 12,50");
    const gain = await stat("Ganho");
    expect(gain).toHaveTextContent(/−R\$\s10,00/);
    expect(await openTip(user, gain, "Sobre Ganho")).toHaveTextContent(/os proventos recebidos ficam ao lado/);

    const worth = screen.getByRole("region", { name: "Patrimônio × aportado" });
    expect(within(worth).getByText("Hoje é parcial")).toBeInTheDocument();
    expect(within(worth).getByRole("img", { name: /Patrimônio de 23\/11 a 25\/11: R\$\s1\.490,00, com R\$\s1\.500,00 aportados/ })).toBeInTheDocument();
    expect(await openTip(user, worth, "Como ler patrimônio × aportado")).toHaveTextContent(/O ponto de hoje \(25\/11\) usa o preço do momento/);

    const returns = screen.getByRole("region", { name: "Rentabilidade contra os índices" });
    expect(within(returns).getByRole("img", { name: /carteira \+1,23%, CDI \+0,10%, IPCA sem dado, Ibovespa −0,43%, IFIX \+0,61%/ })).toBeInTheDocument();
    const notes = await openTip(user, returns, "De quando é cada índice");
    expect(notes).toHaveTextContent(/Ibovespa até 24\/11\/2026/);
    expect(notes).toHaveTextContent(/Ainda sem dado no período: IPCA/);

    const years = screen.getByRole("region", { name: "Ano a ano" });
    const rows = within(within(years).getByRole("table")).getAllByRole("row");
    // Cabeçalho, e cada ano com a linha do CDI embaixo.
    expect(rows).toHaveLength(5);
    // Ano mais novo em cima.
    expect(within(rows[1] as HTMLElement).getByRole("rowheader")).toHaveTextContent("2026");
    expect(rows[1]).toHaveTextContent("−0,25%");
    expect(within(rows[1] as HTMLElement).getAllByText("sem dado")).toHaveLength(9);
    expect(within(rows[2] as HTMLElement).getByRole("rowheader")).toHaveTextContent("2026, CDI");
    expect(rows[2]).toHaveTextContent("+12,10%");
    expect(rows[3]).toHaveTextContent("+0,50%");
  });

  it("olho fechado: R$ mascarado na tela, na régua e no que o leitor de tela lê; percentuais ficam", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    serve(worthFixture(), perfFixture());
    await open();
    const worth = await screen.findByRole("region", { name: "Patrimônio × aportado" });
    await within(await screen.findByRole("region", { name: "Por classe" })).findByText("Fundos Imobiliários");
    const plot = within(worth).getByTestId("chart-plot");
    vi.spyOn(plot, "getBoundingClientRect").mockReturnValue({
      left: 0, top: 0, width: 300, height: 220, right: 300, bottom: 220, x: 0, y: 0, toJSON: () => ({}),
    });
    fireEvent.pointerMove(plot, { clientX: 150 });
    expect(plot.querySelector(".chart__tooltip")).toHaveTextContent("PatrimônioR$ ••••");
    // Nada que se vê ou se ouve na tela traz um valor em reais.
    const main = document.querySelector("main") as HTMLElement;
    const heard = [main.textContent ?? ""];
    for (const element of main.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "title", "alt"]) {
        const value = element.getAttribute(attribute);
        if (value) heard.push(value);
      }
    }
    for (const text of heard) expect(text).not.toMatch(/R\$\s*\d/);
    expect(within(worth).getByRole("img", { name: /R\$ ••••, com R\$ •••• aportados/ })).toBeInTheDocument();
    // O ganho e o mercado não dizem o sentido com o olho fechado.
    expect(within(await stat("Ganho")).queryByText(/^[+−]/)).toBeNull();
    const market = within(worth).getByText("Mercado", { selector: "dt" }).closest("div") as HTMLElement;
    expect(within(market).getByText("R$ ••••")).toBeInTheDocument();
    expect(market).not.toHaveClass("stat--down");
    // A rentabilidade continua à vista.
    expect(within(await stat("Últimos 12 meses")).getByText("+1,23%")).toBeInTheDocument();
  });

  it("a legenda liga e desliga cada índice", async () => {
    serve(worthFixture(), perfFixture());
    const { user } = await open();
    const returns = await screen.findByRole("region", { name: "Rentabilidade contra os índices" });
    const plot = () => within(returns).getByTestId("chart-plot");
    expect(plot().querySelectorAll("[data-series]")).toHaveLength(5);
    expect(plot().querySelector('[data-series="ifix"]')).not.toBeNull();
    const ibov = within(returns).getByRole("button", { name: /Ibovespa/ });
    expect(ibov).toHaveAttribute("aria-pressed", "true");
    await user.click(ibov);
    expect(ibov).toHaveAttribute("aria-pressed", "false");
    expect(plot().querySelector('[data-series="ibov"]')).toBeNull();
    await user.click(ibov);
    expect(plot().querySelector('[data-series="ibov"]')).not.toBeNull();
  });

  it("o ponteiro mostra o valor de cada linha no dia", async () => {
    serve(worthFixture(), perfFixture());
    await open();
    const returns = await screen.findByRole("region", { name: "Rentabilidade contra os índices" });
    const plot = within(returns).getByTestId("chart-plot");
    vi.spyOn(plot, "getBoundingClientRect").mockReturnValue({
      left: 0, top: 0, width: 300, height: 220, right: 300, bottom: 220, x: 0, y: 0, toJSON: () => ({}),
    });
    fireEvent.pointerMove(plot, { clientX: 150 });
    const tooltip = plot.querySelector(".chart__tooltip");
    expect(tooltip).toHaveTextContent("24/11/2026");
    expect(tooltip).toHaveTextContent("Sua carteira+1,0%");
    expect(tooltip).toHaveTextContent("IPCA—");
    fireEvent.pointerMove(plot, { clientX: 300 });
    expect(plot.querySelector(".chart__tooltip")).toHaveClass("is-left");
    fireEvent.pointerLeave(plot);
    expect(plot.querySelector(".chart__tooltip")).toBeNull();
  });

  it("troca o período pelo chip, e o período vale para todos os painéis", async () => {
    serve(worthFixture(), perfFixture());
    const { user } = await open();
    await stat("Últimos 12 meses");
    expect(asked).toContain("/api/v1/performance?period=12m&benchmarks=cdi,ipca,ibov,ifix");
    for (const [label, period] of [
      ["Mês", "month"],
      ["Ano atual", "ytd"],
      ["12 meses", "12m"],
      ["2 anos", "2y"],
      ["5 anos", "5y"],
      ["Tudo", "all"],
    ] as const) {
      await user.click(screen.getByRole("button", { name: label }));
      await waitFor(() => expect(asked).toContain(`/api/v1/performance?period=${period}&benchmarks=cdi,ipca,ibov,ifix`));
      expect(asked).toContain(`/api/v1/history/net-worth?period=${period}`);
      await waitFor(() => expect(asked).toContain(`/api/v1/performance/classes?period=${period}`));
      expect(screen.getByRole("button", { name: label })).toHaveAttribute("aria-pressed", "true");
    }
  });

  it("série longa vinda do livro: datas com ano e uma linha por ano na tabela", async () => {
    const dates = daysBetween("2023-03-01", "2026-11-25");
    const worth = worthFixture({
      period: "all",
      starts_on: "2023-03-01",
      points: dates.map((date, i) => ({
        date,
        market_value: 10000 + i * 10,
        invested: 10000 + i * 5,
        gain: i * 5,
        income: i,
        partial: i === dates.length - 1,
      })),
    });
    const years: PerformanceYear[] = [2023, 2024, 2025, 2026].map((year) => {
      const months = Array.from({ length: 12 }, (_, m) => ((year === 2023 && m < 2) || (year === 2026 && m > 10) ? null : 0.8));
      return {
        year,
        months,
        return_percent: 9.6,
        cdi_percent: 11.2,
        accumulated: months,
        accumulated_percent: 9.6,
        benchmarks: { cdi: { months, accumulated: months, return_percent: 11.2, accumulated_percent: 11.2, as_of: `${year}-12-31` } },
      };
    });
    const perf = perfFixture({
      starts_on: "2023-03-01",
      window: { from: "2023-03-01", to: "2026-11-25" },
      return_percent: 38.5,
      points: dates.map((date, i) => ({ date, portfolio: i / 36, benchmarks: { cdi: i / 30, ipca: i / 60, ibov: null } })),
      years,
    });
    serve(worth, perf);
    await open();
    const worthCard = await screen.findByRole("region", { name: "Patrimônio × aportado" });
    expect(within(worthCard).getByRole("img", { name: /Patrimônio de 01\/03\/2023 a 25\/11\/2026/ })).toBeInTheDocument();
    // As pontas do eixo levam o ano.
    expect(worthCard.querySelector(".chart__dates")).toHaveTextContent("01/03/2023");
    expect(await stat("Proventos")).toHaveTextContent(`R$ ${(dates.length - 1).toLocaleString("pt-BR")},00`);
    const returns = screen.getByRole("region", { name: "Rentabilidade contra os índices" });
    expect(within(returns).getByRole("img", { name: /de 01\/03\/2023 a 25\/11\/2026/ })).toBeInTheDocument();

    const yearPanel = screen.getByRole("region", { name: "Ano a ano" });
    const rows = within(within(yearPanel).getByRole("table")).getAllByRole("row");
    expect(rows).toHaveLength(9);
    expect(within(rows[1] as HTMLElement).getByRole("rowheader")).toHaveTextContent("2026");
    expect(within(rows[7] as HTMLElement).getByRole("rowheader")).toHaveTextContent("2023");
    expect(within(rows[7] as HTMLElement).getAllByText("sem dado")).toHaveLength(2);
  });

  it("série sendo refeita: avisa desde quando e pergunta de novo até acabar", async () => {
    let rebuilding: string | null = "2025-03-04";
    server.use(
      http.get("/api/v1/history/net-worth", () => HttpResponse.json(worthFixture({ rebuilding_since: rebuilding }))),
      http.get("/api/v1/performance", () => HttpResponse.json(perfFixture({ rebuilding_since: rebuilding }))),
    );
    await open();
    expect(await screen.findByText(/Refazendo a série desde 04\/03\/2025/)).toBeInTheDocument();
    rebuilding = null;
    await waitFor(() => expect(screen.queryByText(/Refazendo a série/)).toBeNull(), { timeout: REBUILD_POLL_MS + 3000 });
  }, REBUILD_POLL_MS + 8000);

  it("erro na API: mensagem e tentar de novo", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/history/net-worth", () =>
        fail ? HttpResponse.json({ code: "internal" }, { status: 500 }) : HttpResponse.json(worthFixture()),
      ),
      http.get("/api/v1/performance", () => HttpResponse.json(perfFixture())),
    );
    const { user } = await open();
    expect(await screen.findByRole("alert")).toHaveTextContent("Não consegui carregar o desempenho");
    fail = false;
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("region", { name: "Patrimônio × aportado" })).toBeInTheDocument();
  });
});

/** Todo dia corrido de `from` a `to`, como a série guarda. */
function daysBetween(from: string, to: string): string[] {
  const out: string[] = [];
  for (let day = new Date(`${from}T12:00:00Z`); day <= new Date(`${to}T12:00:00Z`); day.setUTCDate(day.getUTCDate() + 1)) {
    out.push(day.toISOString().slice(0, 10));
  }
  return out;
}

describe("formatação do desempenho", () => {
  it("pergunta de novo só enquanto a série é refeita", () => {
    expect(whileRebuilding({ rebuilding_since: "2025-03-04" })).toBe(REBUILD_POLL_MS);
    expect(whileRebuilding({ rebuilding_since: null })).toBe(false);
    expect(whileRebuilding(undefined)).toBe(false);
  });

  it("datas, sinais e comparação", () => {
    expect(spansYears("2025-12-31", "2026-01-02")).toBe(true);
    expect(spansYears("2026-01-01", "2026-11-25")).toBe(false);
    expect(spansYears(undefined, "2026-11-25")).toBe(false);
    expect(formatDay("2026-11-05")).toBe("05/11");
    expect(formatDay("2026-11-05", true)).toBe("05/11/2026");
    expect(formatDay("x")).toBe("x");
    expect(formatSignedPercent(1.234)).toBe("+1,23%");
    expect(formatSignedPercent(-0.5, 1)).toBe("−0,5%");
    expect(formatSignedPercent(0.001)).toBe("0,00%");
    expect(trend(null)).toBe("flat");
    expect(trend(0.001)).toBe("flat");
    expect(trend(2)).toBe("up");
    expect(trend(-2)).toBe("down");
    expect(versusText(0)).toBe("igual ao CDI");
    expect(versusText(1.5)).toBe("1,50% acima do CDI");
  });
});
