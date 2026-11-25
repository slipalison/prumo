import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";
import { getTaxDb, sampleMonths } from "../../test/taxHandlers";

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

async function openTaxes() {
  const rendered = renderApp("ajustes/impostos");
  await screen.findByRole("heading", { name: "Imposto de renda" });
  await screen.findByRole("region", { name: "Isentômetro" });
  return rendered;
}

const requests = (method: string, prefix: string) =>
  getDb().requests.filter((r) => r.method === method && r.path.startsWith(prefix));

afterEach(() => vi.restoreAllMocks());

describe("Impostos", () => {
  it("entra pela seção de Ajustes com o aviso de responsabilidade sempre à vista", async () => {
    const { user } = renderApp("ajustes");
    await user.click(await screen.findByRole("link", { name: "Impostos" }));
    expect(await screen.findByRole("heading", { name: "Imposto de renda" })).toBeInTheDocument();
    const note = screen.getByRole("note", { name: "Aviso de responsabilidade" });
    expect(note).toHaveTextContent(/não substitui o contador/);
    expect(note).toHaveTextContent(/responsabilidade pelo que vai para a Receita é de quem declara/);
  });

  it("mostra o isentômetro do mês de hoje com a barra e o day trade a confirmar", async () => {
    await openTaxes();
    const meter = screen.getByRole("region", { name: "Isentômetro" });
    expect(meter).toHaveTextContent("novembro de 2026");
    expect(meter).toHaveTextContent(/R\$\s12\.000,00 em vendas de ações, de R\$\s20\.000,00 isentos/);
    expect(within(meter).getByRole("meter")).toHaveAttribute("aria-valuenow", "60");
    expect(meter).toHaveTextContent("60,0% do limite");
    expect(meter).toHaveTextContent(/Ainda cabem R\$\s8\.000,00 em vendas/);
    expect(within(meter).getByText(/Ponto a confirmar com contador/).parentElement).toHaveTextContent(
      /vendas em day trade entraram no total/,
    );
  });

  it("isentômetro estourado diz que o lucro paga 15%", async () => {
    server.use(
      http.get("/api/v1/tax/months", () => {
        const data = sampleMonths(2026);
        const november = data.months[10];
        const exemption = november?.exemptions[0];
        if (exemption) Object.assign(exemption, { sales: 25000, remaining: 0, exempt: false });
        return HttpResponse.json(data);
      }),
    );
    await openTaxes();
    const meter = screen.getByRole("region", { name: "Isentômetro" });
    expect(meter).toHaveTextContent("Passou do limite: o lucro das ações neste mês paga 15%.");
    expect(within(meter).getByRole("meter")).toHaveAttribute("aria-valuenow", "125");
  });

  it("lista os DARF do ano com vencimento e marca e desmarca como pago", async () => {
    const { user } = await openTaxes();
    const table = screen.getByRole("region", { name: "DARF por mês em 2026" });
    const may = within(table).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent(/6015 · R\$\s450,00/);
    expect(may).toHaveTextContent("30/06/2026");
    expect(may).toHaveTextContent("A pagar");
    const march = within(table).getByRole("row", { name: /^março/ });
    expect(march).toHaveTextContent("6015: abaixo do mínimo, soma no mês seguinte");
    const september = within(table).getByRole("row", { name: /^setembro/ });
    expect(september).toHaveTextContent(/4600 · R\$\s750,00/);
    expect(september).toHaveTextContent("30/10/2026");
    const april = within(table).getByRole("row", { name: /^abril/ });
    expect(april).toHaveTextContent("Pago");

    await user.click(within(may).getByRole("button", { name: "Marcar o DARF 6015 de maio como pago" }));
    expect(await screen.findByText("DARF 6015 de maio marcado como pago.")).toBeInTheDocument();
    expect(requests("POST", "/api/v1/tax/darfs/2026/5/paid").at(-1)?.body).toEqual({ code: "6015" });
    await waitFor(() =>
      expect(within(screen.getByRole("row", { name: /^maio/ })).getByRole("button", { name: /Desmarcar/ })).toBeInTheDocument(),
    );

    await user.click(within(april).getByRole("button", { name: "Desmarcar o DARF 6015 de abril" }));
    expect(await screen.findByText("DARF 6015 de abril voltou para a pagar.")).toBeInTheDocument();
    expect(requests("DELETE", "/api/v1/tax/darfs/2026/4/paid?code=6015")).toHaveLength(1);
    expect(getTaxDb().paid.has("2026-4-6015")).toBe(false);
  });

  it("avisa quando o DARF marcado mudou de valor e quando a marca falha", async () => {
    getTaxDb().paid.set("2026-5-6015", { amount: 400, paid_at: "2026-06-20T12:00:00Z" });
    server.use(
      http.delete("/api/v1/tax/darfs/:year/:month/paid", () =>
        HttpResponse.json(
          { title: "Erro interno", status: 500, code: "internal", detail: "x" },
          { status: 500, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openTaxes();
    const may = within(screen.getByRole("region", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent("o valor mudou depois de marcar: confira");
    await user.click(within(may).getByRole("button", { name: "Desmarcar o DARF 6015 de maio" }));
    expect(await screen.findByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
  });

  it("abre a apuração de cada mês com grupo, compensação e as dúvidas que tocam o número", async () => {
    const { user } = await openTaxes();
    await user.click(screen.getByText("Maio", { selector: "summary" }));
    const may = screen.getByRole("region", { name: "Apuração de maio" });
    const row = within(may).getByRole("row", { name: /Ações, ETF e BDR/ });
    expect(row).toHaveTextContent(/R\$\s3\.000,00/);
    expect(row).toHaveTextContent("15%");
    expect(row).toHaveTextContent(/R\$\s450,00/);
    expect(screen.getByRole("list", { name: "Vendas de maio" })).toHaveTextContent(/Ações: vendas de R\$\s24\.000,00/);

    await user.click(screen.getByText("Setembro", { selector: "summary" }));
    expect(screen.getByLabelText("Cripto em setembro")).toHaveTextContent(/imposto de R\$\s750,00/);
    await user.click(screen.getByText("Agosto", { selector: "summary" }));
    expect(screen.getByText(/só compensa com lucro de Fiagro/)).toBeInTheDocument();
    expect(screen.getByText(/Prejuízo a compensar em 31\/12/)).toHaveTextContent(/Fiagro R\$\s100,00/);
  });

  it("monta a declaração para copiar: Bens e Direitos, isentos e JCP", async () => {
    const { user } = await openTaxes();
    // Depois do `userEvent.setup()`, que põe a área de transferência dele.
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const goods = await screen.findByRole("list", { name: "Bens e Direitos" });
    const itsa = within(goods).getByText("ITSA4").closest("li") as HTMLElement;
    expect(itsa).toHaveTextContent("Grupo 03 · Código 01 · Ações (inclusive as listadas em bolsa)");
    expect(itsa).toHaveTextContent(/2\.500 unidades · situação em 31\/12: R\$\s25\.000,00/);
    const btc = within(goods).getByText("BTC").closest("li") as HTMLElement;
    expect(btc).toHaveTextContent("0,05 unidades");
    await user.click(within(itsa).getByRole("button", { name: "Copiar a linha de ITSA4" }));
    expect(writeText).toHaveBeenCalledWith(expect.stringMatching(/ITSA4 \(Ação ou unit\): 2\.500 unidades\. Grupo 03, código 01\. Situação em 31\/12: R\$\s25\.000,00/));
    expect(await screen.findByText("Linha de ITSA4 copiada.")).toBeInTheDocument();

    const exempt = screen.getByRole("list", { name: "Rendimentos isentos" });
    expect(within(exempt).getByText("Código 09").closest("li")).toHaveTextContent(/R\$\s300,00/);
    expect(within(exempt).getByText("Código 99").closest("li")).toHaveTextContent("Outros: rendimentos de FII e Fiagro");
    const exclusive = screen.getByRole("list", { name: "Tributação exclusiva" });
    expect(exclusive).toHaveTextContent(/Líquido R\$\s165,00 \(bruto R\$\s200,00, retido R\$\s35,00\)/);
    expect(screen.getByText(/O JCP foi levado pelo valor líquido/)).toBeInTheDocument();
    expect(screen.getByText(/vieram de informes de corretoras/)).toBeInTheDocument();
  });

  it("copiar sem área de transferência avisa", async () => {
    const { user } = await openTaxes();
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("bloqueado")) },
      configurable: true,
    });
    await user.click(await screen.findByRole("button", { name: "Copiar a linha de BTC" }));
    expect(await screen.findByText("Não consegui copiar. Selecione o texto e copie à mão.")).toBeInTheDocument();
  });

  it("diz o que não foi calculado e por quê, sem repetir", async () => {
    await openTaxes();
    const out = await screen.findByRole("list", { name: "Fora da conta" });
    await waitFor(() => expect(within(out).getAllByRole("listitem")).toHaveLength(3));
    expect(within(out).getByText("CDB Exemplo").closest("li")).toHaveTextContent(/Fora do IR do Prumo/);
    expect(within(out).getByText("KNRI11").closest("li")).toHaveTextContent(/10\/03\/2026 · R\$\s10,00/);
    expect(within(out).getByText("AAPL34").closest("li")).toHaveTextContent(/rendimento do exterior/);
    expect(screen.getByText(/IR retido na fonte da nota/)).toBeInTheDocument();
  });

  it("troca de ano e mostra o erro claro de ano sem regra", async () => {
    const { user } = await openTaxes();
    await user.click(screen.getByRole("button", { name: "2025" }));
    expect(await screen.findByRole("region", { name: "DARF por mês em 2025" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Isentômetro" })).toHaveTextContent("dezembro de 2025");
    expect(await screen.findByRole("heading", { name: "Declaração de 2025" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "2025" })).toHaveAttribute("aria-pressed", "true");
  });

  it("ano sem regra no servidor vira um erro que a pessoa entende", async () => {
    server.use(
      http.get("/api/v1/tax/carryforward", () =>
        HttpResponse.json({ ...getTaxDb().carryforward, covered_years: [2027] }),
      ),
    );
    renderApp("ajustes/impostos");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Não consegui apurar 2027");
    expect(alert).toHaveTextContent(/ainda nao a de 2027/);
  });

  it("informa o saldo de prejuízo de antes do Prumo", async () => {
    const { user } = await openTaxes();
    const card = screen.getByRole("region", { name: "Saldo de antes do Prumo" });
    expect(card).toHaveTextContent("Nada informado: a conta começa do zero em 1º de janeiro de 2025.");
    await user.click(within(card).getByRole("button", { name: "Informar saldo" }));
    const field = within(card).getByLabelText("Prejuízo em ações, ETF e BDR");
    await user.click(field);
    await user.keyboard("150000");
    await user.click(within(card).getByRole("radio", { name: "1º/1/2026" }));
    await user.click(within(card).getByRole("button", { name: "Salvar saldo" }));
    expect(await screen.findByText("Saldo de antes do Prumo salvo.")).toBeInTheDocument();
    expect(requests("PUT", "/api/v1/tax/carryforward").at(-1)?.body).toEqual({
      year: 2026,
      common_loss: 1500,
      day_trade_loss: 0,
      real_estate_fund_loss: 0,
      agribusiness_fund_loss: 0,
      pending_exchange_darf: 0,
      pending_capital_gain_darf: 0,
    });
    const saved = await within(card).findByRole("list", { name: "Saldo em 1º de janeiro de 2026" });
    expect(saved).toHaveTextContent(/Prejuízo em ações, ETF e BDR: R\$\s1\.500,00/);
    await user.click(within(card).getByRole("button", { name: "Editar saldo" }));
    await user.click(within(card).getByRole("button", { name: "Cancelar" }));
    expect(within(card).getByRole("button", { name: "Editar saldo" })).toBeInTheDocument();
  });

  it("saldo recusado mostra o motivo", async () => {
    server.use(
      http.put("/api/v1/tax/carryforward", () =>
        HttpResponse.json(
          { title: "Dados invalidos", status: 422, code: "validation_failed", detail: "O prejuizo vai em centavos." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openTaxes();
    const card = screen.getByRole("region", { name: "Saldo de antes do Prumo" });
    await user.click(within(card).getByRole("button", { name: "Informar saldo" }));
    await user.click(within(card).getByRole("button", { name: "Salvar saldo" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent("O prejuizo vai em centavos.");
  });

  it("com o olho fechado mascara todo R$ e toda quantidade, e deixa percentuais e datas", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    const { user } = await openTaxes();
    await screen.findByRole("list", { name: "Bens e Direitos" });
    for (const summary of document.querySelectorAll("summary")) await user.click(summary);
    await screen.findByRole("list", { name: "Fora da conta" });
    for (const text of everythingHeard(document.body)) expect(text).not.toMatch(MONEY);

    const meter = screen.getByRole("region", { name: "Isentômetro" });
    expect(meter).toHaveTextContent("60,0% do limite");
    expect(meter).toHaveTextContent("R$ •••• em vendas de ações");
    const goods = screen.getByRole("list", { name: "Bens e Direitos" });
    expect(goods).toHaveTextContent("•• unidades");
    expect(goods).not.toHaveTextContent("2.500");
    const may = within(screen.getByRole("region", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent("6015 · R$ ••••");
    expect(may).toHaveTextContent("30/06/2026");

    const card = screen.getByRole("region", { name: "Saldo de antes do Prumo" });
    await user.click(within(card).getByRole("button", { name: "Informar saldo" }));
    for (const text of everythingHeard(card)) expect(text).not.toMatch(MONEY);
  });
});

describe("Instrumento fiscal na folha do ativo", () => {
  it("mostra o padrão, troca e volta ao padrão", async () => {
    const { user } = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const stocks = screen.getByRole("region", { name: "Ações Nacionais" });
    await user.click(within(stocks).getByRole("button", { name: /ITSA4/ }));
    const dialog = await screen.findByRole("dialog", { name: "ITSA4" });
    const select = within(dialog).getByRole("combobox", { name: "No imposto de renda" });
    expect(select).toHaveValue("stock");
    expect(within(dialog).getByRole("option", { name: "Ação ou unit (padrão)" })).toBeInTheDocument();
    expect(within(dialog).getByText(/É o padrão do Prumo/)).toBeInTheDocument();

    await user.selectOptions(select, "etf");
    expect(within(dialog).getByText(/sem a isenção das ações/)).toHaveTextContent("O padrão do Prumo seria Ação ou unit.");
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(requests("PATCH", "/api/v1/assets/a-itsa").at(-1)?.body).toEqual({ tax_instrument: "etf" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    await user.click(within(stocks).getByRole("button", { name: /ITSA4/ }));
    const again = await screen.findByRole("dialog", { name: "ITSA4" });
    const reopened = within(again).getByRole("combobox", { name: "No imposto de renda" });
    expect(reopened).toHaveValue("etf");
    await user.selectOptions(reopened, "stock");
    await user.click(within(again).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(requests("PATCH", "/api/v1/assets/a-itsa").at(-1)?.body).toEqual({ tax_instrument: null }));
  });

  it("renda fixa por valor não tem a escolha", async () => {
    const { user } = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    const rf = screen.getByRole("region", { name: "Renda Fixa" });
    // Na carteira fictícia, a renda fixa é por valor (sem ticker).
    await user.click(within(rf).getByRole("button", { name: /Tesouro IPCA\+ 2035/ }));
    const dialog = await screen.findByRole("dialog", { name: "Tesouro IPCA+ 2035" });
    expect(within(dialog).queryByRole("combobox", { name: "No imposto de renda" })).toBeNull();
  });
});
