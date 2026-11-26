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
  let left =
    align === "stretch"
      ? anchor.left
      : align === "start"
        ? trigger.left
        : align === "end"
          ? trigger.right - width
          : trigger.left + trigger.width / 2 - width / 2;
  left = Math.max(EDGE, Math.min(left, viewport.width - EDGE - width));
  const above = trigger.top - GAP - bubble.height;
  const below = trigger.bottom + GAP;
  const flipped = placement === "top" ? above < EDGE : below + bubble.height > viewport.height - EDGE;
  const side = flipped ? (placement === "top" ? "bottom" : "top") : placement;
  return { left, top: side === "top" ? above : below, flipped };
}

