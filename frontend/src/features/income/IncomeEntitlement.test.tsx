import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { MASK_BRL, MASK_QUANTITY } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { sampleEvent } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

async function openSheet() {
  const rendered = renderApp("renda");
  await screen.findByRole("heading", { name: "Quanto sua carteira paga?" });
  await rendered.user.click(await screen.findByRole("button", { name: /Lançar provento/ }));
  const dialog = await screen.findByRole("dialog", { name: "Lançar provento" });
  return { ...rendered, dialog };
}

/** As consultas de quantidade na data com, para conferir o que a tela perguntou. */
function entitlement(quantity: number, asked: URLSearchParams[] = []) {
  return http.get("/api/v1/income/entitlement", ({ request }) => {
    const params = new URL(request.url).searchParams;
    asked.push(params);
    return HttpResponse.json({ quantity, record_date: params.get("ex_date") ? "2026-11-09" : "2026-11-25" });
  });
}

describe("Lançar provento: a quantidade vem do livro na data com", () => {
  it("com a quantidade em branco, mostra a do livro e deixa o servidor decidir", async () => {
    let body: unknown;
    server.use(
      entitlement(40),
      http.post("/api/v1/income", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(sampleEvent(), { status: 201 });
      }),
    );
    const { user, dialog } = await openSheet();
    const quantity = within(dialog).getByLabelText("Quantidade");
    await waitFor(() => expect(quantity).toHaveAttribute("placeholder", "40"));
    expect(dialog).toHaveTextContent("Em branco vale o que o livro tinha na data com (25/11/2026).");
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "0,50");
    // A prévia usa a quantidade do livro: 40 × 0,50.
    expect(within(dialog).getByText(/^Bruto/).parentElement).toHaveTextContent(/R\$\s20,00/);
    await user.click(within(dialog).getByRole("button", { name: "Lançar provento" }));
    await waitFor(() => expect(body).toEqual({ asset_id: "a-itsa", kind: "dividend", pay_date: "2026-11-25", amount_per_unit: 0.5 }));
  });

  it("a data ex muda a data com que a tela consulta", async () => {
    const asked: URLSearchParams[] = [];
    server.use(entitlement(30, asked));
    const { user, dialog } = await openSheet();
    await waitFor(() => expect(asked.length).toBeGreaterThan(0));
    expect(asked[0]?.get("asset")).toBe("a-itsa");
    expect(asked[0]?.get("ex_date")).toBeNull();
    await user.type(within(dialog).getByLabelText("Data ex (opcional)"), "2026-11-10");
    await waitFor(() => expect(asked.at(-1)?.get("ex_date")).toBe("2026-11-10"));
    await waitFor(() => expect(dialog).toHaveTextContent("na data com (09/11/2026)"));
  });

  it("sem cotas na data com, avisa e só deixa lançar informando a quantidade", async () => {
    let body: unknown;
    server.use(
      entitlement(0),
      http.post("/api/v1/income", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(sampleEvent(), { status: 201 });
      }),
    );
    const { user, dialog } = await openSheet();
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "0,50");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Você não tinha cotas na data com");
    const submit = within(dialog).getByRole("button", { name: "Lançar provento" });
    expect(submit).toBeDisabled();

    await user.type(within(dialog).getByLabelText("Quantidade"), "12");
    expect(within(dialog).queryByText(/Você não tinha cotas/)).not.toBeInTheDocument();
    expect(submit).toBeEnabled();
    await user.click(submit);
    await waitFor(() => expect(body).toEqual({ asset_id: "a-itsa", kind: "dividend", pay_date: "2026-11-25", amount_per_unit: 0.5, quantity: 12 }));
  });

  it("se a consulta falha, ainda dá para lançar: o servidor usa o livro", async () => {
    let body: unknown;
    server.use(
      http.get("/api/v1/income/entitlement", () => HttpResponse.json({ code: "internal", status: 500 }, { status: 500 })),
      http.post("/api/v1/income", async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(sampleEvent(), { status: 201 });
      }),
    );
    const { user, dialog } = await openSheet();
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "0,50");
    // Sem a quantidade, a prévia espera.
    expect(within(dialog).getByText("Preencha o valor para ver o líquido.")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Lançar provento" }));
    await waitFor(() => expect(body).toEqual({ asset_id: "a-itsa", kind: "dividend", pay_date: "2026-11-25", amount_per_unit: 0.5 }));
  });

  it("com o olho fechado, a quantidade e a prévia saem mascaradas", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(entitlement(40));
    const { user, dialog } = await openSheet();
    const quantity = within(dialog).getByLabelText("Quantidade");
    await waitFor(() => expect(quantity).toHaveAttribute("placeholder", MASK_QUANTITY));
    await user.type(within(dialog).getByLabelText("Valor por cota (R$)"), "0,50");
    await waitFor(() => expect(within(dialog).getByText(/^Bruto/).parentElement).toHaveTextContent(MASK_BRL));
    for (const element of dialog.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "title", "placeholder"]) {
        expect(element.getAttribute(attribute) ?? "").not.toMatch(/R\$\s*\d|^40$/);
      }
    }
    expect(dialog.textContent ?? "").not.toMatch(/R\$\s*\d/);
  });
});
