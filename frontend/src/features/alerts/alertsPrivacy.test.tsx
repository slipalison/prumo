import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { alertsHandlers, linkedAlerts } from "../../test/alertsHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

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

describe("Alertas com o olho fechado", () => {
  it("o exemplo do resumo mascara cada valor em reais e deixa nomes e percentuais", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(...alertsHandlers(linkedAlerts()).handlers);
    renderApp("ajustes/alertas");
    const digest = await screen.findByRole("group", { name: "Exemplo do resumo" });
    for (const text of everythingHeard(document.body)) expect(text).not.toMatch(MONEY);
    expect(digest).toHaveTextContent("KNRI11 paga amanha: R$ •••• liquidos.");
    expect(digest).toHaveTextContent("28,4% da carteira (meta 20%)");
    expect(digest).toHaveTextContent("teto R$ ••••");
  });

  it("com o olho aberto o exemplo mostra os valores fictícios", async () => {
    server.use(...alertsHandlers(linkedAlerts()).handlers);
    renderApp("ajustes/alertas");
    const digest = await screen.findByRole("group", { name: "Exemplo do resumo" });
    expect(digest.textContent).toMatch(MONEY);
  });

  it("a tela sem vínculo não carrega valor algum", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(...alertsHandlers().handlers);
    renderApp("ajustes/alertas");
    await screen.findByRole("button", { name: "Vincular o Telegram" });
    for (const text of everythingHeard(document.body)) expect(text).not.toMatch(MONEY);
  });
});
