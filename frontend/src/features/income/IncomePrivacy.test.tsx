import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { samplePlan } from "../../test/fixtures";
import { emptySummary, sampleCash, sampleEvent, sampleSummary } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

/** Os atalhos de valor do campo do aporte são constantes do app, não dado da pessoa. */
const CONSTANTS = '[aria-label="Valores rápidos"]';

/**
 * Tudo o que a pessoa pode ver ou ouvir: o texto da página, os atributos que o
 * leitor de tela lê no lugar do texto e o valor dos campos. Inclui o que é
 * `sr-only`, que existe só para o leitor de tela.
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

function openSummaryWithData() {
  const summary = sampleSummary();
  server.use(
    http.get("/api/v1/income/summary", () => HttpResponse.json(summary)),
    http.get("/api/v1/income", () =>
      HttpResponse.json([sampleEvent(), sampleEvent({ id: "e-2", kind: "jcp", gross: 100, withholding: 15, net: 85 })]),
    ),
  );
}

describe("Renda com o olho fechado", () => {
  it("nenhum valor em reais aparece, nem para o leitor de tela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    openSummaryWithData();
    renderApp("renda");
    await screen.findByRole("progressbar");
    await screen.findAllByText("14/11/2026");

    expectNoMoney();
    // O que não é dinheiro continua: proporção, nome, data.
    expect(screen.getByRole("progressbar").getAttribute("aria-valuetext")).toContain(MASK_BRL);
    expect(screen.getByRole("progressbar").getAttribute("aria-valuetext")).toMatch(/%/);
    expect(screen.getByRole("img", { name: /^Renda dos últimos 12 meses: / }).getAttribute("aria-label")).toContain(MASK_BRL);
    const monthsTable = screen.getByRole("table", { name: "Renda líquida por mês, últimos 12 meses" });
    expect(within(monthsTable).getAllByText(MASK_BRL).length).toBeGreaterThan(12);
    expect(screen.getAllByText("KNRI11").length).toBeGreaterThan(0);
    expect(screen.getByRole("heading", { name: "Quem pagou" })).toBeInTheDocument();
  });

  it("fechar o olho na própria tela mascara tudo e abrir devolve os números", async () => {
    openSummaryWithData();
    const { user } = renderApp("renda");
    await screen.findByRole("progressbar");
    expect(everythingHeard().some((text) => MONEY.test(text))).toBe(true);

    const eye = screen.getByRole("button", { name: "Ocultar valores" });
    await user.click(eye);
    expect(eye).toHaveAttribute("aria-pressed", "true");
    expectNoMoney();
    await user.click(eye);
    expect(eye).toHaveAttribute("aria-pressed", "false");
    expect(everythingHeard().some((text) => MONEY.test(text))).toBe(true);
  });

  it("a folha de lançar provento e a de meta não mostram valor", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    openSummaryWithData();
    const { user } = renderApp("renda");
    await screen.findByRole("progressbar");

    await user.click(screen.getByRole("button", { name: "Editar meta" }));
    expectNoMoney(await screen.findByRole("dialog"));
    await user.click(screen.getByRole("button", { name: "Fechar" }));

    await user.click(screen.getByRole("button", { name: /Lançar provento/ }));
    const dialog = await screen.findByRole("dialog", { name: "Lançar provento" });
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Ativo" }), "a-knri");
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "1,10");
    expect(within(dialog).getByText(/^Líquido/).parentElement).toHaveTextContent(MASK_BRL);
    expectNoMoney(dialog);
    // A quantidade da carteira (placeholder) também é valor de carteira.
    expect(within(dialog).getByLabelText("Quantidade").getAttribute("placeholder")).not.toBe("10");
  });

  it("estimativa e lista de lançados também", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const summary = emptySummary();
    summary.estimate = {
      monthly: 410.5,
      updated_at: "2026-11-25T11:00:00Z",
      assets: [{ asset_id: "a-knri", symbol: "KNRI11", display_name: "Kinea", quantity: 10, per_unit_12m: 12.6, monthly: 10.5, trailing_12m: 126 }],
      missing: [],
    };
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)));
    renderApp("renda");
    const estimate = await screen.findByRole("region", { name: "Estimativa da carteira de hoje" });
    expect(within(estimate).getByRole("table", { name: "Os ativos que mais rendem na estimativa" })).toBeInTheDocument();
    expect(within(estimate).getByText(`~${MASK_BRL}`)).toBeInTheDocument();
    expectNoMoney();
  });
});

describe("Aportar com o olho fechado", () => {
  it("a renda do plano e o caixa saem mascarados", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const plan = samplePlan(6000);
    plan.income = { available: true, missing: [], added_monthly: 35.4, before_monthly: 410.5, after_monthly: 445.9 };
    plan.items = plan.items.map((item, index) => ({ ...item, estimated_monthly_income: index === 0 ? 20.25 : null }));
    server.use(
      http.post("/api/v1/plans", () => HttpResponse.json(plan)),
      http.get("/api/v1/cash", () => HttpResponse.json(sampleCash())),
    );
    const { user } = renderApp("aportar");
    const offer = await screen.findByRole("group", { name: "Caixa disponível para aportar" });
    expect(offer).toHaveTextContent(`+ ${MASK_BRL} em caixa`);

    const input = screen.getByRole("textbox", { name: "Quanto você vai aportar?" });
    await user.click(screen.getByRole("button", { name: "Incluir" }));
    expect(screen.getByRole("group", { name: "Caixa disponível para aportar" })).toHaveTextContent(MASK_BRL);
    expect(input.getAttribute("value")).not.toMatch(MONEY);
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const line = within(result).getByText(/Este aporte soma/).closest("p") as HTMLElement;
    expect(line).toHaveTextContent(`~${MASK_BRL}/mês de renda`);
    expect(line).toHaveTextContent(`de ${MASK_BRL} para ${MASK_BRL} por mês`);
    expect(within(result).getByText(`+~${MASK_BRL}/mês de renda`)).toBeInTheDocument();
    expectNoMoney();
  });
});
