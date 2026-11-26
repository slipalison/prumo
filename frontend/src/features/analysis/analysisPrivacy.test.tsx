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

describe("Análise com o olho fechado", () => {
  it("mascara todo valor em reais da tela e deixa percentuais, contagens e limites", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    renderApp("analise");
    await screen.findByRole("region", { name: "Resumo" });
    await screen.findByRole("region", { name: "Referências de preço" });
    expectNoMoney(document.body);

    // O aporte sugerido nas originais e nas novas.
    const ipca = screen.getByRole("article", { name: "Tesouro IPCA+ 2035 é 70,76% da carteira" });
    expect(within(ipca).getByText(/aporte cerca de R\$ •••• em outros ativos/)).toBeInTheDocument();
    expect(within(ipca).getByText("Agora").nextSibling).toHaveTextContent("70,76%");
    expect(within(ipca).getByText("Limite").nextSibling).toHaveTextContent("15%");
    expect(within(ipca).getByRole("meter")).toHaveAttribute("aria-valuenow", "70.76");
    expect(screen.getByText(/aporte cerca de R\$ •••• em ativos de fora/)).toBeInTheDocument();

    // A renda: o valor que o pagador pagou, o yield e o crescimento somem; os percentuais ficam.
    expect(screen.getByText(/KNRI11 pagou R\$ •••• dos R\$ •••• que você recebeu em 12 meses/)).toBeInTheDocument();
    expect(screen.getByText("Você recebeu R$ •••• em 12 meses sobre R$ •••• em ações e FIIs.")).toBeInTheDocument();
    expect(screen.getByText("Renda de 12 meses: R$ ••••; dos 12 meses anteriores: R$ ••••.")).toBeInTheDocument();

    // As referências de preço: Bazin, teto e preço de hoje são valores em reais.
    const references = within(screen.getByRole("region", { name: "Referências de preço" }));
    const itsa = references.getAllByRole("listitem").find((li) => /ITSA4/.test(li.textContent ?? "")) as HTMLElement;
    expect(within(itsa).getByText("Bazin").nextSibling).toHaveTextContent("R$ ••••");
    expect(within(itsa).getByText("Seu teto").nextSibling).toHaveTextContent("R$ ••••");
    expect(within(itsa).getByText("Preço de hoje").nextSibling).toHaveTextContent("R$ ••••");
    expect(within(itsa).getByText(/Média de 4 anos de dividendos: R\$ •••• por cota ao ano/)).toBeInTheDocument();
    expect(references.getByText(/hoje 6% ao ano/)).toBeInTheDocument();

    // O perfil e os destaques são percentuais: seguem à vista.
    const profile = within(screen.getByRole("region", { name: "Perfil de risco" }));
    expect(profile.getByText("Renda fixa 70,8%")).toBeInTheDocument();
    const highlights = within(screen.getByRole("region", { name: "Destaques de 12 meses" }));
    expect(highlights.getAllByText("+18,40%").length).toBeGreaterThan(0);
  });

  it("a confirmação do teto também não mostra o valor", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { user } = renderApp("analise");
    const references = within(await screen.findByRole("region", { name: "Referências de preço" }));
    await user.click(references.getByRole("button", { name: "Usar a referência de Bazin como teto de ITSA4" }));
    const dialog = await screen.findByRole("dialog", { name: "Usar como teto de preço?" });
    expect(within(dialog).getByText(/Definir R\$ •••• como teto de preço de ITSA4\?/)).toBeInTheDocument();
    expectNoMoney(document.body);
  });

  it("mascara o valor no resumo da Carteira", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    renderApp("carteira");
    const summary = await screen.findByRole("region", { name: "Análise da carteira" });
    expectNoMoney(summary);
    expectNoMoney(document.body);
  });

  it("com o olho aberto o valor aparece", async () => {
    renderApp("analise");
    await screen.findByRole("region", { name: "Resumo" });
    expect(screen.getAllByText(/aporte cerca de R\$\s/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Você recebeu R\$\s6\smil em 12 meses sobre R\$\s84,3\smil em ações e FIIs/)).toBeInTheDocument();
  });
});
