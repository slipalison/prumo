import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { Plan } from "../../api/types";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { samplePlan } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

/** Quatro cartões: dois com as duas referências, um só com o Graham, um sem teto e sem referência. */
function planWithReferences(): Plan {
  const plan = samplePlan(6000);
  // ITSA4 a R$ 11,20: acima do Bazin, abaixo do Graham. BTLG11 a R$ 94,55: abaixo dos dois.
  // TRXF11 a R$ 53,81: só o Graham, acima. BTC (cripto, ou ativo com teto): sem aviso.
  const references = [
    { bazin: 9.5, graham: 14.8 },
    { bazin: 120, graham: 130.5 },
    { bazin: null, graham: 40.1 },
    null,
  ];
  plan.items = plan.items.map((item, index) => ({ ...item, price_references: references[index] ?? null }));
  return plan;
}

async function calculate(plan: Plan) {
  server.use(http.post("/api/v1/plans", () => HttpResponse.json(plan)));
  const rendered = renderApp("aportar");
  const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
  await rendered.user.type(input, "600000");
  await rendered.user.click(screen.getByRole("button", { name: /Calcular/ }));
  const result = await screen.findByRole("region", { name: "Resultado do aporte" });
  return { ...rendered, result };
}

describe("Aportar: as referências do ativo sem teto", () => {
  it("aparecem só nos cartões que têm referência, com o veredito de cada método", async () => {
    const { result } = await calculate(planWithReferences());
    const notes = within(result).getAllByRole("note");
    expect(notes).toHaveLength(3);
    const chips = (note: HTMLElement) =>
      within(note)
        .getAllByRole("button")
        .map((b) => (b.getAttribute("aria-label") ?? "").replace(/\s+/g, " "));
    // ITSA4 a R$ 11,20: acima do Bazin (9,50), abaixo do Graham (14,80).
    expect(chips(notes[0] as HTMLElement)).toEqual([
      "O que são estas referências",
      "Bazin R$ 9,50: não aprova",
      "Graham R$ 14,80: aprova",
    ]);
    // BTLG11 a R$ 94,55: abaixo dos dois.
    expect(chips(notes[1] as HTMLElement)).toEqual([
      "O que são estas referências",
      "Bazin R$ 120,00: aprova",
      "Graham R$ 130,50: aprova",
    ]);
    // TRXF11: só o Graham, acima.
    expect(chips(notes[2] as HTMLElement)).toEqual(["O que são estas referências", "Graham R$ 40,10: não aprova"]);
    // Pouco texto à vista: o chip mostra o método e o preço; a explicação fica no toque.
    expect(notes[0]).toHaveTextContent(/^Sem teto\s*Bazin\s*R\$\s9,50\s*Graham\s*R\$\s14,80$/);
    // O aviso mora no cartão do ativo dele.
    const btc = within(result).getByText("BTC").closest("li") as HTMLElement;
    expect(within(btc).queryByRole("note")).not.toBeInTheDocument();
    const itsa = within(result).getByText("ITSA4").closest("li") as HTMLElement;
    expect(within(itsa).getByRole("note")).toBe(notes[0]);
    // O plano segue o mesmo: o aviso não vira bloqueio nem esconde o botão.
    expect(within(result).getByRole("button", { name: "Comprei ITSA4" })).toBeEnabled();
  });

  it("o toque no chip abre a explicação, e Esc ou tocar fora fecha", async () => {
    const { user, result } = await calculate(planWithReferences());
    const note = within(result).getAllByRole("note")[0] as HTMLElement;
    const bazin = within(note).getByRole("button", { name: /^Bazin/ });
    expect(bazin).toHaveAttribute("aria-expanded", "false");
    await user.click(bazin);
    expect(bazin).toHaveAttribute("aria-expanded", "true");
    const status = within(note).getAllByRole("status").find((s) => s.textContent !== "") as HTMLElement;
    expect(status.textContent?.replace(/\s+/g, " ")).toBe(
      "Bazin: o preço em que a média de dividendos por cota renderia o seu rendimento-alvo. Hoje R$ 11,20, acima do preço do método (R$ 9,50): não aprova a compra a esse preço.",
    );
    await user.keyboard("{Escape}");
    expect(bazin).toHaveAttribute("aria-expanded", "false");
    expect(within(note).getAllByRole("status").every((s) => s.textContent === "")).toBe(true);

    const info = within(note).getByRole("button", { name: "O que são estas referências" });
    await user.click(info);
    expect(within(note).getByText(/a decisão de aportar é sua/)).toBeInTheDocument();
    await user.click(within(result).getByText("BTC"));
    expect(within(note).queryByText(/a decisão de aportar é sua/)).not.toBeInTheDocument();
  });

  it("sem referência em nenhum cartão, o plano não mostra aviso", async () => {
    const { result } = await calculate(samplePlan(6000));
    expect(within(result).queryByRole("note")).not.toBeInTheDocument();
    expect(within(result).queryByText("Sem teto")).not.toBeInTheDocument();
  });

  it("com o olho fechado, nenhum valor aparece, nem no nome do chip nem na explicação", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { user, result } = await calculate(planWithReferences());
    const notes = within(result).getAllByRole("note");
    expect(notes).toHaveLength(3);
    for (const note of notes) {
      expect(note.textContent).not.toMatch(MONEY);
      expect(note.textContent).toContain(MASK_BRL);
      for (const element of [note, ...note.querySelectorAll("*")]) {
        for (const attribute of ["aria-label", "title"]) {
          expect(element.getAttribute(attribute) ?? "").not.toMatch(MONEY);
        }
      }
    }
    // O veredito não é valor em reais e continua dito.
    const bazin = within(notes[0] as HTMLElement).getByRole("button", { name: /^Bazin/ });
    expect(bazin.getAttribute("aria-label")).toMatch(/não aprova$/);
    await user.click(bazin);
    expect(notes[0]?.textContent).not.toMatch(MONEY);
    expect(notes[0]).toHaveTextContent("acima do preço do método");
  });

  it("fechar o olho na própria tela mascara os chips e abrir devolve os números", async () => {
    const { user, result } = await calculate(planWithReferences());
    expect(within(result).getAllByRole("note")[0]?.textContent).toMatch(MONEY);
    const eye = screen.getByRole("button", { name: "Ocultar valores" });
    await user.click(eye);
    expect(within(result).getAllByRole("note")[0]?.textContent).not.toMatch(MONEY);
    await user.click(eye);
    expect(within(result).getAllByRole("note")[0]?.textContent).toMatch(MONEY);
  });
});
