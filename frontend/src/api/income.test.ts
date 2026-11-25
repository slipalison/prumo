import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "../test/server";
import { incomeApi } from "./income";

describe("incomeApi.events", () => {
  it("manda só os filtros que existem", async () => {
    const urls: string[] = [];
    server.use(
      http.get("/api/v1/income", ({ request }) => {
        urls.push(new URL(request.url).search);
        return HttpResponse.json([]);
      }),
    );
    await incomeApi.events();
    await incomeApi.events({ status: "upcoming", limit: 5, from: "2026-01-01", asset: undefined });
    expect(urls[0]).toBe("");
    const params = new URLSearchParams(urls[1]);
    expect(Object.fromEntries(params)).toEqual({ status: "upcoming", limit: "5", from: "2026-01-01" });
  });
});
