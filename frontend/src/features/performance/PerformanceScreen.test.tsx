import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { NetWorth, Performance } from "../../api/performanceTypes";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { formatDay, formatSignedPercent, trend, versusText } from "./performanceFormat";

/** Série fictícia de três fechamentos. */
function worthFixture(overrides: Partial<NetWorth> = {}): NetWorth {
  return {
    period: "12m",
    starts_on: "2026-11-23",
    points: [
      { date: "2026-11-23", market_value: 1000, invested: 1000, gain: 0, partial: false },
      { date: "2026-11-24", market_value: 1510, invested: 1500, gain: 10, partial: false },
      { date: "2026-11-25", market_value: 1490, invested: 1500, gain: -10, partial: true },
    ],
    ...overrides,
  };
}

function perfFixture(overrides: Partial<Performance> = {}): Performance {
  return {
    period: "12m",
    starts_on: "2026-11-23",
    window: { from: "2026-11-23", to: "2026-11-25" },
    return_percent: 1.2345,
    benchmarks: [
      { code: "cdi", label: "CDI", return_percent: 0.1016, as_of: "2026-11-25" },
      { code: "ipca", label: "IPCA", return_percent: null, as_of: null },
      { code: "ibov", label: "Ibovespa", return_percent: -0.4321, as_of: "2026-11-24" },
    ],
    points: [
      { date: "2026-11-23", portfolio: 0, benchmarks: { cdi: 0, ipca: 0, ibov: 0 } },
      { date: "2026-11-24", portfolio: 1, benchmarks: { cdi: 0.05, ipca: null, ibov: -0.4321 } },
      { date: "2026-11-25", portfolio: 1.2345, benchmarks: { cdi: 0.1016, ipca: null, ibov: null } },
    ],
    summary: {
      twelve_months: {
        from: "2026-11-23",
        to: "2026-11-25",
        return_percent: 1.2345,
        cdi_percent: 0.1016,
        cdi_as_of: "2026-11-25",
        excess_over_cdi: 1.1317,
        percent_of_cdi: 1215,
        full_window: false,
      },
      since_start: {
        from: "2026-11-23",
        to: "2026-11-25",
        return_percent: -0.2,
        cdi_percent: 0.1016,
        cdi_as_of: "2026-11-25",
        excess_over_cdi: -0.3014,
        percent_of_cdi: null,
        full_window: true,
      },
    },
    years: [
      { year: 2025, months: [...Array<null>(11).fill(null), 0.5], return_percent: 0.5, cdi_percent: null },
      {
        year: 2026,
        months: [1, -0.25, ...Array<null>(9).fill(null), 1.2345],
        return_percent: 1.99,
        cdi_percent: 12.1,
      },
    ],
    ...overrides,
  };
}

/** Os caminhos pedidos, na ordem. */
let asked: string[] = [];

function serve(worth: NetWorth, perf: Performance) {
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
  );
}

async function open() {
  const rendered = renderApp("desempenho");
  await screen.findByRole("heading", { name: "Você está ganhando do CDI?" });
  return rendered;
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
      worthFixture({ starts_on: null, points: [] }),
      perfFixture({ starts_on: null, window: null, return_percent: null, benchmarks: [], points: [], years: [], summary: { twelve_months: null, since_start: null } }),
    );
    await open();
    const heading = await screen.findByRole("heading", { name: "Ainda não há série" });
    const card = heading.closest("section") as HTMLElement;
    expect(within(card).getByRole("link", { name: "Carteira" })).toHaveAttribute("href", "#/carteira");
  });

  it("série nova: diz a data de início em vez de inventar número", async () => {
    const day = { from: "2026-11-25", to: "2026-11-25", return_percent: 0, cdi_percent: null, cdi_as_of: null, excess_over_cdi: null, percent_of_cdi: null, full_window: false };
    serve(
      worthFixture({ starts_on: "2026-11-25", points: [{ date: "2026-11-25", market_value: 837, invested: 837, gain: 0, partial: true }] }),
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

  it("mostra os cartões contra o CDI, o patrimônio e a tabela", async () => {
    serve(worthFixture(), perfFixture());
    await open();
    const year = await screen.findByRole("region", { name: "Últimos 12 meses" });
    expect(within(year).getByText("+1,23%")).toBeInTheDocument();
    expect(within(year).getByText("1,13% acima do CDI")).toBeInTheDocument();
    expect(year).toHaveTextContent("(1.215% do CDI)");
    expect(year).toHaveTextContent("a série ainda não tem 12 meses");
    const since = screen.getByRole("region", { name: "Desde o início" });
    expect(within(since).getByText("−0,20%")).toBeInTheDocument();
    expect(within(since).getByText("0,30% abaixo do CDI")).toBeInTheDocument();

    const worth = screen.getByRole("region", { name: "Patrimônio no tempo" });
    expect(within(worth).getByText("R$ 1.500,00")).toBeInTheDocument();
    expect(within(worth).getByText(/−R\$\s10,00/)).toBeInTheDocument();
    expect(within(worth).getByRole("img", { name: /Patrimônio de 23\/11 a 25\/11/ })).toBeInTheDocument();
    expect(within(worth).getByText(/O ponto de hoje \(25\/11\) usa o preço do momento/)).toBeInTheDocument();

    const returns = screen.getByRole("region", { name: "Rentabilidade contra os índices" });
    expect(within(returns).getByRole("img", { name: /carteira \+1,23%, CDI \+0,10%, IPCA sem dado, Ibovespa −0,43%/ })).toBeInTheDocument();
    expect(within(returns).getByText(/Ibovespa até 24\/11\/2026/)).toBeInTheDocument();
    expect(within(returns).getByText(/Ainda sem dado no período: IPCA/)).toBeInTheDocument();

    const table = screen.getByRole("table");
    const rows = within(table).getAllByRole("row");
    expect(rows).toHaveLength(3);
    // Ano mais novo em cima.
    expect(within(rows[1] as HTMLElement).getByRole("rowheader")).toHaveTextContent("2026");
    expect(rows[1]).toHaveTextContent("−0,25%");
    expect(rows[1]).toHaveTextContent("+12,10%");
    expect(within(rows[1] as HTMLElement).getAllByText("sem dado")).toHaveLength(9);
    expect(rows[2]).toHaveTextContent("+0,50%");
  });

  it("olho fechado: R$ mascarado na tela, na régua e no que o leitor de tela lê; percentuais ficam", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    serve(worthFixture(), perfFixture());
    await open();
    const worth = await screen.findByRole("region", { name: "Patrimônio no tempo" });
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
    expect(within(worth).getByRole("img", { name: /R\$ ••••, com R\$ •••• aplicados/ })).toBeInTheDocument();
    // O ganho não diz o sentido com o olho fechado.
    expect(within(worth).queryByText(/^[+−]/)).toBeNull();
    // A rentabilidade continua à vista.
    expect(within(screen.getByRole("region", { name: "Últimos 12 meses" })).getByText("+1,23%")).toBeInTheDocument();
  });

  it("a legenda liga e desliga cada índice", async () => {
    serve(worthFixture(), perfFixture());
    const { user } = await open();
    const returns = await screen.findByRole("region", { name: "Rentabilidade contra os índices" });
    const plot = () => within(returns).getByTestId("chart-plot");
    expect(plot().querySelectorAll("[data-series]")).toHaveLength(4);
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

  it("troca o período pelo chip", async () => {
    serve(worthFixture(), perfFixture());
    const { user } = await open();
    await screen.findByRole("region", { name: "Últimos 12 meses" });
    await user.click(screen.getByRole("button", { name: "Desde o início" }));
    expect(screen.getByRole("button", { name: "Desde o início" })).toHaveAttribute("aria-pressed", "true");
    expect(asked).toContain("/api/v1/performance?period=12m&benchmarks=cdi,ipca,ibov");
    await waitFor(() => expect(asked).toContain("/api/v1/performance?period=all&benchmarks=cdi,ipca,ibov"));
    expect(asked).toContain("/api/v1/history/net-worth?period=all");
  });

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
    expect(await screen.findByRole("region", { name: "Patrimônio no tempo" })).toBeInTheDocument();
  });
});

describe("formatação do desempenho", () => {
  it("datas, sinais e comparação", () => {
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
