import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

async function openCriteria() {
  const rendered = renderApp("criterios");
  await screen.findByRole("heading", { name: "As perguntas que dão a nota" });
  return rendered;
}

const card = (name: string) => screen.getByRole("region", { name });

describe("Critérios", () => {
  it("lista as duas listas com quem as usa", async () => {
    await openCriteria();
    expect(within(card("Ações")).getByText(/Ações Nacionais e Ações Internacionais · 1 ativo usa/)).toBeInTheDocument();
    expect(within(card("Imobiliário")).getByText(/5 ativos usam/)).toBeInTheDocument();
    expect(within(card("Imobiliário")).getAllByRole("listitem")).toHaveLength(6);
  });

  it("adiciona pergunta", async () => {
    const { user } = await openCriteria();
    const stocks = card("Ações");
    const input = within(stocks).getByLabelText("Nova pergunta para Ações");
    expect(within(stocks).getByRole("button", { name: "Adicionar pergunta" })).toBeDisabled();
    await user.type(input, "Paga dividendos há mais de 5 anos?");
    await user.click(within(stocks).getByRole("button", { name: "Adicionar pergunta" }));
    expect(await within(stocks).findByText("Paga dividendos há mais de 5 anos?")).toBeInTheDocument();
    expect(input).toHaveValue("");
    expect(getDb().checklists.stocks).toHaveLength(4);
  });

  it("edita com Enter, cancela com Esc e ignora texto igual", async () => {
    const { user } = await openCriteria();
    const stocks = card("Ações");
    await user.click(within(stocks).getByRole("button", { name: "Editar pergunta 2" }));
    let input = within(stocks).getByLabelText("Editar pergunta 2");
    expect(input).toHaveFocus();
    await user.clear(input);
    await user.type(input, "ROE médio acima de 15%?{Enter}");
    expect(await within(stocks).findByText("ROE médio acima de 15%?")).toBeInTheDocument();
    expect(getDb().checklists.stocks[1]?.prompt).toBe("ROE médio acima de 15%?");

    await user.click(within(stocks).getByRole("button", { name: "Editar pergunta 1" }));
    input = within(stocks).getByLabelText("Editar pergunta 1");
    await user.type(input, " mudou{Escape}");
    expect(within(stocks).getByText("Lucro líquido positivo em pelo menos 4 dos últimos 5 anos?")).toBeInTheDocument();

    const requestsBefore = getDb().requests.length;
    await user.click(within(stocks).getByRole("button", { name: "Editar pergunta 3" }));
    await user.click(within(stocks).getByRole("button", { name: "Salvar" }));
    expect(within(stocks).getByText("Dívida líquida/EBITDA abaixo de 2?")).toBeInTheDocument();
    expect(getDb().requests.slice(requestsBefore).some((r) => r.method === "PATCH")).toBe(false);
  });

  it("remove com confirmação, e manter desiste", async () => {
    const { user } = await openCriteria();
    const realEstate = card("Imobiliário");
    await user.click(within(realEstate).getByRole("button", { name: "Remover pergunta 6" }));
    await user.click(within(realEstate).getByRole("button", { name: "Manter" }));
    await user.click(within(realEstate).getByRole("button", { name: "Remover pergunta 6" }));
    await user.click(within(within(realEstate).getByRole("group", { name: "Confirmar remoção" })).getByRole("button", { name: "Remover" }));
    await waitFor(() => expect(getDb().checklists.real_estate).toHaveLength(5));
    await waitFor(() => expect(within(realEstate).queryByText("Está negociado com P/VP abaixo de 1?")).toBeNull(), {
      timeout: 3000,
    });
  });

  it("restaura o padrão depois de confirmar", async () => {
    getDb().checklists.stocks = [];
    const { user } = await openCriteria();
    const stocks = card("Ações");
    expect(within(stocks).getByText(/Sem perguntas/)).toBeInTheDocument();
    await user.click(within(stocks).getByRole("button", { name: "Restaurar padrão" }));
    await user.click(within(stocks).getByRole("button", { name: "Cancelar" }));
    await user.click(within(stocks).getByRole("button", { name: "Restaurar padrão" }));
    await user.click(within(stocks).getByRole("button", { name: "Restaurar" }));
    expect(await screen.findByText("Perguntas de Ações voltaram ao padrão.")).toBeInTheDocument();
    expect(await within(stocks).findAllByRole("listitem")).toHaveLength(3);
  });

  it("erros viram aviso", async () => {
    const fail = () => new HttpResponse(null, { status: 500 });
    server.use(
      http.post("/api/v1/checklists/:kind/questions", fail),
      http.post("/api/v1/checklists/:kind/reset", fail),
      http.patch("/api/v1/checklist-questions/:id", fail),
    );
    const { user } = await openCriteria();
    const stocks = card("Ações");
    await user.type(within(stocks).getByLabelText("Nova pergunta para Ações"), "Nova?");
    await user.click(within(stocks).getByRole("button", { name: "Adicionar pergunta" }));
    expect(await screen.findAllByText(/Algo deu errado do nosso lado/)).not.toHaveLength(0);

    await user.click(within(stocks).getByRole("button", { name: "Editar pergunta 1" }));
    await user.type(within(stocks).getByLabelText("Editar pergunta 1"), "!{Enter}");
    await user.click(within(stocks).getByRole("button", { name: "Restaurar padrão" }));
    await user.click(within(stocks).getByRole("button", { name: "Restaurar" }));
    await waitFor(() => expect(screen.getAllByText(/Algo deu errado do nosso lado/).length).toBeGreaterThanOrEqual(2));
  });
});
