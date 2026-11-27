/** Folga mínima entre o balão e a borda da janela. */
export const EDGE = 8;
/** Vão entre o gatilho e o balão. */
const GAP = 8;

export type ToggletipPlacement = "top" | "bottom";
/**
 * - `center`: centrado no gatilho (o padrão).
 * - `start` / `end`: alinhado à esquerda / à direita do gatilho.
 * - `stretch`: na largura do bloco posicionado mais próximo (o balão das
 *   referências de preço ocupa a largura do bloco, não a do chip).
 */
export type ToggletipAlign = "center" | "start" | "end" | "stretch";

type Box = { left: number; right: number; top: number; bottom: number; width: number };

/** O outro lado: o balão que não cabe em cima abre embaixo, e vice-versa. */
export function oppositeSide(placement: ToggletipPlacement): ToggletipPlacement {
  return placement === "top" ? "bottom" : "top";
}

/** A borda esquerda do balão alinhado como pedido, antes de empurrá-lo para dentro da janela. */
function alignedLeft(trigger: Box, anchor: Box, width: number, align: ToggletipAlign): number {
  switch (align) {
    case "stretch":
      return anchor.left;
    case "start":
      return trigger.left;
    case "end":
      return trigger.right - width;
    case "center":
      return trigger.left + trigger.width / 2 - width / 2;
  }
}

/**
 * Onde o balão fica na janela, dadas as caixas do gatilho e do bloco de
 * referência: alinhado como pedido, empurrado para dentro da janela e virado
 * para o outro lado se não couber. `stretch` ocupa a largura do bloco.
 */
export function placeBubble(
  trigger: Box,
  anchor: Box,
  bubble: { width: number; height: number },
  align: ToggletipAlign,
  placement: ToggletipPlacement,
  viewport: { width: number; height: number },
) {
  const width = align === "stretch" ? anchor.width : bubble.width;
  const left = Math.max(EDGE, Math.min(alignedLeft(trigger, anchor, width, align), viewport.width - EDGE - width));
  const above = trigger.top - GAP - bubble.height;
  const below = trigger.bottom + GAP;
  const flipped = placement === "top" ? above < EDGE : below + bubble.height > viewport.height - EDGE;
  const side = flipped ? oppositeSide(placement) : placement;
  return { left, top: side === "top" ? above : below, flipped };
}

