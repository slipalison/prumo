import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { formatBRL } from "../../lib/format";
import { achievedObjective, fakeObjectives, sampleObjective } from "../../test/objectivesHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

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
    // A meta de renda mensal continua em Renda.
    expect(within(screen.getByText(/A meta de renda mensal fica em/)).getByRole("link", { name: "Renda" })).toHaveAttribute(
      "href",
      "#/renda",
    );
  });

  it("separa o que está em andamento do que foi concluído, com a data", async () => {
    await openObjectives();
    const active = await screen.findByRole("article", { name: "Patrimônio total" });
    expect(within(active).getByText(/R\$\s1\.000\.000,00/, { selector: "strong" })).toBeInTheDocument();
    const bar = within(active).getByRole("progressbar", { name: "Progresso: Patrimônio total" });
    expect(bar).toHaveAttribute("aria-valuenow", "25");
    expect(bar.getAttribute("aria-valuetext")).toMatch(/R\$\s250\.000,00 de R\$\s1\.000\.000,00, 25,0%/);
    expect(within(active).getByText(/a meta chega em mar\/2031 \(daqui a 4 anos e 4 meses\)/)).toBeInTheDocument();

    const done = screen.getByRole("article", { name: "Valor em Fundos Imobiliários" });
    expect(within(done).getByRole("status")).toHaveTextContent("Concluído em 15/10/2026.");
    expect(within(done).queryByRole("list", { name: "Premissas da conta" })).not.toBeInTheDocument();

    expect(screen.getByRole("heading", { name: "Em andamento" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Concluídos" })).toBeInTheDocument();
  });

  it("a premissa aparece sempre, com a origem de cada número", async () => {
    await openObjectives([
      sampleObjective(),
      sampleObjective({
        id: "o-2",
        kind: "class_value",
        asset_class: "crypto",
        monthly_contribution: 500,
        expected_return: 12,
        assumptions: {
          monthly_contribution: 500,
          contribution_source: "informed",
          annual_return_percent: 12,
          return_source: "informed",
        },
        projection: { state: "beyond", months: null, month: null },
      }),
    ]);
    const first = await screen.findByRole("article", { name: "Patrimônio total" });
    const premises = within(first).getByRole("list", { name: "Premissas da conta" });
    expect(within(premises).getByText(/Aporte de R\$\s2\.400,00 por mês: a média das suas compras nos últimos 12 meses/)).toBeInTheDocument();
    expect(within(premises).getByText(/Rentabilidade de 9,80% ao ano: a da sua carteira em Desempenho/)).toBeInTheDocument();

    const second = screen.getByRole("article", { name: "Valor em Criptomoedas" });
    expect(within(second).getByText(/Aporte de R\$\s500,00 por mês: informado por você/)).toBeInTheDocument();
    expect(within(second).getByText(/Rentabilidade de 12,00% ao ano: informada por você/)).toBeInTheDocument();
    expect(within(second).getByText(/leva mais de 100 anos/)).toBeInTheDocument();
  });

  it("diz quando a meta não chega por falta de aporte", async () => {
    await openObjectives([sampleObjective({ projection: { state: "unreachable", months: null, month: null } })]);
    expect(await screen.findByText(/não chega\. Informe um aporte mensal/)).toBeInTheDocument();
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
    expect(within(dialog).getByText(/A conta usa 9,80% ao ano: a rentabilidade da sua carteira em Desempenho/)).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "100000000");
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));

    await waitFor(() => expect(fake.requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "net_worth", target: 1_000_000 }));
    expect(await screen.findByText("Objetivo criado.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await screen.findByRole("article", { name: "Patrimônio total" })).toBeInTheDocument();
  });

  it("cria um objetivo por classe com aporte e rentabilidade próprios", async () => {
    const { user, fake } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "O que medir" }), "class_value");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Classe" }), "crypto");
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "5000000");

    await user.click(within(dialog).getByRole("switch", { name: /Informar o meu aporte/ }));
    await user.type(within(dialog).getByRole("textbox", { name: "Aporte por mês" }), "75000");
    await user.click(within(dialog).getByRole("switch", { name: /Informar a minha rentabilidade/ }));
    const rate = within(dialog).getByRole("textbox", { name: "Rentabilidade esperada (% ao ano)" });
    // O campo nasce com o padrão, para a pessoa só ajustar.
    expect(rate).toHaveValue("9,8");
    await user.clear(rate);
    await user.type(rate, "11,5");
    await user.click(within(dialog).getByRole("button", { name: "Criar objetivo" }));

    await waitFor(() =>
      expect(fake.requests.find((r) => r.method === "POST")?.body).toEqual({
        kind: "class_value",
        asset_class: "crypto",
        target: 50_000,
        monthly_contribution: 750,
        expected_return: 11.5,
      }),
    );
  });

  it("rentabilidade fora da faixa trava o botão e explica", async () => {
    const { user } = await openObjectives([]);
    await user.click(await screen.findByRole("button", { name: "Novo objetivo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo objetivo" });
    await user.type(within(dialog).getByRole("textbox", { name: "Meta" }), "100000");
    await user.click(within(dialog).getByRole("switch", { name: /Informar a minha rentabilidade/ }));
    const rate = within(dialog).getByRole("textbox", { name: "Rentabilidade esperada (% ao ano)" });
    await user.clear(rate);
    await user.type(rate, "300");
    expect(within(dialog).getByRole("alert")).toHaveTextContent("de -50% a 100% ao ano");
    expect(within(dialog).getByRole("button", { name: "Criar objetivo" })).toBeDisabled();
    await user.clear(rate);
    await user.type(rate, "abc");
    expect(within(dialog).getByRole("button", { name: "Criar objetivo" })).toBeDisabled();
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

  it("edita a meta e as premissas, e volta ao padrão ao desligar a chave", async () => {
    const { user, fake } = await openObjectives([
      sampleObjective({
        monthly_contribution: 3000,
        expected_return: 8,
        assumptions: { monthly_contribution: 3000, contribution_source: "informed", annual_return_percent: 8, return_source: "informed" },
      }),
    ]);
    await user.click(await screen.findByRole("button", { name: "Editar objetivo: Patrimônio total" }));
    const dialog = await screen.findByRole("dialog", { name: "Editar objetivo" });
    expect(within(dialog).getByText(/O tipo e a classe não mudam/)).toBeInTheDocument();
    expect(within(dialog).getByRole("textbox", { name: "Meta" })).toHaveValue(formatBRL(1_000_000));
    expect(within(dialog).getByRole("textbox", { name: "Aporte por mês" })).toHaveValue(formatBRL(3000));
    expect(within(dialog).getByRole("textbox", { name: "Rentabilidade esperada (% ao ano)" })).toHaveValue("8");

    // Volta ao padrão do aporte; troca a rentabilidade; sobe a meta.
    await user.click(within(dialog).getByRole("switch", { name: /Informar o meu aporte/ }));
    const rate = within(dialog).getByRole("textbox", { name: "Rentabilidade esperada (% ao ano)" });
    await user.clear(rate);
    await user.type(rate, "10,25");
    const target = within(dialog).getByRole("textbox", { name: "Meta" });
    await user.clear(target);
    await user.type(target, "150000000");
    await user.click(within(dialog).getByRole("button", { name: "Salvar objetivo" }));

    await waitFor(() =>
      expect(fake.requests.find((r) => r.method === "PATCH")).toEqual({
        method: "PATCH",
        path: "/api/v1/objectives/o-1",
        body: { target: 1_500_000, monthly_contribution: null, expected_return: 10.25 },
      }),
    );
    expect(await screen.findByText("Objetivo salvo.")).toBeInTheDocument();
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
          : HttpResponse.json({ defaults: { annual_return_percent: 8, return_source: "fallback" }, objectives: [] });
      }),
    );
    const { user } = renderApp("ajustes/objetivos");
    expect(await screen.findByRole("heading", { name: "Não consegui carregar os objetivos" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("heading", { name: "Nenhum objetivo ainda" })).toBeInTheDocument();
  });
});
