import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";
import { getTaxDb, sampleIrpf, sampleMonths } from "../../test/taxHandlers";

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

/** Abre o ⓘ de nome `name` (o ponto a confirmar só entra na tela quando a pessoa pede). */
async function openTip(user: ReturnType<typeof userEvent.setup>, name: string | RegExp, root: HTMLElement = document.body) {
  const trigger = within(root).getByRole("button", { name });
  await user.click(trigger);
  return within(trigger.closest(".toggletip") as HTMLElement).getByRole("status");
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
    const { user } = await openTaxes();
    const meter = screen.getByRole("region", { name: "Isentômetro" });
    expect(meter).toHaveTextContent("Novembro de 2026");
    expect(meter).toHaveTextContent(/R\$\s12\.000,00 em vendas de ações, de R\$\s20\.000,00 isentos/);
    expect(within(meter).getByRole("meter")).toHaveAttribute("aria-valuenow", "60");
    expect(meter).toHaveTextContent("60,0% do limite");
    expect(meter).toHaveTextContent(/Ainda cabem R\$\s8\.000,00 em vendas/);
    // O ponto a confirmar é um ⓘ azul com nome, ao lado do número que ele mexe.
    expect(await openTip(user, "Ponto a confirmar com contador: vendas em day trade no isentômetro", meter)).toHaveTextContent(
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
    const table = screen.getByRole("table", { name: "DARF por mês em 2026" });
    const may = within(table).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent(/6015 · R\$\s448,80/);
    expect(may).toHaveTextContent("30/06/2026");
    // Vencido em 30/06 e não pago: em atraso, escrito (a cor só reforça).
    expect(may).toHaveTextContent("Em atraso");
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
      expect(
        within(within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ })).getByRole(
          "button",
          { name: /Desmarcar/ },
        ),
      ).toBeInTheDocument(),
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
    const may = within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent("o valor mudou depois de marcar: confira");
    await user.click(within(may).getByRole("button", { name: "Desmarcar o DARF 6015 de maio" }));
    expect(await screen.findByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
  });

  it("abre a apuração de cada mês com grupo, compensação e as dúvidas que tocam o número", async () => {
    const { user } = await openTaxes();
    // A apuração fica recolhida, com a contagem dos meses que têm o que mostrar.
    const fold = screen.getByRole("region", { name: "Como cada mês foi apurado" });
    expect(fold).toHaveTextContent("6");
    await user.click(within(fold).getByText("Como cada mês foi apurado"));
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
    expect(await openTip(user, "Ponto a confirmar com contador: prejuízo de Fiagro separado do FII")).toHaveTextContent(
      /só compensa com lucro de Fiagro/,
    );
    expect(screen.getByText(/Prejuízo a compensar em 31\/12/)).toHaveTextContent(/Fiagro R\$\s100,00/);
  });

  it("monta a declaração para copiar: Bens e Direitos, isentos e JCP", async () => {
    const { user } = await openTaxes();
    // Depois do `userEvent.setup()`, que põe a área de transferência dele.
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const goods = await screen.findByRole("table", { name: "Bens e Direitos" });
    const itsa = within(goods).getByRole("row", { name: /ITSA4/ });
    expect(itsa).toHaveTextContent("Grupo 03 · Código 01 · Ações (inclusive as listadas em bolsa)");
    expect(itsa).toHaveTextContent(/2\.500 unidades/);
    expect(itsa).toHaveTextContent(/R\$\s25\.000,00/);
    const btc = within(goods).getByRole("row", { name: /BTC/ });
    expect(btc).toHaveTextContent("0,05 unidades");
    await user.click(within(itsa).getByRole("button", { name: "Copiar a linha de ITSA4" }));
    expect(writeText).toHaveBeenCalledWith(expect.stringMatching(/ITSA4 \(Ação ou unit\): 2\.500 unidades\. Grupo 03, código 01\. Situação em 31\/12: R\$\s25\.000,00/));
    expect(await screen.findByText("Linha de ITSA4 copiada.")).toBeInTheDocument();

    const income = screen.getByRole("table", { name: "Rendimentos por fonte pagadora" });
    expect(within(income).getByRole("row", { name: /Código 09/ })).toHaveTextContent(/R\$\s300,00/);
    expect(within(income).getByRole("row", { name: /Código 99/ })).toHaveTextContent("Outros: rendimentos de FII e Fiagro");
    const gains = within(income).getByRole("row", { name: /Código 20/ });
    expect(gains).toHaveTextContent("Lucro das suas vendas no ano");
    expect(gains).toHaveTextContent(/R\$\s900,00/);
    // Código sem valor no ano não ocupa linha.
    expect(within(income).queryByRole("row", { name: /Código 05/ })).toBeNull();
    const jcp = within(income).getByRole("row", { name: /Código 10/ });
    expect(jcp).toHaveTextContent(/Líquido R\$\s165,00/);
    expect(jcp).toHaveTextContent(/bruto R\$\s200,00, retido R\$\s35,00/);
    // As dúvidas jurídicas ficam num ⓘ azul ao lado do bloco que elas tocam.
    expect(await openTip(user, "Ponto a confirmar com contador: JCP levado pelo valor líquido")).toHaveTextContent(/O JCP foi levado pelo valor líquido/);
    expect(await openTip(user, "Ponto a confirmar com contador: códigos de Bens e Direitos e de rendimentos")).toHaveTextContent(
      /vieram de informes de corretoras/,
    );
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
    const { user } = await openTaxes();
    const fold = screen.getByRole("region", { name: "O que o Prumo não calculou" });
    await user.click(within(fold).getByText("O que o Prumo não calculou"));
    const out = await screen.findByRole("list", { name: "Fora da conta" });
    await waitFor(() => expect(within(out).getAllByRole("listitem")).toHaveLength(3));
    expect(within(out).getByText("CDB Exemplo").closest("li")).toHaveTextContent(/Fora do IR do Prumo/);
    expect(within(out).getByText("KNRI11").closest("li")).toHaveTextContent(/10\/03\/2026 · R\$\s10,00/);
    expect(within(out).getByText("AAPL34").closest("li")).toHaveTextContent(/rendimento do exterior/);
  });

  it("mostra o que o Prumo cobre, com a regra e a fonte, e o que fica fora com o porquê", async () => {
    const { user } = await openTaxes();
    const fold = screen.getByRole("region", { name: "O que o Prumo cobre" });
    await waitFor(() => expect(fold).toHaveTextContent("10 calcula · 10 fora"));
    await user.click(within(fold).getByText("O que o Prumo cobre"));
    const covered = await within(fold).findByRole("list", { name: "O que o Prumo calcula em 2026" });
    const dayTrade = within(covered).getByText("Day trade").closest("li");
    expect(dayTrade).toHaveTextContent(/20%, sem a isenção das ações, com prejuízo em conta separada e 1% retido/);
    expect(dayTrade).toHaveTextContent("Fonte: Lei de exemplo");
    expect(within(covered).getByText("Ações e units").closest("li")).toHaveTextContent("até 20 mil reais");
    expect(within(covered).getByText("ETF de renda variável e BDR").closest("li")).toHaveTextContent("sem a isenção das ações");
    expect(within(covered).getByText("IR retido na fonte").closest("li")).toHaveTextContent(/0,005% das vendas comuns \(dispensado até 1 real no mês\)/);
    expect(within(covered).getByText("DARF em atraso").closest("li")).toHaveTextContent(/0,33% ao dia, até 20%.*série 4390/);
    expect(within(covered).getByText("Proventos na declaração").closest("li")).toHaveTextContent(
      /JCP com 17,5% na fonte.*acima de 50 mil reais no mês com retenção de 10%/,
    );
    const out = within(fold).getByRole("list", { name: "O que o Prumo não calcula" });
    expect(within(out).getByText("Opções, termo e futuro").closest("li")).toHaveTextContent(/só registra compra e venda à vista/);
    expect(within(out).getByText("Corretora de cada negócio").closest("li")).toHaveTextContent(/Informe o retido da nota/);
  });

  it("a tabela de regras fora do ar não derruba a tela", async () => {
    server.use(
      http.get("/api/v1/tax/rules/:year", () =>
        HttpResponse.json(
          { title: "Erro interno", status: 500, code: "internal", detail: "x" },
          { status: 500, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openTaxes();
    const fold = screen.getByRole("region", { name: "O que o Prumo cobre" });
    await user.click(within(fold).getByText("O que o Prumo cobre"));
    expect(await within(fold).findByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
    expect(within(fold).getByRole("list", { name: "O que o Prumo não calcula" })).toBeInTheDocument();
  });

  it("mostra o prejuízo a compensar de cada grupo, mês a mês, na tabela dos meses", async () => {
    await openTaxes();
    const table = screen.getByRole("table", { name: "DARF por mês em 2026" });
    expect(within(table).getByRole("columnheader", { name: "Prejuízo a compensar" })).toBeInTheDocument();
    const august = within(table).getByRole("row", { name: /^agosto/ });
    expect(august).toHaveTextContent(/Fiagro R\$\s100,00/);
    expect(august).toHaveTextContent(/mais R\$\s100,00 no mês/);
    expect(within(table).getByRole("row", { name: /^dezembro/ })).toHaveTextContent(/Fiagro R\$\s100,00/);
    expect(within(table).getByRole("row", { name: /^julho/ })).not.toHaveTextContent("Fiagro");
  });

  it("compensa o prejuízo e mostra o saldo de 1º de janeiro", async () => {
    server.use(
      http.get("/api/v1/tax/months", () => {
        const data = sampleMonths(2026);
        for (const month of data.months) {
          const common = month.pools[0];
          if (common) Object.assign(common, { loss_before: month.month <= 5 ? 300 : 0, loss_after: month.month < 5 ? 300 : 0 });
        }
        Object.assign(data.months[4]?.pools[0] ?? {}, { loss_used: 300 });
        return HttpResponse.json(data);
      }),
    );
    await openTaxes();
    expect(screen.getByText(/Prejuízo a compensar em 1º de janeiro/)).toHaveTextContent(/Ações, ETF e BDR R\$\s300,00/);
    const may = within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent("Ações, ETF e BDR zerado");
    expect(may).toHaveTextContent(/compensou R\$\s300,00/);
  });

  it("calcula a multa e os juros do DARF em atraso pelo dia do pagamento", async () => {
    const { user } = await openTaxes();
    const may = within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    await user.click(within(may).getByRole("button", { name: "Calcular multa e juros do DARF 6015 de maio" }));
    const sheet = await screen.findByRole("dialog", { name: "DARF 6015 de maio de 2026 em atraso" });
    expect(sheet).toHaveTextContent("Venceu em 30/06/2026.");
    // Hoje (25/11/2026): 148 dias, multa no teto; juros da Selic de julho a
    // outubro mais 1%.
    const lines = await within(sheet).findByLabelText("Conta do DARF em atraso");
    expect(lines).toHaveTextContent(/Imposto \(principal\)R\$\s448,80/);
    expect(lines).toHaveTextContent(/Multa de mora: 20%0,33% × 148 dias, até 20%R\$\s89,76/);
    expect(lines).toHaveTextContent(/Juros de mora: 5,46%/);
    expect(lines).toHaveTextContent(/julho de 2026 \(1,22%\) \+ agosto de 2026 \(1,09%\)/);
    expect(lines).toHaveTextContent(/1% no mês do pagamento/);
    expect(lines).toHaveTextContent(/R\$\s24,50/);
    expect(lines).toHaveTextContent(/Total a pagar em 25\/11\/2026R\$\s563,06/);
    expect(sheet).toHaveTextContent("No DARF: código 6015, período de apuração 05/2026, vencimento 30/06/2026");
    expect(sheet).toHaveTextContent("Fonte: Lei de exemplo");

    // Pago em janeiro de 2027: falta a Selic dos meses que não fecharam.
    const date = within(sheet).getByLabelText("Dia do pagamento");
    await user.clear(date);
    await user.type(date, "2027-01-10");
    expect(await within(sheet).findByText(/Falta a Selic de novembro de 2026, dezembro de 2026/)).toBeInTheDocument();
    expect(within(sheet).getByLabelText("Conta do DARF em atraso")).toHaveTextContent(/Juros de mora: falta a Selic/);
    expect(requests("GET", "/api/v1/tax/darfs/2026/5/late").at(-1)?.path).toContain("pay_on=2027-01-10");

    // No dia do vencimento: nada a mais.
    await user.clear(date);
    await user.type(date, "2026-06-30");
    expect(await within(sheet).findByText(/Pago até 30\/06\/2026, o DARF vai sem multa nem juros/)).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("DARF pago no mês do vencimento tem multa e ainda não tem juros", async () => {
    server.use(
      http.get("/api/v1/tax/darfs/:year/:month/late", () =>
        HttpResponse.json({
          code: "6015",
          year: 2026,
          month: 5,
          principal: 100,
          due_date: "2026-06-29",
          pay_date: "2026-06-30",
          days_late: 1,
          penalty_rate: 0.0033,
          penalty: 0.33,
          selic_months: [],
          payment_month_rate: 0,
          interest_rate: 0,
          interest: 0,
          total: 100.33,
          missing_selic: [],
          rule: {
            daily_penalty: { value: 0.0033, source: "x" },
            penalty_cap: { value: 0.2, source: "x" },
            payment_month_interest: { value: 0.01, source: "x" },
            selic_series: { value: "4390", source: "x" },
          },
        }),
      ),
    );
    const { user } = await openTaxes();
    const may = within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    await user.click(within(may).getByRole("button", { name: "Calcular multa e juros do DARF 6015 de maio" }));
    const sheet = await screen.findByRole("dialog");
    expect(await within(sheet).findByText(/Pago no mês do vencimento: os juros ainda não começaram/)).toBeInTheDocument();
  });

  it("a calculadora mostra o erro do servidor", async () => {
    server.use(
      http.get("/api/v1/tax/darfs/:year/:month/late", () =>
        HttpResponse.json(
          { title: "Dados invalidos", status: 422, code: "validation_failed", detail: "A data do pagamento vai como AAAA-MM-DD." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openTaxes();
    const may = within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    await user.click(within(may).getByRole("button", { name: "Calcular multa e juros do DARF 6015 de maio" }));
    const sheet = await screen.findByRole("dialog");
    expect(await within(sheet).findByRole("alert")).toHaveTextContent("A data do pagamento vai como AAAA-MM-DD.");
  });

  it("mostra o IR retido pela regra e troca pelo valor da nota", async () => {
    const { user } = await openTaxes();
    const fold = screen.getByRole("region", { name: "Como cada mês foi apurado" });
    await user.click(within(fold).getByText("Como cada mês foi apurado"));
    await user.click(screen.getByText("Maio", { selector: "summary" }));
    const block = screen.getByRole("group", { name: "IR retido na fonte em maio" });
    expect(block).toHaveTextContent(/IR retido na fonte: R\$\s1,20 pela regra \(0,005% de R\$\s24\.000,00 vendidos\)/);
    expect(block).toHaveTextContent(/Abateu R\$\s1,20 do imposto/);
    expect(block).toHaveTextContent("confira na nota de corretagem");

    await user.click(within(block).getByRole("button", { name: "Informar o valor da nota" }));
    const field = within(block).getByLabelText("IR retido em maio, como na nota");
    await user.clear(field);
    await user.keyboard("200");
    await user.click(within(block).getByRole("button", { name: "Salvar" }));
    expect(await screen.findByText("IR retido de maio salvo.")).toBeInTheDocument();
    expect(requests("PUT", "/api/v1/tax/withholding/2026/5").at(-1)?.body).toEqual({ amount: 2 });
    const saved = await screen.findByRole("group", { name: "IR retido na fonte em maio" });
    await waitFor(() => expect(saved).toHaveTextContent(/R\$\s2,00 informado da nota/));
    expect(saved).not.toHaveTextContent("confira na nota de corretagem");

    await user.click(within(saved).getByRole("button", { name: "Voltar para a regra" }));
    expect(await screen.findByText("IR retido de maio voltou para a regra.")).toBeInTheDocument();
    expect(requests("DELETE", "/api/v1/tax/withholding/2026/5")).toHaveLength(1);
    expect(getTaxDb().withholding.size).toBe(0);
  });

  it("IR retido recusado mostra o motivo e o cancelar volta", async () => {
    server.use(
      http.put("/api/v1/tax/withholding/:year/:month", () =>
        HttpResponse.json(
          { title: "Dados invalidos", status: 422, code: "validation_failed", detail: "O IR retido vai em centavos." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openTaxes();
    await user.click(within(screen.getByRole("region", { name: "Como cada mês foi apurado" })).getByText("Como cada mês foi apurado"));
    await user.click(screen.getByText("Maio", { selector: "summary" }));
    const block = screen.getByRole("group", { name: "IR retido na fonte em maio" });
    await user.click(within(block).getByRole("button", { name: "Informar o valor da nota" }));
    await user.click(within(block).getByRole("button", { name: "Salvar" }));
    expect(await screen.findByText(/O IR retido vai em centavos/)).toBeInTheDocument();
    await user.click(within(block).getByRole("button", { name: "Cancelar" }));
    expect(within(block).getByRole("button", { name: "Informar o valor da nota" })).toBeInTheDocument();
  });

  it("lista os rendimentos por fonte pagadora, com CNPJ, para copiar", async () => {
    const { user } = await openTaxes();
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const table = await screen.findByRole("table", { name: "Rendimentos por fonte pagadora" });
    const company = within(table).getByRole("row", { name: /EMPRESA EXEMPLO S\.A\./ });
    expect(company).toHaveTextContent("CNPJ 00.000.000/0001-91 · ITSA4");
    expect(company).toHaveTextContent("Código 10");
    expect(company).toHaveTextContent(/Líquido R\$\s165,00/);
    const fund = within(table).getByRole("row", { name: /FUNDO EXEMPLO FII/ });
    expect(fund).toHaveTextContent("CNPJ 22.222.222/0001-22 · KNRI11");
    expect(fund).toHaveTextContent(/R\$\s1\.200,00/);
    const unknown = within(table).getAllByRole("row", { name: /ITSA4/ }).find((row) => row.textContent?.includes("CNPJ não encontrado"));
    expect(unknown).toHaveTextContent("Código 09");

    await user.click(within(company).getByRole("button", { name: "Copiar a linha de EMPRESA EXEMPLO S.A., código 10" }));
    expect(writeText).toHaveBeenCalledWith(
      expect.stringMatching(
        /^Rendimentos sujeitos à tributação exclusiva, código 10 \(Juros sobre capital próprio\): CNPJ 00\.000\.000\/0001-91, EMPRESA EXEMPLO S\.A\., R\$\s165,00 \(líquido; bruto R\$\s200,00, IR retido R\$\s35,00\)\.$/,
      ),
    );
    expect(await screen.findByText("Linha de EMPRESA EXEMPLO S.A. copiada.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Copiar o relatório" }));
    const report = (writeText.mock.calls.at(-1) as unknown as [string])[0];
    expect(report.split("\n")).toHaveLength(4);
    expect(report).toMatch(/código 20 \(Ganhos líquidos em operações no mercado à vista de ações até 20 mil reais no mês\): R\$\s900,00\./);
    expect(report).toContain("CNPJ não encontrado (ITSA4)");
    expect(report).toMatch(/Rendimentos isentos e não tributáveis, código 99 \(Outros: rendimentos de FII e Fiagro\): CNPJ 22\.222\.222\/0001-22, FUNDO EXEMPLO FII, R\$\s1\.200,00\./);
    expect(await screen.findByText("Relatório de rendimentos copiado.")).toBeInTheDocument();
  });

  it("marca o mês em que a mesma empresa passou de 50 mil em dividendos", async () => {
    server.use(
      http.get("/api/v1/tax/irpf/:year", () => {
        const data = sampleIrpf(2026);
        const payer = data.exempt_income[0]?.by_payer[0];
        if (payer) {
          Object.assign(payer, {
            cnpj: "00.000.000/0001-91",
            name: "EMPRESA EXEMPLO S.A.",
            gross: 60000,
            withheld: 6000,
            net: 54000,
            above_threshold: [{ month: 3, gross: 60000, withholding_by_rule: 6000, withheld: 6000 }],
          });
        }
        data.doubts = ["irpf_codes", "dividend_withholding"];
        return HttpResponse.json(data);
      }),
    );
    await openTaxes();
    const table = await screen.findByRole("table", { name: "Rendimentos por fonte pagadora" });
    const row = within(table).getByRole("row", { name: /Código 09.*EMPRESA EXEMPLO S\.A\./ });
    expect(row).toHaveTextContent(
      /Em março: R\$\s60\.000,00 da mesma empresa, acima do limite; retenção de 10% pela regra R\$\s6\.000,00, retido informado R\$\s6\.000,00\./,
    );
  });

  it("troca de ano e mostra o erro claro de ano sem regra", async () => {
    const { user } = await openTaxes();
    await user.click(screen.getByRole("button", { name: "2025" }));
    expect(await screen.findByRole("table", { name: "DARF por mês em 2025" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Isentômetro" })).toHaveTextContent("Dezembro de 2025");
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
    await screen.findByRole("table", { name: "Bens e Direitos" });
    for (const summary of document.querySelectorAll("summary")) await user.click(summary);
    await screen.findByRole("list", { name: "Fora da conta" });
    for (const text of everythingHeard(document.body)) expect(text).not.toMatch(MONEY);

    const meter = screen.getByRole("region", { name: "Isentômetro" });
    expect(meter).toHaveTextContent("60,0% do limite");
    expect(meter).toHaveTextContent("R$ •••• em vendas de ações");
    const goods = screen.getByRole("table", { name: "Bens e Direitos" });
    expect(goods).toHaveTextContent("•• unidades");
    expect(goods).not.toHaveTextContent("2.500");
    const may = within(screen.getByRole("table", { name: "DARF por mês em 2026" })).getByRole("row", { name: /^maio/ });
    expect(may).toHaveTextContent("6015 · R$ ••••");
    expect(may).toHaveTextContent("30/06/2026");

    const card = screen.getByRole("region", { name: "Saldo de antes do Prumo" });
    await user.click(within(card).getByRole("button", { name: "Informar saldo" }));
    for (const text of everythingHeard(card)) expect(text).not.toMatch(MONEY);

    // A calculadora do atraso e o IR retido também ficam mascarados.
    await user.click(within(may).getByRole("button", { name: "Calcular multa e juros do DARF 6015 de maio" }));
    const sheet = await screen.findByRole("dialog");
    await within(sheet).findByText(/Total a pagar/);
    expect(sheet).toHaveTextContent("20%");
    for (const text of everythingHeard(sheet)) expect(text).not.toMatch(MONEY);
    await user.keyboard("{Escape}");
    const withholding = screen.getByRole("group", { name: "IR retido na fonte em maio" });
    expect(withholding).toHaveTextContent("0,005% de R$ •••• vendidos");
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
