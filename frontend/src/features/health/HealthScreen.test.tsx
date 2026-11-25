import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

async function openHealth() {
  const rendered = renderApp("ajustes/saude");
  await screen.findByRole("heading", { name: "Saúde da carteira", level: 1 });
  await screen.findByRole("heading", { name: "Suas regras" });
  return rendered;
}

const lastPut = () => getDb().requests.filter((r) => r.method === "PUT" && r.path === "/api/v1/health/rules").at(-1);

describe("Saúde da carteira", () => {
  it("é uma seção de Ajustes, não um item da navegação, e diz que não há nota", async () => {
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    const main = screen.getByRole("navigation", { name: "Principal" });
    expect(within(main).queryByRole("link", { name: "Saúde" })).not.toBeInTheDocument();
    await user.click(within(main).getByRole("link", { name: "Ajustes" }));
    const sections = await screen.findByRole("navigation", { name: "Seções de Ajustes" });
    await user.click(within(sections).getByRole("link", { name: "Saúde da carteira" }));
    expect(await screen.findByRole("heading", { name: "Saúde da carteira", level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/ajustes/saude");
    expect(document.title).toBe("Ajustes · Prumo");
    expect(screen.getByText(/Não há nota: cada regra é sua/)).toBeInTheDocument();
  });

  it("mostra cada aviso com o que é, o número, o limite e o que fazer", async () => {
    await openHealth();
    const verdict = screen.getByRole("region", { name: "Resumo" });
    expect(within(verdict).getByText("8 avisos")).toBeInTheDocument();
    expect(within(verdict).getByText(/Das 7 regras que tinham o que medir, 7 passaram do limite/)).toBeInTheDocument();

    const warnings = within(screen.getByRole("list", { name: "Avisos" }));
    const ipca = warnings.getByRole("article", { name: "Tesouro IPCA+ 2035 é 70,76% da carteira" });
    expect(within(ipca).getByText("Agora").nextSibling).toHaveTextContent("70,76%");
    expect(within(ipca).getByText("Limite").nextSibling).toHaveTextContent("15%");
    expect(within(ipca).getByRole("meter")).toHaveAttribute("aria-valuenow", "70.76");
    expect(within(ipca).getByText(/O que fazer\./)).toBeInTheDocument();
    expect(within(ipca).getByText(/sem vender nada, aporte cerca de R\$\s446 mil em outros ativos/)).toBeInTheDocument();
    expect(within(ipca).getByRole("link", { name: "Calcular o próximo aporte" })).toHaveAttribute("href", "#/aportar");

    const top = warnings.getByRole("article", { name: "Os 5 maiores ativos somam 97,84% da carteira" });
    expect(within(top).getByText("Tesouro IPCA+ 2035, ITSA4, HGLG11, KNRI11, BTLG11")).toBeInTheDocument();
    expect(warnings.getByRole("article", { name: "Tijolo é 87,01% de Fundos Imobiliários" })).toBeInTheDocument();

    const shallow = warnings.getByRole("article", { name: "Ações Nacionais tem 1 ativo" });
    expect(within(shallow).getByText("Mínimo").nextSibling).toHaveTextContent("3");
    expect(within(shallow).getByText("Adicione mais 2 ativos em Ações Nacionais.", { exact: false })).toBeInTheDocument();
    expect(within(shallow).queryByRole("meter")).not.toBeInTheDocument();
    expect(within(shallow).getByRole("link", { name: "Abrir a Carteira" })).toHaveAttribute("href", "#/carteira");

    const empty = warnings.getByRole("article", { name: "Ações Internacionais tem meta e ainda nada comprado" });
    expect(within(empty).getByText(/Cadastre um ativo em Ações Internacionais/)).toBeInTheDocument();
  });

  it("mostra os avisos de renda: os meses vazios, quem paga neles e o pagador grande", async () => {
    await openHealth();
    const warnings = within(screen.getByRole("list", { name: "Avisos" }));

    const gaps = warnings.getByRole("article", { name: "Você recebeu provento em 10 dos últimos 12 meses" });
    expect(within(gaps).getByText("Meses sem provento: fev/2026, abr/2026")).toBeInTheDocument();
    expect(within(gaps).getByText("Agora").nextSibling).toHaveTextContent("10");
    expect(within(gaps).getByText("Mínimo").nextSibling).toHaveTextContent("12");
    expect(within(gaps).queryByRole("meter")).not.toBeInTheDocument();
    expect(within(gaps).getByText(/HGLG11 \(Fundos Imobiliários; já pagou em fev, abr\)/)).toBeInTheDocument();
    expect(within(gaps).getByRole("link", { name: "Abrir a Renda" })).toHaveAttribute("href", "#/renda");

    const payer = warnings.getByRole("article", { name: "KNRI11 pagou 80,01% da renda de 12 meses" });
    expect(within(payer).getByText("Limite").nextSibling).toHaveTextContent("25%");
    expect(within(payer).getByRole("meter")).toHaveAttribute("aria-valuenow", "80.01");
    expect(within(payer).getByText(/KNRI11 pagou R\$\s4,8\smil dos R\$\s6\smil que você recebeu em 12 meses/)).toBeInTheDocument();
  });

  it("sem pagador no histórico, manda olhar a Renda em vez de inventar ativo", async () => {
    getDb().warnings = [
      {
        rule: "income_every_month",
        subject: { kind: "income_months", empty_months: ["2026-03"], payers: [] },
        unit: "count",
        current: 11,
        limit: 12,
        needed_contribution: null,
      },
    ];
    await openHealth();
    const card = screen.getByRole("article", { name: "Você recebeu provento em 11 dos últimos 12 meses" });
    expect(within(card).getByText(/Nenhum ativo da carteira pagou nesses meses até agora/)).toBeInTheDocument();
  });

  it("o limite de meses vai de 1 a 12", async () => {
    const { user } = await openHealth();
    const rules = within(screen.getByRole("region", { name: "Suas regras" }));
    const months = rules.getByRole("textbox", { name: /Limite: Provento recebido em pelo menos/ });
    expect(months).toHaveValue("12");
    await user.clear(months);
    await user.type(months, "13");
    expect(await rules.findByText("Use um número inteiro de 1 a 12.")).toBeInTheDocument();
    expect(rules.getByRole("button", { name: "Salvar regras" })).toBeDisabled();
    await user.clear(months);
    await user.type(months, "10");
    expect(rules.getByRole("button", { name: "Salvar regras" })).toBeEnabled();
    expect(rules.getByRole("textbox", { name: /Limite: Nenhum pagador acima de/ })).toHaveValue("25");
  });

  it("lista as regras com estado em texto, e quem está desligada fica sem aviso", async () => {
    const { user } = await openHealth();
    const rules = within(screen.getByRole("region", { name: "Suas regras" }));
    const maxAsset = rules.getByRole("switch", { name: "Nenhum ativo acima de … da carteira" });
    expect(maxAsset).toBeChecked();
    expect(rules.getAllByText("Passou do limite")).toHaveLength(7);
    expect(rules.getByRole("textbox", { name: /Limite: Nenhum ativo acima de/ })).toHaveValue("15");

    await user.click(maxAsset);
    expect(rules.getByRole("textbox", { name: /Limite: Nenhum ativo acima de/ })).toBeDisabled();
    await user.click(rules.getByRole("button", { name: "Salvar regras" }));
    await screen.findByText("Regras salvas.");
    expect(lastPut()?.body).toEqual({ rules: { max_asset: { enabled: false } } });
    expect(await within(screen.getByRole("region", { name: "Suas regras" })).findByText("Desligada")).toBeInTheDocument();
    expect(screen.queryByRole("article", { name: /Tesouro IPCA\+ 2035 é/ })).not.toBeInTheDocument();
  });

  it("muda o limite com vírgula e só manda o que mudou", async () => {
    const { user } = await openHealth();
    const rules = within(screen.getByRole("region", { name: "Suas regras" }));
    const save = rules.getByRole("button", { name: "Salvar regras" });
    expect(save).toBeDisabled();

    const input = rules.getByRole("textbox", { name: /Limite: Nenhum setor acima de/ });
    await user.clear(input);
    await user.type(input, "55,5");
    expect(save).toBeEnabled();
    const min = rules.getByRole("textbox", { name: /Limite: Pelo menos/ });
    await user.clear(min);
    await user.type(min, "4");
    await user.click(save);
    await screen.findByText("Regras salvas.");
    expect(lastPut()?.body).toEqual({ rules: { max_sector: { threshold: 55.5 }, min_assets: { threshold: 4 } } });
    expect(getDb().healthRules.find((r) => r.rule === "max_sector")?.threshold).toBe(55.5);
    await waitFor(() => expect(rules.getByRole("textbox", { name: /Limite: Nenhum setor acima de/ })).toHaveValue("55,5"));
  });

  it("recusa limite que não faz sentido antes de enviar", async () => {
    const { user } = await openHealth();
    const rules = within(screen.getByRole("region", { name: "Suas regras" }));
    const input = rules.getByRole("textbox", { name: /Limite: Nenhum ativo acima de/ });
    await user.clear(input);
    await user.type(input, "150");
    expect(await rules.findByRole("alert")).toHaveTextContent("Use um percentual maior que 0 e até 100");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(rules.getByRole("button", { name: "Salvar regras" })).toBeDisabled();

    await user.clear(input);
    await user.type(input, "abc");
    expect(await rules.findByText("Digite um número.")).toBeInTheDocument();

    const min = rules.getByRole("textbox", { name: /Limite: Pelo menos/ });
    await user.clear(min);
    await user.type(min, "2,5");
    expect(await rules.findByText("Use um número inteiro de 1 a 50.")).toBeInTheDocument();

    // Desligar a regra tira a exigência: o que ela guarda não vale mais.
    await user.click(rules.getByRole("switch", { name: "Nenhum ativo acima de … da carteira" }));
    await user.clear(min);
    await user.type(min, "3");
    expect(rules.queryByRole("alert")).not.toBeInTheDocument();
    expect(rules.getByRole("button", { name: "Salvar regras" })).toBeEnabled();
  });

  it("volta ao limite de fábrica e descarta mudanças", async () => {
    const { user } = await openHealth();
    const rules = within(screen.getByRole("region", { name: "Suas regras" }));
    const input = rules.getByRole("textbox", { name: /Limite: Os 5 maiores/ });
    expect(rules.queryByRole("button", { name: /Voltar para/ })).not.toBeInTheDocument();
    await user.clear(input);
    await user.type(input, "80");
    await user.click(rules.getByRole("button", { name: "Voltar para 60%" }));
    expect(input).toHaveValue("60");
    expect(rules.getByRole("button", { name: "Salvar regras" })).toBeDisabled();

    await user.type(input, "5");
    await user.click(rules.getByRole("button", { name: "Descartar mudanças" }));
    expect(input).toHaveValue("60");
    expect(rules.queryByRole("button", { name: "Descartar mudanças" })).not.toBeInTheDocument();
  });

  it("mostra o erro do servidor ao salvar", async () => {
    server.use(
      http.put("/api/v1/health/rules", () =>
        HttpResponse.json(
          { type: "urn:prumo:error:validation_failed", title: "Dados inválidos", status: 422, code: "validation_failed", detail: "max_asset: limite ruim." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openHealth();
    const rules = within(screen.getByRole("region", { name: "Suas regras" }));
    await user.click(rules.getByRole("switch", { name: "Nenhum ativo acima de … da carteira" }));
    await user.click(rules.getByRole("button", { name: "Salvar regras" }));
    expect(await screen.findByText(/max_asset: limite ruim\./)).toBeInTheDocument();
    // A mudança fica na tela para a pessoa corrigir.
    expect(rules.getByRole("switch", { name: "Nenhum ativo acima de … da carteira" })).not.toBeChecked();
  });

  it("sem aviso e com regra medida, a carteira está no eixo", async () => {
    getDb().warnings = [];
    await openHealth();
    const verdict = screen.getByRole("region", { name: "Resumo" });
    expect(within(verdict).getByText("Nenhum aviso")).toBeInTheDocument();
    expect(within(verdict).getByText(/dentro dos seus limites em 7 regras/)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Avisos" })).not.toBeInTheDocument();
    expect(screen.getAllByText("Dentro do limite")).toHaveLength(7);
  });

  it("sem o que medir, diz isso em vez de fingir que está tudo bem", async () => {
    server.use(
      http.get("/api/v1/health", () =>
        HttpResponse.json({
          total_value: 0,
          warnings: [],
          rules: getDb().healthRules.map((r) => ({ ...r, status: "not_applicable" })),
          unclassified_count: 0,
          sources: getDb().sectorSources,
        }),
      ),
    );
    await openHealth();
    const verdict = screen.getByRole("region", { name: "Resumo" });
    expect(within(verdict).getByText("Ainda sem o que medir")).toBeInTheDocument();
    expect(screen.getAllByText("Sem o que medir")).toHaveLength(7);
  });

  it("diz de onde vem o setor, quando foi buscado e o que ficou de fora", async () => {
    await openHealth();
    const sources = within(screen.getByRole("region", { name: "De onde vem o setor" }));
    expect(sources.getByText(/Classificação Setorial da B3/)).toBeInTheDocument();
    expect(sources.getByText(/informe mensal de FII da CVM/)).toBeInTheDocument();
    expect(sources.getAllByText(/Atualizado/)).toHaveLength(2);
    expect(sources.getByText(/SEC\) exige identificar quem consulta/)).toBeInTheDocument();
    expect(sources.getByText(/1 ativo em Ações Nacionais ou FIIs sem setor na fonte/)).toBeInTheDocument();
    expect(sources.getByText(/Ele não entra na regra de setor/)).toBeInTheDocument();
  });

  it("avisa quando a fonte falhou ou nunca respondeu", async () => {
    getDb().sectorSources = [
      { source: "b3_stocks", asset_class: "br_stocks", updated_at: null, failing: true },
      { source: "cvm_reits", asset_class: "br_reits", updated_at: "2026-11-21T09:00:00Z", failing: true },
    ];
    getDb().unclassified = 3;
    await openHealth();
    const sources = within(screen.getByRole("region", { name: "De onde vem o setor" }));
    expect(sources.getByText(/Ainda não buscado\./)).toBeInTheDocument();
    expect(sources.getAllByText(/A última busca falhou/)).toHaveLength(2);
    expect(sources.getByText(/A última busca falhou; vale a anterior/)).toBeInTheDocument();
    expect(sources.getByText(/3 ativos em Ações Nacionais ou FIIs sem setor/)).toBeInTheDocument();
    expect(sources.getByText(/Eles não entram na regra de setor/)).toBeInTheDocument();
  });

  it("mostra o erro com tentar de novo", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/health", () =>
        fail ? HttpResponse.json({ code: "internal", status: 500 }, { status: 500, headers: { "Content-Type": "application/problem+json" } }) : undefined,
      ),
    );
    const { user } = renderApp("ajustes/saude");
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("heading", { name: "Suas regras" })).toBeInTheDocument();
  });
});
