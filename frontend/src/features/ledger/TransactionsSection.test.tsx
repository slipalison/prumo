import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { MASK_BRL, MASK_QUANTITY } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";
import { formatSignedBRL, formatSignedPercent, numberText } from "./format";
import { formatTradeDate, kindLabel, kindsFor, priceLabel, quantityDelta, quantityHint, todayIso } from "./kinds";

async function openAsset(symbol: RegExp, name: string) {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  await rendered.user.click(screen.getByRole("button", { name: symbol }));
  const dialog = await screen.findByRole("dialog", { name });
  const ledger = within(dialog).getByRole("region", { name: "Lançamentos" });
  return { ...rendered, dialog, ledger };
}

const lastRequest = (method: string, path: string) =>
  getDb().requests.filter((r) => r.method === method && r.path === path).at(-1);

describe("Lançamentos", () => {
  it("a linha da Carteira mostra preço médio e rentabilidade, só quando há custo", async () => {
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const stocks = screen.getByRole("region", { name: "Ações Nacionais" });
    const itsa = within(stocks).getByRole("button", { name: /ITSA4/ });
    expect(within(itsa).getByText(/2\.500 ações · R\$\s11,20 · PM R\$\s10,00/)).toBeInTheDocument();
    // 2.500 × 11,20 = 28.000 sobre o custo de 25.000. O sinal escrito diz o sentido.
    expect(within(itsa).getByText("+12,0%")).toHaveAttribute("title", "Rentabilidade sobre o preço médio");
    expect(within(itsa).getByText("rentabilidade")).toHaveClass("sr-only");
    const hglg = within(screen.getByRole("region", { name: "Fundos Imobiliários" })).getByRole("button", { name: /HGLG11/ });
    expect(within(hglg).queryByText(/PM/)).toBeNull();
    expect(within(hglg).queryByText("rentabilidade")).toBeNull();
  });

  it("mostra o livro e a posição do ativo", async () => {
    const { ledger } = await openAsset(/KNRI11/, "KNRI11");
    expect(await within(ledger).findByText("8 cotas × R$ 125,00")).toBeInTheDocument();
    const rows = within(ledger).getAllByRole("listitem");
    // Do mais novo para o mais antigo.
    expect(rows[0]).toHaveTextContent(/Compra\s*do aporte/);
    expect(rows[0]).toHaveTextContent("10/02/2026");
    expect(rows[1]).toHaveTextContent("Saldo inicial");
    expect(rows[1]).toHaveTextContent("03/03/2025");
    expect(within(ledger).getByText("Preço médio").nextSibling).toHaveTextContent("R$ 130,00");
    expect(within(ledger).getByText("Custo").nextSibling).toHaveTextContent("R$ 1.300,00");
    expect(within(ledger).getByText("Resultado").nextSibling).toHaveTextContent("+R$ 120,00 (+9,2%)");
    expect(within(ledger).getByText("Lucro realizado").nextSibling).toHaveTextContent("R$ 0,00");
  });

  it("adiciona uma compra e a quantidade da folha acompanha o livro", async () => {
    const { user, dialog, ledger } = await openAsset(/KNRI11/, "KNRI11");
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const form = within(ledger).getByRole("form", { name: "Novo lançamento" });
    expect(within(form).getByLabelText("Tipo")).toHaveValue("buy");
    expect(within(form).getByLabelText("Data")).toHaveValue(todayIso());
    fireEvent.change(within(form).getByLabelText("Data"), { target: { value: "2026-03-02" } });
    await user.type(within(form).getByLabelText("Quantidade"), "5");
    await user.type(within(form).getByLabelText("Preço por unidade"), "140");
    await user.type(within(form).getByLabelText("Taxas (corretagem, emolumentos)"), "1,50");
    await user.click(within(form).getByRole("button", { name: "Adicionar lançamento" }));

    expect(await screen.findByText("Lançamento salvo.")).toBeInTheDocument();
    expect(lastRequest("POST", "/api/v1/transactions")?.body).toEqual({
      asset_id: "a-knri",
      kind: "buy",
      trade_date: "2026-03-02",
      quantity: 5,
      unit_price: 140,
      fees: 1.5,
      currency: "BRL",
      fx_rate: null,
    });
    // A quantidade é a soma do livro: o campo da folha passa a 15, e salvar a
    // folha depois não grava um ajuste desfazendo a compra.
    await waitFor(() => expect(within(dialog).getByLabelText("Quantidade")).toHaveValue("15"));
    expect(within(ledger).queryByRole("form")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
    expect(lastRequest("PATCH", "/api/v1/assets/a-knri")).toBeUndefined();
    expect(screen.getByText(/15 cotas · R\$\s142,00/)).toBeInTheDocument();
  });

  it("venda maior que a posição: o servidor recusa e a tela diz por quê", async () => {
    const { user, ledger } = await openAsset(/KNRI11/, "KNRI11");
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const form = within(ledger).getByRole("form", { name: "Novo lançamento" });
    await user.selectOptions(within(form).getByLabelText("Tipo"), "sell");
    await user.type(within(form).getByLabelText("Quantidade"), "100");
    await user.type(within(form).getByLabelText("Preço por unidade"), "150");
    await user.click(within(form).getByRole("button", { name: "Adicionar lançamento" }));
    expect(await within(form).findByRole("alert")).toHaveTextContent("deixaria a posição negativa");
    expect(getDb().assets.find((a) => a.id === "a-knri")?.quantity).toBe(10);
  });

  it("confere o formulário antes de mandar e explica cada tipo", async () => {
    const { user, ledger } = await openAsset(/KNRI11/, "KNRI11");
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const form = within(ledger).getByRole("form", { name: "Novo lançamento" });
    const submit = within(form).getByRole("button", { name: "Adicionar lançamento" });
    await user.click(submit);
    expect(within(form).getByRole("alert")).toHaveTextContent("Informe a quantidade.");
    await user.type(within(form).getByLabelText("Quantidade"), "2");
    await user.click(submit);
    expect(within(form).getByRole("alert")).toHaveTextContent("Informe o preço.");
    // Desdobramento não tem preço nem taxa, e diz o que a quantidade é.
    await user.selectOptions(within(form).getByLabelText("Tipo"), "split");
    expect(within(form).queryByLabelText("Preço por unidade")).toBeNull();
    expect(within(form).queryByLabelText(/Taxas/)).toBeNull();
    expect(within(form).getByLabelText("Quantidade")).toHaveAccessibleDescription(/cotas novas/);
    await user.selectOptions(within(form).getByLabelText("Tipo"), "bonus");
    expect(within(form).getByLabelText("Custo atribuído por cota (opcional)")).toBeInTheDocument();
    await user.click(within(form).getByRole("button", { name: "Cancelar" }));
    expect(within(ledger).queryByRole("form")).toBeNull();
    expect(getDb().requests.some((r) => r.method === "POST" && r.path === "/api/v1/transactions")).toBe(false);
  });

  it("saldo inicial sem custo: a folha pede o preço médio e a rentabilidade aparece", async () => {
    const { user, ledger } = await openAsset(/HGLG11/, "HGLG11");
    expect(within(ledger).getByText(/Informe o preço médio para ver a rentabilidade/)).toBeInTheDocument();
    expect(within(ledger).getByText("Preço médio").nextSibling).toHaveTextContent("—");
    await user.click(await within(ledger).findByRole("button", { name: "Informar o preço médio" }));
    const form = within(ledger).getByRole("form", { name: "Editar lançamento" });
    const price = within(form).getByLabelText("Preço médio (deixe vazio se não souber)");
    await waitFor(() => expect(price).toHaveFocus());
    await user.type(price, "120");
    await user.click(within(form).getByRole("button", { name: "Salvar lançamento" }));
    expect(await screen.findByText("Lançamento atualizado.")).toBeInTheDocument();
    expect(lastRequest("PATCH", "/api/v1/transactions/t-hglg-1")?.body).toEqual({
      kind: "opening",
      trade_date: "2025-01-15",
      quantity: 12,
      unit_price: 120,
      fees: 0,
      currency: "BRL",
      fx_rate: null,
    });
    await waitFor(() => expect(within(ledger).getByText("Preço médio").nextSibling).toHaveTextContent("R$ 120,00"));
    expect(within(ledger).queryByText(/Informe o preço médio/)).toBeNull();
    // 12 × 140,80 − 1.440
    expect(within(ledger).getByText("Resultado").nextSibling).toHaveTextContent("+R$ 249,60 (+17,3%)");
  });

  it("edita um lançamento e apaga outro com confirmação", async () => {
    const { user, ledger } = await openAsset(/KNRI11/, "KNRI11");
    await user.click(await within(ledger).findByRole("button", { name: "Editar Compra de 10/02/2026" }));
    const form = within(ledger).getByRole("form", { name: "Editar lançamento" });
    const quantity = within(form).getByLabelText("Quantidade");
    expect(quantity).toHaveValue("2");
    await user.clear(quantity);
    await user.type(quantity, "4");
    await user.click(within(form).getByRole("button", { name: "Salvar lançamento" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/transactions/t-knri-2")?.body).toMatchObject({ quantity: 4, unit_price: 150 }));
    expect(await within(ledger).findByText("4 cotas × R$ 150,00")).toBeInTheDocument();

    await user.click(within(ledger).getByRole("button", { name: "Apagar Saldo inicial de 03/03/2025" }));
    const confirm = within(ledger).getByRole("group", { name: "Apagar Saldo inicial de 03/03/2025" });
    await user.click(within(confirm).getByRole("button", { name: "Cancelar" }));
    expect(within(ledger).queryByRole("group")).toBeNull();
    await user.click(within(ledger).getByRole("button", { name: "Apagar Saldo inicial de 03/03/2025" }));
    await user.click(within(within(ledger).getByRole("group")).getByRole("button", { name: "Apagar" }));
    expect(await screen.findByText("Lançamento apagado.")).toBeInTheDocument();
    expect(lastRequest("DELETE", "/api/v1/transactions/t-knri-1")).toBeDefined();
    await waitFor(() => expect(within(ledger).getAllByRole("listitem")).toHaveLength(1));
  });

  it("apagar o que sustenta uma venda é recusado, com o motivo", async () => {
    server.use(
      http.delete("/api/v1/transactions/:id", () =>
        HttpResponse.json(
          { code: "exceeds_position", status: 422, title: "O lancamento passa da posicao", detail: "A venda de 2026-03-10 (15) passa da posicao de 10 naquela data." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user, ledger } = await openAsset(/KNRI11/, "KNRI11");
    await user.click(await within(ledger).findByRole("button", { name: "Apagar Compra de 10/02/2026" }));
    await user.click(within(within(ledger).getByRole("group")).getByRole("button", { name: "Apagar" }));
    expect(await within(ledger).findByRole("alert")).toHaveTextContent("deixaria a posição negativa");
  });

  it("renda fixa por valor: aplicação, resgate e atualização, sem preço por unidade", async () => {
    const { user, ledger } = await openAsset(/Tesouro IPCA\+ 2035/, "Tesouro IPCA+ 2035");
    expect(await within(ledger).findByText(/Nenhum lançamento ainda/)).toBeInTheDocument();
    expect(within(ledger).queryByText("Preço médio")).toBeNull();
    expect(within(ledger).getByText("Aplicado")).toBeInTheDocument();
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const form = within(ledger).getByRole("form", { name: "Novo lançamento" });
    const options = within(within(form).getByLabelText("Tipo")).getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(["Aplicação", "Resgate", "Atualização do saldo", "Saldo inicial"]);
    expect(within(form).queryByLabelText("Preço por unidade")).toBeNull();
    await user.selectOptions(within(form).getByLabelText("Tipo"), "opening");
    await user.type(within(form).getByLabelText("Valor (R$)"), "85000");
    await user.click(within(form).getByRole("switch", { name: /Esse saldo é o que você aplicou/ }));
    await user.click(within(form).getByRole("button", { name: "Adicionar lançamento" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/transactions")?.body).toMatchObject({ kind: "opening", quantity: 85000, unit_price: 1 }),
    );
    expect(await within(ledger).findByRole("listitem")).toHaveTextContent("R$ 85.000,00");
    // O custo passou a ser conhecido: o aplicado aparece no resumo.
    expect(within(ledger).getByText("Aplicado").nextSibling).toHaveTextContent("R$ 85.000,00");
  });

  it("ativo em dólar: preço na moeda original com o câmbio do dia", async () => {
    getDb().assets.push({
      id: "a-aapl",
      asset_class: "intl_stocks",
      symbol: "AAPL",
      display_name: "Apple",
      quantity: 0,
      quote_price: 1100,
      manual_price: null,
      currency: "USD",
      score_override: 7,
      answers: {},
    });
    const { user, ledger } = await openAsset(/AAPL/, "AAPL");
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const form = within(ledger).getByRole("form", { name: "Novo lançamento" });
    expect(within(form).getByLabelText("Moeda do preço")).toHaveValue("USD");
    await user.type(within(form).getByLabelText("Quantidade"), "2");
    await user.type(within(form).getByLabelText("Preço por unidade"), "180,5");
    await user.click(within(form).getByRole("button", { name: "Adicionar lançamento" }));
    expect(within(form).getByRole("alert")).toHaveTextContent("Informe o câmbio: quantos reais valia 1 USD no dia.");
    await user.type(within(form).getByLabelText("Câmbio (R$ por 1 USD)"), "5,1");
    await user.click(within(form).getByRole("button", { name: "Adicionar lançamento" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/transactions")?.body).toMatchObject({ currency: "USD", fx_rate: 5.1, unit_price: 180.5 }),
    );
    expect(await within(ledger).findByText(/2 ações × USD 180,5 · câmbio R\$\s5,10/)).toBeInTheDocument();
    // Já em reais: sem câmbio.
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const again = within(ledger).getByRole("form", { name: "Novo lançamento" });
    await user.selectOptions(within(again).getByLabelText("Moeda do preço"), "BRL");
    expect(within(again).queryByLabelText(/Câmbio/)).toBeNull();
  });

  it("sem conseguir ler o livro, diz que não carregou", async () => {
    server.use(http.get("/api/v1/transactions", () => new HttpResponse(null, { status: 500 })));
    const { ledger } = await openAsset(/KNRI11/, "KNRI11");
    expect(await within(ledger).findByText("Não consegui carregar os lançamentos agora.")).toBeInTheDocument();
  });

  it("o cadastro aceita o preço médio de entrada e, na renda fixa, o aplicado", async () => {
    const rendered = renderApp("carteira");
    const { user } = rendered;
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    let dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "kncr");
    await user.click(await within(dialog).findByRole("option", { name: /KNCR11/ }));
    // Sem quantidade, não há preço médio para informar.
    expect(within(dialog).queryByLabelText("Preço médio (opcional)")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Aumentar quantidade" }));
    await user.type(within(dialog).getByLabelText("Preço médio (opcional)"), "9850");
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toMatchObject({ symbol: "KNCR11", quantity: 1, average_price: 98.5 }),
    );
    const edit = await screen.findByRole("dialog", { name: "KNCR11" });
    await user.click(within(edit).getByRole("button", { name: "Fechar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });

    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.click(within(dialog).getByRole("radio", { name: /Renda Fixa$/ }));
    await user.type(within(dialog).getByLabelText("Nome"), "CDB ficticio");
    await user.type(within(dialog).getByLabelText("Quanto você tem hoje"), "5200000");
    await user.type(within(dialog).getByLabelText("Quanto você aplicou (opcional)"), "5000000");
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toMatchObject({ amount: 52000, invested: 50000 }),
    );
  });
});

describe("Lançamentos com os valores ocultos", () => {
  /** Qualquer R$ seguido de dígito é valor vazando. */
  const MONEY = /R\$\s*\d/;

  /** O texto, os atributos que o leitor de tela lê e o valor dos campos. */
  function heard(root: HTMLElement): string[] {
    const out: string[] = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) out.push(node.textContent ?? "");
    for (const element of root.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "aria-valuetext", "title", "alt", "placeholder"]) {
        const value = element.getAttribute(attribute);
        if (value) out.push(value);
      }
      if (element instanceof HTMLInputElement) out.push(element.value);
    }
    return out;
  }

  it("PM, custo, resultado, lucro e o livro saem mascarados; a rentabilidade em % fica", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const rendered = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const stocks = screen.getByRole("region", { name: "Ações Nacionais" });
    const itsa = within(stocks).getByRole("button", { name: /ITSA4/ });
    expect(itsa).toHaveTextContent(`PM ${MASK_BRL}`);
    expect(within(itsa).getByText("+12,0%")).toBeInTheDocument();
    for (const text of heard(itsa)) expect(text).not.toMatch(MONEY);

    await rendered.user.click(screen.getByRole("button", { name: /KNRI11/ }));
    const dialog = await screen.findByRole("dialog", { name: "KNRI11" });
    const ledger = within(dialog).getByRole("region", { name: "Lançamentos" });
    expect(await within(ledger).findAllByText(`${MASK_QUANTITY} cotas × ${MASK_BRL}`)).toHaveLength(2);
    expect(within(ledger).getByText("Preço médio").nextSibling).toHaveTextContent(MASK_BRL);
    expect(within(ledger).getByText("Custo").nextSibling).toHaveTextContent(MASK_BRL);
    // O ganho em reais some; o percentual fica, como o "% da classe".
    expect(within(ledger).getByText("Resultado").nextSibling).toHaveTextContent(`${MASK_BRL} (+9,2%)`);
    expect(within(ledger).getByText("Lucro realizado").nextSibling).toHaveTextContent(MASK_BRL);
    for (const text of heard(ledger)) {
      expect(text).not.toMatch(MONEY);
      expect(text).not.toMatch(/[0-9]+ cotas/);
    }

    // No formulário, fora do foco, os campos também mascaram; no foco, o número.
    await rendered.user.click(within(ledger).getByRole("button", { name: "Editar Compra de 10/02/2026" }));
    const form = within(ledger).getByRole("form", { name: "Editar lançamento" });
    const quantity = within(form).getByLabelText("Quantidade");
    expect(quantity).toHaveValue(MASK_QUANTITY);
    expect(within(form).getByLabelText("Preço por unidade")).toHaveValue(MASK_BRL);
    for (const text of heard(form)) expect(text).not.toMatch(MONEY);
    await rendered.user.click(quantity);
    expect(quantity).toHaveValue("2");
    await rendered.user.clear(quantity);
    await rendered.user.type(quantity, "3");
    await rendered.user.click(within(form).getByRole("button", { name: "Salvar lançamento" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/transactions/t-knri-2")?.body).toMatchObject({ quantity: 3, unit_price: 150 }));
  });
});

describe("Lançamentos de título do Tesouro", () => {
  it("fração em centésimos e o substantivo do título", async () => {
    getDb().assets.push({
      id: "a-td",
      asset_class: "fixed_income",
      symbol: "TD-SELIC-20290301",
      display_name: "Tesouro Selic 2029",
      quantity: 0,
      quote_price: 16000.5,
      manual_price: null,
      currency: "BRL",
      score_override: 6,
      answers: {},
      treasury: true,
    });
    const { user, ledger } = await openAsset(/Tesouro Selic 2029/, "Tesouro Selic 2029");
    // Título é por unidade: tem preço médio, compra, venda e preço por título.
    expect(within(ledger).getByText("Preço médio")).toBeInTheDocument();
    await user.click(within(ledger).getByRole("button", { name: /Adicionar/ }));
    const form = within(ledger).getByRole("form", { name: "Novo lançamento" });
    expect(within(within(form).getByLabelText("Tipo")).getAllByRole("option").map((o) => o.textContent)).toContain("Venda");
    await user.type(within(form).getByLabelText("Quantidade"), "0,35");
    await user.type(within(form).getByLabelText("Preço por unidade"), "2450");
    await user.click(within(form).getByRole("button", { name: "Adicionar lançamento" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/transactions")?.body).toMatchObject({ asset_id: "a-td", quantity: 0.35, unit_price: 2450 }),
    );
    expect(await within(ledger).findByText("0,35 títulos × R$ 2.450,00")).toBeInTheDocument();
  });
});

describe("regras do livro na tela", () => {
  it("nomes, tipos, sinais e datas", () => {
    expect(kindLabel("buy", "value")).toBe("Aplicação");
    expect(kindLabel("adjustment", "value")).toBe("Atualização do saldo");
    expect(kindLabel("reverse_split", "unit")).toBe("Grupamento");
    expect(kindsFor("fraction")).toHaveLength(9);
    expect(priceLabel("amortization")).toBe("Valor recebido por cota");
    expect(priceLabel("sell")).toBe("Preço por unidade");
    expect(quantityHint("buy", "unit")).toBeUndefined();
    expect(quantityHint("adjustment", "value")).toMatch(/saldo/);
    expect(quantityHint("adjustment", "unit")).toMatch(/quantidade muda/);
    expect(quantityHint("reverse_split", "unit")).toMatch(/deixaram de existir/);
    expect(quantityHint("amortization", "unit")).toMatch(/custo cai/);
    expect([
      quantityDelta({ kind: "sell", quantity: 3 }),
      quantityDelta({ kind: "amortization", quantity: 3 }),
      quantityDelta({ kind: "adjustment", quantity: -2 }),
      quantityDelta({ kind: "split", quantity: 3 }),
    ]).toEqual([-3, 0, -2, 3]);
    expect(todayIso(new Date(2026, 0, 5))).toBe("2026-01-05");
    expect(formatTradeDate("2026-03-02")).toBe("02/03/2026");
    expect(formatTradeDate("ontem")).toBe("ontem");
    expect(formatSignedBRL(12)).toMatch(/^\+R\$\s12,00$/);
    expect(formatSignedBRL(-3)).toMatch(/^−R\$\s3,00$/);
    expect(formatSignedBRL(0)).toMatch(/^R\$\s0,00$/);
    expect([formatSignedPercent(12.34), formatSignedPercent(-4), formatSignedPercent(0)]).toEqual(["+12,3%", "−4,0%", "0,0%"]);
    expect(numberText(1234.5)).toBe("1234,5");
  });
});
