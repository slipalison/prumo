import { motion, useReducedMotion } from "motion/react";

/** A marca: o prumo pequeno, que dá um balanço curto ao passar o mouse. */
export function Logo() {
  const reduce = useReducedMotion();
  return (
    <a href="#/aportar" className="logo" aria-label="Prumo, ir para Aportar">
      <motion.svg
        className="logo__mark"
        viewBox="0 0 32 40"
        width="26"
        height="32"
        aria-hidden="true"
        whileHover={reduce ? undefined : { rotate: [0, -12, 9, -5, 2, 0], transition: { duration: 0.7 } }}
        style={{ originX: 0.5, originY: 0 }}
      >
        <defs>
          <linearGradient id="logo-gold" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor="#ffe3a3" />
            <stop offset="0.55" stopColor="#f5b73b" />
            <stop offset="1" stopColor="#a8661a" />
          </linearGradient>
        </defs>
        <circle cx="16" cy="2.5" r="2" fill="currentColor" />
        <line x1="16" y1="3" x2="16" y2="16" stroke="currentColor" strokeWidth="1.4" />
        <rect x="9" y="15" width="14" height="4" rx="1.5" fill="#ffe7b0" />
        <path d="M9 19 H23 L16 39 Z" fill="url(#logo-gold)" />
        <path d="M16 19 V39 L23 19 Z" fill="#000" fillOpacity="0.2" />
      </motion.svg>
      <span className="logo__word">Prumo</span>
    </a>
  );
}
