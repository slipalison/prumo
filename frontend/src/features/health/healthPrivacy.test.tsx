import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

/** Texto da página mais os atributos que o leitor de tela lê no lugar dele. */
function everythingHeard(root: HTMLElement): string[] {
  const heard: string[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) heard.push(node.textContent ?? "");
  for (const element of root.querySelectorAll("*")) {
    for (const attribute of ["aria-label", "aria-valuetext", "title", "alt", "placeholder"]) {
      const value = element.getAttribute(attribute);
      if (value) heard.push(value);
    }
    if (element instanceof HTMLInputElement) heard.push(element.value);
  }
  return heard;
}

function expectNoMoney(root: HTMLElement) {
  for (const text of everythingHeard(root)) expect(text).not.toMatch(MONEY);
}

describe("Saúde com o olho fechado", () => {
  it("mascara o aporte sugerido em cada aviso e deixa percentuais e limites", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    renderApp("ajustes/saude");
    const list = await screen.findByRole("list", { name: "Avisos" });
    expectNoMoney(list);
    expectNoMoney(document.body);

    const ipca = within(list).getByRole("article", { name: "Tesouro IPCA+ 2035 é 70,76% da carteira" });
    expect(within(ipca).getByText(/aporte cerca de R\$ •••• em outros ativos/)).toBeInTheDocument();
    expect(within(ipca).getByText("Agora").nextSibling).toHaveTextContent("70,76%");
    expect(within(ipca).getByText("Limite").nextSibling).toHaveTextContent("15%");
    expect(within(ipca).getByRole("meter")).toHaveAttribute("aria-valuenow", "70.76");

    // Renda: o valor que o pagador pagou some; o percentual e os meses ficam.
    const payer = within(list).getByRole("article", { name: "KNRI11 pagou 80,01% da renda de 12 meses" });
    expect(within(payer).getByText(/KNRI11 pagou R\$ •••• dos R\$ •••• que você recebeu em 12 meses/)).toBeInTheDocument();
    expect(within(payer).getByText("Agora").nextSibling).toHaveTextContent("80,01%");
    const gaps = within(list).getByRole("article", { name: "Você recebeu provento em 10 dos últimos 12 meses" });
    expect(within(gaps).getByText("Meses sem provento: fev/2026, abr/2026")).toBeInTheDocument();
  });

  it("mascara o valor também no resumo e na rosca por setor da Carteira", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    renderApp("carteira");
    const summary = await screen.findByRole("region", { name: "Saúde da carteira" });
    expectNoMoney(summary);
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    const breakdown = within(fiis).getByRole("region", { name: "Por tipo de fundo em Fundos Imobiliários" });
    expectNoMoney(breakdown);
    // A fatia é percentual: continua à vista.
    expect(within(breakdown).getByText("70,3%")).toBeInTheDocument();
    expectNoMoney(document.body);
  });

  it("com o olho aberto o valor aparece", async () => {
    renderApp("ajustes/saude");
    const list = await screen.findByRole("list", { name: "Avisos" });
    expect(within(list).getAllByText(/aporte cerca de R\$\s/).length).toBeGreaterThan(0);
  });
});
