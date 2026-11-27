import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { sampleSummary } from "../../test/incomeHandlers";
import { historyHandlers, sampleBase, sampleIncomes, type FakeIncome } from "../../test/incomeHistoryHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/** O espaço inseparável que o `Intl` põe depois do "R$". */
const NBSP = String.fromCharCode(160);
const nbsp = (text: string | null) => (text ?? "").split(NBSP).join(" ");

/** Os endereços pedidos ao histórico e ao mapa, na ordem (o registro passa adiante sem responder). */
let asked: string[] = [];
const record = (path: string) =>
  http.get(path, ({ request }) => {
    const url = new URL(request.url);
    asked.push(url.pathname + url.search);
  });

function serveLedger(incomes: FakeIncome[] = sampleIncomes()) {
  asked = [];
  server.use(
    record("/api/v1/income/history"),
    record("/api/v1/income/heatmap"),
    http.get("/api/v1/income/summary", () => HttpResponse.json(sampleSummary())),
    ...historyHandlers(incomes, sampleBase),
  );
}

async function openHistory() {
  const rendered = renderApp("renda");
  const panel = await screen.findByRole("region", { name: "Histórico" });
  return { ...rendered, panel };
}

/** A tabela escondida do gráfico (a do leitor de tela), pela legenda dela. */
async function barsTable(panel: HTMLElement, caption: RegExp) {
  return within(panel).findByRole("table", { name: caption });
}

const rowNames = (table: HTMLElement) => within(table).getAllByRole("rowheader").map((th) => th.textContent);

const askedFor = (path: string) => asked.filter((p) => p.startsWith(path));

describe("Renda: histórico em barras", () => {
  it("começa por mês, pela data de pagamento, líquido, nos últimos 2 anos", async () => {
    serveLedger();
    const { panel } = await openHistory();
    const table = await barsTable(panel, /Proventos por mês, pela data de pagamento, líquidos, 2 anos/);
    expect(rowNames(table)[0]).toBe("dezembro de 2025");
    expect(rowNames(table).at(-1)).toBe("novembro de 2026");
    const november = within(table).getByRole("row", { name: /novembro de 2026/ });
    // Total e a receber: 150 recebidos e 90 a receber.
    expect(nbsp(november.textContent)).toContain("R$ 240,00");
    expect(nbsp(november.textContent)).toContain("R$ 90,00");
    const legend = within(panel).getByRole("list", { name: "Legenda" });
    expect(within(legend).getByText("Recebido")).toBeInTheDocument();
    expect(within(legend).getByText("A receber")).toBeInTheDocument();
    expect(nbsp(legend.textContent)).toContain("Total R$ 780,00");
    expect(askedFor("/api/v1/income/history").at(-1)).toBe("/api/v1/income/history?interval=month&basis=payment&group=total&value=net");
  });

  it("agrupa por classe com o nome e o total de cada uma na legenda", async () => {
    serveLedger();
    const { panel, user } = await openHistory();
    await user.selectOptions(within(panel).getByLabelText("Agrupar por"), "class");
    const table = await barsTable(panel, /Proventos por mês/);
    await waitFor(() => expect(within(table).getByRole("columnheader", { name: "FIIs" })).toBeInTheDocument());
    const legend = within(panel).getByRole("list", { name: "Legenda" });
    const items = within(legend).getAllByRole("listitem").map((li) => nbsp(li.textContent));
    expect(items).toEqual(["FIIs R$ 605,00", "Ações BR R$ 130,00", "Ativos removidos R$ 45,00", " A receber", "Total R$ 780,00"]);
  });

  it("agrupa por segmento e por ativo", async () => {
    serveLedger();
    const { panel, user } = await openHistory();
    await user.selectOptions(within(panel).getByLabelText("Agrupar por"), "segment");
    const legend = () => within(panel).getByRole("list", { name: "Legenda" });
    await waitFor(() => expect(within(legend()).getByText("FIIs · Tijolo")).toBeInTheDocument());
    expect(within(legend()).getByText("FIIs · Papel")).toBeInTheDocument();
    expect(within(legend()).getByText("Ações BR · Bancos")).toBeInTheDocument();
    await user.selectOptions(within(panel).getByLabelText("Agrupar por"), "asset");
    await waitFor(() => expect(within(legend()).getByText("KNRI11")).toBeInTheDocument());
    expect(within(legend()).getByText("OLDX11")).toBeInTheDocument();
  });

  it("troca o intervalo, o período, a data e o valor", async () => {
    serveLedger();
    const { panel, user } = await openHistory();
    await user.selectOptions(within(panel).getByLabelText("Intervalo"), "quarter");
    let table = await barsTable(panel, /Proventos por trimestre/);
    expect(rowNames(table)).toContain("4º trimestre de 2026");

    await user.selectOptions(within(panel).getByLabelText("Período"), "all");
    await user.click(within(panel).getByRole("button", { name: "Data com" }));
    table = await barsTable(panel, /Proventos por trimestre, pela data com, líquidos, tudo/);
    // O rendimento pago em dezembro de 2025 é da data com de novembro.
    expect(rowNames(table)[0]).toBe("4º trimestre de 2025");

    await user.selectOptions(within(panel).getByLabelText("Intervalo"), "year");
    await user.click(within(panel).getByRole("button", { name: "Bruto" }));
    table = await barsTable(panel, /Proventos por ano, pela data com, brutos, tudo/);
    const year = within(table).getByRole("row", { name: /^2026/ });
    // 2026 bruto: 125 + 157,58 + 140 + 45 + 150 + 90.
    expect(nbsp(year.textContent)).toContain("R$ 707,58");
    expect(within(panel).getByRole("button", { name: "Bruto" })).toHaveAttribute("aria-pressed", "true");
    expect(askedFor("/api/v1/income/history").at(-1)).toBe("/api/v1/income/history?interval=year&basis=record&group=total&value=gross");
  });

  it("diz quando o período escolhido não tem provento", async () => {
    serveLedger([{ pay: "2023-03-10", record: "2023-02-27", gross: 50, net: 50, asset: null, label: "OLDX11" }]);
    const { panel, user } = await openHistory();
    expect(await within(panel).findByText("Nenhum provento nesse período.")).toBeInTheDocument();
    await user.selectOptions(within(panel).getByLabelText("Período"), "all");
    expect(await barsTable(panel, /Proventos por mês/)).toBeInTheDocument();
  });

  it("mostra o erro do servidor sem quebrar a tela", async () => {
    serveLedger();
    server.use(
      http.get("/api/v1/income/history", () =>
        HttpResponse.json({ type: "urn:prumo:error:internal", title: "Erro", status: 500, code: "internal", detail: "falhou" }, { status: 500 }),
      ),
    );
    const { panel } = await openHistory();
    expect(await within(panel).findByText(/Não consegui carregar o histórico/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Quanto sua carteira paga?" })).toBeInTheDocument();
  });
});

describe("Renda: mapa de calor", () => {
  async function openHeatmap() {
    serveLedger();
    const opened = await openHistory();
    await opened.user.click(within(opened.panel).getByRole("button", { name: "Mapa de calor" }));
    const table = await within(opened.panel).findByRole("table", { name: /por mês e ano/ });
    return { ...opened, table };
  }

  const cellOf = (table: HTMLElement, year: string, month: number) => {
    const row = within(table).getByRole("row", { name: new RegExp(`^${year}`) });
    return within(row).getAllByRole("cell")[month - 1] as HTMLElement;
  };

  it("mostra os reais de cada mês, com o a receber marcado e a conta para o leitor de tela", async () => {
    const { table } = await openHeatmap();
    expect(table).toHaveAccessibleName("Proventos em reais por mês e ano, pela data de pagamento, líquidos");
    const november = cellOf(table, "2026", 11);
    expect(november).toHaveClass("is-upcoming", "heat-5");
    expect(nbsp(november.textContent)).toContain("R$ 240,00, dos quais R$ 90,00 a receber");
    expect(cellOf(table, "2026", 1)).toHaveClass("heat-0");
    expect(cellOf(table, "2026", 1)).toHaveTextContent("–");
    // O total do ano não entra na escala.
    const total = within(within(table).getByRole("row", { name: /^2026/ })).getAllByRole("cell")[12] as HTMLElement;
    expect(total).toHaveClass("heatmap__total", "heat-0");
    expect(nbsp(total.textContent)).toContain("R$ 680,00");
    expect(within(table).getByRole("columnheader", { name: "Total" })).toBeInTheDocument();
  });

  it("troca para DY e yield on cost, com a base e o parcial à vista", async () => {
    const { panel, table, user } = await openHeatmap();
    await user.click(within(panel).getByRole("button", { name: "DY" }));
    await waitFor(() => expect(table).toHaveAccessibleName(/^DY/));
    expect(nbsp(cellOf(table, "2026", 11).textContent)).toContain("1,20%");
    expect(nbsp(cellOf(table, "2026", 11).textContent)).toContain("do patrimônio médio de R$ 20.000,00");
    // Junho: só o ativo apagado pagou, então o DY é parcial.
    const june = cellOf(table, "2026", 6);
    expect(within(june).getByText("*")).toBeInTheDocument();
    expect(june.textContent).toContain("parcial");
    // 2025 sem foto do patrimônio: sem DY.
    expect(cellOf(table, "2025", 12).textContent).toContain("sem foto do patrimônio no mês");
    expect(within(table).getByRole("columnheader", { name: "No ano" })).toBeInTheDocument();
    const key = panel.querySelector(".heatmap__key") as HTMLElement;
    expect(within(key).getByText(/parcial: parte do provento fica fora da conta/)).toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "YoC (yield on cost)" }));
    await waitFor(() => expect(table).toHaveAccessibleName(/^Yield on cost/));
    expect(nbsp(cellOf(table, "2025", 12).textContent)).toContain("0,63%");
    expect(nbsp(cellOf(table, "2025", 12).textContent)).toContain("do custo médio de R$ 16.000,00");
  });

  it("segue a data e o valor escolhidos para o histórico", async () => {
    const { panel, table, user } = await openHeatmap();
    await user.click(within(panel).getByRole("button", { name: "Data com" }));
    await user.click(within(panel).getByRole("button", { name: "Bruto" }));
    await waitFor(() => expect(table).toHaveAccessibleName(/pela data com, brutos/));
    // O JCP com data com em fevereiro, bruto.
    await waitFor(() => expect(nbsp(cellOf(table, "2026", 2).textContent)).toContain("R$ 157,58"));
    expect(askedFor("/api/v1/income/heatmap").at(-1)).toBe("/api/v1/income/heatmap?basis=record&value=gross");
  });

  it("sem provento nenhum, diz isso", async () => {
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(sampleSummary())));
    const { panel, user } = await openHistory();
    await user.click(within(panel).getByRole("button", { name: "Mapa de calor" }));
    expect(await within(panel).findByText("Nenhum provento lançado ainda.")).toBeInTheDocument();
  });

  it("com o olho fechado, nenhum real aparece, e o DY continua", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { panel, table, user } = await openHeatmap();
    const money = /R\$\s*\d/;
    const heard = () =>
      [panel.textContent ?? "", ...Array.from(panel.querySelectorAll("[title], [aria-label]")).flatMap((e) => [e.getAttribute("title") ?? "", e.getAttribute("aria-label") ?? ""])];
    for (const text of heard()) expect(text).not.toMatch(money);
    await user.click(within(panel).getByRole("button", { name: "DY" }));
    await waitFor(() => expect(nbsp(cellOf(table, "2026", 11).textContent)).toContain("1,20%"));
    for (const text of heard()) expect(text).not.toMatch(money);
    await user.click(within(panel).getByRole("button", { name: "Barras" }));
    await barsTable(panel, /Proventos por mês/);
    for (const text of heard()) expect(text).not.toMatch(money);
  });
});
