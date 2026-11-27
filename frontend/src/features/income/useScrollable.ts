import { useEffect, useState, type RefObject } from "react";

function overflows(element: HTMLElement): boolean {
  return element.scrollHeight > element.clientHeight + 1 || element.scrollWidth > element.clientWidth + 1;
}

/**
 * Se o elemento rola por dentro (o conteúdo passa da caixa). Quem rola precisa
 * ser alcançável pelo teclado para rolar com as setas, e o Safari não torna o
 * scroller focável sozinho: quem usa o hook dá `tabIndex={0}` (com um nome) só
 * quando ele rola, para não criar uma parada de foco à toa.
 *
 * Confere ao montar, quando a caixa muda de tamanho e quando o conteúdo muda
 * (o filtro por pagador, o grupo "Outros" aberto).
 */
export function useScrollable(ref: RefObject<HTMLElement | null>): boolean {
  const [scrolls, setScrolls] = useState(false);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const check = () => setScrolls(overflows(element));
    check();
    const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(check);
    resize?.observe(element);
    const mutation = new MutationObserver(check);
    mutation.observe(element, { childList: true, subtree: true, characterData: true });
    return () => {
      resize?.disconnect();
      mutation.disconnect();
    };
  }, [ref]);

  return scrolls;
}
