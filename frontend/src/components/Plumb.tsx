import { motion, useReducedMotion, type TargetAndTransition, type Transition } from "motion/react";
import { useId } from "react";
import { cx } from "../lib/cx";

/**
 * - `idle`: respira, quase parado (estado vazio, convite).
 * - `swinging`: balança de verdade (calculando).
 * - `settled`: amortece até o centro — a carteira voltou ao eixo.
 * - `success`: amortece e desenha o check (aporte registrado).
 */
export type PlumbState = "idle" | "swinging" | "settled" | "success";

type PlumbProps = {
  state?: PlumbState;
  size?: number;
  label?: string;
  className?: string;
};

const MOTION: Record<PlumbState, { animate: TargetAndTransition; transition: Transition }> = {
  idle: {
    animate: { rotate: [-5, 5] },
    transition: { duration: 3.2, repeat: Infinity, repeatType: "mirror", ease: "easeInOut" },
  },
  swinging: {
    animate: { rotate: [-26, 26] },
    transition: { duration: 0.85, repeat: Infinity, repeatType: "mirror", ease: "easeInOut" },
  },
  settled: {
    animate: { rotate: [22, -14, 8, -4, 1.5, 0] },
    transition: { duration: 1.25, ease: "easeOut" },
  },
  success: {
    animate: { rotate: [22, -14, 8, -4, 1.5, 0] },
    transition: { duration: 1.25, ease: "easeOut" },
  },
};

export function Plumb({ state = "idle", size = 96, label = "Prumo", className }: PlumbProps) {
  const reduce = useReducedMotion();
  const id = useId().replace(/:/g, "");
  const motionSpec = MOTION[state];

  return (
    <svg
      className={cx("plumb", `plumb--${state}`, className)}
      width={size}
      height={size * 1.4}
      viewBox="0 0 100 140"
      role="img"
      aria-label={label}
    >
      <defs>
        <linearGradient id={`${id}-gold`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#ffe3a3" />
          <stop offset="0.5" stopColor="#f5b73b" />
          <stop offset="1" stopColor="#a8661a" />
        </linearGradient>
        <radialGradient id={`${id}-glow`} cx="0.5" cy="0.5" r="0.5">
          <stop offset="0" stopColor="#f5c25b" stopOpacity="0.55" />
          <stop offset="1" stopColor="#f5c25b" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Eixo de referência: a vertical verdadeira. */}
      <line className="plumb__axis" x1="50" y1="10" x2="50" y2="138" />
      <circle cx="50" cy="6" r="3.2" className="plumb__pivot" />

      <motion.g
        style={{ originX: 0.5, originY: 0 }}
        initial={false}
        animate={reduce ? { rotate: 0 } : motionSpec.animate}
        transition={reduce ? { duration: 0 } : motionSpec.transition}
      >
        <line x1="50" y1="6" x2="50" y2="84" className="plumb__string" />
        <ellipse cx="50" cy="112" rx="26" ry="26" fill={`url(#${id}-glow)`} className="plumb__glow" />
        <rect x="37" y="82" width="26" height="8" rx="3" fill="#ffe7b0" />
        <path d="M36 90 H64 L50 132 Z" fill={`url(#${id}-gold)`} />
        <path d="M50 90 V132 L64 90 Z" fill="#000" fillOpacity="0.2" />
        <path d="M41 93 L47 93 L50 118 Z" fill="#fff" fillOpacity="0.35" />
      </motion.g>

      {state === "success" && (
        <motion.path
          className="plumb__check"
          d="M30 64 L44 76 L70 50"
          fill="none"
          initial={{ pathLength: reduce ? 1 : 0, opacity: 0 }}
          animate={{ pathLength: 1, opacity: 1 }}
          transition={{ delay: reduce ? 0 : 0.7, duration: 0.45, ease: "easeOut" }}
        />
      )}
    </svg>
  );
}
