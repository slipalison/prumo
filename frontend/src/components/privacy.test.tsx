import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Contribution, LivePrices } from "../api/types";
import { HIDE_VALUES_KEY } from "../lib/privacy";
import { FakeEventSource } from "../test/eventSource";
import { toPortfolio } from "../test/fixtures";
import { renderApp } from "../test/render";
import { getDb } from "../test/server";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;
const MASK = "R$ ••••";

/**
 * Os atalhos de valor do campo do aporte (R$ 500, R$ 1.000...) são constantes
 * do app, não dado da pessoa: ficam visíveis de propósito.
 */
const CONSTANTS = '[aria-label="Valores rápidos"]';

/**
 * Tudo o que a pessoa pode ver ou ouvir: o texto da página e os atributos que
 * o leitor de tela lê no lugar do texto (`aria-label`, `title`, `alt`) mais o
 * valor dos campos. Nenhum deles pode trazer valor em reais.
 */
function everythingHeard(root: HTMLElement = document.body): string[] {
  const heard: string[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.parentElement?.closest(CONSTANTS)) heard.push(node.textContent ?? "");
  }
  for (const element of root.querySelectorAll("*")) {
    if (element.closest(CONSTANTS)) continue;
    for (const attribute of ["aria-label", "aria-valuetext", "title", "alt", "placeholder"]) {
      const value = element.getAttribute(attribute);
      if (value) heard.push(value);
    }
    if (element instanceof HTMLInputElement) heard.push(element.value);
  }
  return heard;
}

function expectNoMoney(root?: HTMLElement) {
  for (const text of everythingHeard(root)) expect(text).not.toMatch(MONEY);
}

const eye = () => screen.getByRole("button", { name: "Ocultar valores" });

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  await screen.findByText(/R\$\s120\.132,23/);
  return rendered;
}

const history: Contribution = {
  id: "c1",
  created_at: "2026-11-20T14:00:00Z",
  amount: 3000,
  invested: 2987.5,
  items: [
    { asset_id: "a-knri", symbol: "KNRI11", display_name: "Kinea", quantity: 7, unit_price: 142, value: 994 },
    { asset_id: "a-btc", symbol: "BTC", display_name: "Bitcoin", quantity: 0.0042, unit_price: 345000, value: 1449 },
  ],
};

describe("Ocultar valores", () => {
  it("o olho fica na navegação, como botão de alternância, e começa mostrando tudo", async () => {
    renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    const nav = screen.getByRole("navigation", { name: "Principal" });
    expect(within(nav).getByRole("button", { name: "Ocultar valores" })).toHaveAttribute("aria-pressed", "false");
  });

  it("mascara patrimônio, ativos, quantidades e preços na Carteira", async () => {
    getDb().contributions = [history];
    const { user } = await openPortfolio();
    await screen.findByText(/R\$\s2\.987,50/);
    await user.click(eye());

    expect(eye()).toHaveAttribute("aria-pressed", "true");
    expectNoMoney();
    const hero = screen.getByRole("region", { name: "Patrimônio" });
    expect(hero).toHaveTextContent(MASK);
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    const knri = within(fiis).getByRole("button", { name: /KNRI11/ });
    expect(knri).toHaveTextContent(`•• cotas · ${MASK}`);
    expect(knri).not.toHaveTextContent(/\d+ cotas/);
    // O histórico: o valor investido e as quantidades.
    const historyCard = screen.getByRole("region", { name: "Histórico de aportes" });
    expect(historyCard).toHaveTextContent(MASK);
    expect(historyCard).toHaveTextContent("•• KNRI11 · •• BTC");
    // O que não é valor segue: percentuais, nomes, notas.
    expect(knri).toHaveTextContent("da classe");
    expect(screen.getByRole("img", { name: /Carteira por classe: Ações Nacionais/ })).toBeInTheDocument();
  });

  it("o leitor de tela também lê a máscara: nem o texto só para leitor guarda o valor", async () => {
    const { user } = await openPortfolio();
    const sr = document.querySelectorAll(".sr-only");
    // Antes de ocultar, o count-up já assentou: nenhum texto escondido com valor.
    for (const element of sr) expect(element.textContent).not.toMatch(MONEY);
    await user.click(eye());
    for (const element of document.querySelectorAll(".sr-only")) expect(element.textContent).not.toMatch(MONEY);
    expectNoMoney();
  });

  it("o count-up não conta número nenhum enquanto oculto, e volta ao mostrar", async () => {
    const { user } = await openPortfolio();
    await user.click(eye());
    const total = document.querySelector(".display-number");
    expect(total).toHaveTextContent(MASK);
    expect(total).not.toHaveTextContent(/\d/);
    await user.click(eye());
    expect(eye()).toHaveAttribute("aria-pressed", "false");
    await waitFor(() => expect(document.querySelector(".display-number")).toHaveTextContent(/R\$\s120\.132,23/));
  });

  it("fica salvo no localStorage e vale na próxima abertura", async () => {
    const first = await openPortfolio();
    await first.user.click(eye());
    expect(window.localStorage.getItem(HIDE_VALUES_KEY)).toBe("1");
    first.unmount();

    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await waitFor(() => expect(eye()).toHaveAttribute("aria-pressed", "true"));
    await screen.findByRole("region", { name: "Fundos Imobiliários" });
    expectNoMoney();
  });

  it("mostrar de novo apaga a escolha guardada", async () => {
    const { user } = await openPortfolio();
    await user.click(eye());
    await user.click(eye());
    expect(window.localStorage.getItem(HIDE_VALUES_KEY)).toBeNull();
    expect(await screen.findByText(/R\$\s120\.132,23/)).toBeInTheDocument();
  });

  it("vale em todas as telas: Aportar, com o plano, o troco e as frases dos avisos", async () => {
    const { user } = renderApp("aportar");
    const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    await within(result).findByText(/R\$\s5\.947,92/);
    expect(within(result).getByText(/17 cotas × R\$\s53,81/)).toBeInTheDocument();

    await user.click(eye());
    expectNoMoney();
    expect(within(result).getByText(/troco de R\$ ••••/)).toBeInTheDocument();
    expect(within(result).getAllByText(/•• cotas × R\$ ••••/).length).toBeGreaterThan(0);
    // A frase do aviso ("precisa de R$ 25,2 mil") também.
    expect(within(result).getByText(/Ações Internacionais precisa de R\$ ••••/)).toBeInTheDocument();
    // O que não é valor segue.
    expect(within(result).getByRole("heading", { name: /Compre 4 ativos/ })).toBeInTheDocument();
  });

  it("o campo do aporte segue editável: mostra o número só enquanto tem o foco", async () => {
    const { user } = renderApp("aportar");
    const input = await screen.findByRole<HTMLInputElement>("textbox", { name: "Quanto você vai aportar?" });
    await user.type(input, "600000");
    expect(input.value).toMatch(/^R\$\s6\.000,00$/);

    await user.click(eye());
    expect(input).not.toHaveFocus();
    expect(input).toHaveValue(MASK);

    // Com o foco, a pessoa vê o que digita.
    await user.click(input);
    expect(input.value).toMatch(/^R\$\s6\.000,00$/);
    // O foco seleciona o valor inteiro: o próximo dígito começa um valor novo.
    await user.keyboard("250000");
    expect(input.value).toMatch(/^R\$\s2\.500,00$/);

    // Saiu do campo: volta a máscara, e o valor continua o que foi digitado.
    await user.click(screen.getByRole("heading", { name: "Quanto você vai aportar?" }));
    expect(input).toHaveValue(MASK);
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    expect(getDb().requests.filter((r) => r.path === "/api/v1/plans").at(-1)?.body).toEqual({ amount: 2500 });
  });

  it("a folha de registrar o aporte: valores e quantidades mascarados, editáveis com o foco", async () => {
    const { user } = renderApp("aportar");
    const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(eye());
    await user.click(await screen.findByRole("button", { name: /Já comprei/ }));
    const dialog = await screen.findByRole("dialog", { name: "Registrar aporte" });
    expectNoMoney(dialog);
    expect(within(dialog).getByText("Total").parentElement).toHaveTextContent(MASK);
    const first = within(dialog).getAllByRole("textbox")[0];
    if (!first) throw new Error("folha sem campo de quantidade");
    expect(first).toHaveValue("••");
    await user.click(first);
    expect((first as HTMLInputElement).value).toMatch(/^\d+$/);
  });

  it("o atalho do último aporte não entrega o valor", async () => {
    getDb().contributions = [history];
    const { user } = renderApp("aportar");
    await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    expect(await screen.findByRole("button", { name: /Último: R\$\s3\.000,00/ })).toBeInTheDocument();
    await user.click(eye());
    expect(screen.getByRole("button", { name: `Último: ${MASK}` })).toBeInTheDocument();
    expectNoMoney();
  });

  it("a folha do ativo: resumo mascarado e quantidade editável", async () => {
    const { user } = await openPortfolio();
    await user.click(eye());
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    await user.click(within(fiis).getByRole("button", { name: /KNRI11/ }));
    const dialog = await screen.findByRole("dialog");
    expectNoMoney(dialog);
    expect(within(dialog).getByText(MASK, { selector: ".edit-asset__value" })).toBeInTheDocument();

    const quantity = within(dialog).getByLabelText("Quantidade");
    expect(quantity).toHaveValue("••");
    await user.click(quantity);
    expect(quantity).toHaveValue("10");
    await user.clear(quantity);
    await user.type(quantity, "12");
    expect(quantity).toHaveValue("12");
    await user.click(within(dialog).getByRole("heading", { level: 2 }));
    expect(quantity).toHaveValue("••");
    expectNoMoney(dialog);
  });

  it("o selo ao vivo e as setas de preço não entregam o sentido do patrimônio", async () => {
    const { user } = await openPortfolio();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    await user.click(eye());

    const knri = getDb().assets.find((a) => a.symbol === "KNRI11");
    if (!knri) throw new Error("fixture sem KNRI11");
    knri.quote_price = 150;
    const db = getDb();
    const portfolio = toPortfolio(db.assets, db.goals, db.checklists);
    const event: LivePrices = {
      total_value: portfolio.total_value,
      quotes_updated_at: portfolio.quotes_updated_at,
      classes: portfolio.classes,
      assets: portfolio.assets,
    };
    act(() => FakeEventSource.latest().emit("prices", event));

    // O selo ao vivo continua dizendo o estado, e nenhum valor.
    expect(await screen.findByText("Ao vivo")).toBeInTheDocument();
    expect(screen.queryByText("▲")).not.toBeInTheDocument();
    expect(screen.queryByText("▼")).not.toBeInTheDocument();
    expect(document.querySelector(".price-flash__value--up")).toBeNull();
    expectNoMoney();

    // Mostrando de novo, o valor novo aparece (o stream seguiu rodando).
    await user.click(eye());
    expect(await screen.findByText(/R\$\s1\.500,00/)).toBeInTheDocument();
  });

  it("o botão de olho não carrega valor nenhum no nome", async () => {
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    expect(eye().getAttribute("aria-label")).toBe("Ocultar valores");
  });
});
