import { useRef, type ReactNode, type RefObject } from "react";
import { useScrollable } from "./useScrollable";

type ScrollRegionProps = {
  /** O nome da região quando ela rola: o que a pessoa ouve ao chegar nela pelo Tab. */
  label: string;
  className: string;
  children: ReactNode;
  /** Para quem já guarda a referência do scroller (voltar ao topo, por exemplo). */
  scrollRef?: RefObject<HTMLDivElement | null>;
};

/**
 * Uma caixa com altura limitada que rola por dentro. Só quando ela rola de
 * fato vira uma região com nome e entra no Tab, para quem usa teclado rolar
 * com as setas (o Safari não faz isso sozinho); sem rolagem, é uma caixa comum.
 */
export function ScrollRegion({ label, className, children, scrollRef }: Readonly<ScrollRegionProps>) {
  const own = useRef<HTMLDivElement>(null);
  const ref = scrollRef ?? own;
  const scrolls = useScrollable(ref);
  const focusable = scrolls ? { role: "region", "aria-label": label, tabIndex: 0 } : {};
  return (
    <div ref={ref} className={className} {...focusable}>
      {children}
    </div>
  );
}
