import { motion, useReducedMotion, useSpring, type HTMLMotionProps } from "motion/react";
import type { PointerEvent, ReactNode } from "react";
import { cx } from "../lib/cx";

type MagneticButtonProps = Omit<HTMLMotionProps<"button">, "children"> & {
  children: ReactNode;
  variant?: "primary" | "ghost" | "soft" | "danger";
  loading?: boolean;
  /** Puxa o botão alguns pixels em direção ao cursor. */
  magnetic?: boolean;
};

const PULL = 0.22;
const LIMIT = 10;

const clamp = (value: number) => Math.max(-LIMIT, Math.min(LIMIT, value));

export function MagneticButton({
  children,
  variant = "primary",
  loading = false,
  magnetic = true,
  disabled,
  className,
  type = "button",
  onPointerMove,
  onPointerLeave,
  ...rest
}: MagneticButtonProps) {
  const reduce = useReducedMotion();
  const x = useSpring(0, { stiffness: 260, damping: 18 });
  const y = useSpring(0, { stiffness: 260, damping: 18 });

  const handleMove = (event: PointerEvent<HTMLButtonElement>) => {
    if (magnetic && !reduce && !disabled) {
      const rect = event.currentTarget.getBoundingClientRect();
      x.set(clamp((event.clientX - (rect.left + rect.width / 2)) * PULL));
      y.set(clamp((event.clientY - (rect.top + rect.height / 2)) * PULL));
    }
    onPointerMove?.(event);
  };

  const handleLeave = (event: PointerEvent<HTMLButtonElement>) => {
    x.set(0);
    y.set(0);
    onPointerLeave?.(event);
  };

  return (
    <motion.button
      type={type}
      className={cx("btn", `btn--${variant}`, loading && "is-loading", className)}
      style={{ x, y }}
      whileTap={disabled || loading ? undefined : { scale: 0.96 }}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      onPointerMove={handleMove}
      onPointerLeave={handleLeave}
      {...rest}
    >
      <span className="btn__shine" aria-hidden="true" />
      <span className="btn__label">{children}</span>
    </motion.button>
  );
}
