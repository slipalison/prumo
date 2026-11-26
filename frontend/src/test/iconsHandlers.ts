import { http, HttpResponse } from "msw";
import type { IconEntry } from "../api/iconsTypes";

let icons: IconEntry[] = [];

/** Quais símbolos têm logo guardado no servidor falso (cada teste começa sem nenhum). */
export function setIcons(next: IconEntry[]) {
  icons = next;
}

export function resetIcons() {
  icons = [];
}

/** Um logo fictício, no formato dos que a brapi serve. */
const LOGO = '<svg xmlns="http://www.w3.org/2000/svg" width="56" height="56"><path fill="#0B5FA5" d="M0 0h56v56H0z"/></svg>';

export const iconsHandlers = [
  http.get("/api/v1/icons", () => HttpResponse.json({ icons })),
  http.get("/api/v1/icons/:symbol", ({ params }) =>
    icons.some((icon) => icon.symbol === params.symbol)
      ? new HttpResponse(LOGO, { headers: { "Content-Type": "image/svg+xml" } })
      : new HttpResponse(null, { status: 404 }),
  ),
];
