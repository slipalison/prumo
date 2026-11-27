import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { Risk } from "../../api/riskTypes";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";
import { largeRisk, sampleRisk } from "../../test/riskHandlers";
import { server } from "../../test/server";
import {
  describeCoefficient,
  drawdownDates,
  drawdownShort,
  formatCoefficient,
  formatVolatility,
  niceTicks,
  shortDay,
  windowLabel,
} from "./riskFormat";

/** Os caminhos pedidos ao servidor falso, na ordem. */
let asked: string[] = [];

function serve(risk: Risk | ((window: string) => Risk)) {
  asked = [];
  server.use(
    http.get("/api/v1/risk", ({ request }) => {
      const url = new URL(request.url);
      asked.push(url.pathname + url.search);
      const window = url.searchParams.get("window") ?? "12m";
      return HttpResponse.json(typeof risk === "function" ? risk(window) : risk);
    }),
  );
}

async function open() {
  const rendered = renderApp("desempenho/risco");
  await screen.findByRole("heading", { name: "Risco da carteira" });
  return rendered;
}

/** Um dos números do topo (o `dt` é o rótulo). */
async function stat(label: string) {
  const summary = await screen.findByRole("region", { name: "Resumo do risco" });
  return within(summary).getByText(label, { selector: "dt" }).closest("div") as HTMLElement;
}

describe("Risco", () => {
  it("abre pelo link do Desempenho e volta pela rentabilidade", async () => {
    serve(sampleRisk());
    // A tela-mãe sem série: só o cabeçalho com o link importa aqui.
    server.use(
      http.get("/api/v1/history/net-worth", () =>
        HttpResponse.json({ period: "12m", starts_on: null, rebuilding_since: null, points: [] }),
      ),
      http.get("/api/v1/performance", () =>
        HttpResponse.json({
          period: "12m",
          starts_on: null,
          rebuilding_since: null,
          window: null,
          return_percent: null,
          benchmarks: [],
          points: [],
          summary: { twelve_months: null, since_start: null },
          years: [],
        }),
      ),
    );
    const { user } = renderApp("desempenho");
    await screen.findByRole("heading", { name: "Você está ganhando do CDI?" });
    const link = screen.getByRole("link", { name: /Risco: volatilidade, queda máxima e correlação/ });
    expect(link).toHaveAttribute("href", "#/desempenho/risco");
    await user.click(link);
    await screen.findByRole("heading", { name: "Risco da carteira" });
    // Continua na tela Desempenho da navegação.
    const nav = screen.getByRole("navigation", { name: "Principal" });
    expect(within(nav).getByRole("link", { name: "Desempenho" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "← Rentabilidade" })).toHaveAttribute("href", "#/desempenho");
  });

  it("mostra a carteira contra o Ibovespa no topo", async () => {
    serve(sampleRisk());
    await open();
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("8,4% ao ano");
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("Ibovespa 17,9% ao ano");
    expect(await stat("Queda máxima")).toHaveTextContent("−6,85%");
    expect(await stat("Queda máxima")).toHaveTextContent("de 04/03/2026 a 08/04/2026; recuperou em 21/05/2026");
    expect(await stat("Retorno em 12 meses")).toHaveTextContent("+11,37%");
    expect(await stat("Retorno em 12 meses")).toHaveTextContent("Ibovespa +9,12%");
    expect(await stat("Com os pesos de hoje")).toHaveTextContent("5,6% ao ano");
    expect(await stat("Com os pesos de hoje")).toHaveTextContent("29,3% da carteira na conta");
    expect(
      screen.getByText(/12 meses: do fechamento de 26\/11\/2025 ao último fechamento guardado, 26\/11\/2026 \(249 pregões/),
    ).toBeInTheDocument();
  });

  it("troca a janela e pede de novo", async () => {
    serve((window) => sampleRisk(window as Risk["window"]));
    const { user } = await open();
    const group = screen.getByRole("group", { name: "Janela" });
    expect(within(group).getByRole("button", { name: "12 meses" })).toHaveAttribute("aria-pressed", "true");
    await user.click(within(group).getByRole("button", { name: "2 anos" }));
    await waitFor(() => expect(asked).toContain("/api/v1/risk?window=24m"));
    expect(within(group).getByRole("button", { name: "2 anos" })).toHaveAttribute("aria-pressed", "true");
    expect(await stat("Retorno em 2 anos")).toBeInTheDocument();
    expect(asked[0]).toBe("/api/v1/risk?window=12m");
  });

  it("a correlação é uma tabela com os números e destaca os pares que mais andam juntos", async () => {
    serve(sampleRisk());
    await open();
    const panel = await screen.findByRole("region", { name: "Correlação entre os ativos" });
    const table = within(panel).getByRole("table", { name: /Correlação dos retornos diários entre os ativos/ });
    expect(within(table).getAllByRole("columnheader")).toHaveLength(7);
    const row = within(table).getByRole("row", { name: /^HGLG11/ });
    const cells = within(row).getAllByRole("cell");
    expect(cells[0]).toHaveTextContent("0,31");
    expect(cells[1]).toHaveTextContent("1");
    expect(cells[3]).toHaveTextContent("0,71");
    expect(cells[3]).toHaveTextContent("entre os que mais andam juntos");
    expect(cells[3]).toHaveClass("is-pos", "is-top");
    expect(cells[3]).toHaveAttribute("title", "HGLG11 e BTLG11: 0,71, andam juntos fortemente");
    const btc = within(table).getByRole("row", { name: /^BTC/ });
    expect(within(btc).getAllByRole("cell")[4]).toHaveTextContent("−0,04");
    expect(within(btc).getAllByRole("cell")[4]).toHaveClass("is-neg");
    // Os pares, em texto, no painel ao lado do gráfico.
    const side = await screen.findByRole("region", { name: "Os pares que mais andam juntos" });
    const pairs = within(within(side).getByRole("list")).getAllByRole("listitem");
    expect(pairs).toHaveLength(5);
    expect(pairs[0]).toHaveTextContent("HGLG11 e BTLG110,71andam juntos fortemente");
    // A matriz pequena não é densa.
    expect(table).not.toHaveClass("corr--dense");
  });

  it("as maiores partes do risco ficam ao lado do gráfico", async () => {
    serve(sampleRisk());
    await open();
    const panel = await screen.findByRole("region", { name: "As maiores partes do risco" });
    const items = within(within(panel).getByRole("list")).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    expect(items[0]).toHaveTextContent("ITSA461,2%peso 23,3%");
    expect(items[1]).toHaveTextContent("BTC9,2%");
  });

  it("carteira grande: a matriz fica densa, com o nome das colunas inteiro", async () => {
    serve((window) => largeRisk(window as Risk["window"]));
    await open();
    const table = await screen.findByRole("table", { name: /Correlação dos retornos diários/ });
    expect(table).toHaveClass("corr--dense");
    expect(within(table).getAllByRole("columnheader")).toHaveLength(34);
    expect(within(table).getByRole("columnheader", { name: "Tesouro Prefixado 2031" })).toBeInTheDocument();
    expect(within(table).getByRole("rowheader", { name: "Tesouro IPCA+ 2045" })).toBeInTheDocument();
    // Simétrica, com 1 na diagonal.
    const rows = within(table).getAllByRole("row").slice(1);
    const cell = (r: number, c: number) => within(rows[r] as HTMLElement).getAllByRole("cell")[c];
    expect(cell(3, 3)).toHaveTextContent("1");
    expect(cell(2, 17)?.textContent).toBe(cell(17, 2)?.textContent);
    // Fora da matriz: o CDB (sem série) e o título com juros semestrais.
    const assets = within(await screen.findByRole("region", { name: "Por ativo" })).getByRole("table");
    expect(within(assets).getByRole("row", { name: /^CDB Banco Fictício/ })).toHaveTextContent("Sem preço de mercado");
    expect(within(assets).getByRole("row", { name: /^Tesouro IPCA\+ com Juros/ })).toHaveTextContent("Paga juros");
  });

  it("a tabela por ativo traz a carteira, o Ibovespa e por que cada ativo tem ou não número", async () => {
    serve(sampleRisk());
    await open();
    const panel = await screen.findByRole("region", { name: "Por ativo" });
    const table = within(panel).getByRole("table");
    const portfolio = within(table).getByRole("row", { name: /^Sua carteira/ });
    expect(portfolio).toHaveTextContent("8,4%");
    expect(portfolio).toHaveTextContent("100%");
    const ibov = within(table).getByRole("row", { name: /^Ibovespa/ });
    expect(ibov).toHaveTextContent("04/03/26 a 08/04/26 · ainda não recuperou");
    const itsa = within(table).getByRole("row", { name: /^ITSA4/ });
    expect(itsa).toHaveTextContent("23,3%");
    expect(itsa).toHaveTextContent("21,4%");
    expect(itsa).toHaveTextContent("+18,20%");
    expect(itsa).toHaveTextContent("−14,10%");
    expect(itsa).toHaveTextContent("61,2%");
    expect(itsa).toHaveTextContent("04/03/26 a 08/04/26 · recuperou em 21/05/26");
    expect(itsa.querySelector(".risk-dd")).toHaveAttribute(
      "title",
      "Do pico ao fundo: de 04/03/2026 a 08/04/2026; recuperou em 21/05/2026",
    );
    const cdb = within(table).getByRole("row", { name: /^Tesouro IPCA\+ 2035/ });
    expect(cdb).toHaveTextContent("Sem preço de mercado");
    expect(cdb).toHaveClass("risk-row--muted");
  });

  it("o risco × retorno desenha cada ativo e as referências, com o valor no ponteiro", async () => {
    serve(sampleRisk());
    await open();
    const panel = await screen.findByRole("region", { name: "Risco × retorno" });
    const chart = within(panel).getByRole("img", { name: /Risco × retorno em 12 meses/ });
    expect(chart).toHaveAccessibleName(expect.stringContaining("ITSA4, volatilidade 21,4% ao ano, retorno +18,20%"));
    expect(chart).toHaveAccessibleName(expect.stringContaining("Sua carteira, volatilidade 8,4% ao ano"));
    expect(chart).toHaveAccessibleName(expect.stringContaining("Ibovespa"));
    expect(chart.querySelectorAll(".bubble--asset")).toHaveLength(7);
    expect(chart.querySelector(".bubble--portfolio")).not.toBeNull();
    expect(chart.querySelector(".bubble--benchmark")).not.toBeNull();
    // Os seis de maior peso têm o nome ao lado; o sétimo, só no ponteiro.
    const btc = chart.querySelector("[data-key='a-btc']") as HTMLElement;
    expect(btc.textContent).toBe("");
    fireEvent.pointerEnter(btc);
    expect(btc).toHaveTextContent("BTC");
    const tip = chart.querySelector(".bubble-chart__tooltip") as HTMLElement;
    expect(tip).toHaveTextContent("BTC");
    expect(tip).toHaveTextContent("48,6% ao ano");
    expect(tip).toHaveTextContent("+32,50%");
    expect(tip).toHaveTextContent("0,6%");
    fireEvent.pointerLeave(chart);
    expect(chart.querySelector(".bubble-chart__tooltip")).toBeNull();
    fireEvent.pointerDown(chart.querySelector("[data-key='portfolio']") as HTMLElement);
    expect(chart.querySelector(".bubble-chart__tooltip")).toHaveTextContent("Sua carteira");
  });

  it("explica o que falta em vez de inventar número", async () => {
    const base = sampleRisk();
    const itsa = base.assets[1];
    const btc = base.assets[7];
    if (!itsa || !btc || !itsa.metrics || !base.portfolio) throw new Error("fixture");
    serve({
      ...base,
      rebuilding_since: "2026-11-20",
      portfolio: { ...base.portfolio, full_window: false, from: "2026-02-10", volatility_percent: null, return_percent: null, drawdown: null, returns: 12 },
      model: null,
      assets: [
        { ...itsa, status: "short", metrics: { ...itsa.metrics, returns: 12, volatility_percent: null, return_percent: null, drawdown: null }, risk_share_percent: null, in_correlation: false },
        { ...btc, metrics: btc.metrics ? { ...btc.metrics, full_window: false, from: "2026-02-10", drawdown: null } : null, risk_share_percent: null, in_correlation: false },
        { ...btc, asset_id: "a-bond", label: "Tesouro IPCA+ com Juros Semestrais 2035", status: "pays_along_the_way", metrics: null, risk_share_percent: null, in_correlation: false },
      ],
      correlation: { asset_ids: [], values: [], top_pairs: [] },
    });
    await open();
    expect(await screen.findByText(/Refazendo a série desde 20\/11\/2026/)).toHaveRole("status");
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("histórico curto: 12 pregões");
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("a série começa em 10/02/2026");
    expect(await stat("Com os pesos de hoje")).toHaveTextContent("precisa de 60 pregões em comum");
    const table = within(await screen.findByRole("region", { name: "Por ativo" })).getByRole("table");
    expect(within(table).getByRole("row", { name: /^ITSA4/ })).toHaveTextContent("Histórico curto: 12 pregões na janela (o mínimo é 20)");
    const btcRow = within(table).getByRole("row", { name: /^BTC/ });
    expect(btcRow).toHaveTextContent("Histórico desde 10/02/2026");
    expect(btcRow).toHaveTextContent("Sem queda");
    expect(within(table).getByRole("row", { name: /^Tesouro IPCA\+ com Juros/ })).toHaveTextContent("Paga juros ou renda no caminho");
    const correlation = await screen.findByRole("region", { name: "Correlação entre os ativos" });
    expect(correlation).toHaveTextContent("A correlação precisa de dois ativos com pelo menos 60 pregões");
    expect(await screen.findByRole("region", { name: "Os pares que mais andam juntos" })).toHaveTextContent(
      "Aparecem com dois ativos que tenham pelo menos 60 pregões",
    );
    expect(await screen.findByRole("region", { name: "As maiores partes do risco" })).toHaveTextContent(
      "Aparecem com ativos que tenham histórico na janela inteira",
    );
    expect(correlation).toHaveTextContent("Fora da matriz, com menos de 60 pregões na janela: BTC");
    // A carteira sem número fica fora do gráfico; o BTC continua.
    const chart = within(await screen.findByRole("region", { name: "Risco × retorno" })).getByRole("img");
    expect(chart.querySelector(".bubble--portfolio")).toBeNull();
  });

  it("sem série e sem ativo, aponta o caminho", async () => {
    serve({
      ...sampleRisk(),
      portfolio: null,
      benchmark: null,
      model: null,
      assets: [],
      correlation: { asset_ids: [], values: [], top_pairs: [] },
    });
    renderApp("desempenho/risco");
    const heading = await screen.findByRole("heading", { name: "Ainda não há o que medir" });
    const card = heading.closest("section") as HTMLElement;
    expect(within(card).getByRole("link", { name: "Carteira" })).toHaveAttribute("href", "#/carteira");
  });

  it("sem ativo com preço, o gráfico diz do que precisa", async () => {
    const base = sampleRisk();
    serve({
      ...base,
      benchmark: null,
      assets: base.assets.filter((a) => a.status !== "ok"),
      correlation: { asset_ids: [], values: [], top_pairs: [] },
    });
    await open();
    const panel = await screen.findByRole("region", { name: "Risco × retorno" });
    expect(panel).toHaveTextContent("O gráfico aparece com um ativo que tenha pelo menos 20 pregões");
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("Ibovespa sem dado na janela");
  });

  it("erro do servidor: diz e deixa tentar de novo", async () => {
    let calls = 0;
    server.use(
      http.get("/api/v1/risk", () => {
        calls += 1;
        return calls === 1 ? HttpResponse.json({ detail: "x" }, { status: 500 }) : HttpResponse.json(sampleRisk());
      }),
    );
    const { user } = renderApp("desempenho/risco");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Não consegui carregar o risco");
    await user.click(within(alert).getByRole("button", { name: "Tentar de novo" }));
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("8,4% ao ano");
  });

  it("com o olho fechado não há valor em reais, e os percentuais continuam", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    serve(sampleRisk());
    await open();
    await screen.findByRole("region", { name: "Por ativo" });
    const main = document.querySelector("main") as HTMLElement;
    const heard = [main.textContent ?? ""];
    for (const element of main.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "title", "alt"]) {
        const value = element.getAttribute(attribute);
        if (value) heard.push(value);
      }
    }
    for (const text of heard) expect(text).not.toMatch(/R\$/);
    expect(await stat("Volatilidade da carteira")).toHaveTextContent("8,4% ao ano");
    const table = within(await screen.findByRole("region", { name: "Por ativo" })).getByRole("table");
    expect(within(table).getByRole("row", { name: /^ITSA4/ })).toHaveTextContent("23,3%");
  });

  it("o leitor de tela anda pela matriz com o nome das duas pontas", async () => {
    serve(sampleRisk());
    const user = userEvent.setup();
    await open();
    const table = await screen.findByRole("table", { name: /Correlação dos retornos diários/ });
    expect(within(table).getByRole("rowheader", { name: "ITSA4" })).toBeInTheDocument();
    expect(within(table).getByRole("columnheader", { name: "BTC" })).toBeInTheDocument();
    // A régua da cor é enfeite: o número já está escrito.
    expect(document.querySelector(".corr-scale")).toHaveAttribute("aria-hidden", "true");
    await user.tab();
  });
});

describe("textos do risco", () => {
  it("escreve a correlação com sinal e em palavras", () => {
    expect(formatCoefficient(0.714)).toBe("0,71");
    expect(formatCoefficient(-0.04)).toBe("−0,04");
    expect(formatCoefficient(0.001)).toBe("0,00");
    expect(formatCoefficient(1)).toBe("1");
    expect(formatCoefficient(-1)).toBe("−1");
    expect(describeCoefficient(0.1)).toBe("quase sem relação");
    expect(describeCoefficient(0.3)).toBe("andam juntos fracamente");
    expect(describeCoefficient(0.5)).toBe("andam juntos moderadamente");
    expect(describeCoefficient(-0.8)).toBe("andam em sentidos opostos fortemente");
  });

  it("escreve a volatilidade, as datas da queda e a janela", () => {
    expect(formatVolatility(21.43)).toBe("21,4% ao ano");
    const open = { depth_percent: -5, peak: "2026-03-04", trough: "2026-04-08", recovered: null };
    expect(drawdownDates(open)).toBe("de 04/03/2026 a 08/04/2026; ainda não recuperou");
    expect(drawdownShort({ ...open, recovered: "2026-05-21" })).toBe("04/03/26 a 08/04/26 · recuperou em 21/05/26");
    expect(shortDay("x")).toBe("x");
    expect(windowLabel("60m")).toBe("5 anos");
  });

  it("marcas redondas no eixo", () => {
    expect(niceTicks(0, 50, 4)).toEqual([0, 20, 40]);
    expect(niceTicks(0, 1, 4)).toEqual([0, 0.25, 0.5, 0.75, 1]);
    expect(niceTicks(-10, 30, 4)).toEqual([-10, 0, 10, 20, 30]);
    expect(niceTicks(5, 5)).toEqual([5]);
  });
});
