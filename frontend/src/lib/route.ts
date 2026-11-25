import { useCallback, useEffect, useSyncExternalStore } from "react";

/**
 * Links antigos que continuam valendo: `#/metas` e `#/criterios` viraram
 * seções de Ajustes. Só o que existiu como rota entra aqui.
 */
export const LEGACY_ROUTES: Readonly<Record<string, string>> = {
  metas: "ajustes/metas",
  criterios: "ajustes/criterios",
};

/** `#/ajustes/metas?x=1` → `["ajustes", "metas"]`, com os links antigos já traduzidos. */
export function hashSegments(hash: string): string[] {
  const path = hash.replace(/^#\/?/, "").split("?")[0] ?? "";
  const segments = path.split("/").filter(Boolean);
  const legacy = LEGACY_ROUTES[segments[0] ?? ""];
  return legacy ? legacy.split("/") : segments;
}

/**
 * `#/carteira` → `carteira`. `ids` é a lista de rotas do app (`app/routes.tsx`);
 * qualquer outra coisa cai na primeira, a abertura.
 */
export function parseRoute<Id extends string>(hash: string, ids: readonly Id[]): Id {
  const first = hashSegments(hash)[0] ?? "";
  return ids.find((id) => id === first) ?? (ids[0] as Id);
}

/** `#/ajustes/metas` → `metas`; sem segundo trecho, `null`. */
export function parseSection(hash: string): string | null {
  return hashSegments(hash)[1] ?? null;
}

function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

const snapshot = () => window.location.hash;

/** `navigate("carteira")` ou `navigate("ajustes/metas")`. */
export function navigate(path: string): void {
  window.location.hash = `#/${path}`;
}

/** O hash da página, atualizado quando ele muda. */
export function useHash(): string {
  return useSyncExternalStore(subscribe, snapshot, () => "");
}

/**
 * A rota de agora, entre as de `ids`, e a seção dentro dela (`null` fora de
 * telas com seções). Um link antigo (`#/metas`) já abre na seção certa e o
 * endereço é trocado pelo novo, sem entrada nova no histórico.
 */
export function useRoute<Id extends string>(ids: readonly Id[]): { id: Id; section: string | null; go: (path: string) => void } {
  const hash = useHash();
  const go = useCallback((path: string) => navigate(path), []);

  useEffect(() => {
    const first = hash.replace(/^#\/?/, "").split(/[/?]/)[0] ?? "";
    const target = LEGACY_ROUTES[first];
    if (target) window.history.replaceState(null, "", `#/${target}`);
  }, [hash]);

  return { id: parseRoute(hash, ids), section: parseSection(hash), go };
}
