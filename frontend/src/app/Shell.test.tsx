import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../test/render";
import { server } from "../test/server";

describe("Shell", () => {
  it("abre em Aportar com a pergunta principal e a navegação", async () => {
    renderApp();
    expect(await screen.findByRole("heading", { name: "Quanto você vai aportar?" })).toBeInTheDocument();
    const nav = screen.getByRole("navigation", { name: "Principal" });
    expect(within(nav).getByRole("link", { name: "Aportar" })).toHaveAttribute("aria-current", "page");
    expect(document.title).toBe("Aportar · Prumo");
    expect(await within(nav).findByText("pessoa@exemplo.com")).toBeInTheDocument();
    expect(within(nav).getByText("v0.1.0")).toBeInTheDocument();
  });

  it("troca de tela pelo hash", async () => {
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    await user.click(screen.getByRole("link", { name: "Ajustes" }));
    expect(await screen.findByRole("heading", { name: "Quanto você quer em cada classe?" })).toBeInTheDocument();
    expect(document.title).toBe("Ajustes · Prumo");
    await user.click(screen.getByRole("link", { name: "Critérios" }));
    expect(await screen.findByRole("heading", { name: "As perguntas que dão a nota" })).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Carteira" }));
    expect(await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" })).toBeInTheDocument();
  });

  it("mostra o esqueleto e depois o erro com tentar de novo", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/portfolio", () =>
        fail
          ? HttpResponse.json({ code: "internal", title: "x", status: 500 }, { status: 500, headers: { "Content-Type": "application/problem+json" } })
          : undefined,
      ),
    );
    const { user } = renderApp();
    expect(screen.getByRole("status", { name: "Carregando" })).toBeInTheDocument();
    const alert = await screen.findByRole("alert", {}, { timeout: 4000 });
    expect(within(alert).getByText(/Algo deu errado do nosso lado/)).toBeInTheDocument();
    fail = false;
    await user.click(within(alert).getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("heading", { name: "Quanto você vai aportar?" })).toBeInTheDocument();
  });

  it("o atalho de pular leva o foco ao conteúdo", async () => {
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    await user.click(screen.getByRole("button", { name: "Pular para o conteúdo" }));
    await waitFor(() => expect(document.activeElement?.tagName).toBe("MAIN"));
  });

  it("cada troca de tela volta ao topo, sem rolagem animada", async () => {
    const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => undefined);
    const { user } = renderApp();
    await screen.findByRole("heading", { name: "Quanto você vai aportar?" });
    expect(scrollTo).not.toHaveBeenCalled();
    await user.click(screen.getByRole("link", { name: "Carteira" }));
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 0, left: 0, behavior: "instant" }));
    scrollTo.mockRestore();
  });

  it("voltar para Aportar põe o foco no campo de valor", async () => {
    const { user } = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await user.click(screen.getByRole("link", { name: "Aportar" }));
    const input = await screen.findByRole("textbox", { name: "Quanto você vai aportar?" });
    await waitFor(() => expect(input).toHaveFocus());
  });
});
