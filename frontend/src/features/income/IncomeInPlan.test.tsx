import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { samplePlan } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

async function openCalculator() {
  const rendered = renderApp("aportar");
  const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
  return { ...rendered, input };
}

/** A sugestão de caixa; `calls` conta as buscas (um handler de teste passa na frente do gravador). */
const cash = (amount: number, count = 3, calls = { n: 0 }) =>
  http.get("/api/v1/income/cash-suggestion", () => {
    calls.n += 1;
    return HttpResponse.json({ amount, count, since: "2026-11-01" });
  });

describe("Aportar: proventos recebidos desde o último aporte", () => {
  it("sem proventos, não oferece nada", async () => {
    await openCalculator();
    expect(screen.queryByRole("group", { name: "Proventos recebidos desde o último aporte" })).not.toBeInTheDocument();
  });

  it("oferece incluir, soma ao valor e deixa tirar", async () => {
    server.use(cash(812));
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: "Proventos recebidos desde o último aporte" });
    expect(offer).toHaveTextContent(/\+ R\$\s812,00 de proventos recebidos desde o último aporte \(3 pagamentos\)/);

    await user.type(input, "300000");
    expect(input).toHaveValue("R$ 3.000,00");
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    expect(input).toHaveValue("R$ 3.812,00");
    expect(offer).toHaveTextContent(/R\$\s812,00 de proventos incluídos no aporte/);

    await user.click(within(offer).getByRole("button", { name: "Tirar" }));
    expect(input).toHaveValue("R$ 3.000,00");
    expect(offer).toHaveTextContent(/\+ R\$\s812,00/);
  });

  it("incluir com o campo vazio põe só os proventos, e o plano pede o total", async () => {
    server.use(cash(812.5, 1));
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: "Proventos recebidos desde o último aporte" });
    expect(offer).toHaveTextContent(/\(1 pagamento\)/);
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    expect(input).toHaveValue("R$ 812,50");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(getDb().requests.filter((r) => r.path === "/api/v1/plans").at(-1)?.body).toEqual({ amount: 812.5 });
  });

  it("incluir depois de calcular descarta o plano velho", async () => {
    server.use(cash(100));
    const { user, input } = await openCalculator();
    await screen.findByRole("group", { name: "Proventos recebidos desde o último aporte" });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(screen.getByRole("button", { name: "Incluir" }));
    expect(screen.queryByRole("region", { name: "Resultado do aporte" })).not.toBeInTheDocument();
    expect(input).toHaveValue("R$ 6.100,00");
  });

  it("depois de registrar o aporte, a sugestão é buscada de novo", async () => {
    const first = { n: 0 };
    server.use(cash(812, 3, first));
    const { user, input } = await openCalculator();
    await screen.findByRole("group", { name: "Proventos recebidos desde o último aporte" });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(first.n).toBe(1);
    const second = { n: 0 };
    server.use(cash(0, 0, second));
    await user.click(screen.getByRole("button", { name: /Já comprei/ }));
    const sheet = await screen.findByRole("dialog");
    await user.click(within(sheet).getByRole("button", { name: /Registrar aporte/ }));
    await screen.findByText("Aporte registrado. A carteira já está atualizada.");
    await waitFor(() => expect(second.n).toBeGreaterThan(0));
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Proventos recebidos desde o último aporte" })).not.toBeInTheDocument(),
    );
  });

  it("falha na sugestão não atrapalha o aporte", async () => {
    server.use(http.get("/api/v1/income/cash-suggestion", () => HttpResponse.json({ code: "internal", status: 500 }, { status: 500 })));
    const { user, input } = await openCalculator();
    await user.type(input, "100000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    expect(await screen.findByRole("region", { name: "Resultado do aporte" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Proventos recebidos desde o último aporte" })).not.toBeInTheDocument();
  });
});

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
