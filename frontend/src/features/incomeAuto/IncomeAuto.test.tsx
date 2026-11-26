import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { MASK_BRL, MASK_QUANTITY } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { incomeAutoHandlers, sampleBonus, sampleIncomeAuto } from "../../test/incomeAutoHandlers";
import { sampleEvent, sampleSummary } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { pendingSentence, stockEventText } from "./text";

/** Qualquer R$ seguido de dígito é valor vazando. */
const MONEY = /R\$\s*\d/;

/** Texto da página mais os atributos que o leitor de tela lê no lugar dele. */
function everythingHeard(root: HTMLElement = document.body): string[] {
  const heard: string[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) heard.push(node.textContent ?? "");
  for (const element of root.querySelectorAll("*")) {
    for (const attribute of ["aria-label", "aria-valuetext", "title", "alt", "placeholder"]) {
      const value = element.getAttribute(attribute);
      if (value) heard.push(value);
    }
  }
  return heard;
}

describe("o texto das pendências", () => {
  it("lê o fator como a B3 publica", () => {
    expect(stockEventText({ kind: "bonus", factor: 2 })).toBe("bonificação de 2%");
    expect(stockEventText({ kind: "bonus", factor: 2.5 })).toBe("bonificação de 2,5%");
    expect(stockEventText({ kind: "split", factor: 100 })).toBe("desdobramento de 100% (cada ação vira 2)");
    expect(stockEventText({ kind: "split", factor: 900 })).toBe("desdobramento de 900% (cada ação vira 10)");
    expect(stockEventText({ kind: "split", factor: 50 })).toBe("desdobramento de 50%");
    expect(stockEventText({ kind: "reverse_split", factor: 0.1 })).toBe("grupamento de 10 para 1");
    expect(stockEventText({ kind: "reverse_split", factor: 0.3 })).toBe("grupamento (fator 0,3)");
    expect(stockEventText({ kind: "reverse_split", factor: 0 })).toBe("grupamento (fator 0)");
    expect(pendingSentence(sampleBonus())).toBe(
      "A B3 anunciou bonificação de 2% em ITSA4 com data com 18/12/2025; lance no livro.",
    );
  });
});

describe("Ajustes → Proventos automáticos", () => {
  it("mostra a chave ligada, o que a B3 cobre e as pendências", async () => {
    server.use(
      ...incomeAutoHandlers(
        sampleIncomeAuto({ unmatched: ["XPTO11"], pending_events: [sampleBonus()] }),
      ).handlers,
    );
    renderApp("ajustes/proventos-automaticos");
    const toggle = await screen.findByRole("switch", { name: "Proventos automáticos" });
    expect(toggle).toBeChecked();
    expect(screen.getByText(/Anúncios da B3 conferidos em .* A busca é diária\./)).toBeInTheDocument();
    expect(screen.getByText("Acompanhando: ITSA4, KNRI11.")).toBeInTheDocument();
    expect(screen.getByText(/XPTO11\. Esses continuam só com o que você lança/)).toBeInTheDocument();
    expect(
      screen.getByText("A B3 anunciou bonificação de 2% em ITSA4 com data com 18/12/2025; lance no livro."),
    ).toBeInTheDocument();
    expect(screen.getByText(/Você tinha 140 cotas na data com/)).toBeInTheDocument();
    // A seção está na lista de Ajustes.
    expect(screen.getAllByText("Proventos automáticos").length).toBeGreaterThan(0);
  });

  it("desliga e liga pelo servidor, com aviso", async () => {
    const auto = incomeAutoHandlers(sampleIncomeAuto({ pending_events: [sampleBonus()] }));
    server.use(...auto.handlers);
    const { user } = renderApp("ajustes/proventos-automaticos");
    const toggle = await screen.findByRole("switch", { name: "Proventos automáticos" });
    await user.click(toggle);
    await waitFor(() => expect(auto.saved).toEqual([false]));
    expect(
      await screen.findByText("Proventos automáticos desligados: os automáticos saíram da Renda."),
    ).toBeInTheDocument();
    await waitFor(() => expect(toggle).not.toBeChecked());
    // Desligado: sem cobertura e sem pendências.
    expect(screen.queryByText(/lance no livro/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Acompanhando/)).not.toBeInTheDocument();
    await user.click(toggle);
    await waitFor(() => expect(auto.saved).toEqual([false, true]));
    expect(await screen.findByText("Proventos automáticos ligados.")).toBeInTheDocument();
  });

  it("sem busca ainda, sem pendência e com erro ao salvar", async () => {
    server.use(
      http.get("/api/v1/income/auto", () =>
        HttpResponse.json(sampleIncomeAuto({ fetched_at: null, covered: [] })),
      ),
      http.put("/api/v1/income/auto", () =>
        HttpResponse.json(
          { type: "urn:prumo:error:internal", title: "Erro interno", status: 500, code: "internal", detail: "Banco fora do ar." },
          { status: 500, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = renderApp("ajustes/proventos-automaticos");
    expect(await screen.findByText("A primeira busca na B3 acontece em até uma hora.")).toBeInTheDocument();
    expect(screen.getByText(/Nenhuma bonificação, desdobramento ou grupamento pendente/)).toBeInTheDocument();
    expect(screen.queryByText(/Acompanhando/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("switch", { name: "Proventos automáticos" }));
    expect(await screen.findByText("Algo deu errado do nosso lado. Tente de novo em instantes.")).toBeInTheDocument();
  });

  it("quando o servidor falha, oferece tentar de novo", async () => {
    let calls = 0;
    server.use(
      http.get("/api/v1/income/auto", () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json(
              { type: "urn:prumo:error:internal", title: "Erro interno", status: 500, code: "internal" },
              { status: 500, headers: { "Content-Type": "application/problem+json" } },
            )
          : HttpResponse.json(sampleIncomeAuto());
      }),
    );
    const { user } = renderApp("ajustes/proventos-automaticos");
    await user.click(await screen.findByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("switch", { name: "Proventos automáticos" })).toBeChecked();
  });
});

function automaticIncome() {
  server.use(
    http.get("/api/v1/income/summary", () =>
      HttpResponse.json({
        ...sampleSummary(),
        upcoming: [
          sampleEvent({
            id: "e-auto",
            symbol: "ITSA4",
            kind: "jcp",
            pay_date: "2026-12-01",
            status: "upcoming",
            source: "b3_announcement",
            external_ref: "b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425",
            gross: 24.24,
            withholding: 4.24,
            net: 20,
          }),
        ],
      }),
    ),
    http.get("/api/v1/income", () =>
      HttpResponse.json([
        sampleEvent(),
        sampleEvent({
          id: "e-auto",
          symbol: "ITSA4",
          kind: "jcp",
          source: "b3_announcement",
          external_ref: "b3a:BRITSAACNPR7:JRS-CAP-PROPRIO:2026-08-31:0.0242425",
          gross: 24.24,
          withholding: 4.24,
          net: 20,
        }),
      ]),
    ),
  );
}

describe("Renda com os automáticos", () => {
  it("o automático tem selo e as pendências aparecem num cartão", async () => {
    automaticIncome();
    server.use(...incomeAutoHandlers(sampleIncomeAuto({ pending_events: [sampleBonus()] })).handlers);
    const { user } = renderApp("renda");
    const card = await screen.findByRole("region", { name: "Eventos anunciados pela B3" });
    expect(card).toHaveTextContent("A B3 anunciou bonificação de 2% em ITSA4 com data com 18/12/2025; lance no livro.");
    const ledger = screen.getByRole("region", { name: "Proventos lançados" });
    await within(ledger).findAllByText("14/11/2026");
    expect(within(ledger).getAllByText("automático (B3)")).toHaveLength(1);
    const upcoming = screen.getByRole("region", { name: "Próximos pagamentos" });
    expect(within(upcoming).getByText("automático (B3)")).toBeInTheDocument();
    // Apagar o automático avisa que ele não volta.
    await user.click(within(ledger).getByRole("button", { name: /Apagar provento de ITSA4/ }));
    expect(
      within(ledger).getByRole("group", {
        name: "Apagar o provento automático de ITSA4? Ele não volta na próxima busca.",
      }),
    ).toBeInTheDocument();
  });

  it("desligado ou sem pendência, o cartão não aparece", async () => {
    server.use(
      ...incomeAutoHandlers(sampleIncomeAuto({ enabled: false, pending_events: [sampleBonus()] })).handlers,
    );
    renderApp("renda");
    await screen.findByRole("heading", { name: "Próximos pagamentos" });
    await waitFor(() => expect(screen.queryByText(/lance no livro/)).not.toBeInTheDocument());
    expect(screen.queryByRole("region", { name: "Eventos anunciados pela B3" })).not.toBeInTheDocument();
  });

  it("com o olho fechado, nem valor nem quantidade aparecem", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    automaticIncome();
    server.use(
      ...incomeAutoHandlers(sampleIncomeAuto({ pending_events: [sampleBonus({ quantity: 1 })] })).handlers,
    );
    renderApp("renda");
    const card = await screen.findByRole("region", { name: "Eventos anunciados pela B3" });
    expect(card).toHaveTextContent(`Você tinha ${MASK_QUANTITY} cotas na data com.`);
    const ledger = screen.getByRole("region", { name: "Proventos lançados" });
    await within(ledger).findAllByText("automático (B3)");
    expect(within(ledger).getAllByText(MASK_BRL).length).toBeGreaterThan(0);
    for (const text of everythingHeard()) expect(text).not.toMatch(MONEY);
    expect(card.textContent).not.toMatch(/tinha \d/);
  });

  it("a tela de ajustes com o olho fechado mascara as cotas", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(...incomeAutoHandlers(sampleIncomeAuto({ pending_events: [sampleBonus()] })).handlers);
    renderApp("ajustes/proventos-automaticos");
    expect(await screen.findByText(`Você tinha ${MASK_QUANTITY} cotas na data com.`, { exact: false })).toBeInTheDocument();
    for (const text of everythingHeard()) {
      expect(text).not.toMatch(MONEY);
      expect(text).not.toMatch(/140/);
    }
  });
});
