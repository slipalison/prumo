import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { formatBRL } from "../../lib/format";
import {
  achievedObjective,
  fakeObjectives,
  noPremises,
  sampleAssumptions,
  sampleDefaults,
  sampleIncomeObjective,
  sampleObjective,
} from "../../test/objectivesHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/** Troca o texto de um campo de taxa de uma vez (colar é o mesmo evento de digitar, sem uma volta por tecla). */
async function fill(user: { clear: (e: Element) => Promise<void>; click: (e: Element) => Promise<void>; paste: (t: string) => Promise<void> }, input: HTMLElement, text: string) {
  await user.clear(input);
  await user.click(input);
  await user.paste(text);
}

async function openObjectives(initial = [sampleObjective(), achievedObjective()]) {
  const fake = fakeObjectives(initial);
  server.use(...fake.handlers);
  const rendered = renderApp("ajustes/objetivos");
  await screen.findByRole("heading", { name: "Para onde a carteira vai?", level: 1 });
  return { ...rendered, fake };
}

describe("Objetivos (Ajustes)", () => {
  it("é uma seção de Ajustes e não um item da navegação", async () => {
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    const main = screen.getByRole("navigation", { name: "Principal" });
    expect(within(main).queryByRole("link", { name: "Objetivos" })).not.toBeInTheDocument();
    await user.click(within(main).getByRole("link", { name: "Ajustes" }));
    const sections = await screen.findByRole("navigation", { name: "Seções de Ajustes" });
    await user.click(within(sections).getByRole("link", { name: "Objetivos" }));
    expect(await screen.findByRole("heading", { name: "Para onde a carteira vai?", level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/ajustes/objetivos");
    expect(document.title).toBe("Ajustes · Prumo");
    expect(within(screen.getByText(/A meta de renda também aparece em/)).getByRole("link", { name: "Renda" })).toHaveAttribute(
      "href",
      "#/renda",
    );
  });

  it("separa o que está em andamento do que foi concluído, com as datas dos três cenários", async () => {
    await openObjectives();
    const active = await screen.findByRole("article", { name: "Patrimônio total" });
    expect(within(active).getByText(/R\$\s1\.000\.000,00/, { selector: "strong" })).toBeInTheDocument();
    const bar = within(active).getByRole("progressbar", { name: "Progresso: Patrimônio total" });
    expect(bar).toHaveAttribute("aria-valuenow", "25");
    expect(bar.getAttribute("aria-valuetext")).toMatch(/R\$\s250\.000,00 de R\$\s1\.000\.000,00, 25,0%/);
    expect(within(active).getByText("Cenário base: a meta chega em mar/2031 (daqui a 4 anos e 4 meses).")).toBeInTheDocument();
    expect(within(active).getByText("Faixa: mar/2030 no otimista, set/2032 no pessimista.")).toBeInTheDocument();
    expect(within(active).getByText(/poder de compra de hoje.*mai\/2032 no base/)).toBeInTheDocument();

    const done = screen.getByRole("article", { name: "Valor em Fundos Imobiliários" });
    expect(within(done).getByRole("status")).toHaveTextContent("Concluído em 15/10/2026.");
    expect(within(done).queryByRole("term")).not.toBeInTheDocument();
    expect(within(done).queryByRole("img")).not.toBeInTheDocument();

    expect(screen.getByRole("heading", { name: "Em andamento" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Concluídos" })).toBeInTheDocument();
  });

  it("cada premissa aparece com o número, e a origem de cada uma fica no ⓘ", async () => {
    const { user } = await openObjectives([
      sampleObjective(),
      sampleObjective({
        id: "o-2",
        kind: "class_value",
        asset_class: "crypto",
        premises: { ...noPremises, monthly_contribution: 500, real_return: 12, reinvest_income: false },
        assumptions: {
          ...sampleAssumptions,
          monthly_contribution: 500,
          contribution_source: "informed",
          real_return_percent: 12,
          return_source: "informed",
          reinvest_income: false,
          reinvest_source: "informed",
        },
        projection: { state: "beyond", months: null, month: null },
      }),
    ]);
    const first = await screen.findByRole("article", { name: "Patrimônio total" });
    const premises = within(first).getByLabelText("Premissas da conta: Patrimônio total");
    expect(premises).toHaveTextContent(/Aporte\s*R\$\s2\.400,00 por mês/);
    expect(premises).toHaveTextContent("Rentabilidade real5,10% ao ano");
    expect(premises).toHaveTextContent("Inflação4,25% ao ano");
    expect(premises).toHaveTextContent("Proventosreinvestidos");
    expect(premises).toHaveTextContent("Rendimento do dinheiro novo7,90% ao ano");
    expect(premises).toHaveTextContent("Cenários2 pontos ao ano para cada lado");
    expect(premises).not.toHaveTextContent("· sua");
    await user.click(within(first).getByRole("button", { name: "De onde vem cada premissa: Patrimônio total" }));
    expect(await screen.findByText(/o IPCA esperado pelo Focus do Banco Central de 2027 a 2030, pesquisa de 19\/11\/2026/)).toBeInTheDocument();
    expect(screen.getByText(/a da sua carteira em Desempenho nos últimos 12 meses, menos o IPCA do período/)).toBeInTheDocument();

    const second = screen.getByRole("article", { name: "Valor em Criptomoedas" });
    const mine = within(second).getByLabelText("Premissas da conta: Valor em Criptomoedas");
    expect(mine).toHaveTextContent(/R\$\s500,00 por mês · sua/);
    expect(mine).toHaveTextContent("12,00% ao ano · sua");
    expect(mine).toHaveTextContent("retirados todo mês · sua");
    expect(within(second).getByText(/mais de 100 anos/)).toBeInTheDocument();
  });

  it("a meta de renda aparece com o modo, a base do número e as premissas dela", async () => {
    await openObjectives([
      sampleIncomeObjective(),
      sampleIncomeObjective({
        id: "o-renda-2",
        income_mode: "floor",
        projection: { state: "reached", months: null, month: null },
      }),
    ]);
    const card = await screen.findByRole("article", { name: "Renda mensal (média)" });
    expect(within(card).getByText(/R\$\s3\.000,00 por mês/, { selector: "strong" })).toBeInTheDocument();
    expect(within(card).getByText("estimativa", { exact: false })).toBeInTheDocument();
    expect(within(card).getByRole("progressbar").getAttribute("aria-valuetext")).toMatch(/pela estimativa/);
    const premises = within(card).getByLabelText("Premissas da conta: Renda mensal (média)");
    expect(premises).toHaveTextContent("Crescimento dos proventos0,00% ao ano, acima da inflação");
    expect(premises).not.toHaveTextContent("Rentabilidade real");

    const floor = screen.getByRole("article", { name: "Renda mensal (todo mês)" });
    expect(within(floor).getByText(/Pela estimativa, a renda de hoje já cobre a meta/)).toBeInTheDocument();
  });

  it("a curva troca de leitura e mostra os três cenários ao passar o ponteiro", async () => {
    const { user } = await openObjectives([sampleObjective()]);
    const card = await screen.findByRole("article", { name: "Patrimônio total" });
    const chart = within(card).getByRole("img");
    expect(chart.getAttribute("aria-label")).toMatch(/em reais da época: no cenário base, de R\$\s250\smil hoje/);
    // A tabela de ano em ano, para o leitor de tela.
    expect(within(card).getByRole("table", { name: /de ano em ano/ })).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Reais de hoje" }));
    expect(within(card).getByRole("button", { name: "Reais de hoje" })).toHaveAttribute("aria-pressed", "true");
    expect(within(card).getByRole("img").getAttribute("aria-label")).toMatch(/em reais de hoje/);

    const plot = within(card).getByTestId("projection-plot");
    vi.spyOn(plot, "getBoundingClientRect").mockReturnValue({ left: 0, width: 200, top: 0, height: 150, right: 200, bottom: 150, x: 0, y: 0, toJSON: () => ({}) });
    fireEvent.pointerMove(plot, { clientX: 190 });
    expect(card.querySelector(".proj-chart__tooltip")).toHaveTextContent(/set\/2033.*Otimista.*Base.*Pessimista/);
    fireEvent.pointerLeave(plot);
    expect(card.querySelector(".proj-chart__tooltip")).toBeNull();
  });

  it("diz quando a conta não chega à meta", async () => {
    await openObjectives([sampleObjective({ projection: { state: "unreachable", months: null, month: null } })]);
    expect(await screen.findByText(/nada leva a conta até a meta\. Informe um aporte mensal/)).toBeInTheDocument();
  });

  it("sem objetivo, convida a criar o primeiro", async () => {
    await openObjectives([]);
    expect(await screen.findByRole("heading", { name: "Nenhum objetivo ainda" })).toBeInTheDocument();
  });

  it("cria um objetivo de patrimônio no padrão das premissas", async () => {
    const { user, fake } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    expect(within(dialog).getByRole("button", { name: "Criar objetivo" })).toBeDisabled();
    expect(within(dialog).getByText(/A conta usa a média real das suas compras nos últimos 12 meses/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Padrão: 5,10%: a da sua carteira em Desempenho/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Padrão: 4,25%: o IPCA esperado pelo Focus/)).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "100000000");
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));

    await waitFor(() => expect(fake.requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "net_worth", target: 1_000_000 }));
    expect(await screen.findByText("Objetivo criado.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await screen.findByRole("article", { name: "Patrimônio total" })).toBeInTheDocument();
  });

  it("cria um objetivo por classe com as premissas próprias", async () => {
    const { user, fake } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "O que medir" }), "class_value");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Classe" }), "crypto");
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "5000000");

    await user.click(within(dialog).getByRole("switch", { name: /Retirar os proventos/ }));
    await fill(user, within(dialog).getByRole("textbox", { name: "Rentabilidade acima da inflação (% ao ano)" }), "6,5");
    await fill(user, within(dialog).getByRole("textbox", { name: "Inflação (% ao ano)" }), "4");
    await fill(user, within(dialog).getByRole("textbox", { name: "Amplitude dos cenários (pontos ao ano)" }), "3");
    // A meta de renda não tem crescimento aqui: o campo é só dela.
    expect(within(dialog).queryByRole("textbox", { name: /Crescimento dos proventos/ })).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));

    await waitFor(() =>
      expect(fake.requests.find((r) => r.method === "POST")?.body).toEqual({
        kind: "class_value",
        asset_class: "crypto",
        target: 50_000,
        reinvest_income: false,
        real_return: 6.5,
        inflation: 4,
        scenario_spread: 3,
      }),
    );
  });

  it("cria com o aporte informado no lugar da média do livro", async () => {
    const { user, fake } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "500000");
    await user.click(within(dialog).getByRole("switch", { name: /Informar o meu aporte/ }));
    expect(within(dialog).getByText(/corrigido pela inflação a cada mês/)).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "Aporte por mês" }), "75000");
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));
    await waitFor(() =>
      expect(fake.requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "net_worth", target: 5000, monthly_contribution: 750 }),
    );
  });

  it("cria a meta de renda pelo piso; com uma já criada, a opção fica desligada", async () => {
    const { user, fake } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    let dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "O que medir" }), "monthly_income");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Como a renda conta" }), "floor");
    expect(within(dialog).getByText(/o pior mês de 12 seguidos/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("textbox", { name: /Rentabilidade acima/ })).not.toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "Meta por mês" }), "300000");
    await fill(user, within(dialog).getByRole("textbox", { name: /Crescimento dos proventos/ }), "1,5");
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));
    await waitFor(() =>
      expect(fake.requests.find((r) => r.method === "POST")?.body).toEqual({
        kind: "monthly_income",
        target: 3000,
        income_mode: "floor",
        income_growth: 1.5,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    expect(within(dialog).getByRole("option", { name: /Renda mensal \(já existe/ })).toBeDisabled();
  });

  it("taxa fora da faixa trava o botão e explica", async () => {
    const { user } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "100000");
    const rate = within(dialog).getByRole("textbox", { name: "Rentabilidade acima da inflação (% ao ano)" });
    await fill(user, rate, "300");
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Use um número de -50 a 50.");
    expect(within(dialog).getByRole("button", { name: "Criar objetivo" })).toBeDisabled();
    await fill(user, rate, "abc");
    expect(within(dialog).getByRole("button", { name: "Criar objetivo" })).toBeDisabled();
    await user.clear(rate);
    expect(within(dialog).getByRole("button", { name: "Criar objetivo" })).toBeEnabled();
  });

  it("mostra o erro do servidor sem fechar a folha", async () => {
    const { user } = await openObjectives([]);
    server.use(
      http.post("/api/v1/objectives", () =>
        HttpResponse.json(
          { type: "urn:prumo:error:validation_failed", title: "Dado inválido", status: 422, code: "validation_failed", detail: "A meta precisa ser maior que zero." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "100");
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("maior que zero");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("edita a meta e as premissas; campo vazio e chave desligada voltam ao padrão", async () => {
    const { user, fake } = await openObjectives([
      sampleObjective({
        premises: { ...noPremises, monthly_contribution: 3000, real_return: 8, inflation: 4.5 },
        assumptions: { ...sampleAssumptions, monthly_contribution: 3000, contribution_source: "informed", real_return_percent: 8, return_source: "informed" },
      }),
    ]);
    await user.click(await screen.findByRole("button", { name: "Editar objetivo: Patrimônio total" }));
    const dialog = await screen.findByRole("dialog", { name: "Editar objetivo" });
    expect(within(dialog).getByText(/O tipo e a classe não mudam/)).toBeInTheDocument();
    expect(within(dialog).getByRole("textbox", { name: "Meta" })).toHaveValue(formatBRL(1_000_000));
    expect(within(dialog).getByRole("textbox", { name: "Aporte por mês" })).toHaveValue(formatBRL(3000));
    const rate = within(dialog).getByRole("textbox", { name: "Rentabilidade acima da inflação (% ao ano)" });
    expect(rate).toHaveValue("8");
    const inflation = within(dialog).getByRole("textbox", { name: "Inflação (% ao ano)" });
    expect(inflation).toHaveValue("4,5");

    await user.click(within(dialog).getByRole("switch", { name: /Informar o meu aporte/ }));
    await fill(user, rate, "10,25");
    await user.clear(inflation);
    const target = within(dialog).getByRole("textbox", { name: "Meta" });
    await user.clear(target);
    await user.type(target, "150000000");
    await user.click(within(dialog).getByRole("button", { name: "Salvar objetivo" }));

    await waitFor(() =>
      expect(fake.requests.find((r) => r.method === "PATCH")).toEqual({
        method: "PATCH",
        path: "/api/v1/objectives/o-1",
        body: {
          target: 1_500_000,
          monthly_contribution: null,
          reinvest_income: null,
          real_return: 10.25,
          inflation: null,
          income_yield: null,
          scenario_spread: null,
        },
      }),
    );
    expect(await screen.findByText("Objetivo salvo.")).toBeInTheDocument();
  });

  it("na edição da meta de renda, o modo vai junto", async () => {
    const { user, fake } = await openObjectives([sampleIncomeObjective()]);
    await user.click(await screen.findByRole("button", { name: "Editar objetivo: Renda mensal (média)" }));
    const dialog = await screen.findByRole("dialog", { name: "Editar objetivo" });
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Como a renda conta" }), "floor");
    await user.click(within(dialog).getByRole("button", { name: "Salvar objetivo" }));
    await waitFor(() => expect(fake.requests.find((r) => r.method === "PATCH")?.body).toMatchObject({ income_mode: "floor", income_growth: null }));
  });

  it("remove só depois de confirmar", async () => {
    const { user, fake } = await openObjectives();
    await user.click(await screen.findByRole("button", { name: "Remover objetivo: Patrimônio total" }));
    const confirm = screen.getByRole("group", { name: "Confirmar remoção: Patrimônio total" });
    await user.click(within(confirm).getByRole("button", { name: "Manter" }));
    expect(fake.requests.some((r) => r.method === "DELETE")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Remover objetivo: Patrimônio total" }));
    await user.click(
      within(screen.getByRole("group", { name: "Confirmar remoção: Patrimônio total" })).getByRole("button", { name: "Remover" }),
    );
    await waitFor(() => expect(fake.requests.find((r) => r.method === "DELETE")?.path).toBe("/api/v1/objectives/o-1"));
    expect(await screen.findByText("Objetivo removido.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("article", { name: "Patrimônio total" })).not.toBeInTheDocument());
  });

  it("falha ao carregar: mostra o erro e tenta de novo", async () => {
    let calls = 0;
    server.use(
      http.get("/api/v1/objectives", () => {
        calls += 1;
        return calls === 1
          ? new HttpResponse(null, { status: 500 })
          : HttpResponse.json({ defaults: sampleDefaults, objectives: [] });
      }),
    );
    const { user } = renderApp("ajustes/objetivos");
    expect(await screen.findByRole("heading", { name: "Não consegui carregar os objetivos" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("heading", { name: "Nenhum objetivo ainda" })).toBeInTheDocument();
  });
});
