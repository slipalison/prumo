import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { alertsHandlers, linkedAlerts, problem, sampleAlerts, withoutTaxRules } from "../../test/alertsHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";

const HEADING = "Alertas no Telegram";

function open(initial = sampleAlerts()) {
  const fake = alertsHandlers(initial);
  server.use(...fake.handlers);
  return { ...fake, ...renderApp("ajustes/alertas") };
}

describe("Alertas: estado do Telegram", () => {
  it("sem o token do bot, a tela diz que está desligado, sem erro e sem ajustes", async () => {
    open(sampleAlerts({ telegram: { configured: false, bot_username: null, linked: false, linked_at: null } }));
    expect(await screen.findByRole("heading", { name: HEADING })).toBeInTheDocument();
    expect(await screen.findByText(/desligados neste servidor/)).toBeInTheDocument();
    expect(screen.getByText("TELEGRAM_BOT_TOKEN")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Vincular o Telegram" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "O que avisar" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it(
    "vincular: mostra o código e o link do bot, e confere sozinha até o vínculo fechar",
    async () => {
      const { user, state } = open();
      await user.click(await screen.findByRole("button", { name: "Vincular o Telegram" }));
      expect(await screen.findByText("/start K7M2QX9PTA")).toBeInTheDocument();
      const link = screen.getByRole("link", { name: "@prumo_exemplo_bot" });
      expect(link).toHaveAttribute("href", "https://t.me/prumo_exemplo_bot?start=K7M2QX9PTA");
      expect(link).toHaveAttribute("rel", "noopener noreferrer");
      expect(state.links).toBe(1);

      // A pessoa manda o código ao bot: do lado do servidor, o chat fica ligado.
      state.alerts.telegram.linked = true;
      state.alerts.telegram.linked_at = "2026-11-25T14:00:00Z";
      expect(await screen.findByText(/Vinculado em/, undefined, { timeout: 6000 })).toBeInTheDocument();
      expect(screen.queryByText("/start K7M2QX9PTA")).not.toBeInTheDocument();
      expect(await screen.findByText("Telegram vinculado.")).toBeInTheDocument();
    },
    15_000,
  );

  it("cancelar a vinculação volta ao botão", async () => {
    const { user } = open();
    await user.click(await screen.findByRole("button", { name: "Vincular o Telegram" }));
    await user.click(await screen.findByRole("button", { name: "Cancelar" }));
    expect(await screen.findByRole("button", { name: "Vincular o Telegram" })).toBeInTheDocument();
  });

  it("vinculado: enviar teste e desvincular", async () => {
    const { user, state } = open(linkedAlerts());
    expect(await screen.findByText(/Vinculado em/)).toHaveTextContent("@prumo_exemplo_bot");
    await user.click(screen.getByRole("button", { name: "Enviar teste agora" }));
    expect(await screen.findByText("Teste enviado. Confira o Telegram.")).toBeInTheDocument();
    expect(state.tests).toBe(1);

    await user.click(screen.getByRole("button", { name: "Desvincular" }));
    expect(await screen.findByRole("button", { name: "Vincular o Telegram" })).toBeInTheDocument();
    expect(state.unlinks).toBe(1);
  });

  it("o teste que o Telegram recusa mostra o motivo", async () => {
    const { user, state } = open(linkedAlerts());
    state.failTest = true;
    await user.click(await screen.findByRole("button", { name: "Enviar teste agora" }));
    expect(await screen.findByText(/Telegram indisponivel/)).toBeInTheDocument();
  });

  it("erro ao carregar oferece tentar de novo", async () => {
    const { handlers } = alertsHandlers();
    server.use(...handlers);
    server.use(http.get("/api/v1/alerts", () => new HttpResponse(null, { status: 500 }), { once: true }));
    const { user } = renderApp("ajustes/alertas");
    expect(await screen.findByRole("alert")).toHaveTextContent("Algo deu errado");
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("button", { name: "Vincular o Telegram" })).toBeInTheDocument();
  });

  it("erro ao pedir o código aparece como aviso", async () => {
    const { user } = open();
    server.use(http.post("/api/v1/alerts/telegram", () => problem(422, "validation_failed", "Dados invalidos", "Os alertas estao desligados.")));
    await user.click(await screen.findByRole("button", { name: "Vincular o Telegram" }));
    expect(await screen.findByText(/Os alertas estao desligados/)).toBeInTheDocument();
  });
});

describe("Alertas: tipos, horário e valores", () => {
  it("lista os seis tipos, todos ligados, com o limite do isentômetro", async () => {
    open(linkedAlerts());
    expect(await screen.findAllByRole("switch")).toHaveLength(7);
    const exemption = screen.getByRole("switch", { name: "Isentômetro de ações" });
    expect(exemption).toBeEnabled();
    expect(exemption).toBeChecked();
    expect(screen.getByRole("textbox", { name: "Limite: Isentômetro de ações" })).toHaveValue("80");
    expect(screen.getByRole("switch", { name: "Provento que cai amanhã" })).toBeChecked();
  });

  it("ano sem regra de IR: o isentômetro fica desligado, travado e diz por quê", async () => {
    open(withoutTaxRules(linkedAlerts()));
    const exemption = await screen.findByRole("switch", { name: "Isentômetro de ações" });
    expect(exemption).toBeDisabled();
    expect(exemption).not.toBeChecked();
    expect(screen.getByRole("textbox", { name: "Limite: Isentômetro de ações" })).toBeDisabled();
    expect(screen.getByText(/Indisponível: o Prumo ainda não tem a regra de IR deste ano/)).toBeInTheDocument();
    expect(screen.queryByText("Isentômetro de ações", { selector: ".alerts__digest-section" })).not.toBeInTheDocument();
  });

  it("desligar um tipo e mudar o limite salva só o que mudou", async () => {
    const { user, state } = open(linkedAlerts());
    const save = await screen.findByRole("button", { name: "Salvar alertas" });
    expect(save).toBeDisabled();

    await user.click(screen.getByRole("switch", { name: "Aviso novo da análise da carteira" }));
    const limit = screen.getByRole("textbox", { name: "Limite: Classe fora da meta" });
    await user.clear(limit);
    await user.type(limit, "7,5");
    await user.click(save);

    await waitFor(() => expect(state.saved).toHaveLength(1));
    expect(state.saved[0]).toEqual({
      types: { health_warning: { enabled: false }, class_drift: { threshold: 7.5 } },
    });
    expect(await screen.findByText("Alertas salvos.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Aviso novo da análise da carteira" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Salvar alertas" })).toBeDisabled();
  });

  it("limite inválido trava o salvamento e diz por quê; descartar volta ao salvo", async () => {
    const { user } = open(linkedAlerts());
    const limit = await screen.findByRole("textbox", { name: "Limite: Classe fora da meta" });
    await user.clear(limit);
    await user.type(limit, "150");
    expect(await screen.findByRole("alert")).toHaveTextContent("maior que 0 e até 100");
    expect(screen.getByRole("button", { name: "Salvar alertas" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Descartar mudanças" }));
    expect(screen.getByRole("textbox", { name: "Limite: Classe fora da meta" })).toHaveValue("5");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("o limite de um tipo desligado não é editável", async () => {
    const { user } = open(linkedAlerts());
    const limit = await screen.findByRole("textbox", { name: "Limite: Classe fora da meta" });
    await user.click(screen.getByRole("switch", { name: "Classe fora da meta" }));
    expect(limit).toBeDisabled();
  });

  it("hora inválida trava o salvamento; hora válida e 'sem valores' salvam", async () => {
    const { user, state } = open(linkedAlerts());
    const time = await screen.findByLabelText("Hora do resumo (horário de Brasília)");
    expect(time).toHaveValue("08:00");
    await user.clear(time);
    expect(await screen.findByRole("alert")).toHaveTextContent("HH:MM");
    expect(screen.getByRole("button", { name: "Salvar alertas" })).toBeDisabled();

    await user.type(time, "07:30");
    await user.click(screen.getByRole("switch", { name: "Sem valores no resumo" }));
    await user.click(screen.getByRole("button", { name: "Salvar alertas" }));
    await waitFor(() => expect(state.saved).toHaveLength(1));
    expect(state.saved[0]).toEqual({ send_at: "07:30", hide_values: true });
  });

  it("a recusa do servidor aparece como aviso e não perde o que foi digitado", async () => {
    const { user } = open(linkedAlerts());
    server.use(http.put("/api/v1/alerts", () => problem(422, "validation_failed", "Dados invalidos", "Sem permissao.")));
    await user.click(await screen.findByRole("switch", { name: "Objetivo atingido" }));
    await user.click(screen.getByRole("button", { name: "Salvar alertas" }));
    expect(await screen.findByText(/Sem permissao/)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Objetivo atingido" })).not.toBeChecked();
  });
});

describe("Alertas: exemplo do resumo", () => {
  it("mostra só os tipos ligados e acompanha a escolha de 'sem valores'", async () => {
    const { user } = open(linkedAlerts());
    const digest = await screen.findByRole("group", { name: "Exemplo do resumo" });
    expect(within(digest).getByText("Proventos amanhã")).toBeInTheDocument();
    expect(digest).toHaveTextContent(/KNRI11 paga amanhã: R\$\s90,00 líquidos\./);

    await user.click(screen.getByRole("switch", { name: "Sem valores no resumo" }));
    expect(digest).toHaveTextContent("- KNRI11 paga amanhã.");
    expect(digest.textContent).not.toMatch(/R\$/);

    await user.click(screen.getByRole("switch", { name: "Provento que cai amanhã" }));
    expect(within(digest).queryByText("Proventos amanhã")).not.toBeInTheDocument();
  });

  it("sem nenhum tipo ligado, diz que nada será enviado", async () => {
    const alerts = linkedAlerts();
    for (const type of alerts.types) type.enabled = false;
    open(alerts);
    expect(await screen.findByText("Nenhum tipo ligado: nada será enviado.")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Exemplo do resumo" })).not.toBeInTheDocument();
  });
});
