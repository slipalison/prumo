import confetti from "canvas-confetti";
import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { samplePlan } from "../../test/fixtures";
import { openRegistration } from "../../test/purchase";
import { renderApp } from "../../test/render";
import { getDb, resetDb, server } from "../../test/server";

const zeroGoals = { br_stocks: 0, br_reits: 0, intl_stocks: 0, us_reits: 0, crypto: 0, fixed_income: 0, intl_fixed_income: 0 };

async function openCalculator() {
  const rendered = renderApp("aportar");
  const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
  return { ...rendered, input };
}

describe("Aportar", () => {
  it("calcula e mostra o que comprar, os avisos e o antes/depois", async () => {
    const { user, input } = await openCalculator();
    expect(input).toHaveFocus();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));

    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByRole("heading", { name: /Compre 4 ativos/ })).toBeInTheDocument();
    expect(await within(result).findByText(/R\$\s5\.947,92/)).toBeInTheDocument();
    expect(within(result).getByText(/troco de R\$\s52,08/)).toBeInTheDocument();
    expect(within(result).getByText("BTLG11")).toBeInTheDocument();
    expect(within(result).getByText(/17 cotas × R\$\s53,81/)).toBeInTheDocument();
    expect(within(result).getByText(/0,0045 BTC/)).toBeInTheDocument();
    expect(within(result).getByText(/Ações Internacionais precisa de/)).toBeInTheDocument();
    expect(within(result).getByRole("button", { name: /Adicionar em Ações EUA/ })).toBeInTheDocument();

    const plans = getDb().requests.filter((r) => r.path === "/api/v1/plans");
    expect(plans.at(-1)?.body).toEqual({ amount: 6000 });

    const compare = within(result).getByRole("group", { name: "Comparar" });
    await user.click(within(compare).getByRole("button", { name: "Antes" }));
    expect(within(result).getByRole("heading", { name: "A carteira hoje" })).toBeInTheDocument();
    await user.click(within(compare).getByRole("button", { name: "Depois" }));
    expect(within(result).getByRole("heading", { name: "A carteira depois do aporte" })).toBeInTheDocument();
    expect(within(result).getAllByRole("meter").length).toBeGreaterThan(0);
  });

  it("valor rápido calcula direto, e Enter também", async () => {
    const { user, input } = await openCalculator();
    await user.click(screen.getByRole("button", { name: /R\$\s500$/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(getDb().requests.filter((r) => r.path === "/api/v1/plans").at(-1)?.body).toEqual({ amount: 500 });

    await user.clear(input);
    await user.type(input, "100000{Enter}");
    await waitFor(() =>
      expect(getDb().requests.filter((r) => r.path === "/api/v1/plans").at(-1)?.body).toEqual({ amount: 1000 }),
    );
  });

  it("registra o aporte com a quantidade ajustada de um cartão e comemora", async () => {
    vi.mocked(confetti).mockClear();
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await user.click(await screen.findByRole("button", { name: "Comprei todos como sugerido" }));

    // A corretora executou uma cota a menos de TRXF11: corrige no cartão.
    await user.click(screen.getByRole("button", { name: "Editar a compra de TRXF11" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei TRXF11" });
    await user.click(within(ask).getByRole("button", { name: "Diminuir cotas" }));
    await user.click(within(ask).getByRole("button", { name: "Salvar" }));

    await user.click(screen.getByRole("button", { name: "Registrar 4 comprados" }));
    const dialog = await screen.findByRole("dialog", { name: "Registrar aporte" });
    await user.click(within(dialog).getByRole("button", { name: "Registrar aporte" }));

    expect(await screen.findByRole("heading", { name: "Aporte registrado!" })).toBeInTheDocument();
    const body = getDb().requests.find((r) => r.path === "/api/v1/contributions" && r.method === "POST")?.body;
    // O aporte é a soma do que foi marcado: 2.721,60 + 756,40 + 16 × 53,81 + 1.555,15.
    expect(body).toEqual({
      amount: 5894.11,
      items: [
        { asset_id: "a-itsa", quantity: 243, unit_price: 11.2 },
        { asset_id: "a-btlg", quantity: 8, unit_price: 94.55 },
        { asset_id: "a-trxf", quantity: 16, unit_price: 53.81 },
        { asset_id: "a-btc", quantity: 0.0045, unit_price: 345588.05 },
      ],
    });
    expect(getDb().assets.find((a) => a.id === "a-trxf")?.quantity).toBe(36);
    expect(confetti).toHaveBeenCalled();
    expect(await screen.findByText(/Aporte registrado\. A carteira já está atualizada\./)).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /Último: R\$\s5\.894,11/ })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Quanto você vai aportar?" })).toHaveFocus(), {
      timeout: 3000,
    });
  });

  it("título do Tesouro no plano: nome, fração de título e a quantidade fracionária no registro", async () => {
    getDb().assets.push({
      id: "a-td",
      asset_class: "fixed_income",
      symbol: "TD-IPCA-20350515",
      display_name: "Tesouro IPCA+ 2035",
      quantity: 0,
      quote_price: 2500.95,
      manual_price: null,
      currency: "BRL",
      score_override: 6,
      answers: {},
      treasury: true,
    });
    const bond = {
      asset_id: "a-td",
      asset_class: "fixed_income" as const,
      symbol: "TD-IPCA-20350515",
      display_name: "Tesouro IPCA+ 2035",
      listed_on_b3: false,
      treasury: true,
      quantity: 0.39,
      unit_price: 2500.95,
      value: 975.37,
      divisible: true,
      class_percent_after: 100,
    };
    server.use(
      http.post("/api/v1/plans", () =>
        HttpResponse.json({ ...samplePlan(), invested: 975.37, change: 24.63, items: [bond], notices: [] }),
      ),
    );
    const { user, input } = await openCalculator();
    await user.type(input, "100000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    // O cartão diz o nome do título, não a chave, e a compra em frações de título.
    expect(within(result).getByText("Tesouro IPCA+ 2035")).toBeInTheDocument();
    expect(within(result).queryByText(/TD-IPCA/)).toBeNull();
    expect(within(result).getByText(/0,39 títulos × R\$\s2\.500,95/)).toBeInTheDocument();

    await user.click(await screen.findByRole("button", { name: "Comprei Tesouro IPCA+ 2035" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei Tesouro IPCA+ 2035" });
    // Fração: o campo aceita vírgula e não tem − e +.
    expect(within(ask).queryByRole("button", { name: /Diminuir/ })).toBeNull();
    const quantity = within(ask).getByLabelText("Quantidade");
    expect(quantity).toHaveValue("0,39");
    await user.clear(quantity);
    await user.type(quantity, "0,3");
    await user.click(within(ask).getByRole("button", { name: "Marcar como comprado" }));
    await user.click(await screen.findByRole("button", { name: "Registrar 1 comprado" }));
    const dialog = await screen.findByRole("dialog", { name: "Registrar aporte" });
    expect(within(dialog).getByText("Tesouro IPCA+ 2035")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Registrar aporte" }));
    expect(await screen.findByRole("heading", { name: "Aporte registrado!" })).toBeInTheDocument();
    const body = getDb().requests.find((r) => r.path === "/api/v1/contributions" && r.method === "POST")?.body as {
      items: Array<{ asset_id: string; quantity: number; unit_price: number }>;
    };
    expect(body.items).toEqual([{ asset_id: "a-td", quantity: 0.3, unit_price: 2500.95 }]);
    expect(getDb().assets.find((a) => a.id === "a-td")?.quantity).toBe(0.3);
  });

  it("mostra o erro do registro sem fechar a folha", async () => {
    server.use(
      http.post("/api/v1/contributions", () =>
        HttpResponse.json({ code: "not_found", status: 404, title: "x" }, { status: 404, headers: { "Content-Type": "application/problem+json" } }),
      ),
    );
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const dialog = await openRegistration(user);
    await user.click(within(dialog).getByRole("button", { name: "Registrar aporte" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Não encontrei esse item");
  });

  it("erro no cálculo vira aviso", async () => {
    server.use(http.post("/api/v1/plans", () => HttpResponse.error()));
    const { user, input } = await openCalculator();
    await user.type(input, "100000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    expect(await screen.findByText(/Sem conexão com o servidor/)).toBeInTheDocument();
  });

  it("sem metas: convida a ir em Metas", async () => {
    resetDb({ goals: zeroGoals });
    server.use(
      http.post("/api/v1/plans", () =>
        HttpResponse.json({
          ...samplePlan(),
          invested: 0,
          change: 0,
          unallocated: 6000,
          items: [],
          notices: [{ code: "no_targets" }],
        }),
      ),
    );
    const { user, input } = await openCalculator();
    expect(screen.getByRole("link", { name: "Defina em Metas" })).toHaveAttribute("href", "#/ajustes/metas");
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByRole("heading", { name: "Defina suas metas primeiro" })).toBeInTheDocument();
    expect(within(result).getByText(/R\$\s6\.000,00 ficaram sem destino/)).toBeInTheDocument();
    expect(within(result).getByRole("link", { name: /Ir para Metas/ })).toHaveAttribute("href", "#/ajustes/metas");
    expect(within(result).queryByRole("button", { name: /Registrar .*comprados?/ })).toBeNull();
    expect(within(result).queryByRole("button", { name: /Comprei/ })).toBeNull();
  });

  it("nada a comprar: explica e mostra os outros avisos", async () => {
    server.use(
      http.post("/api/v1/plans", () =>
        HttpResponse.json({
          ...samplePlan(),
          invested: 0,
          change: 40,
          unallocated: 0,
          items: [],
          notices: [
            { code: "assets_without_price", symbols: ["XYZW11"] },
            { code: "assets_without_score", symbols: ["MXRF11"] },
          ],
        }),
      ),
    );
    const { user, input } = await openCalculator();
    await user.type(input, "4000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByRole("heading", { name: "Nada a comprar agora" })).toBeInTheDocument();
    expect(within(result).getByText(/Sem cotação agora: XYZW11/)).toBeInTheDocument();
    expect(within(result).getByText(/Sem nota: MXRF11/)).toBeInTheDocument();
  });

  it("digitar um valor novo descarta o plano anterior", async () => {
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.type(input, "1");
    await waitFor(() => expect(screen.queryByRole("region", { name: "Resultado do aporte" })).toBeNull(), {
      timeout: 3000,
    });
  });
});

describe("avisos do plano", () => {
  const planWith = (symbols: string[]) =>
    HttpResponse.json({ ...samplePlan(), notices: [{ code: "assets_without_score", symbols }] });

  /** Tudo numa classe acima da meta e as outras sem ativo: nada a comprar. */
  const blockedPlan = (assetClass: "intl_stocks" | "crypto") => ({
    ...samplePlan(5000),
    invested: 0,
    change: 0,
    unallocated: 5000,
    items: [],
    notices: [{ code: "class_blocked", asset_class: assetClass, amount: 25226.45, reason: "no_assets" }],
  });

  const recordPlans = (body: object) => {
    const amounts: unknown[] = [];
    server.use(
      http.post("/api/v1/plans", async ({ request }) => {
        amounts.push(await request.json());
        return HttpResponse.json(body);
      }),
    );
    return amounts;
  };

  it("classe sem ativo: o título diz o que falta e o cadastro abre ali, já na classe", async () => {
    const amounts = recordPlans(blockedPlan("intl_stocks"));
    const { user, input } = await openCalculator();
    await user.type(input, "500000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByRole("heading", { name: "Faltam ativos onde o aporte precisa ir" })).toBeInTheDocument();
    expect(within(result).getByText(/Ações Internacionais precisa de .* ainda não tem nenhum ativo/)).toBeInTheDocument();
    await user.click(within(result).getByRole("button", { name: /Adicionar em Ações EUA/ }));

    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    expect(within(dialog).getByRole("radio", { name: /Ações EUA/ })).toHaveAttribute("aria-checked", "true");
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "voo");
    await user.click(await within(dialog).findByRole("option", { name: /VOO/ }));
    // Sem quantidade: ainda não comprou, e pode cadastrar mesmo assim.
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(getDb().requests.filter((r) => r.method === "POST" && r.path === "/api/v1/assets").at(-1)?.body).toEqual({
        asset_class: "intl_stocks",
        symbol: "VOO",
        quantity: 0,
        display_name: "Vanguard S&P 500 ETF",
      }),
    );

    // Ação nasce sem nota: o checklist abre, e salvar refaz o plano.
    const rating = await screen.findByRole("dialog", { name: "VOO" });
    const question = within(rating).getByRole("group", { name: "Lucro líquido positivo em pelo menos 4 dos últimos 5 anos?" });
    await user.click(within(question).getByRole("button", { name: "Sim" }));
    await user.click(within(rating).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(amounts).toEqual([{ amount: 5000 }, { amount: 5000 }]));
  });

  it("ETF de bitcoin da B3 em Cripto: a chave vem ligada e o plano é refeito na hora", async () => {
    const amounts = recordPlans(blockedPlan("crypto"));
    const { user, input } = await openCalculator();
    await user.type(input, "500000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(within(result).getByRole("button", { name: /Adicionar em Cripto/ }));

    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "qbtc");
    const option = await within(dialog).findByRole("option", { name: /QBTC11/ });
    expect(option).toHaveTextContent("Cripto · B3");
    await user.click(option);
    expect(within(dialog).getByRole("switch", { name: /Negociado na B3/ })).toBeChecked();
    // Na B3 é cota inteira: o campo tem − e +.
    await user.click(within(dialog).getByRole("button", { name: "Aumentar quantidade" }));
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(getDb().requests.filter((r) => r.method === "POST" && r.path === "/api/v1/assets").at(-1)?.body).toEqual({
        asset_class: "crypto",
        symbol: "QBTC11",
        quantity: 1,
        display_name: "QR CME CF Bitcoin Reference Rate",
        score_override: 5,
        listed_on_b3: true,
      }),
    );
    // Cripto já nasce com nota: nada de checklist, o plano refaz direto.
    await waitFor(() => expect(amounts).toEqual([{ amount: 5000 }, { amount: 5000 }]));
  });

  it("ticker da B3 sem cotação em Cripto: o aviso sugere a B3 e abre o ativo", async () => {
    resetDb();
    getDb().assets.push({
      id: "a-qbtc",
      asset_class: "crypto",
      symbol: "QBTC11",
      display_name: "QR Bitcoin",
      quantity: 3,
      quote_price: null,
      manual_price: null,
      currency: "BRL",
      score_override: 6,
      answers: {},
      listed_on_b3: false,
    });
    const amounts = recordPlans({ ...samplePlan(), notices: [{ code: "assets_without_price", symbols: ["QBTC11"] }] });
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByText(/Fica fora da conta até ter preço\. Se for ETF ou BDR da B3/)).toBeInTheDocument();
    await user.click(within(result).getByRole("button", { name: /Abrir QBTC11/ }));

    const dialog = await screen.findByRole("dialog", { name: "QBTC11" });
    const b3 = within(dialog).getByRole("switch", { name: /Negociado na B3/ });
    expect(b3).not.toBeChecked();
    await user.click(b3);
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() =>
      expect(getDb().requests.filter((r) => r.method === "PATCH" && r.path === "/api/v1/assets/a-qbtc").at(-1)?.body).toEqual({
        listed_on_b3: true,
      }),
    );
    await waitFor(() => expect(amounts).toEqual([{ amount: 6000 }, { amount: 6000 }]));
  });

  it("sem nota, um ativo só: abre a folha dele no checklist e recalcula depois de salvar", async () => {
    const amounts: unknown[] = [];
    server.use(
      http.post("/api/v1/plans", async ({ request }) => {
        amounts.push(await request.json());
        return planWith(["MXRF11"]);
      }),
    );
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    await user.click(within(result).getByRole("button", { name: /Dar nota ao MXRF11/ }));

    const dialog = await screen.findByRole("dialog", { name: "MXRF11" });
    expect(within(dialog).getByText("Responda o checklist para o MXRF11 ganhar nota — cada sim soma 1.")).toBeInTheDocument();
    const question = within(dialog).getByRole("group", { name: "Os imóveis ficam em regiões de alta demanda?" });
    await user.click(within(question).getByRole("button", { name: "Sim" }));
    expect(amounts).toEqual([{ amount: 6000 }]);
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));

    // A nota nova deixou o plano velho: ele é refeito com o mesmo valor.
    await waitFor(() => expect(amounts).toEqual([{ amount: 6000 }, { amount: 6000 }]));
    expect(getDb().assets.find((a) => a.id === "a-mxrf")?.answers.r1).toBe(true);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
  });

  it("sem nota, vários ativos: leva para a Carteira", async () => {
    server.use(http.post("/api/v1/plans", () => planWith(["MXRF11", "TRXF11"])));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const notice = within(result).getByText(/Sem nota: MXRF11, TRXF11/).closest("li") as HTMLElement;
    expect(within(notice).getByRole("link", { name: /Abrir Carteira/ })).toHaveAttribute("href", "#/carteira");
  });

  it("sem nota de quem não está na lista: também leva para a Carteira", async () => {
    server.use(http.post("/api/v1/plans", () => planWith(["ZZZZ11"])));
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const notice = within(result).getByText(/Sem nota: ZZZZ11/).closest("li") as HTMLElement;
    expect(within(notice).getByRole("link", { name: /Abrir Carteira/ })).toBeInTheDocument();
  });
});

describe("cartões do plano", () => {
  it("renda fixa mostra o nome no lugar do ticker, sem repetir embaixo", async () => {
    const base = samplePlan();
    server.use(
      http.post("/api/v1/plans", () =>
        HttpResponse.json({
          ...base,
          items: [
            ...base.items,
            {
              asset_id: "a-ipca",
              asset_class: "fixed_income",
              symbol: null,
              display_name: "Tesouro Selic 2031",
              quantity: 300,
              unit_price: 1,
              value: 300,
              divisible: true,
              class_percent_after: 100,
            },
          ],
        }),
      ),
    );
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const name = within(result).getByText("Tesouro Selic 2031");
    expect(name).toHaveClass("plan-card__symbol", "plan-card__symbol--name");
    expect(within(result).getAllByText("Tesouro Selic 2031")).toHaveLength(1);
    expect(within(result).getByText("Aplicar este valor")).toBeInTheDocument();
    expect(within(result).getByText("ITSA4")).not.toHaveClass("plan-card__symbol--name");
  });
});

describe("primeiro uso", () => {
  async function addFirstAsset(query: string, option: RegExp) {
    resetDb({ assets: [], goals: zeroGoals });
    const rendered = renderApp("aportar");
    const { user } = rendered;
    await user.click(await screen.findByRole("button", { name: /Montar minha carteira/ }));
    const dialog = await screen.findByRole("dialog", { name: "Montar minha carteira" });
    expect(within(dialog).getByRole("button", { name: "Escolha um perfil" })).toBeDisabled();
    await user.click(within(dialog).getByRole("radio", { name: /Moderado/ }));
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    await waitFor(() => expect(getDb().goals.fixed_income).toBe(35));
    const combobox = await within(dialog).findByRole("combobox", { name: "Ticker ou nome" });
    await user.type(combobox, query);
    await user.click(await within(dialog).findByRole("option", { name: option }));
    // Cota e ação têm o "+"; cripto (fração) tem só o campo, que o teste preenche.
    const plus = within(dialog).queryByRole("button", { name: "Aumentar quantidade" });
    if (plus) await user.click(plus);
    return { ...rendered, dialog };
  }

  const heroInput = () => screen.getByRole("textbox", { name: "Quanto você vai aportar?" });

  it("ativo de FII nasce sem nota: o passo final oferece dar a nota, e ela vira aporte", async () => {
    const { user, dialog } = await addFirstAsset("knri", /KNRI11/);
    expect(within(dialog).getByRole("radio", { name: /FIIs/ })).toHaveAttribute("aria-checked", "true");
    expect(within(dialog).getByText("Kinea Renda Imobiliária")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Adicionar e continuar" }));

    expect(await within(dialog).findByRole("heading", { name: "Falta só a nota do KNRI11" }, { timeout: 3000 })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Calcular mesmo assim" })).toBeInTheDocument();
    expect(getDb().assets).toHaveLength(1);
    await user.click(within(dialog).getByRole("button", { name: "Dar nota ao KNRI11" }));

    const rate = await screen.findByRole("dialog", { name: "KNRI11" });
    expect(within(rate).getByText("Responda o checklist para o KNRI11 ganhar nota — cada sim soma 1.")).toBeInTheDocument();
    const question = within(rate).getByRole("group", { name: "Os imóveis ficam em regiões de alta demanda?" });
    await user.click(within(question).getByRole("button", { name: "Sim" }));
    await user.click(within(rate).getByRole("button", { name: "Salvar" }));

    await waitFor(() => expect(getDb().assets[0]?.answers).toEqual({ r1: true }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
    await waitFor(() => expect(heroInput()).toHaveFocus());
  });

  it("calcular mesmo assim fecha e põe o foco no campo de valor", async () => {
    const { user, dialog } = await addFirstAsset("knri", /KNRI11/);
    await user.click(within(dialog).getByRole("button", { name: "Adicionar e continuar" }));
    await user.click(await within(dialog).findByRole("button", { name: "Calcular mesmo assim" }, { timeout: 3000 }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
    await waitFor(() => expect(heroInput()).toHaveFocus());
  });

  it("cripto já tem nota: o passo final convida a calcular, e o foco vai para o valor", async () => {
    const { user, dialog } = await addFirstAsset("eth", /ETH/);
    const quantity = within(dialog).getByLabelText("Quantidade");
    await user.clear(quantity);
    await user.type(quantity, "0,5");
    await user.click(within(dialog).getByRole("button", { name: "Adicionar e continuar" }));
    expect(await within(dialog).findByRole("heading", { name: "Sua carteira está no prumo" }, { timeout: 3000 })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Calcular meu primeiro aporte" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
    await waitFor(() => expect(heroInput()).toHaveFocus());
  });

  it("com metas já completas, o perfil pode seguir sem escolher", async () => {
    resetDb({ assets: [] });
    const { user } = renderApp("aportar");
    await user.click(await screen.findByRole("button", { name: /Montar minha carteira/ }));
    const dialog = await screen.findByRole("dialog", { name: "Montar minha carteira" });
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    expect(await within(dialog).findByRole("combobox", { name: "Ticker ou nome" })).toBeInTheDocument();
    expect(getDb().requests.some((r) => r.method === "PUT")).toBe(false);
  });
});

describe("Comprei, por ativo", () => {
  async function calculated(amount = "600000") {
    const rendered = await openCalculator();
    await rendered.user.type(rendered.input, amount);
    await rendered.user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    return { ...rendered, result };
  }

  const cardOf = (result: HTMLElement, symbol: string) =>
    within(result).getByText(symbol, { selector: ".plan-card__symbol" }).closest("li") as HTMLElement;

  const contributions = () => getDb().requests.filter((r) => r.path === "/api/v1/contributions" && r.method === "POST");

  /** Marca BTLG11 com outra quantidade (7) e outro preço (R$ 90,00). */
  async function markBtlg(user: Awaited<ReturnType<typeof calculated>>["user"]) {
    await user.click(screen.getByRole("button", { name: "Comprei BTLG11" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei BTLG11" });
    // Vem com a sugestão do plano: 8 cotas a R$ 94,55.
    expect(within(ask).getByRole("textbox", { name: "Cotas" })).toHaveValue("8");
    expect(within(ask).getByRole<HTMLInputElement>("textbox", { name: "Preço pago" }).value).toMatch(/^R\$\s94,55$/);
    await user.click(within(ask).getByRole("button", { name: "Diminuir cotas" }));
    const price = within(ask).getByRole("textbox", { name: "Preço pago" });
    await user.clear(price);
    await user.type(price, "9000");
    expect(within(ask).getByText("Total").parentElement).toHaveTextContent(/R\$\s630,00/);
    await user.click(within(ask).getByRole("button", { name: "Marcar como comprado" }));
  }

  it("cada cartão tem o próprio Comprei, com o nome do ativo, e nada vem marcado", async () => {
    const { result } = await calculated();
    for (const symbol of ["ITSA4", "BTLG11", "TRXF11", "BTC"]) {
      expect(within(result).getByRole("button", { name: `Comprei ${symbol}` })).toBeEnabled();
    }
    expect(within(result).queryByText(/Comprado:/)).toBeNull();
    // Sem nenhum marcado, registrar fica desligado.
    expect(within(result).getByRole("button", { name: "Registrar comprados" })).toBeDisabled();
  });

  it("marcar um: o cartão diz o que foi comprado e o que o plano sugeria", async () => {
    const { user, result } = await calculated();
    await markBtlg(user);

    const card = cardOf(result, "BTLG11");
    expect(card).toHaveTextContent(/Comprado: 7 cotas × R\$\s90,00/);
    // A diferença do plano fica à vista, em texto.
    expect(card).toHaveTextContent(/Plano: 8 cotas × R\$\s94,55/);
    expect(card).toHaveClass("is-marked", "is-different");
    expect(within(card).queryByRole("button", { name: "Comprei BTLG11" })).toBeNull();
    // O foco vai para o botão que nasceu no lugar do que foi usado.
    expect(within(card).getByRole("button", { name: "Editar a compra de BTLG11" })).toHaveFocus();

    expect(within(result).getByRole("button", { name: "Registrar 1 comprado" })).toBeEnabled();
    expect(within(result).getByText(/1 de 4 marcados · 3 ficam de fora deste registro/)).toBeInTheDocument();
    // Os outros seguem como estavam.
    expect(within(cardOf(result, "ITSA4")).getByRole("button", { name: "Comprei ITSA4" })).toBeInTheDocument();
  });

  it("a compra igual à do plano não acusa diferença", async () => {
    const { user, result } = await calculated();
    await user.click(screen.getByRole("button", { name: "Comprei ITSA4" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei ITSA4" });
    await user.click(within(ask).getByRole("button", { name: "Marcar como comprado" }));
    const card = cardOf(result, "ITSA4");
    expect(card).toHaveTextContent(/Comprado: 243 ações × R\$\s11,20/);
    expect(card).not.toHaveTextContent(/Plano:/);
    expect(card).not.toHaveClass("is-different");
  });

  it("editar reabre com o que foi marcado, e desfazer devolve o Comprei", async () => {
    const { user, result } = await calculated();
    await markBtlg(user);

    await user.click(screen.getByRole("button", { name: "Editar a compra de BTLG11" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei BTLG11" });
    expect(within(ask).getByRole("textbox", { name: "Cotas" })).toHaveValue("7");
    expect(within(ask).getByRole<HTMLInputElement>("textbox", { name: "Preço pago" }).value).toMatch(/^R\$\s90,00$/);
    await user.click(within(ask).getByRole("button", { name: "Aumentar cotas" }));
    await user.click(within(ask).getByRole("button", { name: "Salvar" }));
    expect(cardOf(result, "BTLG11")).toHaveTextContent(/Comprado: 8 cotas × R\$\s90,00/);

    await user.click(screen.getByRole("button", { name: "Desfazer a compra de BTLG11" }));
    const card = cardOf(result, "BTLG11");
    expect(within(card).getByRole("button", { name: "Comprei BTLG11" })).toHaveFocus();
    expect(card).not.toHaveTextContent(/Comprado:/);
    expect(within(result).getByRole("button", { name: "Registrar comprados" })).toBeDisabled();
  });

  it("fechar a pergunta sem confirmar não marca nada", async () => {
    const { user, result } = await calculated();
    await user.click(screen.getByRole("button", { name: "Comprei BTC" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei BTC" });
    // Cripto aceita fração: sem − e +.
    expect(within(ask).queryByRole("button", { name: /Diminuir/ })).toBeNull();
    expect(within(ask).getByRole("textbox", { name: "Quantidade" })).toHaveValue("0,0045");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(within(result).queryByText(/Comprado:/)).toBeNull();
    expect(screen.getByRole("button", { name: "Comprei BTC" })).toHaveFocus();
  });

  it("quantidade zerada não deixa marcar", async () => {
    const { user } = await calculated();
    await user.click(screen.getByRole("button", { name: "Comprei ITSA4" }));
    const ask = await screen.findByRole("dialog", { name: "Comprei ITSA4" });
    const quantity = within(ask).getByRole("textbox", { name: "Ações" });
    await user.clear(quantity);
    await user.type(quantity, "0");
    expect(within(ask).getByRole("button", { name: "Marcar como comprado" })).toBeDisabled();
  });

  it("Comprei todos como sugerido marca todos; com algum marcado, só completa os outros", async () => {
    const { user, result } = await calculated();
    await markBtlg(user);
    // Já há um marcado, com a quantidade da pessoa: o atalho só completa os outros.
    await user.click(within(result).getByRole("button", { name: "Marcar os demais como sugerido" }));
    expect(cardOf(result, "BTLG11")).toHaveTextContent(/Comprado: 7 cotas × R\$\s90,00/);
    expect(cardOf(result, "ITSA4")).toHaveTextContent(/Comprado: 243 ações × R\$\s11,20/);
    expect(within(result).getByRole("button", { name: "Registrar 4 comprados" })).toBeEnabled();
    expect(within(result).getByText("4 de 4 marcados")).toBeInTheDocument();
    expect(within(result).queryByRole("button", { name: /como sugerido/ })).toBeNull();
  });

  it("registra só os marcados, com o corpo exato, e os de fora não entram", async () => {
    vi.mocked(confetti).mockClear();
    const itsaBefore = getDb().assets.find((a) => a.id === "a-itsa")?.quantity;
    const { user, result } = await calculated();
    await markBtlg(user);
    await user.click(screen.getByRole("button", { name: "Comprei BTC" }));
    await user.click(await screen.findByRole("button", { name: "Marcar como comprado" }));

    await user.click(within(result).getByRole("button", { name: "Registrar 2 comprados" }));
    const dialog = await screen.findByRole("dialog", { name: "Registrar aporte" });
    // A conferência lista só os dois, com o total, e diz quantos ficaram de fora.
    expect(within(dialog).getAllByRole("listitem")).toHaveLength(2);
    expect(within(dialog).getByText("BTLG11")).toBeInTheDocument();
    expect(within(dialog).getByText("BTC")).toBeInTheDocument();
    expect(within(dialog).queryByText("ITSA4")).toBeNull();
    expect(within(dialog).getByText("Total").parentElement).toHaveTextContent(/R\$\s2\.185,15/);
    expect(dialog).toHaveTextContent(/2 ativos ficaram de fora deste registro/);
    await user.click(within(dialog).getByRole("button", { name: "Registrar aporte" }));

    expect(await screen.findByRole("heading", { name: "Aporte registrado!" })).toBeInTheDocument();
    expect(contributions()).toHaveLength(1);
    expect(contributions()[0]?.body).toEqual({
      amount: 2185.15,
      items: [
        { asset_id: "a-btlg", quantity: 7, unit_price: 90 },
        { asset_id: "a-btc", quantity: 0.0045, unit_price: 345588.05 },
      ],
    });
    expect(getDb().assets.find((a) => a.id === "a-itsa")?.quantity).toBe(itsaBefore);
    expect(confetti).toHaveBeenCalled();
    expect(await screen.findByText(/Aporte registrado\. A carteira já está atualizada\./)).toBeInTheDocument();
  });

  it("recalcular o plano limpa as marcas", async () => {
    const { user, result } = await calculated();
    await markBtlg(user);
    expect(within(result).getByRole("button", { name: "Registrar 1 comprado" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: /R\$\s500$/ }));
    await waitFor(() => expect(getDb().requests.filter((r) => r.path === "/api/v1/plans")).toHaveLength(2));
    const again = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(await within(again).findByRole("button", { name: "Registrar comprados" })).toBeDisabled();
    expect(within(again).queryByText(/Comprado:/)).toBeNull();
    expect(within(again).getByRole("button", { name: "Comprei BTLG11" })).toBeInTheDocument();
  });

  it("mudar o valor descarta o plano e as marcas junto", async () => {
    const { user, input } = await calculated();
    await markBtlg(user);
    await user.type(input, "1");
    await waitFor(() => expect(screen.queryByRole("region", { name: "Resultado do aporte" })).toBeNull(), { timeout: 3000 });
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const again = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(again).queryByText(/Comprado:/)).toBeNull();
  });
});

describe("A carteira ao lado do campo", () => {
  it("antes do cálculo mostra a carteira de hoje, sem alternância", async () => {
    await openCalculator();
    expect(await screen.findByRole("heading", { name: "A carteira hoje" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Comparar" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Resultado do aporte" })).toBeNull();
  });

  it("com o plano, o painel alterna antes e depois e vem antes do que comprar", async () => {
    const { user, input } = await openCalculator();
    await user.type(input, "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));
    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    expect(within(result).getByRole("group", { name: "Comparar" })).toBeInTheDocument();
    const headings = within(result)
      .getAllByRole("heading")
      .map((h) => h.textContent ?? "");
    expect(headings.findIndex((t) => /A carteira/.test(t))).toBeLessThan(headings.findIndex((t) => /Compre 4/.test(t)));
  });
});
