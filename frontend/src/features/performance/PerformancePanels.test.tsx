import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { ClassesPerformance, NetWorth, Performance } from "../../api/performanceTypes";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { sampleClasses, sampleNetWorth, samplePerformance } from "../../test/performanceHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { monthLabel, signedMoney } from "./performanceFormat";
import { createMoney } from "../../lib/money";

/** Os três pedidos da tela; o das classes pode falhar sozinho. */
function serve(
  worth: NetWorth = sampleNetWorth(),
  perf: Performance = samplePerformance(),
  classes: ClassesPerformance | (() => Response) = sampleClasses(),
) {
  server.use(
    http.get("/api/v1/history/net-worth", () => HttpResponse.json(worth)),
    http.get("/api/v1/performance", () => HttpResponse.json(perf)),
    http.get("/api/v1/performance/classes", () =>
      typeof classes === "function" ? classes() : HttpResponse.json(classes),
    ),
  );
}

async function open() {
  const rendered = renderApp("desempenho");
  await screen.findByRole("heading", { name: "Você está ganhando do CDI?" });
  return rendered;
}

/** Um número de uma faixa (o rótulo é o `dt`). */
function statIn(root: HTMLElement, label: string): HTMLElement {
  return within(root).getByText(label, { selector: "dt" }).closest("div") as HTMLElement;
}

describe("Patrimônio × aportado", () => {
  it("desde o começo: o aportado, o mercado e os proventos", async () => {
    serve();
    await open();
    const panel = await screen.findByRole("region", { name: "Patrimônio × aportado" });
    expect(within(panel).queryByText("Na base")).toBeNull();
    expect(statIn(panel, "Aportado")).toHaveTextContent(/\+R\$\s1\.500,00desde o começo/);
    const market = statIn(panel, "Mercado");
    expect(market).toHaveTextContent(/−R\$\s10,00/);
    expect(market).toHaveClass("stat--down");
    expect(statIn(panel, "Proventos")).toHaveTextContent(/R\$\s12,50/);
    // A legenda nomeia as duas linhas.
    expect(within(panel).getByText("Aportado", { selector: ".legend__item span" })).toBeInTheDocument();
  });

  it("num período: a base, os aportes do período e o que o mercado fez", async () => {
    serve(
      sampleNetWorth({
        period: "ytd",
        summary: {
          from: "2025-12-31",
          to: "2026-11-25",
          start_value: 1000,
          contributions: -200,
          market: 690,
          income: 40,
          end_value: 1490,
          invested: 1500,
          gain: -10,
          income_total: 52.5,
        },
      }),
    );
    await open();
    const panel = await screen.findByRole("region", { name: "Patrimônio × aportado" });
    expect(statIn(panel, "Na base")).toHaveTextContent(/R\$\s1\.000,0031\/12\/2025/);
    expect(statIn(panel, "Aportes")).toHaveTextContent(/−R\$\s200,00no período/);
    expect(statIn(panel, "Mercado")).toHaveClass("stat--up");
    expect(statIn(panel, "Mercado")).toHaveTextContent(/\+R\$\s690,00/);
    expect(statIn(panel, "Proventos")).toHaveTextContent(/R\$\s40,00no período/);
  });

  it("sem ponte (série vazia no período) não inventa número", async () => {
    serve(sampleNetWorth({ summary: null }));
    await open();
    const panel = await screen.findByRole("region", { name: "Patrimônio × aportado" });
    expect(within(panel).queryByText("Mercado")).toBeNull();
  });
});

describe("Por classe", () => {
  it("peso, rentabilidade e o índice da classe; a classe que entrou no meio diz desde quando", async () => {
    serve();
    await open();
    const panel = await screen.findByRole("region", { name: "Por classe" });
    const rows = within(await within(panel).findByRole("table")).getAllByRole("row");
    expect(rows).toHaveLength(3);
    expect(rows[1]).toHaveTextContent("Fundos Imobiliários");
    expect(rows[1]).toHaveTextContent("69,3%");
    expect(rows[1]).toHaveTextContent("+2,15%");
    expect(rows[1]).toHaveTextContent("IFIX +0,61%");
    expect(rows[1]).toHaveTextContent(/R\$\s1\.032,50/);
    expect(rows[2]).toHaveTextContent("Criptomoedas");
    expect(rows[2]).toHaveTextContent("desde 24/11");
    expect(rows[2]).toHaveTextContent("−1,50%");
    expect(within(rows[2] as HTMLElement).getByText("sem índice para a classe")).toBeInTheDocument();
  });

  it("a classe que zerou no período diz até quando; índice sem dado é dito", async () => {
    serve(
      undefined,
      undefined,
      sampleClasses({
        window: { from: "2025-11-25", to: "2026-11-25" },
        classes: [
          {
            asset_class: "br_stocks",
            market_value: 0,
            weight_percent: 0,
            return_percent: 3.2,
            from: "2025-11-25",
            to: "2026-03-10",
            full_window: false,
            benchmark: { code: "ibov", label: "Ibovespa", return_percent: null, as_of: null },
          },
        ],
      }),
    );
    await open();
    const panel = await screen.findByRole("region", { name: "Por classe" });
    const table = await within(panel).findByRole("table");
    expect(table).toHaveTextContent("até 10/03/2026");
    expect(table).toHaveTextContent("Ibovespa sem dado");
  });

  it("sem classe no período, diz isso; erro tem como tentar de novo", async () => {
    let fail = true;
    serve(undefined, undefined, () =>
      fail
        ? HttpResponse.json({ code: "internal", message: "Falhou." }, { status: 500 })
        : HttpResponse.json(sampleClasses({ classes: [] })),
    );
    const { user } = await open();
    const panel = await screen.findByRole("region", { name: "Por classe" });
    expect(await within(panel).findByRole("alert")).toHaveTextContent("Não consegui carregar as classes");
    fail = false;
    await user.click(within(panel).getByRole("button", { name: "Tentar de novo" }));
    expect(await within(panel).findByText("Nenhuma classe teve patrimônio no período.")).toBeInTheDocument();
  });

  it("olho fechado: o patrimônio da classe sai mascarado; peso e rentabilidade ficam", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    serve();
    await open();
    const panel = await screen.findByRole("region", { name: "Por classe" });
    const table = await within(panel).findByRole("table");
    expect(table).not.toHaveTextContent(/R\$\s*\d/);
    expect(within(table).getAllByText("R$ ••••")).toHaveLength(2);
    expect(table).toHaveTextContent("69,3%");
  });
});

describe("Ano a ano", () => {
  it("melhor e pior mês, contagens e meses acima do índice escolhido", async () => {
    serve();
    const { user } = await open();
    const panel = await screen.findByRole("region", { name: "Ano a ano" });
    expect(statIn(panel, "Melhor mês")).toHaveTextContent("+1,23%nov/2026");
    expect(statIn(panel, "Pior mês")).toHaveTextContent("−0,25%fev/2026");
    expect(statIn(panel, "Meses positivos")).toHaveTextContent("3 de 41 negativo");
    expect(statIn(panel, "Acima do CDI")).toHaveTextContent("1 de 3meses com dado do índice");

    await user.selectOptions(within(panel).getByRole("combobox", { name: "Comparar com" }), "ifix");
    expect(statIn(panel, "Acima do IFIX")).toHaveTextContent("3 de 4meses com dado do índice");
    const rows = within(within(panel).getByRole("table")).getAllByRole("row");
    expect(within(rows[2] as HTMLElement).getByRole("rowheader")).toHaveTextContent("2026, IFIX");
    expect(rows[2]).toHaveTextContent("+0,70%");
    expect(rows[2]).toHaveTextContent("+6,10%");

    await user.selectOptions(within(panel).getByRole("combobox", { name: "Comparar com" }), "none");
    expect(within(panel).queryByText(/^Acima do/)).toBeNull();
    expect(within(within(panel).getByRole("table")).getAllByRole("row")).toHaveLength(3);
  });

  it("modo acumulado: desde o começo até o fim de cada mês", async () => {
    serve();
    const { user } = await open();
    const panel = await screen.findByRole("region", { name: "Ano a ano" });
    const table = () => within(panel).getByRole("table");
    expect(within(table()).getByRole("columnheader", { name: "No ano" })).toBeInTheDocument();
    await user.click(within(panel).getByRole("button", { name: "Acumulado" }));
    expect(within(panel).getByRole("button", { name: "Acumulado" })).toHaveAttribute("aria-pressed", "true");
    expect(within(table()).getByRole("columnheader", { name: "Acumulado" })).toBeInTheDocument();
    const rows = within(table()).getAllByRole("row");
    // Uma casa no acumulado.
    expect(rows[1]).toHaveTextContent("+1,5%");
    expect(rows[1]).toHaveTextContent("+2,5%");
    await user.click(within(panel).getByRole("button", { name: "Ano" }));
    expect(within(table()).getAllByRole("row")[1]).toHaveTextContent("+1,99%");
  });

  it("índice publicado só até antes do fim (o IPCA) é avisado; sem dado do índice, nada a contar", async () => {
    const perf = samplePerformance();
    perf.months_summary = {
      ...perf.months_summary,
      flat: 1,
      versus: [{ code: "ipca", label: "IPCA", above: 0, compared: 0 }],
    };
    serve(undefined, perf);
    const { user } = await open();
    const panel = await screen.findByRole("region", { name: "Ano a ano" });
    await user.selectOptions(within(panel).getByRole("combobox", { name: "Comparar com" }), "ipca");
    expect(await within(panel).findByText("IPCA publicado até 31/10/2026.")).toBeInTheDocument();
    expect(statIn(panel, "Acima do IPCA")).toHaveTextContent("—sem dado do índice");
    expect(statIn(panel, "Meses positivos")).toHaveTextContent("3 de 51 negativo, 1 no zero");
  });

  it("série nova: sem mês fechado, a tabela espera", async () => {
    serve(
      undefined,
      samplePerformance({
        years: [],
        months_summary: { best: null, worst: null, positive: 0, negative: 0, flat: 0, versus: [] },
      }),
    );
    await open();
    const panel = await screen.findByRole("region", { name: "Ano a ano" });
    await waitFor(() =>
      expect(within(panel).getByText("A tabela aparece quando um mês tiver dois fechamentos.")).toBeInTheDocument(),
    );
    expect(within(panel).queryByRole("table")).toBeNull();
  });
});

describe("formatação nova do desempenho", () => {
  it("mês por extenso curto e dinheiro com sinal", () => {
    expect(monthLabel(3, 2025)).toBe("mar/2025");
    expect(monthLabel(13, 2025)).toBe("13/2025");
    const money = createMoney(false);
    expect(signedMoney(money, 10)).toMatch(/^\+R\$\s10,00$/);
    expect(signedMoney(money, -10)).toMatch(/^−R\$\s10,00$/);
    expect(signedMoney(money, 0)).toMatch(/^R\$\s0,00$/);
    expect(signedMoney(createMoney(true), -10)).toBe("R$ ••••");
  });
});
