import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { analysisState } from "../../test/analysisHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

async function openSettings(path = "ajustes/analise") {
  const rendered = renderApp(path);
  await screen.findByRole("heading", { name: "Regras da análise", level: 1 });
  await screen.findByRole("heading", { name: "Diversificação" });
  return rendered;
}

const lastPut = () => analysisState().saved.at(-1);
const group = (name: string) => within(screen.getByRole("region", { name }));
const limitOf = (name: string | RegExp) => screen.getByRole("textbox", { name });

describe("Ajustes, Análise", () => {
  it("é uma seção de Ajustes e o endereço antigo da Saúde leva para ela", async () => {
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    const main = screen.getByRole("navigation", { name: "Principal" });
    await user.click(within(main).getByRole("link", { name: "Ajustes" }));
    const sections = await screen.findByRole("navigation", { name: "Seções de Ajustes" });
    await user.click(within(sections).getByRole("link", { name: "Análise" }));
    expect(await screen.findByRole("heading", { name: "Regras da análise", level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#/ajustes/analise");
    expect(document.title).toBe("Ajustes · Prumo");
    expect(screen.getByText(/A pontuação de uma dimensão é só o percentual das regras ligadas/)).toBeInTheDocument();
  });

  it("#/ajustes/saude continua abrindo, agora na Análise, e troca o endereço sem nova entrada no histórico", async () => {
    const before = window.history.length;
    renderApp("ajustes/saude");
    expect(await screen.findByRole("heading", { name: "Regras da análise", level: 1 })).toBeInTheDocument();
    await waitFor(() => expect(window.location.hash).toBe("#/ajustes/analise"));
    const sections = screen.getByRole("navigation", { name: "Seções de Ajustes" });
    expect(within(sections).getByRole("link", { name: "Análise" })).toHaveAttribute("aria-current", "page");
    // `replaceState`: o histórico não ganha uma entrada para o endereço novo.
    expect(window.history.length).toBeLessThanOrEqual(before + 1);
  });

  it("lista as regras por dimensão, com a Qualidade e as referências à parte", async () => {
    await openSettings();
    expect(within(group("Diversificação").getByRole("list")).getAllByRole("listitem")).toHaveLength(10);
    expect(within(group("Dividendos").getByRole("list")).getAllByRole("listitem")).toHaveLength(6);
    expect(within(group("Rentabilidade").getByRole("list")).getAllByRole("listitem")).toHaveLength(9);
    expect(screen.getAllByRole("switch")).toHaveLength(25);
    expect(group("Qualidade dos ativos").getByText(/chegam junto com os dados de fundamentos da CVM/)).toBeInTheDocument();
    expect(group("Referências de preço").getByRole("textbox", { name: "Rendimento-alvo do Bazin" })).toHaveValue("6");
  });

  it("só as regras com limite editável têm campo; as de índice só ligam e desligam", async () => {
    await openSettings();
    expect(limitOf(/Limite: Pelo menos … da carteira no exterior/)).toHaveValue("10");
    expect(limitOf(/Limite: Volatilidade anualizada de até/)).toHaveValue("20");
    expect(limitOf(/Limite: Nenhum ativo com perda maior que/)).toHaveValue("30");
    expect(screen.queryByRole("textbox", { name: /Limite: A carteira rende ao menos o CDI/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /Limite: Renda de 12 meses maior/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /Limite: Toda classe com meta tem ativo/ })).not.toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "A carteira rende ao menos o CDI em 12 meses" })).toBeChecked();
  });

  it("liga e desliga uma regra e só manda o que mudou", async () => {
    const { user } = await openSettings();
    const save = screen.getByRole("button", { name: "Salvar regras" });
    expect(save).toBeDisabled();
    const abroad = screen.getByRole("switch", { name: "Pelo menos … da carteira no exterior" });
    expect(abroad).toBeChecked();
    await user.click(abroad);
    expect(limitOf(/Limite: Pelo menos … da carteira no exterior/)).toBeDisabled();
    await user.click(save);
    await screen.findByText("Regras salvas.");
    expect(lastPut()).toEqual({ rules: { min_abroad: { enabled: false } } });
    expect(analysisState().rules.find((r) => r.rule === "min_abroad")?.enabled).toBe(false);
  });

  it("muda limites com vírgula e o rendimento-alvo do Bazin num pedido só", async () => {
    const { user } = await openSettings();
    const volatility = limitOf(/Limite: Volatilidade anualizada de até/);
    await user.clear(volatility);
    await user.type(volatility, "17,5");
    const sectors = limitOf(/Limite: Ações em pelo menos/);
    await user.clear(sectors);
    await user.type(sectors, "6");
    const bazin = screen.getByRole("textbox", { name: "Rendimento-alvo do Bazin" });
    await user.clear(bazin);
    await user.type(bazin, "7,5");
    await user.click(screen.getByRole("button", { name: "Salvar regras" }));
    await screen.findByText("Regras salvas.");
    expect(lastPut()).toEqual({
      rules: { max_volatility: { threshold: 17.5 }, min_sectors: { threshold: 6 } },
      bazin_yield: 7.5,
    });
    expect(analysisState().bazinYield).toBe(7.5);
    await waitFor(() => expect(limitOf(/Limite: Volatilidade anualizada de até/)).toHaveValue("17,5"));
    expect(screen.getByRole("textbox", { name: "Rendimento-alvo do Bazin" })).toHaveValue("7,5");
  });

  it("recusa limite que não faz sentido antes de enviar", async () => {
    const { user } = await openSettings();
    const save = screen.getByRole("button", { name: "Salvar regras" });

    const classes = limitOf(/Limite: Dinheiro em pelo menos … classes de ativos/);
    await user.clear(classes);
    await user.type(classes, "8");
    expect(await screen.findByText("Use um número inteiro de 1 a 7.")).toBeInTheDocument();
    expect(classes).toHaveAttribute("aria-invalid", "true");
    expect(save).toBeDisabled();
    await user.clear(classes);
    await user.type(classes, "3");

    const loss = limitOf(/Limite: Nenhum ativo com perda maior que/);
    await user.clear(loss);
    await user.type(loss, "150");
    expect(await screen.findByText(/Use um percentual maior que 0 e até 100/)).toBeInTheDocument();
    await user.clear(loss);
    await user.type(loss, "abc");
    expect(await screen.findByText("Digite um número.")).toBeInTheDocument();

    // Desligar a regra tira a exigência.
    await user.click(screen.getByRole("switch", { name: "Nenhum ativo com perda maior que … desde a compra" }));
    expect(screen.queryByText("Digite um número.")).not.toBeInTheDocument();

    const bazin = screen.getByRole("textbox", { name: "Rendimento-alvo do Bazin" });
    await user.clear(bazin);
    await user.type(bazin, "0");
    expect(await screen.findByText(/Use um percentual maior que 0 e até 100/)).toBeInTheDocument();
    expect(save).toBeDisabled();
    await user.clear(bazin);
    await user.type(bazin, "6");
    expect(save).toBeEnabled();
  });

  it("os limites de contagem vão até o máximo de cada regra", async () => {
    const { user } = await openSettings();
    const assets = limitOf(/Limite: Pelo menos … ativos na carteira/);
    await user.clear(assets);
    await user.type(assets, "100");
    expect(screen.queryByText("Use um número inteiro de 1 a 100.")).not.toBeInTheDocument();
    await user.clear(assets);
    await user.type(assets, "101");
    expect(await screen.findByText("Use um número inteiro de 1 a 100.")).toBeInTheDocument();
    const months = limitOf(/Limite: Provento recebido em pelo menos/);
    await user.clear(months);
    await user.type(months, "13");
    expect(await screen.findByText("Use um número inteiro de 1 a 12.")).toBeInTheDocument();
  });

  it("volta ao limite de fábrica e descarta mudanças", async () => {
    const { user } = await openSettings();
    const input = limitOf(/Limite: Os 5 maiores/);
    expect(screen.queryByRole("button", { name: /Voltar para/ })).not.toBeInTheDocument();
    await user.clear(input);
    await user.type(input, "80");
    await user.click(screen.getByRole("button", { name: "Voltar para 60%" }));
    expect(input).toHaveValue("60");
    expect(screen.getByRole("button", { name: "Salvar regras" })).toBeDisabled();

    await user.type(input, "5");
    await user.click(screen.getByRole("button", { name: "Descartar mudanças" }));
    expect(input).toHaveValue("60");
    expect(screen.queryByRole("button", { name: "Descartar mudanças" })).not.toBeInTheDocument();
  });

  it("mostra o erro do servidor ao salvar e deixa a mudança na tela", async () => {
    server.use(
      http.put("/api/v1/analysis/rules", () =>
        HttpResponse.json(
          { type: "urn:prumo:error:validation_failed", title: "Dados inválidos", status: 422, code: "validation_failed", detail: "min_abroad: limite ruim." },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openSettings();
    await user.click(screen.getByRole("switch", { name: "Pelo menos … da carteira no exterior" }));
    await user.click(screen.getByRole("button", { name: "Salvar regras" }));
    expect(await screen.findByText(/min_abroad: limite ruim\./)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Pelo menos … da carteira no exterior" })).not.toBeChecked();
  });

  it("mostra o erro com tentar de novo", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/analysis/rules", () =>
        fail
          ? HttpResponse.json({ code: "internal", status: 500 }, { status: 500, headers: { "Content-Type": "application/problem+json" } })
          : HttpResponse.json({ rules: analysisState().rules, bazin_yield: 6 }),
      ),
    );
    const { user } = renderApp("ajustes/analise");
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("heading", { name: "Diversificação" })).toBeInTheDocument();
  });
});
