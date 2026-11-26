import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { TREASURY_AT } from "../../test/fixtures";
import { getDb, server } from "../../test/server";

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  return rendered;
}

const lastRequest = (method: string, path: string) =>
  getDb().requests.filter((r) => r.method === method && r.path === path).at(-1);

describe("Carteira", () => {
  it("mostra patrimônio, classes, ativos e quem precisa de atenção", async () => {
    await openPortfolio();
    // O total conta (CountUp); o texto da região vale durante e depois da contagem.
    await waitFor(() => expect(screen.getByRole("region", { name: "Patrimônio" })).toHaveTextContent(/R\$\s120\.132,23/));
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    expect(within(fiis).getAllByRole("button")).toHaveLength(5);
    // A linha traz tudo: quantidade, valor, rentabilidade, fatia × ideal e nota.
    expect(within(fiis).getByRole("button", { name: /KNRI11/ })).toHaveTextContent(/rentabilidade \+9,2%/);
    expect(within(fiis).getByText(/15 cotas · R\$\s94,55/)).toBeInTheDocument();
    const intl = screen.getByRole("region", { name: "Ações Internacionais" });
    expect(within(intl).getByText(/Tem meta e nenhum ativo/)).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Outras classes" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /Carteira por classe: Ações Nacionais/ })).toBeInTheDocument();
    expect(screen.getByText("Nenhum aporte registrado ainda.", { exact: false })).toBeInTheDocument();
  });

  it("a rosca por classe tem legenda escrita; o botão de adicionar fica no cabeçalho", async () => {
    await openPortfolio();
    const donut = screen.getByRole("region", { name: "Por classe" });
    expect(within(donut).getByRole("img", { name: /Carteira por classe: Ações Nacionais/ })).toBeInTheDocument();
    // O nome e a fatia estão escritos: a cor só ajuda a achar.
    expect(within(donut).getByText("Ações Nacionais")).toBeInTheDocument();
    expect(within(donut).getByText("23,3%")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Adicionar ativo" })).toHaveLength(1);
  });

  it("o histórico de aportes começa recolhido e diz quantos há", async () => {
    getDb().contributions.push({
      id: "c-2",
      created_at: "2026-11-20T15:00:00Z",
      amount: 1000,
      cash_used: 0,
      invested: 990.3,
      items: [{ asset_id: "a-trxf", symbol: "TRXF11", display_name: "TRX Real Estate", quantity: 10, unit_price: 53.81, value: 538.1 }],
    });
    await openPortfolio();
    const history = screen.getByRole("region", { name: "Histórico de aportes" });
    await within(history).findByText("1");
    expect(history.querySelector("details")).not.toHaveAttribute("open");
  });

  it("o texto explicativo da folha mora nos ⓘ: abre no clique e fecha com Esc", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: /TRXF11/ }));
    const dialog = await screen.findByRole("dialog", { name: "TRXF11" });
    expect(within(dialog).queryByText(/Seu critério: acima deste preço/)).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Sobre teto de preço" }));
    expect(within(dialog).getByText(/Seu critério: acima deste preço/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Sobre a quantidade" }));
    expect(within(dialog).getByText(/Mudar aqui grava um ajuste em Lançamentos/)).toBeInTheDocument();
  });

  it("atualiza as cotações e avisa quem falhou", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Atualizar cotações" }));
    expect(await screen.findByText("7 cotações atualizadas.")).toBeInTheDocument();

    server.use(http.post("/api/v1/quotes/refresh", () => HttpResponse.json({ refreshed: 1, failed: ["MXRF11"] })));
    await user.click(screen.getByRole("button", { name: "Atualizar cotações" }));
    expect(await screen.findByText("1 cotação atualizada. Sem resposta: MXRF11.")).toBeInTheDocument();

    server.use(http.post("/api/v1/quotes/refresh", () => HttpResponse.error()));
    await user.click(screen.getByRole("button", { name: "Atualizar cotações" }));
    expect(await screen.findByText(/Sem conexão com o servidor/)).toBeInTheDocument();
  });

  it("adiciona um FII pela busca e já abre o checklist dele", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    const combobox = within(dialog).getByRole("combobox", { name: "Ticker ou nome" });
    await waitFor(() => expect(combobox).toHaveFocus());
    await user.type(combobox, "kinea");
    await within(dialog).findAllByRole("option");
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowUp}{Enter}");
    expect(combobox).toHaveValue("KNRI11");
    await user.clear(combobox);
    await user.type(combobox, "kncr");
    await user.click(await within(dialog).findByRole("option", { name: /KNCR11/ }));
    const quantity = within(dialog).getByLabelText("Quantidade");
    await user.clear(quantity);
    await user.type(quantity, "30");
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));

    expect(await screen.findByText("KNCR11 entrou na carteira.")).toBeInTheDocument();
    expect(lastRequest("POST", "/api/v1/assets")?.body).toEqual({
      asset_class: "br_reits",
      symbol: "KNCR11",
      quantity: 30,
      display_name: "Kinea Rendimentos Imobiliários",
    });
    const edit = await screen.findByRole("dialog", { name: "KNCR11" });
    expect(within(edit).getByText("Responda o checklist para o KNCR11 ganhar nota — cada sim soma 1.")).toBeInTheDocument();
    // O checklist vem antes da quantidade quando a folha abre para dar nota.
    const checklist = within(edit).getByRole("region", { name: "Checklist" });
    const editQuantity = within(edit).getByLabelText("Quantidade");
    expect(checklist.compareDocumentPosition(editQuantity) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("adiciona renda fixa com nome, valor e nota", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.click(within(dialog).getByRole("radio", { name: /Renda Fixa$/ }));
    await user.type(within(dialog).getByLabelText("Nome"), "CDB Banco X 2028");
    await user.type(within(dialog).getByLabelText("Quanto você tem hoje"), "1500000");
    fireEvent.change(within(dialog).getByLabelText("Nota de confiança (0 a 10)"), { target: { value: "6" } });
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toEqual({
        asset_class: "fixed_income",
        display_name: "CDB Banco X 2028",
        amount: 15000,
        score_override: 6,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
  });

  it("cadastra um título do Tesouro Direto pela busca: o nome no campo, a chave no cadastro, fração e preço sozinho", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "tesouro");
    const option = await within(dialog).findByRole("option", { name: /Tesouro IPCA\+ 2035/ });
    // A lista mostra o nome e de onde vem, não a chave.
    expect(option).toHaveTextContent("Tesouro Direto");
    expect(option).toHaveTextContent("Renda Fixa");
    expect(option).not.toHaveTextContent("TD-IPCA");
    await user.click(option);

    expect(within(dialog).getByRole("combobox", { name: "Título do Tesouro Direto" })).toHaveValue("Tesouro IPCA+ 2035");
    expect(within(dialog).getByRole("radio", { name: "Tesouro Direto" })).toHaveAttribute("aria-checked", "true");
    expect(within(dialog).getByRole("radio", { name: /Renda Fixa$/ })).toHaveAttribute("aria-checked", "true");
    // O título é fração: sem − e +, e a vírgula vale.
    expect(within(dialog).queryByRole("button", { name: /Aumentar/ })).toBeNull();
    expect(within(dialog).queryByRole("switch", { name: /Negociado na B3/ })).toBeNull();
    const quantity = within(dialog).getByLabelText("Quantidade de títulos");
    await user.clear(quantity);
    await user.type(quantity, "0,35");
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));

    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toEqual({
        asset_class: "fixed_income",
        symbol: "TD-IPCA-20350515",
        treasury: true,
        quantity: 0.35,
        score_override: 5,
      }),
    );
    expect(await screen.findByText("Tesouro IPCA+ 2035 entrou na carteira.")).toBeInTheDocument();
    const rf = screen.getByRole("region", { name: "Renda Fixa" });
    // 0,35 × R$ 2.500,95: o preço veio da tabela, sem digitar.
    expect(await within(rf).findByText(/0,35 títulos · R\$\s2\.500,95/)).toBeInTheDocument();
    expect(within(rf).getByText("Tesouro Direto")).toBeInTheDocument();
    expect(within(rf).queryByText(/TD-IPCA/)).toBeNull();
  });

  it("Renda Fixa tem dois jeitos: por valor ou Tesouro Direto, que só vale com um título escolhido", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    expect(within(dialog).queryByRole("radiogroup", { name: "Como cadastrar a renda fixa" })).toBeNull();
    await user.click(within(dialog).getByRole("radio", { name: /Renda Fixa$/ }));
    expect(within(dialog).getByRole("radio", { name: /Por valor/ })).toHaveAttribute("aria-checked", "true");
    expect(within(dialog).getByLabelText("Nome")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("radio", { name: "Tesouro Direto" }));
    expect(within(dialog).queryByLabelText("Nome")).toBeNull();
    const combobox = within(dialog).getByRole("combobox", { name: "Título do Tesouro Direto" });
    expect(within(dialog).getByText(/escolha o título na lista/)).toBeInTheDocument();
    // Digitar sem escolher não cadastra: a chave vem da lista.
    await user.type(combobox, "ipca");
    await within(dialog).findAllByRole("option");
    await user.keyboard("{Escape}");
    expect(within(dialog).getByRole("button", { name: "Adicionar à carteira" })).toBeDisabled();
    // A seta para baixo reabre a lista que o Esc fechou.
    await user.keyboard("{ArrowDown}");
    await user.click(await within(dialog).findByRole("option", { name: /Tesouro IPCA\+ 2035/ }));
    expect(within(dialog).getByRole("button", { name: "Adicionar à carteira" })).toBeEnabled();
    expect(within(dialog).queryByText(/escolha o título na lista/)).toBeNull();

    // Voltar para "por valor" solta o título; trocar de classe também.
    await user.click(within(dialog).getByRole("radio", { name: /Por valor/ }));
    expect(within(dialog).getByLabelText("Nome")).toHaveValue("");
    expect(within(dialog).queryByRole("combobox")).toBeNull();
    await user.click(within(dialog).getByRole("radio", { name: "Tesouro Direto" }));
    await user.type(within(dialog).getByRole("combobox", { name: "Título do Tesouro Direto" }), "selic");
    await user.click(await within(dialog).findByRole("option", { name: /Tesouro Selic 2029/ }));
    await user.click(within(dialog).getByRole("radio", { name: /Cripto/ }));
    expect(within(dialog).queryByRole("radiogroup", { name: "Como cadastrar a renda fixa" })).toBeNull();
    expect(within(dialog).getByRole("combobox", { name: "Ticker ou nome" })).toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Quantidade de títulos")).toBeNull();
  });

  it("um título escolhido na busca de outra classe leva a classe e o modo junto", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    expect(within(dialog).getByRole("radio", { name: /Ações BR/ })).toHaveAttribute("aria-checked", "true");
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "selic");
    await user.click(await within(dialog).findByRole("option", { name: /Tesouro Selic 2029/ }));
    expect(within(dialog).getByRole("radio", { name: /Renda Fixa$/ })).toHaveAttribute("aria-checked", "true");
    expect(within(dialog).getByRole("radio", { name: "Tesouro Direto" })).toHaveAttribute("aria-checked", "true");
    // Escolher outro ativo que não é título volta ao modo comum.
    const combobox = within(dialog).getByRole("combobox", { name: "Título do Tesouro Direto" });
    await user.clear(combobox);
    await user.type(combobox, "kncr");
    await user.click(await within(dialog).findByRole("option", { name: /KNCR11/ }));
    expect(within(dialog).getByRole("radio", { name: /FIIs/ })).toHaveAttribute("aria-checked", "true");
    expect(within(dialog).getByRole("combobox", { name: "Ticker ou nome" })).toHaveValue("KNCR11");
  });

  it("título do Tesouro na carteira: nome, fração, data do preço e edição da quantidade", async () => {
    getDb().assets.push({
      id: "a-td",
      asset_class: "fixed_income",
      symbol: "TD-SELIC-20290301",
      display_name: "Tesouro Selic 2029",
      quantity: 0.5,
      quote_price: 20305.65,
      manual_price: null,
      currency: "BRL",
      score_override: 5,
      answers: {},
      listed_on_b3: false,
      treasury: true,
      quote_at: TREASURY_AT,
    });
    const { user } = await openPortfolio();
    const rf = screen.getByRole("region", { name: "Renda Fixa" });
    const row = within(rf).getByRole("button", { name: /Tesouro Selic 2029/ });
    expect(row).toHaveTextContent("Tesouro Direto");
    expect(row).toHaveTextContent(/0,5 títulos · R\$\s20\.305,65/);
    expect(row).not.toHaveTextContent("TD-SELIC");
    await user.click(row);

    const dialog = await screen.findByRole("dialog", { name: "Tesouro Selic 2029" });
    expect(within(dialog).getByText("Renda Fixa · Tesouro Direto")).toBeInTheDocument();
    expect(within(dialog).getByText(/preço de resgate de 19\/11\/2026/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("switch", { name: /Negociado na B3/ })).toBeNull();
    expect(within(dialog).queryByRole("button", { name: /Aumentar/ })).toBeNull();
    const quantity = within(dialog).getByLabelText("Quantidade de títulos");
    expect(quantity).toHaveValue("0,5");
    await user.clear(quantity);
    await user.type(quantity, "0,75");
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/assets/a-td")?.body).toEqual({ quantity: 0.75 }));
    expect(await within(rf).findByText(/0,75 títulos · R\$\s20\.305,65/)).toBeInTheDocument();
  });

  it("ticker digitado sem sugestão vale, com a classe adivinhada; duplicado avisa", async () => {
    server.use(http.get("/api/v1/symbols", () => new HttpResponse(null, { status: 502 })));
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    const combobox = within(dialog).getByRole("combobox", { name: "Ticker ou nome" });
    await user.type(combobox, "btlg11");
    expect(await within(dialog).findByText(/Busca indisponível agora/)).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(within(dialog).getByRole("radio", { name: /FIIs/ })).toHaveAttribute("aria-checked", "true");
    await user.click(within(dialog).getByRole("button", { name: "Aumentar quantidade" }));
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Esse ativo já está na sua carteira.");
  });

  it("cripto pede nota manual e fração", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "eth");
    await user.click(await within(dialog).findByRole("option", { name: /ETH/ }));
    const quantity = within(dialog).getByLabelText("Quantidade");
    await user.clear(quantity);
    await user.type(quantity, "0,5");
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toEqual({
        asset_class: "crypto",
        symbol: "ETH",
        quantity: 0.5,
        display_name: "Ethereum",
        score_override: 5,
      }),
    );
    // Cripto já nasce com nota (a manual): nenhuma folha de checklist abre.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
  });

  it("cadastra um ativo que ainda não comprou: quantidade 0 e a linha diz isso", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    expect(within(dialog).getByText(/Ainda não comprou\? Deixe 0/)).toBeInTheDocument();
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "kncr");
    await user.click(await within(dialog).findByRole("option", { name: /KNCR11/ }));
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toEqual({
        asset_class: "br_reits",
        symbol: "KNCR11",
        quantity: 0,
        display_name: "Kinea Rendimentos Imobiliários",
      }),
    );
    const edit = await screen.findByRole("dialog", { name: "KNCR11" });
    await user.click(within(edit).getByRole("button", { name: "Fechar" }));
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    expect(await within(fiis).findByText(/Ainda não tem · R\$\s100,00/)).toBeInTheDocument();
  });

  it("ticker da B3 em Cripto liga a chave sozinho; desligar volta a ser cripto", async () => {
    server.use(http.get("/api/v1/symbols", () => HttpResponse.json([])));
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    // Ações BR e FIIs já são da B3: não há chave.
    expect(within(dialog).queryByRole("switch", { name: /Negociado na B3/ })).toBeNull();
    await user.click(within(dialog).getByRole("radio", { name: /Cripto/ }));
    await user.type(within(dialog).getByRole("combobox", { name: "Ticker ou nome" }), "hash11");
    const b3 = within(dialog).getByRole("switch", { name: /Negociado na B3/ });
    expect(b3).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Aumentar quantidade" }));
    await user.click(within(dialog).getByRole("button", { name: "Adicionar à carteira" }));
    await waitFor(() =>
      expect(lastRequest("POST", "/api/v1/assets")?.body).toEqual({
        asset_class: "crypto",
        symbol: "HASH11",
        quantity: 1,
        score_override: 5,
        listed_on_b3: true,
      }),
    );
    const crypto = screen.getByRole("region", { name: "Criptomoedas" });
    expect(await within(crypto).findByTitle("Negociado na B3")).toHaveTextContent("B3");
    expect(within(crypto).getByText(/1 cota · R\$\s100,00/)).toBeInTheDocument();

    // Desligada a chave, o mesmo ticker volta a ser cripto: fração, sem − e +.
    await user.click(screen.getByRole("button", { name: "Adicionar ativo" }));
    const second = await screen.findByRole("dialog", { name: "Novo ativo" });
    await user.click(within(second).getByRole("radio", { name: /Cripto/ }));
    await user.type(within(second).getByRole("combobox", { name: "Ticker ou nome" }), "bith11");
    const toggle = within(second).getByRole("switch", { name: /Negociado na B3/ });
    expect(toggle).toBeChecked();
    await user.click(toggle);
    expect(within(second).queryByRole("button", { name: "Aumentar quantidade" })).toBeNull();
  });

  it("liga a B3 num ativo de Cripto já cadastrado", async () => {
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
    const { user } = await openPortfolio();
    const crypto = screen.getByRole("region", { name: "Criptomoedas" });
    await user.click(within(crypto).getByRole("button", { name: /QBTC11/ }));
    const dialog = await screen.findByRole("dialog", { name: "QBTC11" });
    await user.click(within(dialog).getByRole("switch", { name: /Negociado na B3/ }));
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/assets/a-qbtc")?.body).toEqual({ listed_on_b3: true }));
    expect(await within(crypto).findByText(/3 cotas · R\$\s52,00/)).toBeInTheDocument();
  });

  it("renda fixa não troca de mercado na edição: ticker e valor são cadastros diferentes", async () => {
    const { user } = await openPortfolio();
    const rf = screen.getByRole("region", { name: "Renda Fixa" });
    await user.click(within(rf).getByRole("button", { name: /Tesouro IPCA\+ 2035/ }));
    const dialog = await screen.findByRole("dialog", { name: "Tesouro IPCA+ 2035" });
    expect(within(dialog).queryByRole("switch", { name: /Negociado na B3/ })).toBeNull();
  });

  it("adicionar a partir da classe vazia já vem com a classe escolhida", async () => {
    const { user } = await openPortfolio();
    const intl = screen.getByRole("region", { name: "Ações Internacionais" });
    await user.click(within(intl).getByRole("button", { name: /Adicionar/ }));
    const dialog = await screen.findByRole("dialog", { name: "Novo ativo" });
    expect(within(dialog).getByRole("radio", { name: /Ações EUA/ })).toHaveAttribute("aria-checked", "true");
    await user.click(within(dialog).getByRole("button", { name: "Fechar" }));
    await user.click(within(screen.getByRole("region", { name: "Outras classes" })).getByRole("button", { name: /REITs/ }));
    const second = await screen.findByRole("dialog", { name: "Novo ativo" });
    expect(within(second).getByRole("radio", { name: /REITs/ })).toHaveAttribute("aria-checked", "true");
  });

  it("edita o checklist: a nota muda ao vivo e só o que mudou vai no PATCH", async () => {
    const { user } = await openPortfolio();
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    await user.click(within(fiis).getByRole("button", { name: /BTLG11/ }));
    const dialog = await screen.findByRole("dialog", { name: "BTLG11" });
    expect(within(dialog).getByText("6 de 6 respondidas")).toBeInTheDocument();
    const question = within(dialog).getByRole("group", { name: "Está negociado com P/VP abaixo de 1?" });
    await user.click(within(question).getByRole("button", { name: "Sim" }));
    expect(within(dialog).getAllByText("6").length).toBeGreaterThan(0);
    const other = within(dialog).getByRole("group", { name: "Os imóveis ficam em regiões de alta demanda?" });
    await user.click(within(other).getByRole("button", { name: "Sim" }));
    expect(within(dialog).getByText("5 de 6 respondidas")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));

    expect(await screen.findByText("BTLG11 atualizado.")).toBeInTheDocument();
    expect(lastRequest("PATCH", "/api/v1/assets/a-btlg")?.body).toEqual({ answers: { r6: true, r1: null } });
  });

  it("preço manual, nota manual e quantidade", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: /TRXF11/ }));
    const dialog = await screen.findByRole("dialog", { name: "TRXF11" });
    await user.click(within(dialog).getByRole("button", { name: "Aumentar quantidade" }));
    await user.click(within(dialog).getByRole("switch", { name: /Usar preço manual/ }));
    const price = within(dialog).getByLabelText("Preço por unidade");
    await user.clear(price);
    await user.type(price, "5500");
    await user.click(within(dialog).getByRole("switch", { name: /Definir a nota na mão/ }));
    expect(within(dialog).queryByRole("group", { name: /regiões de alta demanda/ })).toBeNull();
    fireEvent.change(within(dialog).getByLabelText("Nota de confiança (0 a 10)"), { target: { value: "0" } });
    expect(within(dialog).getByText("Com nota 0, este ativo não recebe aporte.")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() =>
      expect(lastRequest("PATCH", "/api/v1/assets/a-trxf")?.body).toEqual({
        quantity: 21,
        manual_price: 55,
        score_override: 0,
      }),
    );
  });

  it("tirar preço e nota manuais manda null", async () => {
    const db = getDb();
    const trxf = db.assets.find((a) => a.id === "a-trxf");
    if (trxf) {
      trxf.manual_price = 50;
      trxf.score_override = 3;
    }
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: /TRXF11/ }));
    const dialog = await screen.findByRole("dialog", { name: "TRXF11" });
    expect(within(dialog).getByText(/R\$\s50,00 ·\s+preço manual/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("switch", { name: /Usar preço manual/ }));
    await user.click(within(dialog).getByRole("switch", { name: /Definir a nota na mão/ }));
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() =>
      expect(lastRequest("PATCH", "/api/v1/assets/a-trxf")?.body).toEqual({ manual_price: null, score_override: null }),
    );
  });

  it("cripto e renda fixa editam nota e valor; salvar sem mudança só fecha", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: /BTC/ }));
    let dialog = await screen.findByRole("dialog", { name: "BTC" });
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull(), { timeout: 3000 });
    expect(lastRequest("PATCH", "/api/v1/assets/a-btc")).toBeUndefined();

    await user.click(screen.getByRole("button", { name: /BTC/ }));
    dialog = await screen.findByRole("dialog", { name: "BTC" });
    fireEvent.change(within(dialog).getByLabelText("Nota de confiança (0 a 10)"), { target: { value: "9" } });
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/assets/a-btc")?.body).toEqual({ score_override: 9 }));

    await user.click(await screen.findByRole("button", { name: /Tesouro IPCA\+ 2035/ }));
    dialog = await screen.findByRole("dialog", { name: "Tesouro IPCA+ 2035" });
    const value = within(dialog).getByLabelText("Valor atual");
    await user.clear(value);
    await user.type(value, "9000000");
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(lastRequest("PATCH", "/api/v1/assets/a-ipca")?.body).toEqual({ amount: 90000 }));
  });

  it("remove com confirmação na própria folha", async () => {
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: /MXRF11/ }));
    const dialog = await screen.findByRole("dialog", { name: "MXRF11" });
    await user.click(within(dialog).getByRole("button", { name: /Remover da carteira/ }));
    const confirm = within(dialog).getByRole("group", { name: "Confirmar remoção" });
    await user.click(within(confirm).getByRole("button", { name: "Cancelar" }));
    await user.click(within(dialog).getByRole("button", { name: /Remover da carteira/ }));
    await user.click(within(within(dialog).getByRole("group", { name: "Confirmar remoção" })).getByRole("button", { name: "Remover" }));
    expect(await screen.findByText("MXRF11 saiu da carteira.")).toBeInTheDocument();
    expect(getDb().assets.some((a) => a.id === "a-mxrf")).toBe(false);
  });

  it("mostra erro de salvar sem fechar", async () => {
    server.use(
      http.patch("/api/v1/assets/:id", () =>
        HttpResponse.json({ code: "not_found", status: 404, title: "x" }, { status: 404, headers: { "Content-Type": "application/problem+json" } }),
      ),
    );
    const { user } = await openPortfolio();
    await user.click(screen.getByRole("button", { name: /KNRI11/ }));
    const dialog = await screen.findByRole("dialog", { name: "KNRI11" });
    await user.click(within(dialog).getByRole("button", { name: "Aumentar quantidade" }));
    await user.click(within(dialog).getByRole("button", { name: "Salvar" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Não encontrei esse item");
  });

  it("lista o histórico de aportes", async () => {
    getDb().contributions.push({
      id: "c-1",
      created_at: "2026-11-20T15:00:00Z",
      amount: 1000,
      cash_used: 0,
      invested: 990.3,
      items: [
        { asset_id: "a-trxf", symbol: "TRXF11", display_name: "TRX Real Estate", quantity: 10, unit_price: 53.81, value: 538.1 },
        { asset_id: "a-btc", symbol: "BTC", display_name: "Bitcoin", quantity: 0.0013, unit_price: 347846, value: 452.2 },
      ],
    });
    await openPortfolio();
    const history = screen.getByRole("region", { name: "Histórico de aportes" });
    expect(await within(history).findByText(/R\$\s990,30/)).toBeInTheDocument();
    expect(within(history).getByText("10 TRXF11 · 0,0013 BTC")).toBeInTheDocument();
  });

  it("histórico com erro diz que não carregou", async () => {
    server.use(http.get("/api/v1/contributions", () => new HttpResponse(null, { status: 500 })));
    await openPortfolio();
    expect(await screen.findByText("Não consegui carregar o histórico agora.", {}, { timeout: 4000 })).toBeInTheDocument();
  });
});
