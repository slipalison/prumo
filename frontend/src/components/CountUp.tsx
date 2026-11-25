import { animate, useMotionValue, useMotionValueEvent, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { cx } from "../lib/cx";
import { formatBRL } from "../lib/format";

type CountUpProps = {
  value: number;
  format?: (value: number) => string;
  className?: string;
};

/**
 * Número que conta até o valor com mola sempre que ele muda. Enquanto conta, o
 * leitor de tela ouve só o valor final — os intermediários ficam escondidos.
 */
export function CountUp({ value, format = formatBRL, className }: CountUpProps) {
  const reduce = useReducedMotion();
  const motionValue = useMotionValue(reduce ? value : 0);
  const [display, setDisplay] = useState(() => format(reduce ? value : 0));
  // `format` costuma ser uma seta inline, com identidade nova a cada render: ela
  // fica num ref, e só o número dispara a animação.
  const formatRef = useRef(format);
  useEffect(() => {
    formatRef.current = format;
  });

  useMotionValueEvent(motionValue, "change", (latest) => setDisplay(formatRef.current(latest)));

  useEffect(() => {
    if (reduce) {
      motionValue.set(value);
      setDisplay(formatRef.current(value));
      return undefined;
    }
    const controls = animate(motionValue, value, { type: "spring", stiffness: 70, damping: 18, restDelta: 0.001 });
    return () => controls.stop();
  }, [value, reduce, motionValue]);

  const final = format(value);
  const settled = display === final;

  return (
    <span className={cx("count-up", className)}>
      <span aria-hidden={settled ? undefined : true}>{display}</span>
      {!settled && <span className="sr-only">{final}</span>}
    </span>
  );
}
