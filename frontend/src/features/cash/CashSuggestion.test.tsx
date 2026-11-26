import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { Cash } from "../../api/cashTypes";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { emptyCash, sampleCash } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

const GROUP = "Caixa disponível para aportar";

/** O caixa que o servidor devolve; `calls` conta as buscas. */
const cash = (value: Cash, calls = { n: 0 }) =>
  http.get("/api/v1/cash", () => {
    calls.n += 1;
    return HttpResponse.json(value);
  });

async function openCalculator() {
  const rendered = renderApp("aportar");
  const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
  return { ...rendered, input };
}

describe("Aportar: o caixa", () => {
  it("sem caixa, não oferece nada", async () => {
    await openCalculator();
    expect(screen.queryByRole("group", { name: GROUP })).not.toBeInTheDocument();
  });

  it("oferece o saldo, soma ao valor e deixa tirar", async () => {
    server.use(cash(sampleCash()));
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: GROUP });
    expect(offer).toHaveTextContent(/\+ R\$\s812,00 em caixa/);

    await user.type(input, "300000");
    expect(input).toHaveValue("R$ 3.000,00");
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    expect(input).toHaveValue("R$ 3.812,00");
    expect(offer).toHaveTextContent(/R\$\s812,00 do caixa incluídos no aporte/);

    await user.click(within(offer).getByRole("button", { name: "Tirar" }));
    expect(input).toHaveValue("R$ 3.000,00");
    expect(offer).toHaveTextContent(/\+ R\$\s812,00 em caixa/);
  });

  it("mostra de onde veio o caixa: proventos, vendas e o que os aportes já usaram", async () => {
    server.use(cash(sampleCash()));
    const { user } = await openCalculator();
    const offer = await screen.findByRole("group", { name: GROUP });
    const detail = within(offer).getByText("De onde vem esse caixa").closest("details") as HTMLElement;
    expect(detail).not.toHaveAttribute("open");
    expect(detail).toHaveTextContent(/Proventos recebidos \(3 pagamentos\)\s*R\$\s612,00/);
    expect(detail).toHaveTextContent(/Vendas, líquido de taxas \(1 venda\)\s*R\$\s300,00/);
    expect(detail).toHaveTextContent(/Já usado em aportes\s*− R\$\s100,00/);
    // A data de partida e o aporte de dinheiro de fora ficam no ⓘ da linha do caixa.
    await user.click(within(offer).getByRole("button", { name: "Como o caixa entra no aporte" }));
    const tip = within(offer).getByRole("status");
    expect(tip).toHaveTextContent("Conta desde 01/10/2026");
    expect(tip).toHaveTextContent(/Aportou dinheiro de fora\? Digite o valor total/);
    // Do mais novo ao mais velho, com o sentido escrito.
    const items = within(detail).getAllByRole("listitem");
    expect(items[0]).toHaveTextContent(/20\/11 · Venda · ITSA4\s*\+ R\$\s300,00/);
    expect(items[2]).toHaveTextContent(/02\/11 · Aporte · Aporte\s*− R\$\s100,00/);
  });

  it("incluir com o campo vazio põe só o caixa, e o plano pede o total", async () => {
    server.use(cash({ ...sampleCash(), balance: 812.5 }));
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: GROUP });
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    expect(input).toHaveValue("R$ 812,50");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(getDb().requests.filter((r) => r.path === "/api/v1/plans").at(-1)?.body).toEqual({ amount: 812.5 });
  });

  it("incluir depois de calcular descarta o plano velho", async () => {
    server.use(cash({ ...sampleCash(), balance: 100 }));
    const { user, input } = await openCalculator();
    await screen.findByRole("group", { name: GROUP });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(screen.getByRole("button", { name: "Incluir" }));
    expect(screen.queryByRole("region", { name: "Resultado do aporte" })).not.toBeInTheDocument();
    expect(input).toHaveValue("R$ 6.100,00");
  });

  it("digitar um valor menor que o caixa incluído desfaz a inclusão", async () => {
    server.use(cash({ ...sampleCash(), balance: 500 }));
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: GROUP });
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    expect(input).toHaveValue("R$ 500,00");
    await user.clear(input);
    await user.type(input, "20000");
    expect(input).toHaveValue("R$ 200,00");
    // O caixa nunca passa do aporte: a oferta volta.
    expect(offer).toHaveTextContent(/\+ R\$\s500,00 em caixa/);
    expect(within(offer).getByRole("button", { name: "Incluir" })).toBeInTheDocument();
  });

  it("registrar o aporte com o caixa incluído manda `cash_used`, e o saldo baixa", async () => {
    let balance = 812;
    let sent: { amount: number; cash_used?: number } | undefined;
    server.use(
      http.get("/api/v1/cash", () => HttpResponse.json({ ...sampleCash(), balance })),
      http.post("/api/v1/contributions", async ({ request }) => {
        const body = (await request.json()) as { amount: number; cash_used?: number };
        sent = body;
        balance -= body.cash_used ?? 0;
        return HttpResponse.json(
          { id: "c-9", created_at: "2026-11-25T12:00:00Z", amount: body.amount, cash_used: body.cash_used ?? 0, invested: body.amount, items: [] },
          { status: 201 },
        );
      }),
    );
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: GROUP });
    await user.type(input, "600000");
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(screen.getByRole("button", { name: /Já comprei/ }));
    const sheet = await screen.findByRole("dialog");
    expect(sheet).toHaveTextContent(/R\$\s812,00 deste aporte saem do caixa/);
    await user.click(within(sheet).getByRole("button", { name: /Registrar aporte/ }));
    await screen.findByText("Aporte registrado. A carteira já está atualizada.");

    expect(sent?.amount).toBe(6812);
    expect(sent?.cash_used).toBe(812);
    // O saldo é buscado de novo e a oferta some (o caixa foi gasto).
    await waitFor(() => expect(screen.queryByRole("group", { name: GROUP })).not.toBeInTheDocument());
  });

  it("aporte sem o caixa não manda `cash_used`: é dinheiro de fora", async () => {
    server.use(cash(sampleCash()));
    const { user, input } = await openCalculator();
    await screen.findByRole("group", { name: GROUP });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(screen.getByRole("button", { name: /Já comprei/ }));
    const sheet = await screen.findByRole("dialog");
    expect(sheet).not.toHaveTextContent(/saem do caixa/);
    await user.click(within(sheet).getByRole("button", { name: /Registrar aporte/ }));
    await screen.findByText("Aporte registrado. A carteira já está atualizada.");
    const sent = getDb().requests.filter((r) => r.path === "/api/v1/contributions" && r.method === "POST").at(-1)?.body as Record<string, unknown>;
    expect(sent).not.toHaveProperty("cash_used");
  });

  it("falha no caixa não atrapalha o aporte", async () => {
    server.use(http.get("/api/v1/cash", () => HttpResponse.json({ code: "internal", status: 500 }, { status: 500 })));
    const { user, input } = await openCalculator();
    await user.type(input, "100000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    expect(await screen.findByRole("region", { name: "Resultado do aporte" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: GROUP })).not.toBeInTheDocument();
  });

  it("caixa zerado não aparece, mesmo com histórico", async () => {
    server.use(cash({ ...emptyCash(), used: 300, income: { amount: 300, count: 1 } }));
    await openCalculator();
    await waitFor(() => expect(screen.queryByRole("group", { name: GROUP })).not.toBeInTheDocument());
  });
});

describe("Aportar com o olho fechado: o caixa", () => {
  it("nenhum valor do caixa aparece, nem no detalhe nem para o leitor de tela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(cash(sampleCash()));
    const { user, input } = await openCalculator();
    const offer = await screen.findByRole("group", { name: GROUP });
    expect(offer).toHaveTextContent(`+ ${MASK_BRL} em caixa`);
    await user.click(within(offer).getByRole("button", { name: "Incluir" }));
    expect(input.getAttribute("value")).not.toMatch(/R\$\s*\d/);
    expect(offer).toHaveTextContent(`${MASK_BRL} do caixa incluídos no aporte`);

    const walker = document.createTreeWalker(offer, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      expect(node.textContent ?? "").not.toMatch(/R\$\s*\d/);
    }
    for (const element of offer.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "title", "placeholder"]) {
        expect(element.getAttribute(attribute) ?? "").not.toMatch(/R\$\s*\d/);
      }
    }
  });
});
