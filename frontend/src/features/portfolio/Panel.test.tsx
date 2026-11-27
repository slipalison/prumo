import { act, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it } from "vitest";
import type { LivePrices } from "../../api/types";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { FakeEventSource } from "../../test/eventSource";
import { toPortfolio } from "../../test/fixtures";
import { panelState, sampleMonth } from "../../test/panelHandlers";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  return rendered;
}

const strip = () => screen.findByRole("region", { name: "Indicadores do mês" });

/** Texto da região mais os atributos que o leitor de tela lê no lugar dele. */
function heardIn(root: HTMLElement): string[] {
  const heard: string[] = [root.textContent ?? ""];
  for (const element of root.querySelectorAll("*")) {
    for (const attribute of ["aria-label", "aria-valuetext", "title"]) {
      const value = element.getAttribute(attribute);
      if (value) heard.push(value);
    }
  }
  return heard;
}

const realMatchMedia = window.matchMedia.bind(window);

afterEach(() => {
  window.matchMedia = realMatchMedia;
});

describe("Faixa de indicadores do mês", () => {
  it("mostra os quatro números, cada um contra o mês anterior, com o sentido escrito", async () => {
    await openPortfolio();
    const region = await strip();
    const stat = (label: RegExp) => within(region).getByText(label).closest(".stat") as HTMLElement;

    // 120.132,23 contra os 112.000,00 do fim de outubro.
    const patrimony = stat(/^Patrimônio no mês/);
    expect(patrimony).toHaveTextContent(/\+R\$\s8\.132,23/);
    expect(patrimony).toHaveTextContent("+7,3% · acima do fim de outubro");

    const ret = stat(/^Rentabilidade em novembro/);
    expect(ret).toHaveTextContent("+1,8%");
    expect(ret).toHaveTextContent("abaixo de outubro (+2,3%)");

    const income = stat(/^Proventos em novembro/);
    expect(income).toHaveTextContent(/R\$\s410,25/);
    expect(income).toHaveTextContent(/acima de outubro \(R\$\s380,00\)/);

    const contribution = stat(/^Aporte em novembro/);
    expect(contribution).toHaveTextContent(/R\$\s1\.750,00/);
    expect(contribution).toHaveTextContent(/acima de outubro \(R\$\s1\.200,00\)/);
  });

  it("diz abaixo e igual, sem depender da cor", async () => {
    panelState.month = sampleMonth({
      previous_month_end: { date: "2026-10-30", value: 130_000 },
      current_return: { from: "2026-10-30", to: "2026-11-25", return_percent: -0.8, full_window: true },
      current: { income: 380, contribution: 0 },
      previous: { income: 380, contribution: 0 },
    });
    await openPortfolio();
    const region = await strip();
    expect(region).toHaveTextContent(/−R\$\s9\.867,77/);
    expect(region).toHaveTextContent("−7,6% · abaixo do fim de outubro");
    expect(region).toHaveTextContent("−0,8%");
    expect(region).toHaveTextContent(/igual a outubro \(R\$\s380,00\)/);
    expect(region).toHaveTextContent("Nada neste mês nem em outubro");
  });

  it("série que começou no meio do mês: janela curta e sem mês anterior para comparar", async () => {
    panelState.month = sampleMonth({
      previous_month_end: null,
      current_return: { from: "2026-11-12", to: "2026-11-25", return_percent: 0.6, full_window: false },
      previous_return: null,
      previous: { income: 0, contribution: 0 },
    });
    await openPortfolio();
    const region = await strip();
    expect(region).toHaveTextContent("A carteira ainda não tem um fim de mês para comparar.");
    expect(region).toHaveTextContent("Sem outubro para comparar · desde 12/11");
    expect(region).toHaveTextContent(/R\$\s410,25/);
  });

  it("sem dias para medir a rentabilidade, diz isso", async () => {
    panelState.month = sampleMonth({ current_return: null });
    await openPortfolio();
    expect(await strip()).toHaveTextContent("Ainda sem dias para medir neste mês.");
  });

  it("sem série ou com a API fora, a faixa não aparece e a carteira segue", async () => {
    panelState.month = sampleMonth({ as_of: null });
    await openPortfolio();
    await waitFor(() => expect(screen.getByRole("region", { name: "Fundos Imobiliários" })).toBeInTheDocument());
    expect(screen.queryByRole("region", { name: "Indicadores do mês" })).not.toBeInTheDocument();
  });

  it("a API fora não derruba a tela", async () => {
    server.use(http.get("/api/v1/portfolio/month", () => HttpResponse.json({ code: "internal" }, { status: 500 })));
    await openPortfolio();
    expect(screen.getByRole("region", { name: "Fundos Imobiliários" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Indicadores do mês" })).not.toBeInTheDocument();
  });

  it("com o olho fechado, os reais viram máscara e a comparação deles some; os percentuais ficam", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    await openPortfolio();
    const region = await strip();
    for (const text of heardIn(region)) expect(text).not.toMatch(MONEY);
    expect(region).toHaveTextContent(MASK_BRL);
    expect(region).toHaveTextContent("+7,3% · acima do fim de outubro");
    expect(region).toHaveTextContent("abaixo de outubro (+2,3%)");
    expect(region).toHaveTextContent("outubro inteiro, valores ocultos");
    // Sem "acima de outubro" para o que é dinheiro: diria se o mês foi maior.
    expect(region).not.toHaveTextContent(/acima de outubro \(/);
  });
});

describe("Composição", () => {
  const region = () => screen.findByRole("region", { name: "Composição" });

  it("abre no diagrama até o ativo, com a descrição para o leitor de tela", async () => {
    await openPortfolio();
    const panel = await region();
    const diagram = within(panel).getByRole("img", { name: /^Composição da carteira até ativo: Renda Fixa 70,8%, Ações Nacionais 23,3%/ });
    expect(diagram.getAttribute("aria-label")).toMatch(/Fundos Imobiliários \d+,\d%/);
    // Nome e fatia no próprio desenho; reais só na tabela.
    expect(diagram).toHaveTextContent("Carteira");
    expect(diagram).toHaveTextContent("ITSA4");
    expect(diagram.textContent).not.toMatch(MONEY);
    expect(within(panel).getByRole("button", { name: "Ativo", pressed: true })).toBeInTheDocument();
  });

  it("troca o último nível: setor, segmento e ativo", async () => {
    const { user } = await openPortfolio();
    const panel = await region();
    await user.click(within(panel).getByRole("button", { name: "Setor" }));
    const bySector = within(panel).getByRole("img", { name: /até setor/ });
    expect(bySector).toHaveTextContent("Financeiro");
    expect(bySector).toHaveTextContent("Tijolo");
    expect(bySector).not.toHaveTextContent("ITSA4");

    await user.click(within(panel).getByRole("button", { name: "Segmento" }));
    const bySegment = within(panel).getByRole("img", { name: /até segmento/ });
    expect(bySegment).toHaveTextContent("Multicategoria");
    expect(bySegment).toHaveTextContent("Holdings Diversificadas");
    expect(within(panel).getByRole("button", { name: "Segmento", pressed: true })).toBeInTheDocument();
  });

  it("a tabela traz cada valor em reais e a fatia da carteira", async () => {
    const { user } = await openPortfolio();
    const panel = await region();
    await user.click(within(panel).getByText("Ver como tabela"));
    const table = within(panel).getByRole("table", { name: /Composição da carteira: valor e fatia/ });
    const row = within(table).getByRole("row", { name: /ITSA4/ });
    expect(row).toHaveTextContent(/R\$\s28\.000,00/);
    expect(row).toHaveTextContent("23,3%");
    expect(row).toHaveTextContent("Ativo");
    expect(within(table).getByRole("row", { name: /^Ações Nacionais Classe/ })).toHaveTextContent("Classe");
  });

  it("as barras empilhadas escrevem o nome e a fatia de cada parte, por classe", async () => {
    const { user } = await openPortfolio();
    const panel = await region();
    await user.click(within(panel).getByRole("button", { name: "Barras" }));
    await user.click(within(panel).getByRole("button", { name: "Setor" }));
    expect(within(panel).queryByRole("img", { name: /^Composição da carteira até/ })).not.toBeInTheDocument();
    const fiis = within(panel).getByRole("list", { name: "Fundos Imobiliários por setor" });
    expect(within(fiis).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Tijolo70,3%", "Papel13,0%", "Sem setor16,7%"]);
  });

  it("no celular só há as barras, sem a escolha de vista", async () => {
    window.matchMedia = (query: string) =>
      ({ matches: true, media: query, addEventListener: () => undefined, removeEventListener: () => undefined }) as unknown as MediaQueryList;
    await openPortfolio();
    const panel = await region();
    expect(within(panel).queryByRole("group", { name: "Vista" })).not.toBeInTheDocument();
    expect(within(panel).queryByRole("img", { name: /^Composição da carteira até/ })).not.toBeInTheDocument();
    expect(within(panel).getByRole("list", { name: "Ações Nacionais por ativo" })).toBeInTheDocument();
  });

  it("sem posição com valor, explica e não desenha", async () => {
    getDb().assets = [];
    await openPortfolio();
    const panel = await region();
    expect(panel).toHaveTextContent("Sem ativo com valor ainda");
    expect(within(panel).queryByRole("img")).not.toBeInTheDocument();
  });

  it("com o olho fechado, nada em reais aparece, nem na tabela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { user } = await openPortfolio();
    const panel = await region();
    await user.click(within(panel).getByText("Ver como tabela"));
    for (const text of heardIn(panel)) expect(text).not.toMatch(MONEY);
    const row = within(panel).getByRole("row", { name: /ITSA4/ });
    expect(row).toHaveTextContent(MASK_BRL);
    expect(row).toHaveTextContent("23,3%");
  });
});

describe("Hoje na carteira", () => {
  const region = () => screen.findByRole("region", { name: "Hoje na carteira" });
  const names = (list: HTMLElement) => within(list).getAllByRole("listitem").map((li) => li.textContent);

  it("lista as maiores altas e baixas do dia, com o sentido escrito, e deixa de fora o que não mexeu", async () => {
    await openPortfolio();
    const panel = await region();
    const gainers = within(panel).getByRole("region", { name: "Maiores altas" });
    expect(names(gainers)).toEqual(["ITSA4 (Ações Nacionais)alta de +1,8%", "KNRI11 (Fundos Imobiliários)alta de +1,4%"]);
    const losers = within(panel).getByRole("region", { name: "Maiores baixas" });
    expect(names(losers)).toEqual(["HGLG11 (Fundos Imobiliários)baixa de −1,2%", "MXRF11 (Fundos Imobiliários)baixa de −0,4%"]);
    // BTLG11 não mexeu; BTC, TRXF11 e a renda fixa por valor não têm fechamento anterior.
    expect(panel).not.toHaveTextContent("BTLG11");
    expect(panel).not.toHaveTextContent("BTC");
  });

  it("mostra o dia por classe e filtra pela classe escolhida", async () => {
    const { user } = await openPortfolio();
    const panel = await region();
    const classes = within(panel).getByRole("list", { name: "Variação do dia por classe" });
    const stocks = within(classes).getByRole("button", { name: /Ações BR/ });
    expect(stocks).toHaveTextContent(/baixa|alta/);
    expect(stocks).toHaveTextContent("+1,8%");
    expect(stocks).toHaveTextContent(/\+R\$\s500,00/);
    const fiis = within(classes).getByRole("button", { name: /FIIs/ });
    expect(fiis).toHaveTextContent("−0,1%");
    expect(fiis).toHaveTextContent(/−R\$\s3,40/);

    await user.click(fiis);
    expect(fiis).toHaveAttribute("aria-pressed", "true");
    expect(names(within(panel).getByRole("region", { name: "Maiores altas" }))).toEqual(["KNRI11+1,4%"].map((t) => t.replace("+", "alta de +")));
    expect(panel).not.toHaveTextContent("ITSA4");
    // Clicar de novo tira o filtro.
    await user.click(fiis);
    expect(panel).toHaveTextContent("ITSA4");
  });

  it("anda com o preço ao vivo", async () => {
    await openPortfolio();
    const panel = await region();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const db = getDb();
    const knri = db.assets.find((a) => a.id === "a-knri");
    if (knri) knri.quote_price = 150;
    const portfolio = toPortfolio(db.assets, db.goals, db.checklists);
    const prices: LivePrices = {
      total_value: portfolio.total_value,
      quotes_updated_at: portfolio.quotes_updated_at,
      classes: portfolio.classes,
      profit: portfolio.profit,
      assets: portfolio.assets.map((a) => ({
        id: a.id,
        unit_price: a.unit_price,
        currency: a.currency,
        price_source: a.price_source,
        quote_updated_at: a.quote_updated_at,
        current_value: a.current_value,
        class_percent: a.class_percent,
        ideal_class_percent: a.ideal_class_percent,
        unrealized_gain: a.unrealized_gain,
        return_percent: a.return_percent,
      })),
    };
    act(() => FakeEventSource.latest().emit("prices", prices));
    const gainers = within(panel).getByRole("region", { name: "Maiores altas" });
    // 150 contra 140 de fechamento: +7,1%, agora a maior alta.
    await waitFor(() => expect(names(gainers)[0]).toBe("KNRI11 (Fundos Imobiliários)alta de +7,1%"));
  });

  it("sem fechamento anterior para nenhum ativo, explica", async () => {
    panelState.references = { assets: [] };
    await openPortfolio();
    expect(await region()).toHaveTextContent("Ainda sem fechamento anterior para medir o dia");
  });

  it("a API fora não derruba a tela: o painel do dia some", async () => {
    server.use(http.get("/api/v1/portfolio/day-reference", () => HttpResponse.json({ code: "internal" }, { status: 500 })));
    await openPortfolio();
    await screen.findByRole("region", { name: "Composição" });
    expect(screen.queryByRole("region", { name: "Hoje na carteira" })).not.toBeInTheDocument();
  });

  it("com o olho fechado, o valor por classe vira máscara; o percentual fica", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    await openPortfolio();
    const panel = await region();
    for (const text of heardIn(panel)) expect(text).not.toMatch(MONEY);
    const classes = within(panel).getByRole("list", { name: "Variação do dia por classe" });
    expect(classes).toHaveTextContent(MASK_BRL);
    expect(classes).toHaveTextContent("+1,8%");
  });
});
