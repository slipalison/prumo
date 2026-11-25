import { useEffect, useRef } from "react";

/**
 * A folha modal aberta agora, se houver. `except` ignora uma folha específica —
 * a que está fechando continua no DOM durante a animação de saída.
 */
export function openModal(except: HTMLElement | null = null): HTMLElement | null {
  const modals = document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]');
  return Array.from(modals).find((modal) => modal !== except) ?? null;
}

/**
 * Foco no campo quando ele aparece — a pergunta da tela é ele. Também marca
 * `data-autofocus` (quem usa é a folha), por isso o atributo vai junto.
 *
 * Com uma folha aberta POR CIMA do campo, ele não rouba o foco dela: quem
 * devolve o foco ao campo é a própria folha, ao fechar (ver `Sheet`).
 */
export function useFocusOnMount<T extends HTMLElement>(enabled: boolean | undefined) {
  const ref = useRef<T>(null);
  useEffect(() => {
    const element = ref.current;
    if (!enabled || !element) return;
    const modal = openModal();
    if (modal && !modal.contains(element)) return;
    element.focus({ preventScroll: true });
  }, [enabled]);
  return ref;
}
