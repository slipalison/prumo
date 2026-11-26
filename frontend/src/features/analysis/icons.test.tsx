import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { setIcons } from "../../test/iconsHandlers";
import { renderApp } from "../../test/render";
import { LOGOS, logoOf } from "../../test/icons";

/* O ícone do ativo nas listas desta área (ajuda em `test/icons.ts`). */

describe("Análise", () => {
  it("os achados mostram o ícone dos ativos que citam, e os de setor e classe não", async () => {
    setIcons(LOGOS);
    renderApp("analise");
    await screen.findByRole("heading", { name: "Análise da carteira", level: 1 });
    const warnings = within(await screen.findByRole("region", { name: "Diversificação" }));

    const itsa = warnings.getByRole("article", { name: "ITSA4 é 23,31% da carteira" });
    expect(await logoOf(itsa)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    const topFive = warnings.getAllByRole("article").find((a) => a.querySelectorAll(".asset-icon").length === 5) as HTMLElement;
    expect(topFive).toBeDefined();
    // O Tesouro do aviso é citado por nome (não há chave): iniciais do nome.
    const ipca = warnings.getByRole("article", { name: "Tesouro IPCA+ 2035 é 70,76% da carteira" });
    expect(ipca.querySelector(".asset-icon__initials")).toHaveTextContent("TI");
    // Setor e classe não citam ativo.
    const sector = warnings.getAllByRole("article").find((a) => /Tijolo/.test(a.textContent ?? "")) as HTMLElement;
    expect(sector.querySelector(".asset-icon")).toBeNull();
  });
});
