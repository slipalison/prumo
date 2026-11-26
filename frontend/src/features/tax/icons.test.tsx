import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { setIcons } from "../../test/iconsHandlers";
import { renderApp } from "../../test/render";
import { LOGOS, kind, logoOf } from "../../test/icons";

/* O ícone do ativo nas listas desta área (ajuda em `test/icons.ts`). */

describe("Impostos", () => {
  it("cada linha de Bens e Direitos tem o ícone do ativo", async () => {
    setIcons(LOGOS);
    renderApp("ajustes/impostos");
    await screen.findByRole("heading", { name: "Imposto de renda" });
    const goods = await screen.findByRole("list", { name: "Bens e Direitos" });
    const rows = within(goods).getAllByRole("listitem");
    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) expect(row.querySelector(".asset-icon")).not.toBeNull();
    expect(await logoOf(rows.find((r) => /ITSA4/.test(r.textContent ?? "")) as HTMLElement)).toBe(
      "/api/v1/icons/ITSA4?v=v-itsa",
    );
    // O bitcoin da declaração usa o desenho próprio.
    expect(kind(rows.find((r) => /BTC/.test(r.textContent ?? "")) as HTMLElement)).toBe("bitcoin");
  });
});
