import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { FINANCIAL_TEMPLATE_TEXT, GAP_TEXT, NOT_APPLICABLE_TEXT } from "../fundamentals/fundamentalsText";
import { compareRequests, failCompare, sampleTable, setCompareReturns, setCompareTable, sampleReturns } from "../../test/compareHandlers";
import { renderApp } from "../../test/render";
import { readSelection, selectionHash, tokenFor } from "./selection";

async function openCompare(keys: string) {
  const rendered = renderApp(`analise/comparar${keys ? `?ativos=${keys}` : ""}`);
  await screen.findByRole("heading", { name: "Comparar ativos", level: 1 });
  return rendered;
}

const lastRequest = (path: "compare" | "returns") =>
  [...compareRequests].reverse().find((q) => (path === "returns" ? q.includes("period=") : !q.includes("period=")));

/** Todo texto da página mais o que o leitor de tela lê no lugar dele. */
function heard(): string {
  return `${document.body.textContent ?? ""} ${[...document.querySelectorAll("[aria-label],[title]")]
    .map((e) => `${e.getAttribute("aria-label") ?? ""} ${e.getAttribute("title") ?? ""}`)
    .join(" ")}`;
}

describe("a escolha no endereço", () => {
  it("lê e escreve a lista, sem repetição e até cinco", () => {
    expect(readSelection("#/analise/comparar?ativos=PETR4,VALE3,PETR4")).toEqual(["PETR4", "VALE3"]);
    expect(readSelection("#/analise/comparar?ativos=A1,B2,C3,D4,E5,F6")).toHaveLength(5);
    expect(readSelection("#/analise/comparar")).toEqual([]);
    expect(readSelection("#/analise/comparar?ativos=KNRI11:br_reits,AAPL:intl_stocks")).toEqual(["KNRI11:br_reits", "AAPL:intl_stocks"]);
    expect(selectionHash(["PETR4", "VALE3"])).toBe("#/analise/comparar?ativos=PETR4,VALE3");
    expect(selectionHash([])).toBe("#/analise/comparar");
  });

  it("escreve a escolha como o servidor lê", () => {
    expect(tokenFor("vale3", "br_stocks", true)).toBe("VALE3:br_stocks");
    expect(tokenFor("AAPL", "intl_stocks", false)).toBe("AAPL:intl_stocks");
    expect(tokenFor("AAPL34", "intl_stocks", true)).toBe("AAPL34:intl_stocks:b3");
  });
});

describe("tela Comparar ativos", () => {
  it("abre pelo link da Análise e fica no endereço da Análise", async () => {
    const { user } = renderApp("analise");
    await user.click(await screen.findByRole("link", { name: "Comparar ativos" }));
    expect(await screen.findByRole("heading", { name: "Comparar ativos", level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/analise/comparar");
    expect(screen.getByText(/Escolha ao menos um ativo/)).toBeInTheDocument();
    expect(compareRequests).toEqual([]);
    await user.click(screen.getByRole("link", { name: "Voltar à análise" }));
    expect(await screen.findByRole("heading", { name: "Análise da carteira", level: 1 })).toBeInTheDocument();
  });

  it("mostra os ativos do endereço lado a lado, com o destaque neutro e o motivo do que falta", async () => {
    const { user } = await openCompare("ITSA4,KNRI11,VALE3:br_stocks");
    const chosen = within(await screen.findByRole("list", { name: "Ativos na comparação" }));
    expect(chosen.getAllByRole("listitem").map((li) => li.querySelector(".compare-chip__symbol")?.textContent)).toEqual(["ITSA4", "KNRI11", "VALE3"]);
    expect(screen.getByText("3 de 5 ativos")).toBeInTheDocument();

    const general = within(await screen.findByRole("region", { name: "Em comum" }));
    const row = (name: string | RegExp) => general.getByRole("rowheader", { name }).closest("tr") as HTMLElement;
    // O P/VP: menor valor de cada tipo (empresas: ITSA4; fundo sozinho não marca).
    const pvp = row("P/VP");
    expect(within(pvp).getByText("0,90×")).toBeInTheDocument();
    expect(pvp.querySelectorAll(".compare-mark")).toHaveLength(1);
    expect(pvp.querySelector(".compare-mark")).toHaveTextContent("0,90×");
    expect(pvp.querySelector(".compare-mark")).toHaveTextContent("menor");
    expect(within(pvp).getByText("(menor valor da linha)")).toBeInTheDocument();
    // Linha sem direção não marca ninguém.
    expect(row("Preço").querySelector(".compare-mark")).toBeNull();
    // Rendimento: o maior entre as empresas; o CDI líquido e o juro real ao
    // lado do nome, só como contexto (sem destaque).
    const dy = row(/^Rendimento em 12 meses \(DY\)/);
    expect(dy.querySelector(".compare-mark")).toHaveTextContent("8,00%");
    expect(within(dy).getByRole("rowheader")).toHaveTextContent("CDI 12 meses líquido 12,1% · juro real 7,1%");
    // As linhas novas de fundo: o caixa sem destaque, a liquidez com o maior.
    const fundRows = within(screen.getByRole("region", { name: "Fundos imobiliários" }));
    const cash = fundRows.getByRole("rowheader", { name: "Caixa" }).closest("tr") as HTMLElement;
    expect(cash.querySelector(".compare-mark")).toBeNull();
    expect(fundRows.getByRole("rowheader", { name: "Liquidez média, 30 pregões" })).toBeInTheDocument();

    // Blocos de empresa e de fundo, com "não se aplica" no tipo errado.
    const companies = within(screen.getByRole("region", { name: "Empresas" }));
    const roe = companies.getByRole("rowheader", { name: "ROE" }).closest("tr") as HTMLElement;
    expect(roe.querySelectorAll("td")).toHaveLength(3);
    expect(within(roe).getAllByText("Não se aplica a este tipo de ativo.")).toHaveLength(1);
    expect(within(roe).getByText(/Sem fundamentos: o Prumo só baixa os balanços/)).toBeInTheDocument();
    const funds = within(screen.getByRole("region", { name: "Fundos imobiliários" }));
    expect(funds.getByRole("rowheader", { name: "Vacância física" })).toBeInTheDocument();

    // A dica diz para onde o destaque olha.
    await user.click(screen.getByRole("button", { name: "Sobre P/VP" }));
    expect(screen.getByText(/O destaque marca o menor valor da linha\./)).toBeInTheDocument();
  });

  it("holding e banco: o indicador que enganaria diz \"não se aplica\", com o porquê, e fica fora do destaque", async () => {
    const base = sampleTable(["ITSA4", "BBAS3:br_stocks", "VALE3:br_stocks"]);
    const cell = (value: number | null, reason: "holding" | "financial_template" | null = null, mark: "max" | null = null) => ({
      value,
      text: null,
      reason,
      mark,
    });
    setCompareTable({
      ...base,
      rows: [
        // A margem EBITDA de 241,91% da holding não vira o "maior da linha": só a VALE3 tem número.
        { id: "ebitda_margin", group: "company", unit: "percent", direction: "higher", cells: [cell(null, "holding"), cell(null, "financial_template"), cell(31.5)] },
        { id: "roe", group: "company", unit: "percent", direction: "higher", cells: [cell(18, null, "max"), cell(15), cell(12)] },
      ],
    });
    await openCompare("ITSA4,BBAS3:br_stocks,VALE3:br_stocks");
    const companies = within(await screen.findByRole("region", { name: "Empresas" }));
    const margin = companies.getByRole("rowheader", { name: /Margem EBITDA/ }).closest("tr") as HTMLElement;
    const [holding, bank, other] = [...margin.querySelectorAll("td")];
    expect(holding).toHaveTextContent("não se aplica");
    expect(within(holding as HTMLElement).getByText(GAP_TEXT.holding)).toHaveClass("sr-only");
    expect(bank).toHaveTextContent("não se aplica");
    expect(other).toHaveTextContent("31,50%");
    expect(margin.querySelector(".compare-mark")).toBeNull();
    // O porquê, embaixo do bloco das empresas.
    expect(companies.getByText(NOT_APPLICABLE_TEXT.holding).closest("li")).toHaveTextContent("ITSA4");
    expect(companies.getByText(FINANCIAL_TEMPLATE_TEXT).closest("li")).toHaveTextContent("BBAS3");
  });

  it("soma da carteira, tira, e a escolha fica no endereço", async () => {
    const { user } = await openCompare("ITSA4");
    await screen.findByRole("region", { name: "Em comum" });
    await user.click(await screen.findByRole("button", { name: "Somar KNRI11 à comparação" }));
    await waitFor(() => expect(window.location.hash).toBe("#/analise/comparar?ativos=ITSA4,KNRI11"));
    await waitFor(() => expect(lastRequest("compare")).toBe("?assets=ITSA4%2CKNRI11"));
    expect((await screen.findAllByRole("columnheader", { name: "KNRI11" })).length).toBeGreaterThan(0);
    // O ativo que já entrou sai da oferta.
    expect(screen.queryByRole("button", { name: "Somar KNRI11 à comparação" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Tirar ITSA4 da comparação" }));
    await waitFor(() => expect(window.location.hash).toBe("#/analise/comparar?ativos=KNRI11"));
    await waitFor(() => expect(screen.queryAllByRole("columnheader", { name: "ITSA4" })).toHaveLength(0));
    // Voltar no navegador traz a escolha anterior.
    window.history.back();
    await waitFor(() => expect(window.location.hash).toBe("#/analise/comparar?ativos=ITSA4,KNRI11"));
  });

  it("soma pela busca de ticker, com a classe no endereço, e respeita o limite de cinco", async () => {
    const { user } = await openCompare("ITSA4,KNRI11,HGLG11,MXRF11");
    await screen.findByRole("region", { name: "Em comum" });
    await user.type(screen.getByRole("combobox"), "VOO");
    await user.click(await screen.findByRole("option", { name: /VOO/ }));
    await waitFor(() => expect(window.location.hash).toBe("#/analise/comparar?ativos=ITSA4,KNRI11,HGLG11,MXRF11,VOO:intl_stocks"));
    expect(screen.getByText("5 de 5 ativos")).toBeInTheDocument();
    // Cheio: a carteira não oferece mais, e a busca explica.
    expect(screen.getByRole("button", { name: "Somar BTLG11 à comparação" })).toBeDisabled();
    await user.type(screen.getByRole("combobox"), "ITSA");
    await user.click(await screen.findByRole("option", { name: /ITSA4/ }));
    expect(screen.getByText(/ITSA4 já está na comparação\./)).toBeInTheDocument();
    await user.type(screen.getByRole("combobox"), "QBTC");
    await user.click(await screen.findByRole("option", { name: /QBTC11/ }));
    expect(screen.getByText(/no máximo 5 ativos/)).toBeInTheDocument();
    expect(window.location.hash.split("=")[1]?.split(",")).toHaveLength(5);
  });

  it("não aceita renda fixa nem Tesouro", async () => {
    const { user } = await openCompare("ITSA4");
    await screen.findByRole("region", { name: "Em comum" });
    await user.type(screen.getByRole("combobox"), "TD-");
    await user.click(await screen.findByRole("option", { name: /Tesouro Selic 2029/ }));
    expect(screen.getByText(/Renda fixa e Tesouro não entram/)).toBeInTheDocument();
    expect(window.location.hash).toBe("#/analise/comparar?ativos=ITSA4");
  });

  it("diz o que fazer quando o servidor falha e tenta de novo", async () => {
    failCompare(true);
    const { user } = await openCompare("ITSA4,KNRI11");
    const alerts = await screen.findAllByRole("alert");
    expect(alerts.length).toBeGreaterThan(0);
    failCompare(false);
    await user.click(within(alerts[0] as HTMLElement).getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("region", { name: "Em comum" })).toBeInTheDocument();
  });

  it("não mostra o destaque de linha com um valor só", async () => {
    setCompareTable({
      ...sampleTable(["ITSA4", "KNRI11"]),
    });
    await openCompare("ITSA4,KNRI11");
    const general = within(await screen.findByRole("region", { name: "Em comum" }));
    const pvp = general.getByRole("rowheader", { name: "P/VP" }).closest("tr") as HTMLElement;
    expect(pvp.querySelectorAll(".compare-mark")).toHaveLength(0);
  });

  it("com o olho fechado, mascara os reais da tabela e deixa percentuais e múltiplos", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    await openCompare("ITSA4,KNRI11,VOO:intl_stocks");
    await screen.findByRole("region", { name: "Em comum" });
    await screen.findByRole("list", { name: "Linhas do gráfico" });
    expect(heard()).not.toMatch(/R\$\s*\d/);
    const price = (await screen.findByRole("rowheader", { name: "Preço" })).closest("tr") as HTMLElement;
    expect(within(price).getAllByText("R$ ••••")).toHaveLength(2);
    expect(screen.getByText("0,90×")).toBeInTheDocument();
    expect(screen.getAllByText(/\d,\d\d%/).length).toBeGreaterThan(0);
  });
});

describe("rentabilidade comparada", () => {
  it("pede o período, os proventos e o início igualado ao servidor", async () => {
    const { user } = await openCompare("ITSA4,KNRI11");
    const list = within(await screen.findByRole("list", { name: "Linhas do gráfico" }));
    // Ativos, a carteira e os dois índices, cada um com o retorno escrito.
    const names = list.getAllByRole("listitem").map((li) => li.querySelector("span")?.textContent);
    expect(names).toEqual(["ITSA4", "KNRI11", "Carteira", "CDI", "Ibovespa"]);
    expect(list.getByText("+7,50%")).toBeInTheDocument();
    expect(lastRequest("returns")).toContain("period=12m&total_return=false&align_start=false&portfolio=true&benchmarks=cdi,ibov");

    await user.click(screen.getByRole("button", { name: "3 anos" }));
    await waitFor(() => expect(lastRequest("returns")).toContain("period=3y"));
    await user.click(screen.getByRole("switch", { name: "Com proventos" }));
    await waitFor(() => expect(lastRequest("returns")).toContain("total_return=true"));
    expect(await screen.findByText(/com proventos brutos reinvestidos na data ex/)).toBeInTheDocument();
    await user.click(screen.getByRole("switch", { name: "Igualar o início" }));
    await waitFor(() => expect(lastRequest("returns")).toContain("align_start=true"));
    expect(await screen.findByText(/com o início igualado/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sua carteira" }));
    await waitFor(() => expect(lastRequest("returns")).toContain("portfolio=false"));
    await user.click(screen.getByRole("button", { name: "CDI" }));
    await waitFor(() => expect(lastRequest("returns")).toContain("benchmarks=ibov"));
    // Com a carteira fora, não há nota sobre os proventos dela.
    expect(screen.queryByText(/A carteira já inclui os proventos/)).not.toBeInTheDocument();
  });

  it("avisa que a carteira já inclui os proventos e que o dólar está em reais", async () => {
    await openCompare("VOO:intl_stocks,ITSA4");
    expect(await screen.findByText(/A carteira já inclui os proventos que você recebeu/)).toBeInTheDocument();
    expect(screen.getByText(/Ativos em dólar e cripto estão em reais/)).toBeInTheDocument();
    expect(screen.getByText(/só o preço dos ativos/)).toBeInTheDocument();
  });

  it("diz quando a curva começa depois e por que falta uma série", async () => {
    const base = sampleReturns(["ITSA4", "NADA3:br_stocks"], new URLSearchParams("period=12m&portfolio=true&benchmarks=cdi"));
    setCompareReturns({
      ...base,
      series: base.series.map((curve) => {
        if (curve.key === "ITSA4") return { ...curve, starts_on: "2026-03-15" };
        if (curve.key === "NADA3:br_stocks")
          return { ...curve, values: curve.values.map(() => null), return_percent: null, starts_on: null, reason: "no_price_history" as const };
        if (curve.key === "portfolio") return { ...curve, values: curve.values.map(() => null), return_percent: null, starts_on: null, reason: "no_series" as const };
        return curve;
      }),
    });
    setCompareTable(sampleTable(["ITSA4", "NADA3:br_stocks"]));
    await openCompare("ITSA4,NADA3:br_stocks");
    const list = within(await screen.findByRole("list", { name: "Linhas do gráfico" }));
    expect(list.getByText("começa em 15/03/2026")).toBeInTheDocument();
    expect(list.getAllByText("A fonte não tem histórico de preço deste ativo.")).toHaveLength(1);
    expect(list.getByText("A carteira ainda não tem série de cotas.")).toBeInTheDocument();
    // O gráfico desenha só o que tem curva e descreve os números para o leitor de tela.
    expect(screen.getByRole("img", { name: /Rentabilidade de .*ITSA4 .*CDI/ })).toBeInTheDocument();
  });

  it("sem dois fechamentos, diz em vez de desenhar um gráfico vazio", async () => {
    const base = sampleReturns(["ITSA4"], new URLSearchParams("period=1m&benchmarks="));
    setCompareReturns({ ...base, dates: ["2026-06-15"], series: base.series.map((c) => ({ ...c, values: [0] })) });
    await openCompare("ITSA4");
    expect(await screen.findByText(/Ainda não há dois fechamentos/)).toBeInTheDocument();
  });
});
