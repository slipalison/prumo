import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { cx } from "../lib/cx";

/** Folga mínima entre o balão e a borda da janela. */
const EDGE = 8;

export type ToggletipPlacement = "top" | "bottom";
/**
 * - `center`: centrado no gatilho (o padrão).
 * - `start` / `end`: alinhado à esquerda / à direita do gatilho.
 * - `stretch`: na largura do bloco posicionado mais próximo (o balão das
 *   referências de preço ocupa a largura do bloco, não a do chip).
 */
export type ToggletipAlign = "center" | "start" | "end" | "stretch";

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
 * região `role="status"`, que o leitor de tela lê quando abre (o padrão
 * "toggletip"), e some do DOM quando fecha.
 *
 * O balão fica no DOM junto do gatilho (a ordem de leitura é a da tela) e,
 * aberto, se desloca para não passar da borda da janela; sem espaço em cima,
 * abre embaixo. Quando usar: docs/layout.md, "Tooltip, recolher ou folha".
 */
export function Toggletip({ label, content, children, className, placement = "top", align = "center" }: ToggletipProps) {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [fit, setFit] = useState<{ shift: number; flipped: boolean }>({ shift: 0, flipped: false });
  const root = useRef<HTMLSpanElement>(null);
  const bubble = useRef<HTMLSpanElement>(null);
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

  // Aberto, mede o balão uma vez e o empurra para dentro da janela.
  useLayoutEffect(() => {
    if (!open) {
      setFit({ shift: 0, flipped: false });
      return;
    }
    const rect = bubble.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return;
    const width = document.documentElement.clientWidth;
    const shift = rect.left < EDGE ? EDGE - rect.left : rect.right > width - EDGE ? width - EDGE - rect.right : 0;
    const flipped =
      placement === "top" ? rect.top < EDGE : rect.bottom > document.documentElement.clientHeight - EDGE;
    setFit({ shift: Math.round(shift), flipped });
  }, [open, placement]);

  const side = fit.flipped ? (placement === "top" ? "bottom" : "top") : placement;

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
      <span
        ref={bubble}
        id={bubbleId}
        role="status"
        className="toggletip__bubble"
        style={fit.shift === 0 ? undefined : ({ "--tip-shift": `${fit.shift}px` } as CSSProperties)}
      >
        {open ? content : null}
      </span>
    </span>
  );
}
