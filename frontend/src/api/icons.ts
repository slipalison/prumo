import { request } from "./client";
import type { IconIndex } from "./iconsTypes";

/** O endereço do logo de um símbolo, no domínio do Prumo (nunca de terceiro). */
export function iconUrl(symbol: string, version: string): string {
  return `/api/v1/icons/${encodeURIComponent(symbol)}?v=${encodeURIComponent(version)}`;
}

export const iconsApi = {
  index: () => request<IconIndex>("GET", "/icons"),
};
