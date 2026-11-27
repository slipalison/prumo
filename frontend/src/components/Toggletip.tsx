import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { cx } from "../lib/cx";
import { EDGE, oppositeSide, placeBubble, type ToggletipAlign, type ToggletipPlacement } from "../lib/placeBubble";

export type { ToggletipAlign, ToggletipPlacement };

/**
 * O balão vai para a camada de topo do navegador (`popover`) quando ela existe:
 * ali nenhum `overflow` de tabela, painel ou folha o corta, e o `backdrop-filter`
 * do vidro não prende a posição. Sem a camada de topo (navegador antigo, jsdom),
 * o balão fica no fluxo, posicionado em CSS, como antes.
 */
function hasTopLayer() {
  return typeof HTMLElement !== "undefined" && typeof HTMLElement.prototype.showPopover === "function";
}

/** Quanto deslocar o balão no fluxo para ele não passar da borda da janela (0 se já cabe). */
function edgeShift(rect: DOMRect, viewportWidth: number): number {
  if (rect.left < EDGE) return EDGE - rect.left;
  if (rect.right > viewportWidth - EDGE) return viewportWidth - EDGE - rect.right;
  return 0;
}

type ToggletipProps = {
  /** O nome acessível do gatilho: o que ele é, já com o veredito (não só a cor). */
  label: string;
  /** A explicação que abre. */
  content: ReactNode;
  /** O que aparece no gatilho (um chip, um ícone). */
  children: ReactNode;
  className?: string;
  /** De que lado abre; vira para o outro se não couber na janela. */
  placement?: ToggletipPlacement;
  align?: ToggletipAlign;
};

/**
 * Explicação curta que abre no toque ou clique (iPad e celular), no foco do
 * teclado e ao passar o mouse; fecha com Esc, ao sair do foco ou tocando fora.
 * Aberta por toque/clique, fica aberta ao tirar o mouse. O texto entra numa
 * região de status (`<output>`, papel `status`), que o leitor de tela lê quando
 * abre (o padrão "toggletip"), e some do DOM quando fecha.
 *
 * O balão fica no DOM junto do gatilho (a ordem de leitura é a da tela) e,
 * aberto, se desloca para não passar da borda da janela; sem espaço em cima,
 * abre embaixo. Quando usar: docs/layout.md, "Tooltip, recolher ou folha".
 */
export function Toggletip({ label, content, children, className, placement = "top", align = "center" }: Readonly<ToggletipProps>) {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [fit, setFit] = useState<{ shift: number; flipped: boolean }>({ shift: 0, flipped: false });
  const [topLayer] = useState(hasTopLayer);
  const root = useRef<HTMLSpanElement>(null);
  const bubble = useRef<HTMLOutputElement>(null);
  const bubbleId = useId();

  useEffect(() => {
    if (!open) return;
    const close = () => {
      setOpen(false);
      setPinned(false);
    };
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  // Aberto, mede o balão e o empurra para dentro da janela. Na camada de topo
  // posiciona-se por inteiro (e de novo a cada rolagem); no fluxo, só desloca.
  useLayoutEffect(() => {
    const element = bubble.current;
    if (!open || !element) {
      setFit({ shift: 0, flipped: false });
      return;
    }
    if (topLayer) {
      const place = () => {
        const triggerBox = root.current?.getBoundingClientRect();
        if (!triggerBox) return;
        const anchorBox = (root.current?.offsetParent ?? root.current)?.getBoundingClientRect() ?? triggerBox;
        // Mede o balão já na camada de topo, na largura natural (ou na do bloco, no `stretch`).
        element.style.width = align === "stretch" ? `${anchorBox.width}px` : "";
        const at = placeBubble(triggerBox, anchorBox, element.getBoundingClientRect(), align, placement, {
          width: document.documentElement.clientWidth,
          height: document.documentElement.clientHeight,
        });
        element.style.left = `${Math.round(at.left)}px`;
        element.style.top = `${Math.round(at.top)}px`;
        setFit((previous) => (previous.flipped === at.flipped ? previous : { shift: 0, flipped: at.flipped }));
      };
      element.showPopover();
      place();
      window.addEventListener("resize", place);
      window.addEventListener("scroll", place, true);
      return () => {
        window.removeEventListener("resize", place);
        window.removeEventListener("scroll", place, true);
        try {
          element.hidePopover();
        } catch {
          // Já fechado: o navegador esconde o popover sozinho ao tirá-lo do DOM.
        }
        element.style.left = "";
        element.style.top = "";
        element.style.width = "";
      };
    }
    const rect = element.getBoundingClientRect();
    if (rect.width === 0) return;
    const width = document.documentElement.clientWidth;
    const shift = edgeShift(rect, width);
    const flipped =
      placement === "top" ? rect.top < EDGE : rect.bottom > document.documentElement.clientHeight - EDGE;
    setFit({ shift: Math.round(shift), flipped });
  }, [open, placement, align, topLayer]);

  const side = fit.flipped ? oppositeSide(placement) : placement;

  return (
    <span
      ref={root}
      className="toggletip"
      data-placement={side}
      data-align={align}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => {
        if (!pinned) setOpen(false);
      }}
    >
      <button
        type="button"
        className={cx("toggletip__trigger", className)}
        aria-label={label}
        aria-expanded={open}
        aria-controls={bubbleId}
        onClick={() => {
          const next = !(open && pinned);
          setOpen(next);
          setPinned(next);
        }}
        onFocus={() => setOpen(true)}
        onBlur={(event) => {
          if (!root.current?.contains(event.relatedTarget)) {
            setOpen(false);
            setPinned(false);
          }
        }}
      >
        {children}
      </button>
      <output
        ref={bubble}
        id={bubbleId}
        className="toggletip__bubble"
        popover={topLayer ? "manual" : undefined}
        style={fit.shift === 0 ? undefined : ({ "--tip-shift": `${fit.shift}px` } as CSSProperties)}
      >
        {open ? content : null}
      </output>
    </span>
  );
}
