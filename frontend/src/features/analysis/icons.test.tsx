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
    const findings = warnings.getAllByRole("listitem").filter((li) => li.classList.contains("rule-finding"));
    const finding = (text: RegExp) => findings.find((li) => text.test(li.textContent ?? "")) as HTMLElement;

    const itsa = finding(/ITSA4 é 23,31% da carteira/);
    expect(await logoOf(itsa)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    const topFive = findings.find((li) => li.querySelectorAll(".asset-icon").length === 5);
    expect(topFive).toBeDefined();
    // O Tesouro do aviso é citado por nome (não há chave): iniciais do nome.
    const ipca = finding(/Tesouro IPCA\+ 2035 é 70,76% da carteira/);
    expect(ipca.querySelector(".asset-icon__initials")).toHaveTextContent("TI");
    // Setor e classe não citam ativo.
    expect(finding(/Tijolo/).querySelector(".asset-icon")).toBeNull();
    expect(finding(/Ações Nacionais tem 1 ativo/).querySelector(".asset-icon")).toBeNull();
  });
});
