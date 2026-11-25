import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { samplePlan } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

async function openCalculator() {
  const rendered = renderApp("aportar");
  const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
  return { ...rendered, input };
}

describe("Aportar: a renda que o plano soma", () => {
  function planWithIncome() {
    const plan = samplePlan(6000);
    plan.income = { available: true, missing: [], added_monthly: 35.4, before_monthly: 410.5, after_monthly: 445.9 };
    plan.items = plan.items.map((item, index) => ({ ...item, estimated_monthly_income: index === 0 ? 20.25 : index === 1 ? 15.15 : null }));
    return plan;
  }

  it("mostra no topo quanto o aporte soma e de quanto para quanto a renda vai", async () => {
    server.use(http.post("/api/v1/plans", () => HttpResponse.json(planWithIncome())));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const line = within(result).getByText(/Este aporte soma/).closest("p") as HTMLElement;
    expect(line).toHaveTextContent(/~R\$\s35,40\/mês de renda/);
    expect(line).toHaveTextContent(/de R\$\s410,50 para R\$\s445,90 por mês/);
    expect(within(line).getByText("estimativa")).toBeInTheDocument();
  });

  it("mostra em cada cartão a renda mensal dele, só quando há", async () => {
    server.use(http.post("/api/v1/plans", () => HttpResponse.json(planWithIncome())));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByText(/\+~R\$\s20,25\/mês de renda/)).toBeInTheDocument();
    expect(within(result).getByText(/\+~R\$\s15,15\/mês de renda/)).toBeInTheDocument();
    expect(within(result).getAllByText(/\/mês de renda/)).toHaveLength(2);
  });

  it("sem renda estimada, o plano segue como estava", async () => {
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).queryByText(/Este aporte soma/)).not.toBeInTheDocument();
    expect(within(result).queryByText(/\/mês de renda/)).not.toBeInTheDocument();
  });

  it("aporte que não soma renda não mostra a frase", async () => {
    const plan = samplePlan(6000);
    plan.income = { available: true, missing: [], added_monthly: 0, before_monthly: 410.5, after_monthly: 410.5 };
    server.use(http.post("/api/v1/plans", () => HttpResponse.json(plan)));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).queryByText(/Este aporte soma/)).not.toBeInTheDocument();
  });

  it("sem histórico em cache ainda, diz que a estimativa está indisponível", async () => {
    const plan = samplePlan(6000);
    plan.income = { available: false, missing: ["ITSA4", "KNRI11"], added_monthly: 0, before_monthly: 0, after_monthly: 0 };
    server.use(http.post("/api/v1/plans", () => HttpResponse.json(plan)));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByText(/Estimativa de renda indisponível por enquanto/)).toBeInTheDocument();
    expect(within(result).queryByText(/Este aporte soma/)).not.toBeInTheDocument();
    expect(within(result).queryByText(/\/mês de renda/)).not.toBeInTheDocument();
  });

  it("com parte dos papéis sem histórico, avisa quais ficaram de fora", async () => {
    const plan = samplePlan(6000);
    plan.income = { available: true, missing: ["ZZZZ3"], added_monthly: 12, before_monthly: 100, after_monthly: 112 };
    server.use(http.post("/api/v1/plans", () => HttpResponse.json(plan)));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByText(/Sem histórico: ZZZZ3/)).toBeInTheDocument();
  });
});
