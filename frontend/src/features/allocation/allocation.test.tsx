import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { PlanNotice } from "../../api/types";
import { createMoney, MASK_BRL } from "../../lib/money";
import { noticeText } from "../../lib/planText";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { isAboveCeiling, isCeilingUnavailable } from "../../lib/rating";
import { samplePlan } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

// O Intl separa "R$" do número com espaço que não quebra (U+00A0).
const NBSP = new RegExp(String.fromCharCode(160), "g");
const nbsp = (text: string) => text.replace(NBSP, " ");

const shown = createMoney(false);
const hidden = createMoney(true);

const lastRequest = (method: string, path: string) =>
  getDb().requests.filter((r) => r.method === method && r.path === path).at(-1);

/** O teto de valor próprio, como o servidor devolve. */
const manual = (value: number) => ({ method: "manual" as const, value, unavailable_reason: null });

/** Abre a escolha do teto na folha do ativo (ela fica recolhida numa linha). */
async function openCeiling(user: { click: (element: Element) => Promise<void> }, dialog: HTMLElement) {
  await user.click(within(dialog).getByRole("button", { name: /Mudar o teto de preço/ }));
}

function setCeiling(id: string, ceiling: ReturnType<typeof manual> | null) {
  const stored = getDb().assets.find((a) => a.id === id);
  if (stored) stored.ceiling = ceiling;
}

describe("textos do teto e do limite", () => {
  it("o ativo acima do teto diz o preço, o teto e que fica fora", () => {
    const text = noticeText(
      { code: "asset_above_ceiling", asset_id: "a-itsa", symbol: "ITSA4", unit_price: 11.2, max_price: 10, method: "manual" },
      [],
      shown,
    );
    expect(nbsp(text)).toBe("ITSA4 está a R$ 11,20, acima do seu teto de R$ 10,00. Fica fora deste aporte.");
    // Pelo método, o aviso diz qual.
    const byMethod = noticeText(
      { code: "asset_above_ceiling", asset_id: "a-itsa", symbol: "ITSA4", unit_price: 11.2, max_price: 9.5, method: "graham" },
      [],
      shown,
    );
    expect(nbsp(byMethod)).toBe("ITSA4 está a R$ 11,20, acima do seu teto pelo Graham de R$ 9,50. Fica fora deste aporte.");
  });

  it("o teto pelo método sem valor diz o método, o motivo e o que fazer", () => {
    const text = noticeText(
      { code: "asset_ceiling_unavailable", asset_id: "a-itsa", symbol: "ITSA4", method: "graham", reason: "loss" },
      [],
      shown,
    );
    expect(text).toBe(
      "ITSA4 fica fora deste aporte: o teto pelo Graham está sem valor hoje (prejuízo: lucro por ação zero ou negativo). Troque o teto ou escolha Sem teto.",
    );
    const fund = noticeText(
      { code: "asset_ceiling_unavailable", asset_id: "a-trxf", symbol: "TRXF11", method: "combined", reason: "no_reference" },
      [],
      shown,
    );
    expect(fund).toContain("o teto pelas duas referências está sem valor hoje");
  });

  it("o limite diz quem parou nele e, se for o caso, quanto ficou sem destino", () => {
    const limit = (symbols: string[], amount: number): PlanNotice => ({
      code: "concentration_limit",
      max_percent: 15,
      asset_ids: symbols.map((s) => `id-${s}`),
      symbols,
      amount,
    });
    expect(noticeText(limit(["ITSA4"], 0), [], shown)).toBe(
      "ITSA4 chegou no seu limite de 15% da carteira; o que passaria foi para os outros ativos da classe.",
    );
    expect(nbsp(noticeText(limit(["ITSA4", "KNRI11", "HGLG11"], 1200), [], shown))).toBe(
      "ITSA4, KNRI11 e HGLG11 chegaram no seu limite de 15% da carteira. R$ 1.200,00 ficaram sem destino: não cabem em nenhum ativo sem passar do limite.",
    );
    expect(nbsp(noticeText(limit([], 300), [], shown))).toBe(
      "R$ 300,00 ficaram sem destino: nenhum ativo cabe mais sem passar do seu limite de 15% da carteira.",
    );
  });

  it("com o olho fechado, preço, teto e sobra do limite saem mascarados", () => {
    const above = noticeText(
      { code: "asset_above_ceiling", asset_id: "a-itsa", symbol: "ITSA4", unit_price: 11.2, max_price: 10, method: "manual" },
      [],
      hidden,
    );
    expect(above).toBe(`ITSA4 está a ${MASK_BRL}, acima do seu teto de ${MASK_BRL}. Fica fora deste aporte.`);
    const limit = (symbols: string[]) =>
      noticeText({ code: "concentration_limit", max_percent: 15, asset_ids: [], symbols, amount: 1200 }, [], hidden);
    expect(limit(["ITSA4"])).toContain(`${MASK_BRL} ficaram sem destino`);
    expect(limit([])).toContain(`${MASK_BRL} ficaram sem destino`);
    for (const text of [above, limit(["ITSA4"]), limit([])]) expect(text).not.toMatch(/R\$\s*\d/);
  });

  it("a classe bloqueada pelo teto ou pelo limite diz o que fazer", () => {
    const blocked = (reason: "above_ceiling" | "ceiling_unavailable" | "concentration_limit") =>
      noticeText({ code: "class_blocked", asset_class: "br_stocks", amount: 5000, reason }, [], shown);
    expect(blocked("above_ceiling")).toContain("acima do teto de preço que você definiu");
    expect(blocked("ceiling_unavailable")).toContain("o método de teto escolhido está sem valor hoje");
    expect(blocked("concentration_limit")).toContain("Cadastre outro ativo na classe ou suba o limite");
  });

  it("acima do teto só com preço de verdade e teto definido", () => {
    const asset = { unit_price: 11.2, ceiling: manual(10), price_source: "quote" as const };
    expect(isAboveCeiling(asset)).toBe(true);
    expect(isAboveCeiling({ ...asset, ceiling: manual(11.2) })).toBe(false);
    expect(isAboveCeiling({ ...asset, ceiling: null })).toBe(false);
    expect(isAboveCeiling({ ...asset, price_source: "missing" })).toBe(false);
    expect(isAboveCeiling({ ...asset, unit_price: null })).toBe(false);
    // Método sem valor: não está "acima", está sem teto válido.
    const off = { method: "graham" as const, value: null, unavailable_reason: "loss" as const };
    expect(isAboveCeiling({ ...asset, ceiling: off })).toBe(false);
    expect(isCeilingUnavailable({ ceiling: off })).toBe(true);
    expect(isCeilingUnavailable({ ceiling: manual(10) })).toBe(false);
    expect(isCeilingUnavailable({ ceiling: null })).toBe(false);
  });
});

describe("avisos do teto e do limite no plano", () => {
  async function calculate(notices: PlanNotice[]) {
    server.use(http.post("/api/v1/plans", () => HttpResponse.json({ ...samplePlan(), notices })));
    const rendered = renderApp("aportar");
    const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    await rendered.user.type(input, "600000");
    await rendered.user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    return { ...rendered, result };
  }

  it("ativo acima do teto: o aviso abre o ativo, e o teto está lá para revisar", async () => {
    setCeiling("a-itsa", manual(10));
    const { user, result } = await calculate([
      { code: "asset_above_ceiling", asset_id: "a-itsa", symbol: "ITSA4", unit_price: 11.2, max_price: 10, method: "manual" },
    ]);
    expect(within(result).getByText(/ITSA4 está a .* acima do seu teto/)).toBeInTheDocument();
    await user.click(within(result).getByRole("button", { name: /Abrir ITSA4/ }));
    const dialog = await screen.findByRole("dialog", { name: "ITSA4" });
    // Fechada, a linha do teto já diz qual é; aberta, a escolha vem marcada.
    expect(within(dialog).getByRole("button", { name: /agora: Valor próprio · R\$\s10,00/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(within(dialog).queryByRole("radio")).toBeNull();
    await openCeiling(user, dialog);
    expect(within(dialog).getByRole("radio", { name: /Valor próprio/ })).toBeChecked();
    expect(within(dialog).getByText(/Hoje está a R\$\s11,20, acima do teto/)).toBeInTheDocument();
  });

  it("classe bloqueada pelo teto, um ativo só: abre ele", async () => {
    setCeiling("a-itsa", manual(10));
    const { user, result } = await calculate([
      { code: "class_blocked", asset_class: "br_stocks", amount: 5000, reason: "above_ceiling" },
    ]);
    await user.click(within(result).getByRole("button", { name: /Abrir ITSA4/ }));
    expect(await screen.findByRole("dialog", { name: "ITSA4" })).toBeInTheDocument();
  });

  it("teto pelo método sem valor: o aviso diz o motivo e abre o ativo; a classe bloqueada também", async () => {
    const stored = getDb().assets.find((a) => a.id === "a-itsa");
    if (stored) stored.ceiling = { method: "graham", value: null, unavailable_reason: "loss" };
    const { user, result } = await calculate([
      { code: "asset_ceiling_unavailable", asset_id: "a-itsa", symbol: "ITSA4", method: "graham", reason: "loss" },
      { code: "class_blocked", asset_class: "br_stocks", amount: 5000, reason: "ceiling_unavailable" },
    ]);
    expect(within(result).getByText(/o teto pelo Graham está sem valor hoje \(prejuízo/)).toBeInTheDocument();
    expect(within(result).getAllByRole("button", { name: /Abrir ITSA4/ })).toHaveLength(2);
    await user.click(within(result).getAllByRole("button", { name: /Abrir ITSA4/ })[0] as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "ITSA4" });
    await openCeiling(user, dialog);
    expect(within(dialog).getByRole("radio", { name: /^Graham/ })).toBeChecked();
  });

  it("limite e classe bloqueada pelo limite levam ao controle do limite", async () => {
    const { result } = await calculate([
      { code: "concentration_limit", max_percent: 20, asset_ids: ["a-itsa"], symbols: ["ITSA4"], amount: 0 },
      { code: "class_blocked", asset_class: "br_reits", amount: 900, reason: "concentration_limit" },
    ]);
    const links = within(result).getAllByRole("link", { name: /Ajustar o limite/ });
    expect(links).toHaveLength(2);
    for (const link of links) expect(link).toHaveAttribute("href", "#/ajustes/metas");
  });
});

describe("teto de preço na folha do ativo", () => {
  async function openItsa() {
    const rendered = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const stocks = screen.getByRole("region", { name: "Ações Nacionais" });
    await rendered.user.click(within(stocks).getByRole("button", { name: /ITSA4/ }));
    const dialog = await screen.findByRole("dialog", { name: "ITSA4" });
    return { ...rendered, dialog };
  }

  it("nasce sem teto; o valor próprio avisa que hoje está acima e grava", async () => {
    const { user, dialog } = await openItsa();
    expect(within(dialog).getByRole("button", { name: "Mudar o teto de preço (agora: Sem teto)" })).toBeInTheDocument();
    await openCeiling(user, dialog);
    expect(within(dialog).getByRole("radio", { name: /Sem teto/ })).toBeChecked();
    expect(within(dialog).queryByLabelText("Pagar no máximo, por unidade")).toBeNull();
    await user.click(within(dialog).getByRole("radio", { name: /Valor próprio/ }));
    const field = within(dialog).getByLabelText("Pagar no máximo, por unidade");
    await user.clear(field);
    await user.type(field, "1000");
    expect(within(dialog).getByText(/Hoje está a R\$\s11,20, acima do teto: fica fora do aporte/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() =>
      expect(lastRequest("PATCH", "/api/v1/assets/a-itsa")?.body).toEqual({ ceiling: { method: "manual", value: 10 } }),
    );
    expect(getDb().assets.find((a) => a.id === "a-itsa")?.ceiling).toEqual(manual(10));
  });

  it("escolhe um método: mostra o valor de hoje e o veredito, e grava só o método", async () => {
    const { user, dialog } = await openItsa();
    await openCeiling(user, dialog);
    // As referências de hoje, uma por método, com o chip contra o preço de hoje (R$ 11,20).
    const bazin = await within(dialog).findByRole("radio", { name: /^Bazin R\$\s12,50 aprova/ });
    const graham = within(dialog).getByRole("radio", { name: /^Graham R\$\s9,50 não aprova/ });
    expect(within(dialog).getByRole("radio", { name: /^Pelas duas R\$\s9,50 não aprova/ })).toBeInTheDocument();
    expect(bazin).not.toBeChecked();
    // A conta no ⓘ.
    await user.click(within(dialog).getByRole("button", { name: "Como sai o Bazin de ITSA4" }));
    expect(within(dialog).getByText(/Média de 4 anos de dividendos: R\$\s0,75 por cota ao ano, dividida por 6% ao ano/)).toBeInTheDocument();
    await user.click(graham);
    expect(within(dialog).getByText(/Hoje está a R\$\s11,20, acima do teto/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /agora: Graham · R\$\s9,50/ })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() =>
      expect(lastRequest("PATCH", "/api/v1/assets/a-itsa")?.body).toEqual({ ceiling: { method: "graham" } }),
    );
    // O servidor devolve o teto de hoje pelo método; a Carteira marca "acima do teto".
    expect(getDb().assets.find((a) => a.id === "a-itsa")?.ceiling).toEqual({
      method: "graham",
      value: 9.5,
      unavailable_reason: null,
    });
  });

  it("tirar o teto manda null; renda fixa por valor não tem teto", async () => {
    setCeiling("a-itsa", manual(15));
    const { user, dialog } = await openItsa();
    await openCeiling(user, dialog);
    expect(within(dialog).getByRole("radio", { name: /Valor próprio/ })).toBeChecked();
    expect(within(dialog).queryByText(/acima do teto/)).toBeNull();
    await user.click(within(dialog).getByRole("radio", { name: /Sem teto/ }));
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/assets/a-itsa")?.body).toEqual({ ceiling: null }));

    const rf = screen.getByRole("region", { name: "Renda Fixa" });
    await user.click(within(rf).getByRole("button", { name: /Tesouro IPCA\+ 2035/ }));
    const fixed = await screen.findByRole("dialog", { name: "Tesouro IPCA+ 2035" });
    expect(within(fixed).queryByRole("button", { name: /Mudar o teto de preço/ })).toBeNull();
  });

  it("FII: VP e Rendimento no lugar de Bazin e Graham; método sem valor avisa que tira o ativo do aporte", async () => {
    const rendered = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    await rendered.user.click(within(fiis).getByRole("button", { name: /TRXF11/ }));
    const dialog = await screen.findByRole("dialog", { name: "TRXF11" });
    await openCeiling(rendered.user, dialog);
    await within(dialog).findByRole("radio", { name: /^VP R\$\s110,20 aprova/ });
    expect(within(dialog).queryByRole("radio", { name: /Bazin|Graham/ })).toBeNull();
    const rendimento = within(dialog).getByRole("radio", { name: /^Rendimento sem valor hoje: tipo do FII ainda desconhecido/ });
    await rendered.user.click(rendimento);
    expect(within(dialog).getByRole("note")).toHaveTextContent(
      "O teto pelo Rendimento está sem valor hoje (tipo do FII ainda desconhecido): com ele, TRXF11 fica fora do aporte",
    );
    await rendered.user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() =>
      expect(lastRequest("PATCH", "/api/v1/assets/a-trxf")?.body).toEqual({ ceiling: { method: "fund_yield" } }),
    );
    expect(getDb().assets.find((a) => a.id === "a-trxf")?.ceiling).toEqual({
      method: "fund_yield",
      value: null,
      unavailable_reason: "unknown_fund_type",
    });
  });
});

describe("limite por ativo, na tela de Metas", () => {
  async function openLimit() {
    const rendered = renderApp("ajustes/metas");
    const card = await screen.findByRole("region", { name: "Limite por ativo" });
    await within(card).findByRole("switch", { name: /Limitar a concentração/ });
    return { ...rendered, card };
  }

  it("começa desligado; ligar mostra o controle, e salvar grava", async () => {
    const { user, card } = await openLimit();
    const toggle = within(card).getByRole("switch", { name: /Limitar a concentração/ });
    expect(toggle).not.toBeChecked();
    expect(within(card).getByText("Desligado: cada ativo recebe pela nota, sem teto.")).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Salvar limite" })).toBeDisabled();
    await user.click(toggle);
    const slider = within(card).getByRole("slider", { name: "Máximo por ativo" });
    expect(slider).toHaveValue("20");
    expect(slider).toHaveAttribute("aria-valuetext", "20% da carteira");
    await user.click(within(card).getByRole("button", { name: "Salvar limite" }));
    expect(await screen.findByText("Limite salvo: nenhum ativo passa de 20% da carteira.")).toBeInTheDocument();
    expect(getDb().maxAssetPercent).toBe(20);
    expect(within(card).getByText("Ligado em 20% da carteira.")).toBeInTheDocument();
  });

  it("já ligado: mexer no valor e desligar", async () => {
    getDb().maxAssetPercent = 15;
    const { user, card } = await openLimit();
    const slider = within(card).getByRole("slider", { name: "Máximo por ativo" });
    expect(slider).toHaveValue("15");
    fireEvent.change(slider, { target: { value: "30" } });
    expect(within(card).getByText("30%")).toBeInTheDocument();
    await user.click(within(card).getByRole("switch", { name: /Limitar a concentração/ }));
    expect(within(card).queryByRole("slider")).toBeNull();
    await user.click(within(card).getByRole("button", { name: "Salvar limite" }));
    expect(await screen.findByText("Limite desligado.")).toBeInTheDocument();
    expect(getDb().maxAssetPercent).toBeNull();
    expect(lastRequest("PUT", "/api/v1/settings/allocation")?.body).toEqual({ max_asset_percent: null });
  });

  it("erro ao salvar e ao carregar aparecem no cartão", async () => {
    server.use(
      http.put("/api/v1/settings/allocation", () =>
        HttpResponse.json(
          { code: "validation_failed", status: 422, title: "Dado inválido", detail: "O limite por ativo vai de 0,01% a 100%." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user, card } = await openLimit();
    await user.click(within(card).getByRole("switch", { name: /Limitar a concentração/ }));
    await user.click(within(card).getByRole("button", { name: "Salvar limite" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent("O limite por ativo vai de 0,01% a 100%.");
  });

  it("sem resposta do servidor, o cartão diz o erro", async () => {
    server.use(http.get("/api/v1/settings/allocation", () => HttpResponse.json({}, { status: 500 })));
    renderApp("ajustes/metas");
    const card = await screen.findByRole("region", { name: "Limite por ativo" });
    expect(await within(card).findByRole("alert")).toHaveTextContent("Algo deu errado");
  });
});

describe("selo acima do teto na Carteira", () => {
  it("o ativo acima do teto mostra o selo, e o ideal dele não muda", async () => {
    setCeiling("a-itsa", manual(10));
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const stocks = screen.getByRole("region", { name: "Ações Nacionais" });
    const row = within(stocks).getByRole("button", { name: /ITSA4/ });
    expect(within(row).getByText("acima do teto")).toBeInTheDocument();
    expect(within(row).getByText(/ideal 100,0%/)).toBeInTheDocument();
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    expect(within(fiis).queryByText("acima do teto")).toBeNull();
  });

  it("o ativo com o teto pelo método sem valor mostra o selo", async () => {
    const stored = getDb().assets.find((a) => a.id === "a-knri");
    if (stored) stored.ceiling = { method: "nav", value: null, unavailable_reason: "no_fundamentals" };
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    const row = within(fiis).getByRole("button", { name: /KNRI11/ });
    expect(within(row).getByText("teto sem valor")).toBeInTheDocument();
  });
});

describe("teto e limite com o olho fechado", () => {
  const MONEY = /R\$\s*\d/;

  it("os avisos do plano não mostram nem dizem valor", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    server.use(
      http.post("/api/v1/plans", () =>
        HttpResponse.json({
          ...samplePlan(),
          notices: [
            { code: "asset_above_ceiling", asset_id: "a-itsa", symbol: "ITSA4", unit_price: 11.2, max_price: 10, method: "manual" },
            { code: "concentration_limit", max_percent: 20, asset_ids: ["a-btc"], symbols: ["BTC"], amount: 1200 },
          ],
        }),
      ),
    );
    const { user } = renderApp("aportar");
    const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const notices = within(result).getAllByRole("listitem").filter((li) => li.classList.contains("notice"));
    expect(notices.length).toBeGreaterThanOrEqual(2);
    for (const notice of notices) {
      expect(notice.textContent).not.toMatch(MONEY);
      expect(notice.getAttribute("aria-label") ?? "").not.toMatch(MONEY);
    }
    expect(within(result).getByText(/ITSA4 está a R\$ ••••, acima do seu teto de R\$ ••••/)).toBeInTheDocument();
    expect(within(result).getByText(/R\$ •••• ficaram sem destino: não cabem/)).toBeInTheDocument();
  });

  it("o campo do teto fora do foco e o aviso da folha saem mascarados", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    setCeiling("a-itsa", manual(10));
    const { user } = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const stocks = screen.getByRole("region", { name: "Ações Nacionais" });
    await user.click(within(stocks).getByRole("button", { name: /ITSA4/ }));
    const dialog = await screen.findByRole("dialog", { name: "ITSA4" });
    // A linha do teto, fechada, também mascara.
    const toggle = within(dialog).getByRole("button", { name: /Mudar o teto de preço/ });
    expect(toggle.textContent ?? "").not.toMatch(MONEY);
    expect(toggle.getAttribute("aria-label") ?? "").not.toMatch(MONEY);
    await openCeiling(user, dialog);
    const field = within(dialog).getByLabelText("Pagar no máximo, por unidade");
    expect(field).toHaveValue(MASK_BRL);
    // As referências da escolha também saem mascaradas.
    const bazin = await within(dialog).findByRole("radio", { name: /^Bazin/ });
    expect(bazin.closest(".ceiling-option")?.textContent ?? "").not.toMatch(MONEY);
    const warning = within(dialog).getByText(/acima do teto: fica fora do aporte/);
    expect(warning.textContent).not.toMatch(MONEY);
    expect(warning).toHaveTextContent(`Hoje está a ${MASK_BRL}, acima do teto`);
    // Com o foco, é a pessoa digitando o próprio valor: o número aparece.
    await user.click(field);
    expect(field).not.toHaveValue(MASK_BRL);
    await user.tab();
    expect(field).toHaveValue(MASK_BRL);
  });
});
