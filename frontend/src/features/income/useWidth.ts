import { useEffect, useRef, useState } from "react";

/**
 * A largura em pixels do elemento: o gráfico se desenha na escala real e o
 * texto não estica. Começa em `fallback` (e fica nela onde não há medida, como
 * no teste); depois vale a do painel.
 */
export function useWidth(fallback: number) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(fallback);
  useEffect(() => {
    const node = ref.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      const next = Math.round(entry?.contentRect.width ?? 0);
      if (next > 0) setWidth(next);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}
