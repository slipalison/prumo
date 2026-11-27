import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { cx } from "../lib/cx";
import { IconAlert, IconCheck, IconClose, IconSparkle } from "./icons";
import { ToastContext, type ToastInput } from "./toastContext";
import { SPRING } from "../lib/motion";

type Toast = ToastInput & { id: number };

const DURATION_MS = 4500;

const ICON = { success: IconCheck, error: IconAlert, info: IconSparkle } as const;

export function ToastProvider({ children }: Readonly<{ children: ReactNode }>) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);
  const timers = useRef(new Map<number, number>());

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((t) => t.id !== id));
    const timer = timers.current.get(id);
    if (timer) window.clearTimeout(timer);
    timers.current.delete(id);
  }, []);

  const push = useCallback(
    (toast: ToastInput) => {
      const id = nextId.current++;
      setToasts((current) => [...current.slice(-2), { ...toast, id }]);
      timers.current.set(
        id,
        window.setTimeout(() => dismiss(id), DURATION_MS),
      );
    },
    [dismiss],
  );

  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach((timer) => window.clearTimeout(timer));
  }, []);

  return (
    <ToastContext.Provider value={push}>
      {children}
      <output className="toasts" aria-live="polite">
        <AnimatePresence initial={false}>
          {toasts.map((toast) => {
            const Icon = ICON[toast.tone];
            return (
              <motion.span
                key={toast.id}
                className={cx("toast glass", `toast--${toast.tone}`)}
                initial={{ opacity: 0, y: 24, scale: 0.94 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: 12, scale: 0.96 }}
                transition={SPRING.enter}
              >
                <Icon className="toast__icon" />
                <span className="toast__message">{toast.message}</span>
                <button type="button" className="icon-btn icon-btn--sm" aria-label="Dispensar aviso" onClick={() => dismiss(toast.id)}>
                  <IconClose />
                </button>
              </motion.span>
            );
          })}
        </AnimatePresence>
      </output>
    </ToastContext.Provider>
  );
}
