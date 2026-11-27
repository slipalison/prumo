import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { sampleEvent, sampleSummary } from "../../test/incomeHandlers";
import { setIcons } from "../../test/iconsHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { LOGOS, logoOf } from "../../test/icons";

/* O ícone do ativo nas listas desta área (ajuda em `test/icons.ts`). */

describe("Renda", () => {
  it("a lista de proventos, a estimativa e os pagadores trazem o ícone", async () => {
    setIcons(LOGOS);
    server.use(
      http.get("/api/v1/income/summary", () => HttpResponse.json(sampleSummary())),
      http.get("/api/v1/income", () =>
        HttpResponse.json([
          sampleEvent(),
          sampleEvent({ id: "e-2", asset_id: "a-itsa", symbol: "ITSA4", display_name: "Itaúsa", kind: "dividend" }),
          sampleEvent({ id: "e-3", asset_id: null, symbol: null, display_name: "CDB do Banco Exemplo", kind: "other" }),
        ]),
      ),
    );
    renderApp("renda");
    await screen.findByRole("heading", { name: "Quanto sua carteira paga?" });

    // Estimativa: uma linha por pagador estimado.
    const estimate = (await screen.findByRole("table", { name: "Os ativos que mais rendem na estimativa" })).querySelector("tbody tr") as HTMLElement;
    expect(estimate.querySelector(".asset-icon")).not.toBeNull();

    // Pagadores: um ícone por pagador, e "Outros" não tem.
    const payers = document.querySelector(".payers__table") as HTMLElement;
    expect(payers.querySelectorAll(".asset-icon")).toHaveLength(3);
    expect(await logoOf(within(payers).getByText("KNRI11").closest("tr") as HTMLElement)).toBe("/api/v1/icons/KNRI11?v=v-knri");

    // Lista de lançados: o sem ticker usa as iniciais do nome.
    const cdb = (await screen.findAllByText("CDB do Banco Exemplo"))[0]?.closest("li") as HTMLElement;
    expect(cdb.querySelector(".asset-icon__initials")).toHaveTextContent("CD");
    const itsa = screen.getAllByText("ITSA4").map((el) => el.closest("li")).find((li) => li?.classList.contains("income-row--event"));
    expect(await logoOf(itsa as HTMLElement)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
  });
});
