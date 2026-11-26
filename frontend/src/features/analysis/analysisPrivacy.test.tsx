import { screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
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

/** A linha de uma regra (abre o recolher da dimensão antes). */
function ruleRow(sentence: string): HTMLElement {
  for (const details of document.querySelectorAll("details")) details.open = true;
  return screen.getByRole("rowheader", { name: sentence }).closest("tr") as HTMLElement;
}

/** Abre um ⓘ pelo nome e confere que, aberto, ele também não mostra valor. */
async function openTip(user: UserEvent, name: string | RegExp) {
  await user.click(screen.getByRole("button", { name }));
  expectNoMoney(document.body);
}

describe("Análise com o olho fechado", () => {
  it("mascara todo valor em reais da tela e deixa percentuais, contagens e limites", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { user } = renderApp("analise");
    await screen.findByRole("region", { name: "Resumo" });
    await screen.findByRole("region", { name: "Referências de preço" });
    expectNoMoney(document.body);

    // O aporte sugerido nas originais e nas novas, no ⓘ do "o que fazer".
    const largest = ruleRow("Nenhum ativo acima de 15% da carteira");
    expect(within(largest).getByText("Tesouro IPCA+ 2035 é 70,76% da carteira")).toBeInTheDocument();
    expect(largest.querySelector('td[data-label="Agora"]')).toHaveTextContent("70,76%");
    expect(largest.querySelector('td[data-label="Limite"]')).toHaveTextContent("máx. 15%");
    expect(within(largest).getByRole("meter")).toHaveAttribute("aria-valuenow", "70.76");
    await openTip(user, "O que fazer: Nenhum ativo acima de 15% da carteira");
    expect(within(largest).getAllByText(/aporte cerca de R\$ •••• em outros ativos/)).toHaveLength(2);
    await openTip(user, "O que fazer: Pelo menos 10% da carteira no exterior");
    expect(screen.getByText(/aporte cerca de R\$ •••• em ativos de fora/)).toBeInTheDocument();

    // A renda: o valor que o pagador pagou, o yield e o crescimento somem; os percentuais ficam.
    await openTip(user, "O que fazer: Nenhum pagador acima de 25% da renda de 12 meses");
    expect(screen.getByText(/KNRI11 pagou R\$ •••• dos R\$ •••• que você recebeu em 12 meses/)).toBeInTheDocument();
    ruleRow("Dividend yield de 12 meses de pelo menos 5%");
    expect(screen.getByText("Você recebeu R$ •••• em 12 meses sobre R$ •••• em ações e FIIs.")).toBeInTheDocument();
    expect(screen.getByText("Renda de 12 meses: R$ ••••; dos 12 meses anteriores: R$ ••••.")).toBeInTheDocument();
    expectNoMoney(document.body);

    // As referências de preço: Bazin, teto e preço de hoje são valores em reais.
    const references = within(screen.getByRole("region", { name: "Referências de preço" }));
    const itsa = references.getByRole("rowheader", { name: "ITSA4" }).closest("tr") as HTMLElement;
    const cell = (column: string) => itsa.querySelector(`td[data-label="${column}"]`);
    expect(cell("Bazin")).toHaveTextContent("R$ ••••");
    expect(cell("Graham")).toHaveTextContent("R$ ••••");
    expect(cell("Pelas duas")).toHaveTextContent("R$ ••••");
    expect(cell("Seu teto")).toHaveTextContent("R$ ••••");
    expect(cell("Hoje")).toHaveTextContent("R$ ••••");
    expect(within(itsa).getByRole("button", { name: "Usar a referência como teto de ITSA4" })).toHaveTextContent("R$ ••••");
    await openTip(user, "Como sai o Bazin de ITSA4");
    expect(within(itsa).getByText(/Média de 4 anos de dividendos: R\$ •••• por cota ao ano/)).toBeInTheDocument();
    await openTip(user, "Como as referências são calculadas");
    expect(references.getByText(/hoje 6% ao ano/)).toBeInTheDocument();

    // Qualidade e pontos de atenção são percentuais, contagens e nomes: seguem à vista.
    const attention = within(screen.getByRole("region", { name: "Pontos de atenção dos ativos" }));
    expect(attention.getByText("ROE de 4,12%, abaixo de 10%")).toBeInTheDocument();
    expect(attention.getByText("Provento de 12 meses caiu 22,50% em relação aos 12 meses anteriores")).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Qualidade dos ativos" })).getByText("Entraram na conta 4 de 4 ativos. Abaixo do critério: KNRI11, HGLG11."),
    ).toBeInTheDocument();

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
    await user.click(references.getByRole("button", { name: "Usar a referência como teto de ITSA4" }));
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
    const { user } = renderApp("analise");
    await screen.findByRole("region", { name: "Resumo" });
    await user.click(screen.getByRole("button", { name: "O que fazer: Pelo menos 10% da carteira no exterior" }));
    expect(screen.getAllByText(/aporte cerca de R\$\s/).length).toBeGreaterThan(0);
    ruleRow("Dividend yield de 12 meses de pelo menos 5%");
    expect(screen.getByText(/Você recebeu R\$\s6\smil em 12 meses sobre R\$\s84,3\smil em ações e FIIs/)).toBeInTheDocument();
  });
});
