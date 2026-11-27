import { act, screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { IncomeSummary } from "../../api/incomeTypes";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { estimateAsset, estimateOf, knownYield, sampleEvent, sampleSummary } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

/*
 * Explorar a Renda: "Quem pagou" (entrar em "Outros", filtrar por pagador, a
 * rosca ao ponteiro, ao foco e ao teclado) e a estimativa com a lista inteira.
 */

/** Nove pagadores: seis à vista e "Outros (3)". `PAG0` paga mais, `PAG8` menos. */
function exploreSummary(): IncomeSummary {
  const summary = sampleSummary();
  summary.by_payer = Array.from({ length: 9 }, (_, i) => ({
    asset_id: `a-${i}`,
    label: `PAG${i}`,
    total: 900 - i * 100,
    percent: 20 - i,
    yield_on_cost: knownYield(900 - i * 100, 10000),
  }));
  summary.upcoming = [
    sampleEvent({ id: "u-0", asset_id: "a-0", symbol: "PAG0", pay_date: "2026-11-30", net: 11, status: "upcoming" }),
    sampleEvent({ id: "u-1", asset_id: "a-1", symbol: "PAG1", pay_date: "2026-12-01", net: 22, status: "upcoming" }),
    sampleEvent({ id: "u-7", asset_id: "a-7", symbol: "PAG7", pay_date: "2026-12-02", net: 33, status: "upcoming" }),
  ];
  summary.estimate = estimateOf(
    Array.from({ length: 8 }, (_, i) =>
      estimateAsset({
        asset_id: `e-${i}`,
        symbol: `EST${i}`,
        display_name: `Estimado ${i}`,
        quantity: 10,
        per_unit_12m: 1,
        // Fora de ordem de propósito: a tela ordena pelo que mais rende.
        monthly: [5, 40, 12, 30, 8, 20, 15, 10][i] ?? 0,
      }),
    ),
  );
  return summary;
}

async function openIncome(hidden = false) {
  if (hidden) window.localStorage.setItem(HIDE_VALUES_KEY, "1");
  server.use(
    http.get("/api/v1/income/summary", () => HttpResponse.json(exploreSummary())),
    http.get("/api/v1/income", () => HttpResponse.json([])),
  );
  const rendered = renderApp("renda");
  const panel = await screen.findByRole("region", { name: "Quem pagou" });
  return { ...rendered, panel };
}

const arcOf = (key: string) => document.querySelector(`[data-segment="${key}"]`) as SVGElement;
/** O botão do pagador na tabela (a rosca tem os arcos, com nomes parecidos). */
const pickOf = (panel: HTMLElement, name: string) =>
  within(panel.querySelector(".payers__table") as HTMLElement).getByRole("button", { name: new RegExp("^" + name) });
const rowOf = (panel: HTMLElement, name: string) => pickOf(panel, name).closest("tr") as HTMLElement;
const namesIn = (panel: HTMLElement) => Array.from(panel.querySelectorAll(".payers__name"), (el) => el.textContent);

describe("Quem pagou: entrar em Outros", () => {
  it("pela legenda mostra quem está dentro, com valor, parte e yield on cost, e o Voltar leva de volta", async () => {
    const { user, panel } = await openIncome();
    expect(namesIn(panel)).toEqual(["PAG0", "PAG1", "PAG2", "PAG3", "PAG4", "PAG5", "Outros (3)"]);

    await user.click(pickOf(panel, "Outros"));
    expect(namesIn(panel)).toEqual(["PAG6", "PAG7", "PAG8"]);
    const row = rowOf(panel, "PAG7");
    expect(row).toHaveTextContent("R$ 200,00");
    expect(row).toHaveTextContent("13,0%");
    expect(row).toHaveTextContent(/yield on cost 2,0%/);
    // A rosca passa a mostrar só o grupo, e o foco vai para o Voltar.
    expect(within(panel).getByRole("group", { name: /por pagador dentro de Outros \(3\): PAG6 14,0%, PAG7 13,0%, PAG8 12,0%/ })).toBeInTheDocument();
    const back = within(panel).getByRole("button", { name: "Voltar a todos os pagadores" });
    expect(back).toHaveFocus();

    await user.click(back);
    expect(namesIn(panel)).toHaveLength(7);
    expect(within(panel).queryByRole("button", { name: /Voltar/ })).not.toBeInTheDocument();
    // O foco volta para quem abriu o grupo.
    expect(pickOf(panel, "Outros")).toHaveFocus();
  });

  it("pela fatia, com o teclado: setas andam entre as fatias e Enter entra no grupo", async () => {
    const { user, panel } = await openIncome();
    const first = arcOf("a-0");
    expect(first).toHaveAttribute("role", "button");
    // Uma parada só de Tab para a rosca inteira.
    expect(document.querySelectorAll('.payer-donut__arc[tabindex="0"]')).toHaveLength(1);
    act(() => first.focus());
    await user.keyboard("{ArrowRight}");
    expect(arcOf("a-1")).toHaveFocus();
    await user.keyboard("{End}");
    const others = arcOf("others");
    expect(others).toHaveFocus();
    expect(others.getAttribute("aria-label")).toMatch(/^Outros \(3\): R\$\s600,00, 39,0% da renda\. Ver quem está no grupo$/);
    await user.keyboard("{Enter}");
    expect(namesIn(panel)).toEqual(["PAG6", "PAG7", "PAG8"]);
    expect(arcOf("a-6")).toBeInTheDocument();
    await user.keyboard("{Enter}");
    // O foco está no Voltar: Enter volta.
    expect(namesIn(panel)).toHaveLength(7);
  });

  it("pela fatia, com o ponteiro", async () => {
    const { user, panel } = await openIncome();
    await user.click(arcOf("others"));
    expect(namesIn(panel)).toEqual(["PAG6", "PAG7", "PAG8"]);
  });
});

describe("Quem pagou: o nome e o valor ao ponteiro e ao foco", () => {
  it("passar o mouse na fatia ou na linha mostra o nome, o valor e a parte no meio da rosca", async () => {
    const { user, panel } = await openIncome();
    const center = panel.querySelector(".donut__center") as HTMLElement;
    expect(center).toHaveTextContent("12 meses");

    await user.hover(arcOf("a-2"));
    expect(center).toHaveTextContent("PAG2");
    expect(center).toHaveTextContent("R$ 700,00");
    expect(center).toHaveTextContent("18,0%");
    expect(arcOf("a-2")).toHaveAttribute("data-state", "active");

    await user.unhover(arcOf("a-2"));
    await user.hover(rowOf(panel, "PAG3"));
    expect(center).toHaveTextContent("PAG3");
    expect(arcOf("a-3")).toHaveAttribute("data-state", "active");
    await user.unhover(rowOf(panel, "PAG3"));
    expect(center).toHaveTextContent("12 meses");
  });

  it("o foco no teclado faz o mesmo", async () => {
    const { panel } = await openIncome();
    act(() => arcOf("a-4").focus());
    expect(panel.querySelector(".donut__center")).toHaveTextContent("PAG4");
    expect(panel.querySelector(".donut__center")).toHaveTextContent("R$ 500,00");
  });
});

describe("Quem pagou: filtrar os pagamentos por pagador", () => {
  const upcoming = () => screen.getByRole("region", { name: "Próximos pagamentos" });

  it("escolher um pagador filtra Próximos pagamentos, destaca e limpa de três jeitos", async () => {
    const { user, panel } = await openIncome();
    expect(within(upcoming()).getAllByRole("listitem")).toHaveLength(3);

    await user.click(pickOf(panel, "PAG1"));
    expect(rowOf(panel, "PAG1")).toHaveClass("is-selected");
    expect(pickOf(panel, "PAG1")).toHaveAttribute("aria-pressed", "true");
    const list = within(upcoming()).getAllByRole("listitem");
    expect(list).toHaveLength(1);
    expect(list[0]).toHaveTextContent("PAG1");
    // O selo diz quem filtra, nos dois painéis, e limpa.
    expect(within(panel).getByRole("button", { name: "Limpar o filtro por PAG1" })).toBeInTheDocument();
    await user.click(within(upcoming()).getByRole("button", { name: "Limpar o filtro por PAG1" }));
    expect(within(upcoming()).getAllByRole("listitem")).toHaveLength(3);
    expect(screen.queryByRole("button", { name: /Limpar o filtro/ })).not.toBeInTheDocument();

    // Clicar de novo no mesmo pagador desliga; pela fatia também liga.
    await user.click(arcOf("a-0"));
    expect(within(upcoming()).getAllByRole("listitem")).toHaveLength(1);
    expect(arcOf("a-0")).toHaveAttribute("aria-pressed", "true");
    await user.click(arcOf("a-0"));
    expect(within(upcoming()).getAllByRole("listitem")).toHaveLength(3);
  });

  it("pagador sem pagamento a receber diz isso, e o filtro de quem está em Outros some ao voltar", async () => {
    const { user, panel } = await openIncome();
    await user.click(pickOf(panel, "PAG3"));
    expect(within(upcoming()).getByText("Nenhum pagamento a receber de PAG3 lançado.")).toBeInTheDocument();

    await user.click(pickOf(panel, "Outros"));
    await user.click(pickOf(panel, "PAG7"));
    expect(within(upcoming()).getAllByRole("listitem")).toHaveLength(1);
    expect(within(upcoming()).getAllByRole("listitem")[0]).toHaveTextContent("PAG7");
    await user.click(within(panel).getByRole("button", { name: /Voltar/ }));
    expect(screen.queryByRole("button", { name: /Limpar o filtro/ })).not.toBeInTheDocument();
    expect(within(upcoming()).getAllByRole("listitem")).toHaveLength(3);
  });
});

describe("Estimativa: todos os ativos", () => {
  it("mostra os quatro que mais rendem e 'Ver todos' abre a lista inteira, ordenada", async () => {
    const { user } = await openIncome();
    const estimate = screen.getByRole("region", { name: "Estimativa da carteira de hoje" });
    // O texto da célula leva as iniciais do ícone (EST1 → "ESEST1") e a fonte ("B3").
    const assets = (root: HTMLElement) => within(root).getAllByRole("rowheader").map((th) => th.textContent?.match(/EST\d/)?.[0]);
    expect(assets(within(estimate).getByRole("table"))).toEqual(["EST1", "EST3", "EST5", "EST6"]);

    await user.click(within(estimate).getByRole("button", { name: "Ver todos os 8 ativos" }));
    const dialog = await screen.findByRole("dialog", { name: "Estimativa por ativo" });
    expect(assets(within(dialog).getByRole("table"))).toEqual(["EST1", "EST3", "EST5", "EST6", "EST2", "EST7", "EST4", "EST0"]);
    expect(within(dialog).getByText(/^~R\$\s40,00$/)).toBeInTheDocument();
    expect(dialog).toHaveTextContent(/~R\$\s140,00 por mês, líquido, 8 ativos/);
  });

  it("sem mais que quatro, não há o controle", async () => {
    const summary = exploreSummary();
    summary.estimate.assets = summary.estimate.assets.slice(0, 4);
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)), http.get("/api/v1/income", () => HttpResponse.json([])));
    renderApp("renda");
    const estimate = await screen.findByRole("region", { name: "Estimativa da carteira de hoje" });
    expect(within(estimate).queryByRole("button", { name: /Ver todos/ })).not.toBeInTheDocument();
  });
});

describe("Explorar com o olho fechado", () => {
  it("nenhum valor em reais aparece na rosca, no meio dela, no grupo Outros nem na folha da estimativa", async () => {
    const { user, panel } = await openIncome(true);
    const money = /R\$\s*\d/;
    const heard = (root: HTMLElement) => {
      const texts = [root.textContent ?? ""];
      for (const el of root.querySelectorAll("[aria-label]")) texts.push(el.getAttribute("aria-label") ?? "");
      return texts;
    };
    const noMoney = (root: HTMLElement) => heard(root).forEach((text) => expect(text).not.toMatch(money));

    await user.hover(arcOf("a-2"));
    expect(panel.querySelector(".donut__center")).toHaveTextContent(MASK_BRL);
    expect(arcOf("a-2").getAttribute("aria-label")).toContain(MASK_BRL);
    noMoney(panel);

    await user.click(pickOf(panel, "Outros"));
    noMoney(panel);
    expect(within(panel).getAllByText(MASK_BRL).length).toBeGreaterThan(2);
    expect(arcOf("others")).toBeNull();
    expect(arcOf("a-7").getAttribute("aria-label")).toContain(MASK_BRL);

    const estimate = screen.getByRole("region", { name: "Estimativa da carteira de hoje" });
    await user.click(within(estimate).getByRole("button", { name: "Ver todos os 8 ativos" }));
    noMoney(await screen.findByRole("dialog"));
  });
});
