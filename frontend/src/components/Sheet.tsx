import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cx } from "../lib/cx";
import { openModal } from "../lib/useFocusOnMount";
import { IconClose } from "./icons";
import { DURATION, SPRING } from "../lib/motion";

type SheetProps = {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
  /**
   * `wide`: no desktop, até 1040 px de largura, para a folha que tem duas
   * colunas (a do ativo: dados à esquerda, lançamentos e fundamentos à
   * direita). No celular não muda nada.
   */
  size?: "default" | "wide";
};

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(root: HTMLElement | null): HTMLElement[] {
  if (!root) return [];
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => !el.hasAttribute("data-focus-skip"),
  );
}

/**
 * Devolve o foco a quem abriu a folha. Se esse elemento sumiu enquanto ela
 * estava aberta (o botão "Já comprei" some quando o aporte é registrado; o
 * convite do primeiro uso some quando nasce o primeiro ativo), o foco vai
 * para o campo principal da tela, marcado com `data-screen-focus` — senão o
 * que a pessoa digitar em seguida se perde. Com outra folha ainda aberta,
 * quem manda no foco é ela.
 */
function restoreFocus(previous: HTMLElement | null, panel: HTMLElement | null): void {
  const other = openModal(panel);
  if (other) return;
  if (previous?.isConnected) {
    previous.focus();
    return;
  }
  document.querySelector<HTMLElement>("[data-screen-focus]")?.focus({ preventScroll: true });
}

/**
 * Folha modal: sobe de baixo no celular, centralizada no desktop. Prende o foco
 * enquanto aberta, fecha com Esc e com toque fora, e devolve o foco a quem a
 * abriu. Elemento com `data-autofocus` ganha o foco ao abrir.
 */
export function Sheet({ open, onClose, title, description, children, footer, className, size = "default" }: SheetProps) {
  const titleId = useId();
  const descriptionId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  // Quem tinha o foco ao abrir, guardado antes de qualquer efeito: um campo do
  // conteúdo que se foca ao nascer (`focusOnMount`) roda antes do efeito desta
  // folha e se passaria por "quem abriu".
  const openerRef = useRef<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (open) openerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }, [open]);

  useEffect(() => {
    if (!open) return undefined;
    const previous = openerRef.current;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    const panel = panelRef.current;
    const preferred = panel?.querySelector<HTMLElement>("[data-autofocus]");
    (preferred ?? focusables(panel)[0] ?? panel)?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusables(panelRef.current);
      const first = items[0];
      const last = items[items.length - 1];
      if (!first || !last) {
        event.preventDefault();
        return;
      }
      const active = document.activeElement;
      const outside = !panelRef.current?.contains(active);
      if (event.shiftKey && (active === first || outside)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || outside)) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);

    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = overflow;
      restoreFocus(previous, panel);
    };
  }, [open]);

  return createPortal(
    <AnimatePresence>
      {open && (
        <div className="sheet-layer">
          <motion.button
            type="button"
            className="sheet__backdrop"
            aria-hidden="true"
            tabIndex={-1}
            data-focus-skip
            onClick={onClose}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: DURATION.fast }}
          />
          <motion.div
            ref={panelRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            aria-describedby={description ? descriptionId : undefined}
            tabIndex={-1}
            className={cx("sheet glass", size === "wide" && "sheet--wide", className)}
            initial={{ y: 48, opacity: 0, scale: 0.98 }}
            animate={{ y: 0, opacity: 1, scale: 1 }}
            exit={{ y: 48, opacity: 0, scale: 0.98 }}
            transition={SPRING.enter}
          >
            <span className="sheet__grabber" aria-hidden="true" />
            <header className="sheet__header">
              <div>
                <h2 id={titleId} className="sheet__title">
                  {title}
                </h2>
                {description && (
                  <p id={descriptionId} className="sheet__description">
                    {description}
                  </p>
                )}
              </div>
              <button type="button" className="icon-btn" aria-label="Fechar" onClick={onClose}>
                <IconClose />
              </button>
            </header>
            <div className="sheet__body">{children}</div>
            {footer && <footer className="sheet__footer">{footer}</footer>}
          </motion.div>
        </div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
