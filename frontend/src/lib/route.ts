import { useCallback, useSyncExternalStore } from "react";

export const ROUTES = ["aportar", "carteira", "metas", "criterios"] as const;
export type RouteId = (typeof ROUTES)[number];

export const ROUTE_LABEL: Record<RouteId, string> = {
  aportar: "Aportar",
  carteira: "Carteira",
  metas: "Metas",
  criterios: "Critérios",
};

/** `#/carteira` → `carteira`; qualquer outra coisa cai na abertura. */
export function parseRoute(hash: string): RouteId {
  const id = hash.replace(/^#\/?/, "").split(/[/?]/)[0] ?? "";
  return (ROUTES as readonly string[]).includes(id) ? (id as RouteId) : "aportar";
}

function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

const snapshot = () => window.location.hash;

export function navigate(route: RouteId): void {
  window.location.hash = `#/${route}`;
}

export function useRoute(): [RouteId, (route: RouteId) => void] {
  const hash = useSyncExternalStore(subscribe, snapshot, () => "");
  const go = useCallback((route: RouteId) => navigate(route), []);
  return [parseRoute(hash), go];
}
