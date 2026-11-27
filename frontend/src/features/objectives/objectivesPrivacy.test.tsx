import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import {
  achievedObjective,
  fakeObjectives,
  noPremises,
  sampleAssumptions,
  sampleImpact,
  sampleIncomeObjective,
  sampleObjective,
} from "../../test/objectivesHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

/** Os atalhos de valor do campo do aporte são constantes do app, não dado da pessoa. */
const CONSTANTS = '[aria-label="Valores rápidos"]';

/** Texto da página mais os atributos que o leitor de tela lê no lugar dele. */
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

describe("Objetivos com o olho fechado", () => {
  it("a tela de Objetivos não mostra nenhum valor em reais, nem para o leitor de tela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(
      ...fakeObjectives([
        sampleObjective({ premises: { ...noPremises, monthly_contribution: 3000 } }),
        sampleIncomeObjective(),
        achievedObjective(),
      ]).handlers,
    );
    const { user } = renderApp("ajustes/objetivos");
    const active = await screen.findByRole("article", { name: "Patrimônio total" });

    expectNoMoney();
    // O que não é dinheiro continua: percentual, data estimada, nome.
    const bar = within(active).getByRole("progressbar");
    expect(bar.getAttribute("aria-valuetext")).toContain(MASK_BRL);
    expect(bar.getAttribute("aria-valuetext")).toMatch(/25,0%/);
    expect(within(active).getByText(/a meta chega em mar\/2031/)).toBeInTheDocument();
    const premises = within(active).getByLabelText("Premissas da conta: Patrimônio total");
    expect(premises).toHaveTextContent(`${MASK_BRL} por mês`);
    expect(premises).toHaveTextContent("5,10% ao ano");
    expect(screen.getByText("Concluído em 15/10/2026.", { exact: false })).toBeInTheDocument();
    // A curva: o resumo, a tabela e o balão do ponteiro também saem mascarados.
    expect(within(active).getByRole("img").getAttribute("aria-label")).toContain(MASK_BRL);
    const plot = within(active).getByTestId("projection-plot");
    vi.spyOn(plot, "getBoundingClientRect").mockReturnValue({ left: 0, width: 200, top: 0, height: 150, right: 200, bottom: 150, x: 0, y: 0, toJSON: () => ({}) });
    fireEvent.pointerMove(plot, { clientX: 100 });
    expectNoMoney();
    // A origem do rendimento cita a renda e o valor de hoje: mascarados também.
    await user.click(within(active).getByRole("button", { name: "De onde vem cada premissa: Patrimônio total" }));
    expect(await screen.findByText(/o da carteira de hoje:/)).toHaveTextContent(`${MASK_BRL} por mês de renda estimada sobre ${MASK_BRL}`);
    expectNoMoney();
  });

  it("a folha de edição não vaza o valor da meta nem do aporte", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(
      ...fakeObjectives([
        sampleObjective({
          premises: { ...noPremises, monthly_contribution: 3000 },
          assumptions: { ...sampleAssumptions, monthly_contribution: 3000, contribution_source: "informed" },
        }),
      ]).handlers,
    );
    const { user } = renderApp("ajustes/objetivos");
    await user.click(await screen.findByRole("button", { name: "Editar objetivo: Patrimônio total" }));
    const dialog = await screen.findByRole("dialog", { name: "Editar objetivo" });
    expectNoMoney(dialog);
    expect(within(dialog).getByRole("textbox", { name: "Meta" })).toHaveValue(MASK_BRL);
    expect(within(dialog).getByRole("textbox", { name: "Aporte por mês" })).toHaveValue(MASK_BRL);
  });

  it("o card do Aportar mascara a meta e o aporte, e deixa o percentual e o mês", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(...fakeObjectives([sampleObjective()], [sampleImpact()]).handlers);
    const { user } = renderApp("aportar");
    const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const card = await screen.findByRole("region", { name: "Seus objetivos" });

    expectNoMoney(card);
    expectNoMoney();
    expect(within(card).getByText(/Com este aporte, você chega a/)).toHaveTextContent(
      `chega a 25,6% da meta de ${MASK_BRL} (hoje, 25,0%)`,
    );
    expect(within(card).getByText(/Com aportes assim, a meta chega em/)).toHaveTextContent("fev/2031");

    // As premissas moram no ⓘ do objetivo: aberto, o balão também sai mascarado.
    await user.click(within(card).getByRole("button", { name: "Premissas da conta: Patrimônio total" }));
    expect(within(card).getByText(/Aporte de/)).toHaveTextContent(`Aporte de ${MASK_BRL} por mês`);
    expectNoMoney(card);
    expectNoMoney();
  });
});
