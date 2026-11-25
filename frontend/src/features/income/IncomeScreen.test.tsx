import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { IncomeEvent, IncomeSummary } from "../../api/incomeTypes";
import { emptySummary, knownYield, sampleEvent, sampleSummary } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

const problem = (status: number, code: string, detail: string) =>
  HttpResponse.json(
    { type: `urn:prumo:error:${code}`, title: "Dados inválidos", status, code, detail },
    { status, headers: { "Content-Type": "application/problem+json" } },
  );

function useSummary(summary: IncomeSummary) {
  server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)));
}

function useEvents(events: IncomeEvent[]) {
  server.use(http.get("/api/v1/income", () => HttpResponse.json(events)));
}

async function openIncome() {
  const rendered = renderApp("renda");
  await screen.findByRole("heading", { name: "Quanto sua carteira paga?" });
  return rendered;
}

const requestsTo = (method: string, path: string) =>
  getDb().requests.filter((r) => r.method === method && r.path === path);

describe("Renda: navegação", () => {
  it("tem a aba Renda e abre a tela", async () => {
    const { user } = renderApp("aportar");
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    await user.click(screen.getByRole("link", { name: "Renda" }));
    expect(await screen.findByRole("heading", { name: "Quanto sua carteira paga?" })).toBeInTheDocument();
    expect(document.title).toBe("Renda · Prumo");
  });
});

describe("Renda: sem proventos", () => {
  it("convida a definir a meta e a lançar, sem tabela nem rosca", async () => {
    await openIncome();
    expect(await screen.findByRole("button", { name: "Definir meta" })).toBeInTheDocument();
    expect(screen.getByText("Nenhum pagamento a receber lançado. Lance os proventos anunciados para vê-los aqui.")).toBeInTheDocument();
    expect(await screen.findByText("Nenhum provento lançado ainda.")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Renda por ano e mês" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Quem pagou" })).not.toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    // O gráfico existe e diz que não há nada.
    expect(screen.getByRole("img", { name: /nenhum provento lançado/ })).toBeInTheDocument();
  });

  it("sem nada recebido, a renda mostrada é a estimativa e diz isso", async () => {
    const summary = emptySummary();
    summary.estimate = { monthly: 410.5, updated_at: "2026-11-25T11:00:00Z", assets: [], missing: [] };
    useSummary(summary);
    await openIncome();
    const goal = await screen.findByRole("region", { name: "Meta de renda mensal" });
    expect(within(goal).getByText(/Estimativa pelo que cada cota pagou/)).toBeInTheDocument();
    expect(within(goal).getByText(/R\$\s410,50/)).toBeInTheDocument();
  });

  it("com o histórico ainda sendo buscado, a estimativa está indisponível", async () => {
    const summary = emptySummary();
    summary.estimate = { monthly: 0, updated_at: null, assets: [], missing: ["KNRI11"] };
    useSummary(summary);
    await openIncome();
    const estimate = await screen.findByRole("region", { name: "Estimativa da carteira de hoje" });
    expect(within(estimate).getByText("Estimativa indisponível por enquanto.")).toBeInTheDocument();
    expect(within(estimate).getByText(/KNRI11/)).toBeInTheDocument();
  });

  it("estado de erro com tentar de novo", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/income/summary", () => (fail ? problem(500, "internal", "x") : HttpResponse.json(emptySummary()))),
    );
    const { user } = await openIncome();
    expect(await screen.findByRole("alert")).toHaveTextContent("Não consegui carregar a renda");
    fail = false;
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("button", { name: "Definir meta" })).toBeInTheDocument();
  });
});

describe("Renda: com proventos", () => {
  it("mostra os 12 meses, a barra da meta, os próximos pagamentos e a estimativa", async () => {
    const summary = sampleSummary();
    useSummary(summary);
    useEvents([sampleEvent(), sampleEvent({ id: "e-2", symbol: "ITSA4", kind: "jcp", gross: 100, withholding: 15, net: 85, pay_date: "2026-10-30" })]);
    await openIncome();

    // Barra de progresso contra a meta, com texto para o leitor de tela.
    const bar = await screen.findByRole("progressbar", { name: "Progresso da meta de renda mensal" });
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "100");
    expect(Number(bar.getAttribute("aria-valuenow"))).toBe(Math.round(summary.goal?.percent ?? -1));
    expect(bar.getAttribute("aria-valuetext")).toMatch(/de R\$\s500,00 por mês/);

    // Números do topo.
    const stats = screen.getByRole("region", { name: "Resumo dos últimos 12 meses" });
    expect(within(stats).getByText("Recebido em 12 meses")).toBeInTheDocument();
    expect(within(stats).getByText("A receber")).toBeInTheDocument();
    expect(within(stats).getByText(/R\$\s90,00/)).toBeInTheDocument();

    // O gráfico: imagem descrita + tabela com os 12 meses.
    const chart = screen.getByRole("img", { name: /Renda dos últimos 12 meses: .* recebidos\. Maior mês: nov\/26/ });
    expect(chart).toBeInTheDocument();
    const monthsTable = screen.getByRole("table", { name: "Renda líquida por mês, últimos 12 meses" });
    expect(within(monthsTable).getAllByRole("row")).toHaveLength(13);
    const legend = screen.getByRole("list", { name: "Legenda" });
    expect(within(legend).getByText(/Recebido/)).toBeInTheDocument();
    expect(within(legend).getByText(/A receber/)).toBeInTheDocument();
    expect(within(legend).getByText(/Meta mensal/)).toBeInTheDocument();

    // Próximos pagamentos.
    const upcoming = screen.getByRole("heading", { name: "Próximos pagamentos" }).closest("section");
    expect(upcoming).not.toBeNull();
    expect(within(upcoming as HTMLElement).getByText("KNRI11")).toBeInTheDocument();
    expect(within(upcoming as HTMLElement).getByText("30/11")).toBeInTheDocument();

    // Estimativa, rotulada.
    const estimate = screen.getByRole("region", { name: "Estimativa da carteira de hoje" });
    expect(within(estimate).getByText("estimativa")).toBeInTheDocument();
    expect(within(estimate).getByText(/Não é extrato/)).toBeInTheDocument();
    expect(within(estimate).getByText(/ZZZZ3/)).toBeInTheDocument();
    expect(within(estimate).getByText("~R$ 10,50/mês")).toBeInTheDocument();

    // Ano × mês.
    const table = screen.getByRole("table", { name: "Renda líquida por ano e por mês" });
    expect(within(table).getByRole("rowheader", { name: "2026" })).toBeInTheDocument();
    expect(within(table).getByRole("rowheader", { name: "2025" })).toBeInTheDocument();
    expect(within(table).getAllByRole("columnheader")).toHaveLength(14);

    // Distribuição por pagador.
    const payers = screen.getByRole("heading", { name: "Quem pagou" }).closest("section");
    expect(within(payers as HTMLElement).getByText("KNRI11")).toBeInTheDocument();
    expect(within(payers as HTMLElement).getByText(/60,0%/)).toBeInTheDocument();
    expect(within(payers as HTMLElement).getByRole("img", { name: /por pagador: KNRI11 60,0%/ })).toBeInTheDocument();

    // Lançados.
    const ledger = await screen.findByRole("heading", { name: "Proventos lançados" });
    const list = ledger.closest("section") as HTMLElement;
    expect(await within(list).findByText("14/11/2026")).toBeInTheDocument();
    expect(within(list).getByText(/IR R\$\s15,00/)).toBeInTheDocument();
    expect(within(list).getAllByText("Recebido")).toHaveLength(2);
  });

  it("meta atingida mostra a data", async () => {
    const summary = sampleSummary();
    summary.goal = { id: "g", target: 100, achieved_at: "2026-10-05T15:00:00Z", current_monthly: 130, basis: "received", percent: 130, reached: true };
    useSummary(summary);
    await openIncome();
    expect(await screen.findByText(/Meta atingida em/)).toBeInTheDocument();
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "100");
    expect(bar.getAttribute("aria-valuetext")).toMatch(/130%/);
  });

  it("a barra pela estimativa avisa que é estimativa", async () => {
    const summary = sampleSummary();
    summary.monthly_average = 0;
    summary.goal = { id: "g", target: 500, achieved_at: null, current_monthly: 410.5, basis: "estimate", percent: 82.1, reached: false };
    useSummary(summary);
    await openIncome();
    const bar = await screen.findByRole("progressbar");
    expect(bar.getAttribute("aria-valuetext")).toMatch(/pela estimativa/);
    expect(screen.getByText(/Ainda não há proventos recebidos lançados/)).toBeInTheDocument();
  });

  it("junta o que passa de seis pagadores em Outros", async () => {
    const summary = sampleSummary();
    summary.by_payer = Array.from({ length: 8 }, (_, i) => ({
      asset_id: `a-${i}`,
      label: `PAG${i}`,
      total: 100 - i,
      percent: 12.5,
      yield_on_cost: knownYield(100 - i, 1000),
    }));
    useSummary(summary);
    await openIncome();
    expect(await screen.findByText("Outros (2)")).toBeInTheDocument();
  });
});

describe("Renda: meta", () => {
  it("define a meta de renda", async () => {
    let body: unknown;
    server.use(
      http.put("/api/v1/income/goal", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ id: "g", target: 800, achieved_at: null, current_monthly: 0, basis: "estimate", percent: 0, reached: false });
      }),
    );
    const { user } = await openIncome();
    await user.click(await screen.findByRole("button", { name: "Definir meta" }));
    const dialog = await screen.findByRole("dialog", { name: "Meta de renda mensal" });
    const input = within(dialog).getByRole("textbox", { name: "Meta por mês" });
    expect(within(dialog).getByRole("button", { name: "Salvar meta" })).toBeDisabled();
    await user.type(input, "80000");
    await user.click(within(dialog).getByRole("button", { name: "Salvar meta" }));
    await waitFor(() => expect(body).toEqual({ target: 800 }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await screen.findByText("Meta de renda salva.")).toBeInTheDocument();
    // O resumo é buscado de novo: a barra vem do servidor.
    await waitFor(() => expect(requestsTo("GET", "/api/v1/income/summary").length).toBeGreaterThan(1));
  });

  it("mostra o erro do servidor sem fechar a folha", async () => {
    server.use(http.put("/api/v1/income/goal", () => problem(422, "validation_failed", "A meta de renda precisa ser maior que zero.")));
    const { user } = await openIncome();
    await user.click(await screen.findByRole("button", { name: "Definir meta" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByRole("textbox"), "100");
    await user.click(within(dialog).getByRole("button", { name: "Salvar meta" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("maior que zero");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("edita a meta que existe, já com o valor salvo, e remove com confirmação", async () => {
    useSummary(sampleSummary());
    let removed = false;
    server.use(
      http.delete("/api/v1/income/goal", () => {
        removed = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { user } = await openIncome();
    await user.click(await screen.findByRole("button", { name: "Editar meta" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("textbox", { name: "Meta por mês" })).toHaveValue("R$ 500,00");
    await user.click(within(dialog).getByRole("button", { name: "Remover meta" }));
    expect(removed).toBe(false);
    const confirm = within(dialog).getByRole("group", { name: "Confirmar remoção da meta" });
    await user.click(within(confirm).getByRole("button", { name: "Manter" }));
    expect(within(dialog).getByRole("button", { name: "Remover meta" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Remover meta" }));
    await user.click(within(screen.getByRole("group", { name: "Confirmar remoção da meta" })).getByRole("button", { name: "Remover" }));
    await waitFor(() => expect(removed).toBe(true));
    expect(await screen.findByText("Meta de renda removida.")).toBeInTheDocument();
  });
});

describe("Renda: lançar provento", () => {
  async function openSheet() {
    const rendered = await openIncome();
    await rendered.user.click(await screen.findByRole("button", { name: /Lançar provento/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lançar provento" });
    return { ...rendered, dialog };
  }

  it("calcula a prévia e grava com o corpo certo", async () => {
    let body: unknown;
    server.use(
      http.post("/api/v1/income", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(sampleEvent(), { status: 201 });
      }),
    );
    const { user, dialog } = await openSheet();
    const submit = within(dialog).getByRole("button", { name: "Lançar provento" });
    expect(submit).toBeDisabled();

    // O primeiro ativo da carteira fictícia é a Itaúsa (ação): dividendo, 2.500 ações.
    expect(within(dialog).getByRole("combobox", { name: "Ativo" })).toHaveValue("a-itsa");
    expect(within(dialog).getByRole("radio", { name: "Dividendo" })).toBeChecked();
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "0,04");
    expect(within(dialog).getByText(/^Bruto/).parentElement).toHaveTextContent(/R\$\s100,00/);

    // JCP pago em 2026: 17,5% retido na fonte (LC 224/2025).
    await user.click(within(dialog).getByRole("radio", { name: "JCP" }));
    expect(within(dialog).getByText(/^IR retido/).parentElement).toHaveTextContent(/R\$\s17,50/);
    expect(within(dialog).getByText(/^Líquido/).parentElement).toHaveTextContent(/R\$\s82,50/);
    expect(within(dialog).getByText(/17,5% desde 2026/)).toBeInTheDocument();

    await user.clear(within(dialog).getByLabelText("Quantidade"));
    await user.type(within(dialog).getByLabelText("Quantidade"), "1000");
    expect(within(dialog).getByText(/^Bruto/).parentElement).toHaveTextContent(/R\$\s40,00/);

    await user.click(submit);
    await waitFor(() =>
      expect(body).toEqual({ asset_id: "a-itsa", kind: "jcp", pay_date: "2026-11-25", amount_per_unit: 0.04, quantity: 1000 }),
    );
    expect(await screen.findByText("Provento de ITSA4 lançado.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("FII sugere rendimento e deixa a quantidade para o livro na data com; data ex opcional vai junto", async () => {
    let body: unknown;
    server.use(
      http.post("/api/v1/income", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(sampleEvent(), { status: 201 });
      }),
    );
    const { user, dialog } = await openSheet();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Ativo" }), "a-knri");
    expect(within(dialog).getByRole("radio", { name: "Rendimento" })).toBeChecked();
    await waitFor(() => expect(within(dialog).getByLabelText("Quantidade")).toHaveAttribute("placeholder", "10"));
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "1,10");
    expect(within(dialog).getByText(/^Líquido/).parentElement).toHaveTextContent(/R\$\s11,00/);
    await user.type(within(dialog).getByLabelText("Data ex (opcional)"), "2026-11-10");
    await user.click(within(dialog).getByRole("button", { name: "Lançar provento" }));
    await waitFor(() =>
      expect(body).toEqual({ asset_id: "a-knri", kind: "reit_income", pay_date: "2026-11-25", ex_date: "2026-11-10", amount_per_unit: 1.1 }),
    );
  });

  it("renda fixa por valor não pede quantidade", async () => {
    let body: unknown;
    server.use(
      http.post("/api/v1/income", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(sampleEvent(), { status: 201 });
      }),
    );
    const { user, dialog } = await openSheet();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Ativo" }), "a-ipca");
    expect(within(dialog).queryByLabelText("Quantidade")).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("radio", { name: "Outro" }));
    await user.type(within(dialog).getByLabelText("Valor recebido (R$)"), "250");
    await user.click(within(dialog).getByRole("button", { name: "Lançar provento" }));
    await waitFor(() => expect(body).toEqual({ asset_id: "a-ipca", kind: "other", pay_date: "2026-11-25", amount_per_unit: 250 }));
  });

  it("não deixa gravar com data ex depois do pagamento e mostra o erro do servidor", async () => {
    server.use(http.post("/api/v1/income", () => problem(422, "validation_failed", "O imposto retido precisa ficar entre 0 e o valor bruto.")));
    const { user, dialog } = await openSheet();
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "1");
    await user.type(within(dialog).getByLabelText("Data ex (opcional)"), "2026-12-01");
    expect(within(dialog).getByText("A data ex não pode ser depois do pagamento.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Lançar provento" })).toBeDisabled();

    await user.clear(within(dialog).getByLabelText("Data ex (opcional)"));
    await user.click(within(dialog).getByRole("button", { name: "Lançar provento" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("imposto retido");
  });

  it("sem ativo na carteira manda cadastrar um primeiro", async () => {
    server.use(http.get("/api/v1/portfolio", () => HttpResponse.json({ total_value: 0, quotes_updated_at: null, classes: [], assets: [], checklists: { stocks: [], real_estate: [] } })));
    const rendered = renderApp("renda");
    await rendered.user.click(await screen.findByRole("button", { name: /Lançar provento/ }));
    expect(await screen.findByText(/Cadastre um ativo na Carteira primeiro/)).toBeInTheDocument();
  });
});

describe("Renda: apagar provento", () => {
  it("pede confirmação na linha e apaga", async () => {
    useEvents([sampleEvent()]);
    let deleted: string | undefined;
    server.use(
      http.delete("/api/v1/income/:id", ({ params }) => {
        deleted = String(params.id);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { user } = await openIncome();
    await user.click(await screen.findByRole("button", { name: "Apagar provento de KNRI11 de 14/11/2026" }));
    expect(deleted).toBeUndefined();
    const confirm = screen.getByRole("group", { name: "Apagar provento de KNRI11?" });
    await user.click(within(confirm).getByRole("button", { name: "Manter" }));
    await user.click(screen.getByRole("button", { name: "Apagar provento de KNRI11 de 14/11/2026" }));
    await user.click(within(screen.getByRole("group", { name: "Apagar provento de KNRI11?" })).getByRole("button", { name: "Apagar" }));
    await waitFor(() => expect(deleted).toBe("e-1"));
    expect(await screen.findByText("Provento de KNRI11 apagado.")).toBeInTheDocument();
  });

  it("mostra o erro quando o servidor recusa", async () => {
    useEvents([sampleEvent({ source: "b3_import" })]);
    server.use(http.delete("/api/v1/income/:id", () => problem(404, "not_found", "x")));
    const { user } = await openIncome();
    await user.click(await screen.findByRole("button", { name: /Apagar provento de KNRI11/ }));
    await user.click(within(screen.getByRole("group", { name: "Apagar provento de KNRI11?" })).getByRole("button", { name: "Apagar" }));
    expect(await screen.findByText(/Não encontrei esse item/)).toBeInTheDocument();
  });

  it("a lista de lançados mostra o que ainda vai cair como a receber", async () => {
    useEvents([sampleEvent({ status: "upcoming", pay_date: "2026-12-10", symbol: null, display_name: "CDB Banco X" })]);
    await openIncome();
    const ledger = (await screen.findByRole("heading", { name: "Proventos lançados" })).closest("section") as HTMLElement;
    expect(await within(ledger).findByText("CDB Banco X")).toBeInTheDocument();
    expect(within(ledger).getByText("A receber")).toBeInTheDocument();
  });

  it("não consegue carregar a lista: avisa sem derrubar a tela", async () => {
    server.use(http.get("/api/v1/income", () => problem(500, "internal", "x")));
    await openIncome();
    expect(await screen.findByText("Não consegui carregar os proventos agora.")).toBeInTheDocument();
  });
});
