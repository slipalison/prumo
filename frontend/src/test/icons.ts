import { waitFor } from "@testing-library/react";
import { expect } from "vitest";

/*
 * Ajuda dos testes do ícone do ativo em cada tela (cada área tem o seu
 * `icons.test.tsx`). O componente em si está em `components/AssetIcon.test.tsx`.
 */

/** Os logos que o servidor falso tem guardados. */
export const LOGOS = [
  { symbol: "ITSA4", version: "v-itsa" },
  { symbol: "KNRI11", version: "v-knri" },
];

export const img = (root: HTMLElement) => root.querySelector<HTMLImageElement>("img.asset-icon__img");
export const kind = (root: HTMLElement) => root.querySelector<HTMLElement>(".asset-icon")?.dataset.kind;

/** O logo da linha, quando o índice de logos chega (ele vem depois da tela). */
export async function logoOf(root: HTMLElement): Promise<string | null> {
  await waitFor(() => expect(img(root)).not.toBeNull());
  return img(root)?.getAttribute("src") ?? null;
}
