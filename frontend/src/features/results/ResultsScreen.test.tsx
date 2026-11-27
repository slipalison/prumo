import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";
import { companyAsset, demoResults, fundAsset, sampleCompany, sampleFund, setResults } from "../../test/resultsHandlers";
import { server } from "../../test/server";
import { NOT_APPLICABLE_TEXT } from "../fundamentals/fundamentalsText";

/** A linha da tabela do trimestre mais novo de um ativo. */
async function row(symbol: string) {
  const table = await screen.findByRole("table", { name: "O trimestre mais novo de cada empresa da carteira" });
  const header = within(table).getByRole("rowheader", { name: new RegExp(symbol) });
  return header.closest("tr") as HTMLElement;
}

const cell = (tr: HTMLElement, label: string) => tr.querySelector(`td[data-label="${label}"]`) as HTMLElement;

/** As duas variações da célula: contra um ano antes e contra o trimestre anterior. */
const versus = (tr: HTMLElement, label: string) => {
  const [yearAgo, previous] = [...cell(tr, label).querySelectorAll<HTMLElement>(".change")];
  return { yearAgo: yearAgo as HTMLElement, previous: previous as HTMLElement };
};

describe("Resultados trimestrais", () => {
  it("abre pela Análise, num link do cabeçalho", async () => {
    setResults(demoResults());
    const { user } = renderApp("analise");
    await user.click(await screen.findByRole("link", { name: "Resultados trimestrais" }));
    expect(await screen.findByRole("heading", { name: "Resultados trimestrais" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/analise/resultados");
    expect(screen.getByRole("link", { name: "Voltar à análise" })).toHaveAttribute("href", "#/analise");
  });

  it("mostra o trimestre mais novo com as duas comparações e a data de entrega", async () => {
    setResults(demoResults());
    renderApp("analise/resultados");
    const industry = await row("INDU3");
    expect(industry).toHaveTextContent("3º tri 2026");
    expect(industry).toHaveTextContent("entregue em 09/11/2026");
    expect(cell(industry, "Receita")).toHaveTextContent(/R\$\s1,5\sbi/);
    expect(versus(industry, "Variação da receita").yearAgo).toHaveTextContent("a/a");
    expect(versus(industry, "Variação da receita").yearAgo).toHaveTextContent("+25,0%");
    expect(versus(industry, "Variação da receita").previous).toHaveTextContent("t/t");
    expect(versus(industry, "Variação da receita").previous).toHaveTextContent("+20,0%");
    expect(cell(industry, "Lucro líquido")).toHaveTextContent(/R\$\s150\smi/);
    expect(versus(industry, "Variação do lucro").yearAgo).toHaveTextContent("+25,0%");
    expect(versus(industry, "Variação do lucro").previous).toHaveTextContent("+87,5%");
    expect(cell(industry, "Margem líquida")).toHaveTextContent("10,0%");
    expect(cell(industry, "Margem líquida")).toHaveTextContent("0,0 p.p. a/a");

    // Prejuízo um ano antes: rótulo, não percentual.
    const retail = await row("VARE3");
    expect(versus(retail, "Variação do lucro").yearAgo).toHaveTextContent("saiu do prejuízo");
    expect(versus(retail, "Variação do lucro").yearAgo).not.toHaveTextContent("%");
    expect(cell(retail, "Margem líquida")).toHaveTextContent("+2,8 p.p. a/a");
    const mine = await row("MINE3");
    expect(versus(mine, "Variação do lucro").yearAgo).toHaveTextContent("−30,0%");
    expect(versus(mine, "Variação do lucro").yearAgo).toHaveClass("change--down");
  });

  it("holding e banco: a margem não se aplica, com o porquê; o banco mostra o resultado bruto", async () => {
    setResults(demoResults());
    const { user } = renderApp("analise/resultados");
    const holding = await row("ITSA4");
    expect(cell(holding, "Margem líquida")).toHaveTextContent("não se aplica");
    expect(cell(holding, "Margem líquida")).not.toHaveTextContent("%");
    await user.click(within(cell(holding, "Margem líquida")).getByRole("button", { name: "Por que a margem não se aplica a ITSA4" }));
    expect(await within(cell(holding, "Margem líquida")).findByText(NOT_APPLICABLE_TEXT.holding)).toBeInTheDocument();
    const bank = await row("BANC4");
    expect(cell(bank, "Receita")).toHaveTextContent("resultado bruto da intermediação");
    expect(cell(bank, "Margem líquida")).toHaveTextContent("não se aplica");
    expect(versus(bank, "Variação do lucro").yearAgo).toHaveTextContent("+33,3%");
  });

  it("as bolhas: quem tem as duas variações entra; prejuízo numa ponta fica fora, com o rótulo", async () => {
    setResults(demoResults());
    const { user } = renderApp("analise/resultados");
    const chart = await screen.findByRole("region", { name: "Lucro × receita" });
    const data = within(chart).getByRole("table", { hidden: true });
    expect(within(data).getAllByRole("row", { hidden: true })).toHaveLength(5);
    expect(within(data).getByRole("rowheader", { name: "INDU3", hidden: true }).closest("tr")).toHaveTextContent("+25,0%");
    expect(within(chart).getByText(/Fora do gráfico: VARE3 \(lucro: saiu do prejuízo\)/)).toBeInTheDocument();
    expect(within(chart).getByRole("img")).toHaveAccessibleName(/4 empresas/);

    // Contra o trimestre anterior, o varejo (de 2 para 12) entra com +500%.
    await user.click(within(chart).getByRole("button", { name: "Trimestre anterior" }));
    expect(within(chart).getByRole("button", { name: "Trimestre anterior" })).toHaveAttribute("aria-pressed", "true");
    expect(within(chart).queryByText(/Fora do gráfico/)).toBeNull();
    expect(within(data).getByRole("rowheader", { name: "VARE3", hidden: true }).closest("tr")).toHaveTextContent("+500,0%");
    expect(within(chart).getByRole("img")).toHaveAccessibleName(/5 empresas/);
  });

  it("oito trimestres por empresa, com o prejuízo para baixo", async () => {
    setResults(demoResults());
    renderApp("analise/resultados");
    const series = await screen.findByRole("region", { name: "Oito trimestres" });
    const retail = within(series).getByRole("img", { name: /^VARE3/ });
    expect(retail.querySelectorAll(".mini-bars__bar.is-negative")).toHaveLength(5);
    expect(retail).toHaveAccessibleName(/3º tri 2026: lucro R\$\s12\smi/);
    expect(within(series).getAllByText("4T24").length).toBeGreaterThan(0);
  });

  it("FIIs: o rendimento do mês do informe, e o mês impossível sem número", async () => {
    setResults(demoResults());
    renderApp("analise/resultados");
    const panel = await screen.findByRole("region", { name: "FIIs mês a mês" });
    const knri = within(panel).getByRole("rowheader", { name: /KNRI11/ }).closest("tr") as HTMLElement;
    expect(cell(knri, "Informe")).toHaveTextContent("out/2026");
    expect(cell(knri, "Rendimento do mês")).toHaveTextContent("0,69%");
    // A média só dos meses com número (0,70% e 0,69%).
    expect(cell(knri, "Média do período")).toHaveTextContent("0,70%");
    expect(cell(knri, "Valor patrimonial por cota")).toHaveTextContent(/R\$\s99,00/);
    expect(within(knri).getByRole("img")).toHaveAccessibleName(/set\/26: sem número/);
  });

  it("com o olho fechado, nenhum R$ aparece (nem no title nem no leitor de tela)", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    setResults(demoResults());
    renderApp("analise/resultados");
    const industry = await row("INDU3");
    expect(cell(industry, "Receita")).toHaveTextContent(MASK_BRL);
    expect(cell(industry, "Lucro líquido")).toHaveTextContent(MASK_BRL);
    // O percentual fica.
    expect(versus(industry, "Variação do lucro").yearAgo).toHaveTextContent("+25,0%");
    const main = document.querySelector("main") as HTMLElement;
    expect(main.textContent).not.toMatch(/R\$\s*\d/);
    for (const element of main.querySelectorAll("[title], [aria-label]")) {
      expect(element.getAttribute("title") ?? "").not.toMatch(/R\$\s*\d/);
      expect(element.getAttribute("aria-label") ?? "").not.toMatch(/R\$\s*\d/);
    }
  });

  it("empresa sem trimestre e sem comparação: o motivo, nunca um zero", async () => {
    setResults([
      companyAsset("a-1", "NOVA3", 0.5, sampleCompany({ quarters: [], year_ago: null, previous_quarter: null })),
      companyAsset(
        "a-2",
        "UMAQ3",
        0.5,
        sampleCompany({
          year_ago: null,
          previous_quarter: {
            period_end: "2026-06-30",
            top_line: { kind: "no_base", value: null, gap: null },
            net_income: { kind: "missing", value: null, gap: "not_reported" },
            net_margin_points: { value: null, gap: "not_reported" },
          },
        }),
      ),
    ]);
    renderApp("analise/resultados");
    const fresh = await row("NOVA3");
    expect(cell(fresh, "Trimestre")).toHaveTextContent("sem ITR nem DFP lidos");
    expect(versus(fresh, "Variação do lucro").yearAgo).toHaveTextContent("sem o trimestre de um ano antes");
    const odd = await row("UMAQ3");
    expect(versus(odd, "Variação da receita").previous).toHaveTextContent("sem base");
    expect(versus(odd, "Variação do lucro").previous).toHaveTextContent("a conta não veio no balanço");
    const chart = await screen.findByRole("region", { name: "Lucro × receita" });
    expect(within(chart).getByText(/NOVA3 \(sem o trimestre de um ano antes\)/)).toBeInTheDocument();
    expect(within(chart).getByRole("img")).toHaveAccessibleName(/Nenhuma empresa/);
  });

  it("só FII e ativos sem dado: sem a tabela das empresas, e a lista dos sem resultado", async () => {
    setResults([
      fundAsset("a-knri", "KNRI11", 1, sampleFund({ months: [] })),
      { asset_id: "x", symbol: "ETFX11", weight: 0, kind: "unsupported", company: null, fund: null, reason: "no_cvm_code" },
    ]);
    const { user } = renderApp("analise/resultados");
    const panel = await screen.findByRole("region", { name: "FIIs mês a mês" });
    expect(within(panel).getByRole("rowheader", { name: /KNRI11/ }).closest("tr")).toHaveTextContent("sem informe");
    expect(screen.queryByRole("table", { name: "O trimestre mais novo de cada empresa da carteira" })).toBeNull();
    await user.click(screen.getByText("Sem resultados"));
    expect(screen.getByText(/ETFX11/).closest("li")).toHaveTextContent("A B3 não informa código CVM");
  });

  it("carteira sem ação nem FII: diz que não há o que mostrar", async () => {
    setResults([{ asset_id: "x", symbol: "ETFX11", weight: 1, kind: "unsupported", company: null, fund: null, reason: "exchange_traded_product" }]);
    renderApp("analise/resultados");
    expect(await screen.findByText(/Sem ações nacionais nem FIIs com dado da CVM/)).toBeInTheDocument();
  });

  it("erro do servidor: avisa e deixa tentar de novo", async () => {
    server.use(http.get("/api/v1/results", () => HttpResponse.json({ code: "internal" }, { status: 500 })));
    const { user } = renderApp("analise/resultados");
    expect(await screen.findByRole("alert")).toHaveTextContent("Algo deu errado do nosso lado");
    server.resetHandlers();
    setResults(demoResults());
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await row("INDU3")).toBeInTheDocument();
  });
});
