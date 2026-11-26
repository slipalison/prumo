import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { renderApp } from "../../test/render";
import { getDb, server } from "../../test/server";

async function openGoals() {
  const rendered = renderApp("metas");
  await screen.findByRole("heading", { name: "Quanto você quer em cada classe?" });
  return rendered;
}

const slider = (label: string) => screen.getByRole("slider", { name: label });

describe("Metas", () => {
  it("mostra as metas salvas fechando 100%", async () => {
    await openGoals();
    expect(slider("Ações Nacionais")).toHaveValue("30");
    expect(slider("Criptomoedas")).toHaveValue("5");
    expect(screen.getByRole("status", { name: /Soma das metas: 100%\. Fechou 100%/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Salvar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Equilibrar" })).toBeDisabled();
    expect(screen.getByText("hoje 23,3%")).toBeInTheDocument();
  });

  it("passou de 100: equilibrar fecha a conta e salvar grava", async () => {
    const { user } = await openGoals();
    fireEvent.change(slider("Criptomoedas"), { target: { value: "10" } });
    expect(screen.getByRole("status", { name: /105%\. Passou 5%/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Salvar" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Equilibrar" }));
    expect(screen.getByRole("status", { name: /Fechou 100%/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Salvar" }));
    expect(await screen.findByText("Metas salvas. O próximo aporte já segue elas.")).toBeInTheDocument();
    // 30/10/20/10/35 somam 105; pelo maior resto vira 29/10/19/9/33.
    expect(getDb().goals).toEqual({
      br_stocks: 29,
      br_reits: 10,
      intl_stocks: 19,
      us_reits: 0,
      crypto: 9,
      fixed_income: 33,
      intl_fixed_income: 0,
    });
  });

  it("faltando: mostra quanto falta; descartar volta ao salvo", async () => {
    const { user } = await openGoals();
    fireEvent.change(slider("Renda Fixa"), { target: { value: "20" } });
    expect(screen.getByRole("status", { name: /85%\. Faltam 15%/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Descartar mudanças" }));
    expect(slider("Renda Fixa")).toHaveValue("35");
    expect(screen.queryByRole("button", { name: "Descartar mudanças" })).toBeNull();
  });

  it("perfil pronto preenche tudo de uma vez", async () => {
    const { user } = await openGoals();
    const presets = screen.getByRole("group", { name: "Perfis prontos" });
    await user.click(within(presets).getByRole("button", { name: /Conservador/ }));
    expect(within(presets).getByRole("button", { name: /Conservador/ })).toHaveAttribute("aria-pressed", "true");
    expect(slider("Renda Fixa")).toHaveValue("60");
    await user.click(screen.getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(getDb().goals.fixed_income).toBe(60));
  });

  it("erro do servidor volta como aviso e desfaz o otimismo", async () => {
    server.use(
      http.put("/api/v1/goals", () =>
        HttpResponse.json(
          { code: "targets_must_sum_100", status: 422, title: "x" },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openGoals();
    await user.click(within(screen.getByRole("group", { name: "Perfis prontos" })).getByRole("button", { name: /Arrojado/ }));
    await user.click(screen.getByRole("button", { name: "Salvar" }));
    expect(await screen.findByText("As metas precisam somar exatamente 100%.")).toBeInTheDocument();
  });

  it("a descrição de cada perfil e o porquê das metas moram nos ⓘ", async () => {
    const { user } = await openGoals();
    await user.click(screen.getByRole("button", { name: "O que cada perfil pronto faz" }));
    expect(screen.getByText(/Proteção e liquidez primeiro/)).toBeInTheDocument();
    expect(screen.getByText(/Crescimento no longo prazo/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Como as metas guiam o aporte" }));
    expect(screen.getByText("Os aportes vão sempre para o que está mais longe da meta.")).toBeInTheDocument();
  });
});
