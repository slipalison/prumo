import { useEffect, useState } from "react";

const QUERY = "(max-width: 719px)";

/**
 * Janela de celular (a barra de abas embaixo). Serve para recolher o que é
 * secundário só no celular; o resto do layout responde por container query.
 * Sem `matchMedia` (teste), vale a tela larga.
 */
export function useNarrow(): boolean {
  const [narrow, setNarrow] = useState(
    () => typeof window.matchMedia === "function" && window.matchMedia(QUERY).matches,
  );
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return undefined;
    const media = window.matchMedia(QUERY);
    const update = () => setNarrow(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return narrow;
}
