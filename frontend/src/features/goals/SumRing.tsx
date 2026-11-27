import { motion } from "motion/react";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { DURATION, SPRING } from "../../lib/motion";
import "./nativeTags.css";

type SumRingProps = { sum: number };

const SIZE = 120;
const STROKE = 10;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** O que falta, o que passou ou "Fechou 100%". */
function statusText(sum: number, complete: boolean, over: boolean): string {
  if (complete) return "Fechou 100%";
  if (over) return `Passou ${formatPercent(sum - 100, 0)}`;
  return `Faltam ${formatPercent(100 - sum, 0)}`;
}

/** O anel da soma: enche até 100%, fica verde e dá um estalo quando fecha. */
export function SumRing({ sum }: Readonly<SumRingProps>) {
  const complete = Math.abs(sum - 100) < 0.005;
  const over = sum > 100.005;
  const fraction = Math.min(sum, 100) / 100;
  const status = statusText(sum, complete, over);

  return (
    <motion.output
      className={cx("sum-ring", complete && "is-complete", over && "is-over")}
      animate={complete ? { scale: [1, 1.08, 1] } : { scale: 1 }}
      transition={{ duration: DURATION.slow }}
      aria-live="polite"
      aria-label={`Soma das metas: ${formatPercent(sum, 0)}. ${status}`}
    >
      <svg width={SIZE} height={SIZE} viewBox={`0 0 ${SIZE} ${SIZE}`} aria-hidden="true">
        <circle className="sum-ring__track" cx={SIZE / 2} cy={SIZE / 2} r={RADIUS} strokeWidth={STROKE} fill="none" />
        <motion.circle
          className="sum-ring__fill"
          cx={SIZE / 2}
          cy={SIZE / 2}
          r={RADIUS}
          strokeWidth={STROKE}
          fill="none"
          strokeLinecap="round"
          transform={`rotate(-90 ${SIZE / 2} ${SIZE / 2})`}
          strokeDasharray={CIRCUMFERENCE}
          animate={{ strokeDashoffset: CIRCUMFERENCE * (1 - fraction) }}
          transition={SPRING.value}
        />
      </svg>
      <div className="sum-ring__center" aria-hidden="true">
        <strong className="sum-ring__value">{formatPercent(sum, 0)}</strong>
        <span className="sum-ring__status">{status}</span>
      </div>
    </motion.output>
  );
}
