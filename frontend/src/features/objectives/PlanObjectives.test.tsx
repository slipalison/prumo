import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { fakeObjectives, sampleImpact, sampleObjective } from "../../test/objectivesHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

async function calculate(user: ReturnType<typeof renderApp>["user"]) {
  const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
  await user.type(input, "600000");
  await user.click(screen.getByRole("button", { name: /Calcular/ }));
  return screen.findByRole("region", { name: "Resultado do aporte" });
}

describe("Objetivos no Aportar", () => {
  it("diz quanto do objetivo o aporte entrega e quando a meta chega", async () => {
    const fake = fakeObjectives([sampleObjective()], [sampleImpact()]);
    server.use(...fake.handlers);
    const { user } = renderApp("aportar");
    await calculate(user);

    const card = await screen.findByRole("region", { name: "Seus objetivos" });
    expect(within(card).getByText("Patrimônio total")).toBeInTheDocument();
    expect(within(card).getByText(/Com este aporte, você chega a/)).toHaveTextContent(
      /chega a 25,6% da meta de R\$\s1\.000\.000,00 \(hoje, 25,0%\)/,
    );
    expect(within(card).getByText(/Com aportes assim, a meta chega em/)).toHaveTextContent("fev/2031");
    // A conta não é prometida sem as premissas: elas vêm junto, no ⓘ do objetivo.
    await user.click(within(card).getByRole("button", { name: "Premissas da conta: Patrimônio total" }));
    const premises = card;
    expect(within(premises).getByText(/Aporte de R\$\s2\.400,00 por mês/)).toBeInTheDocument();
    expect(within(premises).getByText(/Rentabilidade de 9,80% ao ano/)).toBeInTheDocument();
    expect(within(card).getByRole("link", { name: "Trocar premissas" })).toHaveAttribute(
      "href",
      "#/ajustes/objetivos",
    );

    // O plano manda o que ele compra, por classe: é a conta do servidor.
    const sent = fake.requests.find((r) => r.path === "/api/v1/objectives/impact")?.body as {
      items: Array<{ asset_class: string; value: number }>;
    };
    expect(sent.items.length).toBe(4);
    expect(sent.items.map((i) => i.asset_class)).toContain("br_reits");
    expect(sent.items.reduce((sum, i) => sum + i.value, 0)).toBeCloseTo(5947.92, 2);
  });

  it("o card fica abaixo do resultado do plano", async () => {
    server.use(...fakeObjectives([sampleObjective()], [sampleImpact()]).handlers);
    const { user } = renderApp("aportar");
    const result = await calculate(user);
    const card = await screen.findByRole("region", { name: "Seus objetivos" });
    expect(result.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("com aporte informado, a frase diz o valor em vez de 'assim'", async () => {
    const informed = sampleImpact({
      assumptions: { monthly_contribution: 1500, contribution_source: "informed", annual_return_percent: 8, return_source: "informed" },
    });
    server.use(...fakeObjectives([sampleObjective()], [informed]).handlers);
    const { user } = renderApp("aportar");
    await calculate(user);
    const card = await screen.findByRole("region", { name: "Seus objetivos" });
    expect(within(card).getByText(/Com aportes de R\$\s1\.500,00 por mês, a meta chega em/)).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Premissas da conta: Patrimônio total" }));
    expect(within(card).getByText(/Aporte de R\$\s1\.500,00 por mês: informado por você/)).toBeInTheDocument();
  });

  it("um aporte que fecha a meta diz que completa", async () => {
    const closes = sampleImpact({
      kind: "class_value",
      asset_class: "br_reits",
      target: 50_000,
      current: 48_000,
      after: 54_000,
      percent_before: 96,
      percent_after: 100,
      projection: { state: "reached", months: null, month: null },
    });
    server.use(...fakeObjectives([sampleObjective()], [closes]).handlers);
    const { user } = renderApp("aportar");
    await calculate(user);
    const card = await screen.findByRole("region", { name: "Seus objetivos" });
    expect(within(card).getByText("Valor em Fundos Imobiliários")).toBeInTheDocument();
    expect(within(card).getByText(/Com este aporte, você/)).toHaveTextContent(/completa a meta de R\$\s50\.000,00/);
    expect(within(card).queryByText(/a meta chega em/)).not.toBeInTheDocument();
  });

  it("sem aporte e sem rentabilidade, explica em vez de inventar uma data", async () => {
    const stuck = sampleImpact({
      assumptions: { monthly_contribution: 0, contribution_source: "ledger", annual_return_percent: 0, return_source: "informed" },
      projection: { state: "unreachable", months: null, month: null },
    });
    server.use(...fakeObjectives([sampleObjective()], [stuck]).handlers);
    const { user } = renderApp("aportar");
    await calculate(user);
    const card = await screen.findByRole("region", { name: "Seus objetivos" });
    expect(within(card).getByText(/a meta não chega\. Informe um aporte mensal/)).toBeInTheDocument();
  });

  it("sem objetivo ativo, o plano segue sem o card", async () => {
    const { user } = renderApp("aportar");
    await calculate(user);
    expect(screen.queryByRole("region", { name: "Seus objetivos" })).not.toBeInTheDocument();
  });

  it("se a consulta falha, o plano continua e o card some", async () => {
    server.use(http.post("/api/v1/objectives/impact", () => new HttpResponse(null, { status: 500 })));
    const { user } = renderApp("aportar");
    const result = await calculate(user);
    expect(within(result).getByRole("heading", { name: /Compre 4 ativos/ })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Seus objetivos" })).not.toBeInTheDocument();
  });
});
