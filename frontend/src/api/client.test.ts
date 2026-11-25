import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { shouldRetry } from "../app/queryClient";
import { getDb, server } from "../test/server";
import { api, ApiError, errorMessage } from "./client";

describe("api client", () => {
  it("lê JSON e manda corpo com Content-Type", async () => {
    const info = await api.info();
    expect(info).toEqual({ version: "0.1.0", commit: "abc1234" });

    await api.saveGoals({ targets: { br_stocks: 50, fixed_income: 50 } });
    const put = getDb().requests.find((r) => r.method === "PUT");
    expect(put?.path).toBe("/api/v1/goals");
    expect(put?.body).toEqual({ targets: { br_stocks: 50, fixed_income: 50 } });
  });

  it("devolve undefined no 204", async () => {
    await expect(api.deleteAsset("a-knri")).resolves.toBeUndefined();
  });

  it("transforma problem+json em ApiError com mensagem amigável", async () => {
    const error = await api.saveGoals({ targets: { br_stocks: 10 } }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).code).toBe("targets_must_sum_100");
    expect((error as ApiError).status).toBe(422);
    expect((error as ApiError).detail).toBe("A soma enviada foi 10%.");
    expect(errorMessage(error)).toBe("As metas precisam somar exatamente 100%.");
  });

  it("usa título e detalhe quando o código não tem frase própria", async () => {
    server.use(
      http.post("/api/v1/assets", () =>
        HttpResponse.json(
          { code: "validation_failed", title: "Campo inválido.", detail: "quantity deve ser maior que zero", status: 422 },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const error = (await api.createAsset({ asset_class: "br_reits", symbol: "X", quantity: 0 }).catch((e: unknown) => e)) as ApiError;
    expect(error.message).toBe("Campo inválido. quantity deve ser maior que zero");
  });

  it("deduz o código pelo status quando a resposta não é JSON", async () => {
    server.use(
      http.get("/api/v1/me", () => new HttpResponse("nope", { status: 401 })),
      http.get("/api/v1/contributions", () => new HttpResponse("<html>", { status: 503, headers: { "Content-Type": "text/html" } })),
      http.get("/api/v1/portfolio", () =>
        new HttpResponse("{quebrado", { status: 404, headers: { "Content-Type": "application/json" } }),
      ),
    );
    await expect(api.me()).rejects.toMatchObject({ code: "unauthenticated", status: 401 });
    await expect(api.contributions()).rejects.toMatchObject({ code: "internal" });
    await expect(api.portfolio()).rejects.toMatchObject({ code: "not_found" });
  });

  it("mapeia 409 e 502 pelo status", async () => {
    server.use(
      http.get("/api/v1/symbols", () => new HttpResponse(null, { status: 502 })),
      http.post("/api/v1/assets", () => new HttpResponse(null, { status: 409 })),
    );
    await expect(api.searchSymbols("knri")).rejects.toMatchObject({ code: "quote_provider_unavailable" });
    await expect(api.createAsset({ asset_class: "br_reits", symbol: "KNRI11", quantity: 1 })).rejects.toMatchObject({
      code: "asset_already_exists",
    });
  });

  it("falha de rede vira mensagem de conexão", async () => {
    server.use(http.get("/api/v1/info", () => HttpResponse.error()));
    const error = await api.info().catch((e: unknown) => e);
    expect(error).toMatchObject({ code: "network", status: 0 });
    expect(errorMessage(error)).toMatch(/Sem conexão/);
    expect(errorMessage(new Error("qualquer"))).toMatch(/Algo deu errado/);
  });

  it("cancelamento continua sendo cancelamento", async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(api.searchSymbols("knri", controller.signal)).rejects.toMatchObject({ name: "AbortError" });
  });

  it("cobre as rotas de checklist, contribuição e cotação", async () => {
    const question = await api.addQuestion("stocks", "Paga dividendos?");
    expect(question.prompt).toBe("Paga dividendos?");
    await api.updateQuestion(question.id, "Paga dividendos há 5 anos?");
    await api.deleteQuestion(question.id);
    const reset = await api.resetChecklist("real_estate");
    expect(reset).toHaveLength(6);
    const plan = await api.plan(6000);
    expect(plan.items).toHaveLength(4);
    const contribution = await api.contribute({ amount: 100, items: [{ asset_id: "a-trxf", quantity: 1, unit_price: 53.81 }] });
    expect(contribution.invested).toBe(53.81);
    expect(await api.contributions()).toHaveLength(1);
    expect((await api.refreshQuotes()).failed).toEqual([]);
    const updated = await api.updateAsset("a-trxf", { quantity: 40 });
    expect(updated.quantity).toBe(40);
  });
});

describe("shouldRetry", () => {
  it("não repete erro de quem pediu; repete rede e 5xx até duas vezes", () => {
    expect(shouldRetry(0, new ApiError(422, "validation_failed", "x"))).toBe(false);
    expect(shouldRetry(0, new ApiError(500, "internal", "x"))).toBe(true);
    expect(shouldRetry(1, new ApiError(0, "network", "x"))).toBe(true);
    expect(shouldRetry(2, new Error("x"))).toBe(false);
  });
});
