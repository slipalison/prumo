import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { Analysis, RuleResult, RuleStatus } from "../../api/analysisTypes";
import { analysisState, defaultAnalysis, result, scoreOf } from "../../test/analysisHandlers";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

async function openAnalysis() {
  const rendered = renderApp("analise");
  await screen.findByRole("heading", { name: "Análise da carteira", level: 1 });
  await screen.findByRole("region", { name: "Resumo" });
  return rendered;
}

const card = (name: string) => within(screen.getByRole("region", { name }));
/** A linha da regra na tabela da dimensão (abre o recolher do que está ok, sem dado ou desligado). */
const ruleItem = (region: ReturnType<typeof card>, sentence: string) => {
  for (const details of region.queryAllByRole("group")) {
    if (details instanceof HTMLDetailsElement) details.open = true;
  }
  return region.getByRole("rowheader", { name: sentence }).closest("tr") as HTMLElement;
};
/** O que está numa coluna da linha ("Agora", "Limite", "Diagnóstico"). */
const cellOf = (row: HTMLElement, column: string) => row.querySelector(`td[data-label="${column}"]`) as HTMLElement;
/** A pontuação de uma dimensão no topo. */
const scoreCard = (name: string) =>
  within(
    within(screen.getByRole("list", { name: "Pontuação por dimensão" }))
      .getAllByRole("listitem")
      .find((li) => li.querySelector(".dim-score__title")?.textContent === name) as HTMLElement,
  );

/** A análise da carteira fictícia com todas as regras no estado dado. */
function allIn(status: RuleStatus): Analysis {
  const base = defaultAnalysis();
  const dimensions = base.dimensions.map((d) => {
    const rules: RuleResult[] = d.rules.map((r) => result(r.rule, status, status === "ok" ? { current: 1 } : {}));
    return { ...d, rules, score: scoreOf(rules) };
  });
  return {
    ...base,
    dimensions,
    attention_count: dimensions.flatMap((d) => d.rules).filter((r) => r.status === "warning").length,
  };
}

describe("tela Análise", () => {
  it("é uma tela de topo entre Desempenho e Ajustes", async () => {
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    const nav = screen.getByRole("navigation", { name: "Principal" });
    const labels = within(nav)
      .getAllByRole("link")
      .map((link) => (link.textContent ?? "").trim())
      .filter((text) => ["Desempenho", "Análise", "Ajustes"].includes(text));
    expect(labels).toEqual(["Desempenho", "Análise", "Ajustes"]);
    await user.click(within(nav).getByRole("link", { name: "Análise" }));
    expect(await screen.findByRole("heading", { name: "Análise da carteira", level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/analise");
    expect(document.title).toBe("Análise · Prumo");
    expect(within(nav).getByRole("link", { name: "Análise" })).toHaveAttribute("aria-current", "page");
  });

  it("resume os pontos a ver e diz que regra sem dado não é falha", async () => {
    const { user } = await openAnalysis();
    // A explicação da tela fica no ⓘ do título.
    expect(screen.queryByText(/Regra sem dado para medir fica de fora da conta/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sobre a análise" }));
    expect(screen.getByText(/Regra sem dado para medir fica de fora da conta/)).toBeInTheDocument();
    const verdict = card("Resumo");
    expect(verdict.getByText("13 pontos a ver")).toBeInTheDocument();
    expect(verdict.getByText(/Das 26 regras que tinham o que medir, 13 passaram do seu limite/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Ajustar as regras" })).toHaveAttribute("href", "#/ajustes/analise");
  });

  it("cada dimensão mostra o percentual de regras ok com o número exato", async () => {
    await openAnalysis();

    // No topo, as quatro pontuações lado a lado.
    const diversification = scoreCard("Diversificação");
    expect(diversification.getByText("33,3%")).toBeInTheDocument();
    expect(diversification.getByText(/3 de 9 regras ok · 1 regra sem dado, fora da conta/)).toBeInTheDocument();
    expect(diversification.getByText("6 a ver")).toBeInTheDocument();
    const meter = diversification.getByRole("meter", { name: "Diversificação: 3 de 9 regras ok" });
    expect(meter).toHaveAttribute("aria-valuenow", "33.3");

    const dividends = scoreCard("Dividendos");
    expect(dividends.getByText("40%")).toBeInTheDocument();
    expect(dividends.getByText(/2 de 5 regras ok · 1 regra sem dado, fora da conta/)).toBeInTheDocument();

    const returns = scoreCard("Rentabilidade");
    expect(returns.getByText("66,7%")).toBeInTheDocument();
    expect(returns.getByText(/4 de 6 regras ok · 3 regras sem dado, fora da conta/)).toBeInTheDocument();

    // A tabela da dimensão repete a pontuação no cabeçalho.
    expect(card("Diversificação").getByText("33,3%")).toBeInTheDocument();
    expect(card("Diversificação").getByText("3 de 9 regras ok")).toBeInTheDocument();
  });

  it("o cartão da pontuação leva até a tabela da dimensão", async () => {
    const { user } = await openAnalysis();
    await user.click(screen.getByRole("button", { name: "Ver as regras de Rentabilidade" }));
    expect(screen.getByRole("region", { name: "Rentabilidade" })).toHaveFocus();
  });

  it("põe os pontos a ver primeiro, depois o que está ok, o que não tem dado e o que está desligado", async () => {
    analysisState().analysis = (() => {
      const base = defaultAnalysis();
      const diversification = base.dimensions[0];
      if (!diversification) throw new Error("sem dimensão");
      const rules = diversification.rules.map((r) => (r.rule === "min_classes" ? result("min_classes", "off") : r));
      base.dimensions[0] = { ...diversification, rules, score: scoreOf(rules) };
      return base;
    })();
    await openAnalysis();
    const region = card("Diversificação");
    // Os pontos a ver ficam à vista; o resto, recolhido com a contagem.
    expect(region.getByText("2 regras ok · 1 sem dado · 1 desligada")).toBeInTheDocument();
    const statuses = region.getAllByRole("row").filter((tr) => tr.classList.contains("analysis-rule"));
    expect(statuses.map((tr) => [...tr.classList].find((c) => c.startsWith("analysis-rule--")))).toEqual([
      ...Array.from({ length: 6 }, () => "analysis-rule--warning"),
      "analysis-rule--ok",
      "analysis-rule--ok",
      "analysis-rule--no_data",
      "analysis-rule--off",
    ]);
  });

  it("as sete regras originais mostram os achados com o número, o limite e o que fazer", async () => {
    const { user } = await openAnalysis();
    const diversification = card("Diversificação");
    const largest = ruleItem(diversification, "Nenhum ativo acima de 15% da carteira");
    // Cada achado numa linha do diagnóstico, com o nome à vista.
    expect(within(largest).getByText("Tesouro IPCA+ 2035 é 70,76% da carteira")).toBeInTheDocument();
    expect(within(largest).getByText("ITSA4 é 23,31% da carteira")).toBeInTheDocument();
    expect(cellOf(largest, "Agora")).toHaveTextContent("70,76%");
    expect(cellOf(largest, "Limite")).toHaveTextContent("máx. 15%");
    expect(within(largest).getByRole("meter")).toHaveAttribute("aria-valuenow", "70.76");
    expect(within(largest).getByRole("link", { name: "Calcular o próximo aporte" })).toHaveAttribute("href", "#/aportar");
    // O que fazer por extenso, com o aporte de cada achado, no ⓘ da linha.
    await user.click(within(largest).getByRole("button", { name: "O que fazer: Nenhum ativo acima de 15% da carteira" }));
    expect(
      within(largest).getByText(/Tesouro IPCA\+ 2035 voltar ao limite sem vender nada, aporte cerca de R\$\s446 mil em outros ativos/),
    ).toBeInTheDocument();
    expect(within(largest).getByText(/ITSA4 voltar ao limite sem vender nada, aporte cerca de R\$\s66,5\smil/)).toBeInTheDocument();

    const shallow = ruleItem(diversification, "Pelo menos 3 ativos em cada classe que tem meta");
    expect(within(shallow).getByText("Ações Nacionais tem 1 ativo")).toBeInTheDocument();
    expect(cellOf(shallow, "Limite")).toHaveTextContent("mín. 3");
    expect(within(shallow).queryByRole("meter")).not.toBeInTheDocument();

    // As de renda moram em Dividendos agora.
    const dividends = card("Dividendos");
    const gaps = ruleItem(dividends, "Provento recebido em pelo menos 12 dos últimos 12 meses fechados");
    expect(within(gaps).getByText("Você recebeu provento em 10 dos últimos 12 meses")).toBeInTheDocument();
    expect(within(gaps).getByText("Meses sem provento: fev/2026, abr/2026")).toBeInTheDocument();
    expect(within(gaps).getByRole("link", { name: "Abrir a Renda" })).toHaveAttribute("href", "#/renda");
    expect(dividends.getByText("KNRI11 pagou 80,01% da renda de 12 meses")).toBeInTheDocument();
  });

  it("as regras novas mostram o número, o limite, a conta e o que fazer", async () => {
    const { user } = await openAnalysis();
    const diversification = card("Diversificação");
    const abroad = ruleItem(diversification, "Pelo menos 10% da carteira no exterior");
    expect(within(abroad).getByText("Ponto a ver")).toBeInTheDocument();
    expect(cellOf(abroad, "Agora")).toHaveTextContent("0%");
    expect(cellOf(abroad, "Limite")).toHaveTextContent("mín. 10%");
    expect(within(abroad).getByText("0% da carteira está no exterior.")).toBeInTheDocument();
    expect(within(abroad).queryByText(/aporte cerca de/)).not.toBeInTheDocument();
    await user.click(within(abroad).getByRole("button", { name: "O que fazer: Pelo menos 10% da carteira no exterior" }));
    expect(within(abroad).getByText(/aporte cerca de R\$\s14,5\smil em ativos de fora/)).toBeInTheDocument();
    expect(within(abroad).queryByRole("meter")).not.toBeInTheDocument();

    const dividends = card("Dividendos");
    const paying = ruleItem(dividends, "Pelo menos 70% dos ativos de renda variável pagaram provento em 12 meses");
    expect(within(paying).getByText("2 de 4 ativos pagaram provento em 12 meses. Sem provento: BTLG11, ITSA4.")).toBeInTheDocument();
    const growth = ruleItem(dividends, "Renda de 12 meses maior que a dos 12 meses anteriores");
    expect(within(growth).getByText("Ok")).toBeInTheDocument();
    expect(cellOf(growth, "Agora")).toHaveTextContent("18,50%");
    expect(cellOf(growth, "Limite")).toBeEmptyDOMElement();

    const returns = card("Rentabilidade");
    const cdi = ruleItem(returns, "A carteira rende ao menos o CDI em 12 meses");
    expect(cellOf(cdi, "Limite")).toHaveTextContent("CDI 13,20%");
    expect(cellOf(cdi, "Agora")).toHaveTextContent("9,40%");
    expect(
      within(cdi).getByText("De 25/11/2025 a 25/11/2026 a carteira rendeu +9,40%; o CDI, +13,20%."),
    ).toBeInTheDocument();
    expect(within(cdi).getByRole("link", { name: "Abrir o Desempenho" })).toHaveAttribute("href", "#/desempenho");
    await user.click(within(cdi).getByRole("button", { name: /^O que fazer: A carteira rende ao menos o CDI/ }));
    expect(within(cdi).getByText(/não manda trocar nada/)).toBeInTheDocument();

    const loss = ruleItem(returns, "Nenhum ativo com perda maior que 30% desde a compra");
    expect(within(loss).getByText("BTLG11 está 41,50% abaixo do que você pagou.")).toBeInTheDocument();
    expect(within(loss).getByRole("meter")).toHaveAttribute("aria-valuenow", "41.5");
    await user.click(within(loss).getByRole("button", { name: /^O que fazer: Nenhum ativo com perda/ }));
    expect(within(loss).getByText(/O Prumo não manda vender/)).toBeInTheDocument();
  });

  it("regra sem dado diz o motivo e não vira aviso", async () => {
    await openAnalysis();
    const sectors = ruleItem(card("Diversificação"), "Ações em pelo menos 4 setores");
    expect(within(sectors).getByText("Sem dado")).toBeInTheDocument();
    expect(within(sectors).getByText("Precisa de ações com setor conhecido na fonte pública.")).toBeInTheDocument();
    expect(within(sectors).queryByText("Ponto a ver")).not.toBeInTheDocument();
    expect(cellOf(sectors, "Agora")).toBeEmptyDOMElement();

    const fiveYears = ruleItem(card("Rentabilidade"), "A carteira rende ao menos o CDI em 5 anos");
    expect(within(fiveYears).getByText("A série da carteira ou do índice ainda não cobre 5 anos.")).toBeInTheDocument();
  });

  it("regra desligada diz que não entra na pontuação", async () => {
    analysisState().analysis = (() => {
      const base = defaultAnalysis();
      const returns = base.dimensions[2];
      if (!returns) throw new Error("sem dimensão");
      const rules = returns.rules.map((r) => (r.rule === "max_loss" ? result("max_loss", "off") : r));
      base.dimensions[2] = { ...returns, rules, score: scoreOf(rules) };
      return base;
    })();
    await openAnalysis();
    const loss = ruleItem(card("Rentabilidade"), "Nenhum ativo com perda maior que 30% desde a compra");
    expect(within(loss).getByText("Desligada")).toBeInTheDocument();
    expect(within(loss).getByText(/não entra na pontuação/)).toBeInTheDocument();
  });

  it("a Qualidade dos ativos pontua como as outras e pesa pelo valor de quem tem o dado", async () => {
    const { user } = await openAnalysis();
    const quality = scoreCard("Qualidade dos ativos");
    expect(quality.getByText("66,7%")).toBeInTheDocument();
    expect(quality.getByText(/4 de 6 regras ok · 2 regras sem dado, fora da conta/)).toBeInTheDocument();
    expect(quality.getByRole("meter", { name: "Qualidade dos ativos: 4 de 6 regras ok" })).toHaveAttribute(
      "aria-valuenow",
      "66.7",
    );
    expect(document.body.textContent).not.toMatch(/Aguardando dados de fundamentos/);

    const table = card("Qualidade dos ativos");
    const roe = ruleItem(table, "Pelo menos 70% do valor em ações vem de empresas com ROE de 10% ou mais");
    expect(within(roe).getByText("Ponto a ver")).toBeInTheDocument();
    expect(cellOf(roe, "Agora")).toHaveTextContent("0%");
    expect(cellOf(roe, "Limite")).toHaveTextContent("mín. 70%");
    expect(within(roe).getByText("Entraram na conta 1 de 1 ativo. Abaixo do critério: ITSA4.")).toBeInTheDocument();
    expect(within(roe).getByRole("link", { name: "Abrir a Carteira" })).toHaveAttribute("href", "#/carteira");
    await user.click(within(roe).getByRole("button", { name: /^O que fazer: Pelo menos 70% do valor em ações vem de empresas com ROE/ }));
    expect(within(roe).getByText(/não manda vender/)).toBeInTheDocument();

    const pvp = ruleItem(table, "Pelo menos 70% do valor em FIIs vem de fundos com P/VP de até 1,10");
    expect(
      within(pvp).getByText("Entraram na conta 4 de 4 ativos. Abaixo do critério: KNRI11, HGLG11."),
    ).toBeInTheDocument();

    // Quem não tem dado fica fora da conta, com o motivo, e não é aviso.
    const leverage = ruleItem(table, "Pelo menos 70% do valor em ações vem de empresas com dívida líquida de até 3 vezes o EBITDA");
    expect(within(leverage).getByText("Sem dado")).toBeInTheDocument();
    expect(within(leverage).getByText("Precisa de empresa que não seja banco, seguradora nem holding, com EBITDA positivo.")).toBeInTheDocument();

    // A regra sem limite: recuperação judicial.
    const recovery = ruleItem(table, "Nenhuma empresa da carteira está em recuperação judicial");
    expect(within(recovery).getByText("Ok")).toBeInTheDocument();
    expect(within(recovery).getByText("Nenhuma empresa está em recuperação judicial (1 com cadastro na CVM).")).toBeInTheDocument();
  });

  it("os pontos de atenção listam, por ativo, fatos curtos com o ícone do ativo", async () => {
    await openAnalysis();
    const attention = card("Pontos de atenção dos ativos");
    const itsa = attention.getByRole("list", { name: "Pontos de atenção de ITSA4" });
    expect(within(itsa).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "ROE de 4,12%, abaixo de 10%",
      "Margem líquida de 8,33%, abaixo de 10%",
      "Dívida líquida de 1,5 vezes o patrimônio líquido",
      "Receita cresceu 2,34% ao ano em 5 anos, abaixo de 5%",
    ]);
    const btlg = attention.getByRole("list", { name: "Pontos de atenção de BTLG11" });
    expect(within(btlg).getByText("Provento de 12 meses caiu 22,50% em relação aos 12 meses anteriores")).toBeInTheDocument();
    const rows = attention.getAllByRole("listitem").filter((li) => li.classList.contains("attention"));
    expect(rows).toHaveLength(2);
    for (const row of rows) expect(row.querySelector(".asset-icon")).not.toBeNull();
    expect(attention.getByText(/5 ativos avaliados; 1 ação ou FII ainda sem dados/)).toBeInTheDocument();
    expect(attention.getByText(/Não são recomendação/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/Aguardando dados de fundamentos/);
  });

  it("os pontos de atenção dizem quando não há ponto e quando não há dado, sem fingir tudo certo", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = { ...base, attention: { assessed: 3, without_data: 0, items: [] } };
    const first = await openAnalysis();
    expect(card("Pontos de atenção dos ativos").getByText("Nenhum ponto de atenção nos 3 ativos com fundamentos.")).toBeInTheDocument();
    first.unmount();

    analysisState().analysis = { ...base, attention: { assessed: 1, without_data: 0, items: [] } };
    const second = await openAnalysis();
    expect(card("Pontos de atenção dos ativos").getByText("Nenhum ponto de atenção nos 1 ativo com fundamentos.")).toBeInTheDocument();
    second.unmount();

    analysisState().analysis = { ...base, attention: { assessed: 0, without_data: 2, items: [] } };
    await openAnalysis();
    const attention = card("Pontos de atenção dos ativos");
    expect(attention.getByText(/Ainda sem dados de fundamentos para avaliar/)).toBeInTheDocument();
    expect(attention.getByText(/nem “tudo certo”/)).toBeInTheDocument();
    expect(attention.queryByText(/Nenhum ponto de atenção nos/)).not.toBeInTheDocument();
  });

  it("perfil de risco: agora e metas, com rótulo, números e a diferença", async () => {
    const { user } = await openAnalysis();
    const profile = card("Perfil de risco");
    expect(profile.getByText("Conservador")).toBeInTheDocument();
    expect(profile.getByText("Moderado")).toBeInTheDocument();
    expect(
      profile.getByRole("img", { name: "Agora: perfil conservador, renda fixa 70,8%, renda variável 29,2%" }),
    ).toBeInTheDocument();
    expect(
      profile.getByRole("img", { name: "Suas metas: perfil moderado, renda fixa 35,0%, renda variável 65,0%" }),
    ).toBeInTheDocument();
    expect(profile.getByText("Renda fixa 70,8%")).toBeInTheDocument();
    expect(profile.getByText("Renda variável 65,0%")).toBeInTheDocument();
    expect(
      profile.getByText("A renda variável está 35,8 pontos percentuais abaixo do que as suas metas pedem."),
    ).toBeInTheDocument();
    // Os limiares do Prumo ficam no ⓘ.
    await user.click(profile.getByRole("button", { name: "Como o perfil de risco é calculado" }));
    expect(profile.getByText(/até 30% em renda variável é conservador, até 70% é moderado e acima disso é arrojado/)).toBeInTheDocument();
  });

  it("perfil de risco sem meta ou sem ativos diz o que falta", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = { ...base, profile: { ...base.profile, target: null } };
    const first = await openAnalysis();
    expect(card("Perfil de risco").getByText(/Defina as metas por classe, em Ajustes, Metas/)).toBeInTheDocument();
    expect(card("Perfil de risco").queryByText(/pontos percentuais/)).not.toBeInTheDocument();
    first.unmount();

    analysisState().analysis = { ...base, profile: { ...base.profile, current: null } };
    await openAnalysis();
    expect(card("Perfil de risco").getByText("Ainda não há ativos com valor.")).toBeInTheDocument();
  });

  it("perfil de risco no perfil da meta diz isso", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = {
      ...base,
      profile: {
        ...base.profile,
        current: { fixed_income: 35.4, variable: 64.6, profile: "moderate" },
      },
    };
    await openAnalysis();
    expect(card("Perfil de risco").getByText("A sua carteira está no perfil que as suas metas pedem.")).toBeInTheDocument();
  });

  it("destaques: quem subiu e quem caiu, em preço e desde a compra", async () => {
    await openAnalysis();
    const highlights = card("Destaques de 12 meses");
    const lists = highlights.getAllByRole("list");
    const price = within(lists[0] as HTMLElement);
    expect(price.getByText("KNRI11").nextSibling).toHaveTextContent("+18,40%");
    expect(price.getByText("ITSA4").nextSibling).toHaveTextContent("+6,25%");
    const fell = highlights.getAllByRole("list", { name: "Mais caíram" });
    expect(within(fell[0] as HTMLElement).getByText("BTLG11").nextSibling).toHaveTextContent("−12,80%");
    expect(within(fell[1] as HTMLElement).getByText("BTLG11").nextSibling).toHaveTextContent("−41,50%");
    expect(highlights.getByText("Pela variação do preço em 12 meses")).toBeInTheDocument();
    expect(highlights.getByText("Pela rentabilidade desde a compra")).toBeInTheDocument();
  });

  it("destaques sem histórico dizem por que estão vazios", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = {
      ...base,
      highlights: {
        price_12m: { best: [], worst: [] },
        since_purchase: { best: [], worst: [] },
      },
    };
    await openAnalysis();
    const highlights = card("Destaques de 12 meses");
    expect(highlights.getAllByText("Nenhum, ou ainda sem 12 meses de histórico de preço.")).toHaveLength(2);
    expect(highlights.getAllByText("Nenhum com custo conhecido no livro.")).toHaveLength(2);
  });

  it("referências de preço: ações e FIIs em tabelas próprias, rotuladas como públicas e sem recomendar", async () => {
    const { user } = await openAnalysis();
    const references = card("Referências de preço");
    expect(references.getByRole("note")).toHaveTextContent("Referência pública, não é recomendação");
    expect(references.getByRole("link", { name: "Mudar o rendimento-alvo e os prêmios" })).toHaveAttribute(
      "href",
      "#/ajustes/analise",
    );
    // Ações: Bazin, Graham e Pelas duas.
    const stocks = within(references.getByRole("table", { name: "Ações" }));
    const itsa = stocks.getByRole("rowheader", { name: "ITSA4" }).closest("tr") as HTMLElement;
    expect(cellOf(itsa, "Bazin")).toHaveTextContent(/R\$\s12,50/);
    expect(cellOf(itsa, "Graham")).toHaveTextContent(/R\$\s9,50/);
    expect(cellOf(itsa, "Pelas duas")).toHaveTextContent(/R\$\s9,50/);
    expect(cellOf(itsa, "Seu teto")).toHaveTextContent(/R\$\s10,00/);
    expect(cellOf(itsa, "Seu teto")).toHaveTextContent("Valor próprio");
    expect(cellOf(itsa, "Hoje")).toHaveTextContent(/R\$\s11,20/);
    await user.click(within(itsa).getByRole("button", { name: "Como sai: Bazin de ITSA4" }));
    expect(
      within(itsa).getByText(/Média de 4 anos de dividendos: R\$\s0,75 por cota ao ano, dividida por 6% ao ano/),
    ).toBeInTheDocument();
    // FIIs: o tipo, VP, Rendimento e Pelas duas; nunca Bazin nem Graham.
    const funds = within(references.getByRole("table", { name: "FIIs" }));
    expect(funds.queryByRole("columnheader", { name: "Bazin" })).toBeNull();
    const knri = funds.getByRole("rowheader", { name: "KNRI11" }).closest("tr") as HTMLElement;
    expect(cellOf(knri, "Tipo")).toHaveTextContent("Tijolo");
    expect(cellOf(knri, "VP")).toHaveTextContent(/R\$\s98,40/);
    expect(cellOf(knri, "Rendimento")).toHaveTextContent(/R\$\s137,21/);
    expect(cellOf(knri, "Pelas duas")).toHaveTextContent(/R\$\s98,40/);
    expect(cellOf(knri, "Seu teto")).toHaveTextContent("Sem teto");
    // A conta de verdade do Rendimento, no ⓘ.
    await user.click(within(knri).getByRole("button", { name: "Como sai: Rendimento de KNRI11" }));
    expect(
      within(knri).getByText(
        /Tijolo: alvo do tijolo 7,12% \+ 2,5 p\.p\. = 9,62%; proventos 12m R\$\s13,20 ÷ 9,62% = R\$\s137,21\./,
      ),
    ).toBeInTheDocument();
    const mxrf = funds.getByRole("rowheader", { name: "MXRF11" }).closest("tr") as HTMLElement;
    await user.click(within(mxrf).getByRole("button", { name: "Como sai: Rendimento de MXRF11" }));
    expect(within(mxrf).getByText(/alvo do papel: CDI 14,20% × 0,85 = 12,07% \+ 1,0 p\.p\. = 13,07%/)).toBeInTheDocument();
    const hibr = funds.getByRole("rowheader", { name: "HIBR11" }).closest("tr") as HTMLElement;
    expect(cellOf(hibr, "Tipo")).toHaveTextContent("Híbrido");
    await user.click(within(hibr).getByRole("button", { name: "Como sai: Rendimento de HIBR11" }));
    expect(within(hibr).getByText(/60% tijolo \(9,62%\) \+ 40% papel \(13,07%\) = 11,00%/)).toBeInTheDocument();
    const foff = funds.getByRole("rowheader", { name: "FOFF11" }).closest("tr") as HTMLElement;
    await user.click(within(foff).getByRole("button", { name: "Como sai: Rendimento de FOFF11" }));
    expect(within(foff).getByText(/a carteira dos fundos de baixo não é conhecida/)).toBeInTheDocument();
    // Sem tipo: sem Rendimento, com o motivo para o leitor de tela.
    const trxf = funds.getByRole("rowheader", { name: "TRXF11" }).closest("tr") as HTMLElement;
    expect(cellOf(trxf, "Tipo")).toHaveTextContent("Sem tipo");
    expect(cellOf(trxf, "Rendimento")).toHaveTextContent("tipo do FII ainda desconhecido");
    expect(cellOf(trxf, "Rendimento")).not.toHaveTextContent(/R\$/);
    // As fórmulas, com o rendimento-alvo e as taxas de hoje, no ⓘ do título.
    await user.click(references.getByRole("button", { name: "Como as referências são calculadas" }));
    expect(references.getByText(/hoje 6% ao ano/)).toBeInTheDocument();
    expect(
      references.getByText(/juro real 7,12% em 19\/11\/2026 \(Tesouro IPCA\+ com Juros Semestrais 2037\) \+ 2,5 p\.p\./),
    ).toBeInTheDocument();
    // Nada de "oportunidade" nem de "compre".
    expect(document.body.textContent).not.toMatch(/oportunidade|compre|preço justo|upside/i);
  });

  it("escolhe o teto pelo método, com o valor e o veredito de cada um, e grava só depois de confirmar", async () => {
    const { user } = await openAnalysis();
    const references = card("Referências de preço");
    await user.click(references.getByRole("button", { name: "Escolher o teto de KNRI11" }));
    const dialog = await screen.findByRole("dialog", { name: "Teto de KNRI11" });
    // O que está gravado vem marcado; cada método com o valor de hoje e o chip contra R$ 142,00.
    expect(within(dialog).getByRole("radio", { name: /Sem teto/ })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /^VP R\$\s98,40 não aprova/ })).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: /^Rendimento R\$\s137,21 não aprova/ })).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: /^Pelas duas R\$\s98,40 não aprova/ })).toBeInTheDocument();
    expect(within(dialog).queryByRole("radio", { name: /Bazin|Graham/ })).toBeNull();
    expect(getDb().requests.filter((r) => r.method === "PATCH")).toHaveLength(0);

    await user.click(within(dialog).getByRole("radio", { name: /^Rendimento/ }));
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(getDb().requests.filter((r) => r.method === "PATCH")).toHaveLength(0);

    await user.click(references.getByRole("button", { name: "Escolher o teto de KNRI11" }));
    const again = await screen.findByRole("dialog", { name: "Teto de KNRI11" });
    await user.click(within(again).getByRole("radio", { name: /^Rendimento/ }));
    await user.click(within(again).getByRole("button", { name: "Confirmar" }));
    await screen.findByText("Teto de KNRI11 atualizado.");
    const patch = getDb().requests.filter((r) => r.method === "PATCH" && r.path === "/api/v1/assets/a-knri").at(-1);
    expect(patch?.body).toEqual({ ceiling: { method: "fund_yield" } });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    // A tabela mostra o teto novo: o valor de hoje e o método.
    const knri = within(references.getByRole("table", { name: "FIIs" }))
      .getByRole("rowheader", { name: "KNRI11" })
      .closest("tr") as HTMLElement;
    await waitFor(() => expect(cellOf(knri, "Seu teto")).toHaveTextContent(/R\$\s137,21\s*Rendimento/));
  });

  it("Sem teto tira o teto; Valor próprio grava o número; confirmar sem mudar não grava", async () => {
    const { user } = await openAnalysis();
    const references = card("Referências de preço");
    await user.click(references.getByRole("button", { name: "Escolher o teto de ITSA4" }));
    const dialog = await screen.findByRole("dialog", { name: "Teto de ITSA4" });
    expect(within(dialog).getByRole("radio", { name: /Valor próprio/ })).toBeChecked();
    expect(within(dialog).getByLabelText<HTMLInputElement>("Pagar no máximo, por unidade").value).toMatch(/10,00$/);
    await user.click(within(dialog).getByRole("button", { name: "Confirmar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(getDb().requests.filter((r) => r.method === "PATCH")).toHaveLength(0);

    await user.click(references.getByRole("button", { name: "Escolher o teto de ITSA4" }));
    const remove = await screen.findByRole("dialog", { name: "Teto de ITSA4" });
    await user.click(within(remove).getByRole("radio", { name: /Sem teto/ }));
    await user.click(within(remove).getByRole("button", { name: "Confirmar" }));
    await screen.findByText("Teto de ITSA4 removido.");
    expect(getDb().requests.filter((r) => r.method === "PATCH").at(-1)?.body).toEqual({ ceiling: null });
  });

  it("o teto pelo método sem valor aparece com o motivo, e o erro do servidor deixa a folha aberta", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = {
      ...base,
      references: {
        ...base.references,
        funds: base.references.funds.map((f) =>
          f.label === "TRXF11" ? { ...f, ceiling: { method: "fund_yield", value: null, unavailable_reason: "unknown_fund_type" } } : f,
        ),
      },
    };
    server.use(
      http.patch("/api/v1/assets/a-knri", () =>
        HttpResponse.json(
          { type: "urn:prumo:error:validation_failed", title: "Dados inválidos", status: 422, code: "validation_failed", detail: "Teto ruim." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openAnalysis();
    const references = card("Referências de preço");
    const funds = within(references.getByRole("table", { name: "FIIs" }));
    const trxf = funds.getByRole("rowheader", { name: "TRXF11" }).closest("tr") as HTMLElement;
    expect(cellOf(trxf, "Seu teto")).toHaveTextContent("Rendimento: sem valor");
    await user.click(within(trxf).getByRole("button", { name: "Por que o teto de TRXF11 está sem valor" }));
    expect(within(trxf).getByText(/tipo do FII ainda desconhecido\): TRXF11 fica fora do aporte/)).toBeInTheDocument();

    await user.click(references.getByRole("button", { name: "Escolher o teto de KNRI11" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("radio", { name: /^VP/ }));
    await user.click(within(dialog).getByRole("button", { name: "Confirmar" }));
    expect(await screen.findByText(/Teto ruim\./)).toBeInTheDocument();
    // O diálogo continua aberto para tentar de novo ou cancelar.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("sem ações nem FIIs, diz o que falta em vez de uma lista vazia", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = { ...base, references: { ...base.references, stocks: [], funds: [] } };
    await openAnalysis();
    expect(card("Referências de preço").getByText(/Sem ações nem FIIs da B3 na carteira/)).toBeInTheDocument();
    expect(card("Referências de preço").queryByRole("table")).toBeNull();
  });

  it("diz de onde vêm os dados, quando foram buscados e o que ficou de fora", async () => {
    const { user } = await openAnalysis();
    const sources = card("De onde vêm os dados");
    // Recolhido no fim; sem falha, a frase fechada não tem contagem.
    expect(sources.queryByText(/falh/)).not.toBeInTheDocument();
    await user.click(sources.getByText("De onde vêm os dados"));
    expect(sources.getByText(/Classificação Setorial da B3/)).toBeInTheDocument();
    expect(sources.getByText(/informe mensal de FII da CVM/)).toBeInTheDocument();
    expect(sources.getAllByText(/Atualizado/)).toHaveLength(2);
    expect(sources.getByText(/SEC\) exige identificar quem consulta/)).toBeInTheDocument();
    expect(sources.getByText(/1 ativo em Ações Nacionais ou FIIs sem setor na fonte/)).toBeInTheDocument();
    expect(sources.getByText(/Ele não entra nas regras de setor/)).toBeInTheDocument();
  });

  it("avisa quando a fonte de setor falhou ou nunca respondeu", async () => {
    const base = defaultAnalysis();
    analysisState().analysis = {
      ...base,
      unclassified_count: 3,
      sources: [
        { source: "b3_stocks", asset_class: "br_stocks", updated_at: null, failing: true },
        { source: "cvm_reits", asset_class: "br_reits", updated_at: "2026-11-21T09:00:00Z", failing: true },
      ],
    };
    const { user } = await openAnalysis();
    const sources = card("De onde vêm os dados");
    // A falha aparece já na frase fechada.
    expect(sources.getByText("2 fontes falharam")).toBeInTheDocument();
    await user.click(sources.getByText("De onde vêm os dados"));
    expect(sources.getByText(/Ainda não buscado\./)).toBeInTheDocument();
    expect(sources.getAllByText(/A última busca falhou/)).toHaveLength(2);
    expect(sources.getByText(/A última busca falhou; vale a anterior/)).toBeInTheDocument();
    expect(sources.getByText(/Eles não entram nas regras de setor/)).toBeInTheDocument();
  });

  it("sem nenhum ponto a ver e com regra medida, a carteira está no eixo", async () => {
    analysisState().analysis = allIn("ok");
    await openAnalysis();
    const verdict = card("Resumo");
    expect(verdict.getByText("Nenhum ponto a ver")).toBeInTheDocument();
    expect(verdict.getByText(/dentro dos seus limites em 33 regras/)).toBeInTheDocument();
  });

  it("sem o que medir, diz isso em vez de fingir que está tudo bem", async () => {
    analysisState().analysis = allIn("no_data");
    await openAnalysis();
    const verdict = card("Resumo");
    expect(verdict.getByText("Ainda sem o que medir")).toBeInTheDocument();
    const diversification = scoreCard("Diversificação");
    expect(diversification.getByText("Sem dado", { selector: "p" })).toBeInTheDocument();
    expect(diversification.getByText(/Nenhuma regra com dado ainda · 10 regras sem dado, fora da conta/)).toBeInTheDocument();
    expect(diversification.queryByRole("meter")).not.toBeInTheDocument();
    // Nenhum ponto a ver: a tabela diz isso, e as regras sem dado ficam recolhidas.
    expect(card("Diversificação").getByText("Nenhum ponto a ver nesta dimensão.")).toBeInTheDocument();
    expect(card("Diversificação").getByText("10 sem dado")).toBeInTheDocument();
  });

  it("um ponto a ver usa o singular", async () => {
    const base = allIn("ok");
    base.dimensions = base.dimensions.map((d) => {
      if (d.id !== "returns") return d;
      const rules = d.rules.map((r) => (r.rule === "max_loss" ? result("max_loss", "warning", { current: 41.5 }) : r));
      return { ...d, rules, score: scoreOf(rules) };
    });
    base.attention_count = 1;
    analysisState().analysis = base;
    await openAnalysis();
    expect(card("Resumo").getByText("1 ponto a ver")).toBeInTheDocument();
    expect(card("Resumo").getByText(/Das 33 regras que tinham o que medir, 1 passou do seu limite/)).toBeInTheDocument();
  });

  it("mostra o erro com tentar de novo", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/analysis", () =>
        fail
          ? HttpResponse.json({ code: "internal", status: 500 }, { status: 500, headers: { "Content-Type": "application/problem+json" } })
          : HttpResponse.json(defaultAnalysis()),
      ),
    );
    const { user } = renderApp("analise");
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("region", { name: "Resumo" })).toBeInTheDocument();
  });
});
