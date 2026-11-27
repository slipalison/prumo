import type { Direction } from "../../lib/panel";

/**
 * A seta do sentido (acima/abaixo); `equal` vira um traço. É só desenho: o
 * sentido vem sempre escrito ao lado ("acima", "abaixo", sinal + ou −).
 */
export function DeltaArrow({ direction }: { direction: Direction }) {
  return (
    <svg className={`delta-arrow delta-arrow--${direction}`} width="14" height="14" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {direction === "above" && <path d="M12 19V5M5 12l7-7 7 7" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />}
      {direction === "below" && <path d="M12 5v14M19 12l-7 7-7-7" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />}
      {direction === "equal" && <path d="M5 12h14" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" />}
    </svg>
  );
}
