import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { setIcons } from "../../test/iconsHandlers";
import { openRegistration } from "../../test/purchase";
import { renderApp } from "../../test/render";
import { LOGOS, kind, logoOf } from "../../test/icons";

/* O ícone do ativo nas listas desta área (ajuda em `test/icons.ts`). */

describe("Aportar", () => {
  it("o plano e a conferência do registro mostram o ícone de cada ativo", async () => {
    setIcons(LOGOS);
    const { user } = renderApp("aportar");
    await user.type(await screen.findByRole("textbox", { name: "Quanto você vai aportar?" }), "600000");
    await user.click(screen.getByRole("button", { name: /Calcular/ }));

    const result = await screen.findByRole("region", { name: "Resultado do aporte" });
    const cards = within(result).getAllByRole("listitem").filter((li) => li.classList.contains("plan-card"));
    const card = (symbol: string) => cards.find((c) => c.querySelector(".plan-card__symbol")?.textContent === symbol) as HTMLElement;
    expect(await logoOf(card("ITSA4"))).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    expect(card("BTLG11").querySelector(".asset-icon__initials")).toHaveTextContent("BT");
    expect(kind(card("BTC"))).toBe("bitcoin");

    const dialog = await openRegistration(user);
    const rows = within(dialog).getAllByRole("listitem");
    expect(await logoOf(rows[0] as HTMLElement)).toBe("/api/v1/icons/ITSA4?v=v-itsa");
    expect(rows.every((r) => r.querySelector(".asset-icon"))).toBe(true);
  });
});
