import { motion } from "motion/react";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";

type SumRingProps = { sum: number };

const SIZE = 120;
const STROKE = 10;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** O anel da soma: enche até 100%, fica verde e dá um estalo quando fecha. */
export function SumRing({ sum }: SumRingProps) {
  const complete = Math.abs(sum - 100) < 0.005;
  const over = sum > 100.005;
  const fraction = Math.min(sum, 100) / 100;
  const status = complete ? "Fechou 100%" : over ? `Passou ${formatPercent(sum - 100, 0)}` : `Faltam ${formatPercent(100 - sum, 0)}`;

  return (
    <motion.div
      className={cx("sum-ring", complete && "is-complete", over && "is-over")}
      animate={complete ? { scale: [1, 1.08, 1] } : { scale: 1 }}
      transition={{ duration: 0.4 }}
      role="status"
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
          transition={{ type: "spring", stiffness: 90, damping: 18 }}
        />
      </svg>
      <div className="sum-ring__center" aria-hidden="true">
        <strong className="sum-ring__value">{formatPercent(sum, 0)}</strong>
        <span className="sum-ring__status">{status}</span>
      </div>
    </motion.div>
  );
}
