import confetti from "canvas-confetti";

const GOLD = ["#ffd98a", "#f5c25b", "#ffb938", "#fff3d1", "#e8a33a"];

export function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Confete dourado saindo do ponto onde a pessoa clicou (ou do centro).
 * Com `prefers-reduced-motion`, não dispara — o check desenhado basta.
 */
export function celebrate(origin?: { x: number; y: number }): boolean {
  if (prefersReducedMotion()) return false;
  const at = origin ?? { x: 0.5, y: 0.6 };
  const base = { origin: at, colors: GOLD, disableForReducedMotion: true, zIndex: 9999 };
  void confetti({ ...base, particleCount: 90, spread: 75, startVelocity: 42, scalar: 1 });
  void confetti({ ...base, particleCount: 40, spread: 120, startVelocity: 28, scalar: 0.75, ticks: 160 });
  return true;
}

/** Converte um elemento em origem normalizada (0–1) para o confete. */
export function originFrom(element: Element | null): { x: number; y: number } | undefined {
  if (!element || !window.innerWidth || !window.innerHeight) return undefined;
  const rect = element.getBoundingClientRect();
  return {
    x: (rect.left + rect.width / 2) / window.innerWidth,
    y: (rect.top + rect.height / 2) / window.innerHeight,
  };
}
