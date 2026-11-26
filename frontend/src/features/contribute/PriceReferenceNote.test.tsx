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

describe("Aportar: o aviso do ativo sem teto", () => {
  it("aparece só nos cartões que têm referência, com o lado dito em texto", async () => {
    const { result } = await calculate(planWithReferences());
    const notes = within(result).getAllByRole("note");
    expect(notes).toHaveLength(3);
    const text = notes.map((note) => (note.textContent ?? "").replace(/\s+/g, " "));
    expect(text[0]).toBe(
      "Sem teto de preço. Referências públicas: Bazin R$ 9,50 e Graham R$ 14,80; hoje R$ 11,20 (acima do Bazin, abaixo do Graham). A decisão de aportar é sua.",
    );
    expect(text[1]).toContain("hoje R$ 94,55 (abaixo do Bazin e do Graham)");
    expect(text[2]).toBe(
      "Sem teto de preço. Referência pública: Graham R$ 40,10; hoje R$ 53,81 (acima do Graham). A decisão de aportar é sua.",
    );
    // O aviso mora no cartão do ativo dele.
    const btc = within(result).getByText("BTC").closest("li") as HTMLElement;
    expect(within(btc).queryByRole("note")).not.toBeInTheDocument();
    const itsa = within(result).getByText("ITSA4").closest("li") as HTMLElement;
    expect(within(itsa).getByRole("note")).toBe(notes[0]);
    // O plano segue o mesmo: o aviso não vira bloqueio nem esconde o botão.
    expect(within(result).getByRole("button", { name: /Já comprei/ })).toBeEnabled();
  });

  it("sem referência em nenhum cartão, o plano não mostra aviso", async () => {
    const { result } = await calculate(samplePlan(6000));
    expect(within(result).queryByRole("note")).not.toBeInTheDocument();
    expect(within(result).queryByText(/Sem teto de preço/)).not.toBeInTheDocument();
  });

  it("com o olho fechado, nenhum valor do aviso aparece, nem para o leitor de tela", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { result } = await calculate(planWithReferences());
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
    // O lado do preço não é valor em reais e continua dito.
    expect(notes[0]).toHaveTextContent("(acima do Bazin, abaixo do Graham)");
  });

  it("fechar o olho na própria tela mascara o aviso e abrir devolve os números", async () => {
    const { user, result } = await calculate(planWithReferences());
    expect(within(result).getAllByRole("note")[0]?.textContent).toMatch(MONEY);
    const eye = screen.getByRole("button", { name: "Ocultar valores" });
    await user.click(eye);
    expect(within(result).getAllByRole("note")[0]?.textContent).not.toMatch(MONEY);
    await user.click(eye);
    expect(within(result).getAllByRole("note")[0]?.textContent).toMatch(MONEY);
  });
});
