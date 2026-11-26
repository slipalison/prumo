import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { cx } from "../lib/cx";

type ToggletipProps = {
  /** O nome acessível do gatilho: o que ele é, já com o veredito (não só a cor). */
  label: string;
  /** A explicação que abre. */
  content: ReactNode;
  /** O que aparece no gatilho (um chip, um ícone). */
  children: ReactNode;
  className?: string;
};

/**
 * Explicação curta que abre no toque ou clique (iPad e celular), no foco do
 * teclado e ao passar o mouse; fecha com Esc, ao sair do foco ou tocando fora.
 * Aberta por toque/clique, fica aberta ao tirar o mouse. O texto entra numa
 * região `role="status"`, que o leitor de tela lê quando abre (o padrão
 * "toggletip"), e some do DOM quando fecha.
 */
export function Toggletip({ label, content, children, className }: ToggletipProps) {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
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

  return (
    <span
      ref={root}
      className="toggletip"
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
      <span id={bubbleId} role="status" className="toggletip__bubble">
        {open ? content : null}
      </span>
    </span>
  );
}
