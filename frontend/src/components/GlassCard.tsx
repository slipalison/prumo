import type { HTMLAttributes, PointerEvent } from "react";
import { cx } from "../lib/cx";

type GlassCardProps = HTMLAttributes<HTMLElement> & {
  as?: "div" | "section" | "article" | "aside" | "li";
  /** Brilho que acompanha o cursor e acende a borda. */
  spotlight?: boolean;
};

/**
 * Cartão de vidro: desfoque do que está atrás, borda de 1px em gradiente e um
 * holofote que segue o ponteiro (variáveis CSS `--mx`/`--my`).
 */
export function GlassCard({
  as: Tag = "div",
  spotlight = true,
  className,
  onPointerMove,
  children,
  ...rest
}: GlassCardProps) {
  const handleMove = (event: PointerEvent<HTMLElement>) => {
    if (spotlight) {
      const rect = event.currentTarget.getBoundingClientRect();
      event.currentTarget.style.setProperty("--mx", `${event.clientX - rect.left}px`);
      event.currentTarget.style.setProperty("--my", `${event.clientY - rect.top}px`);
    }
    onPointerMove?.(event);
  };

  return (
    <Tag className={cx("glass", spotlight && "glass--spotlight", className)} onPointerMove={handleMove} {...rest}>
      {children}
    </Tag>
  );
}
