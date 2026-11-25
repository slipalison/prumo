import { AnimatePresence, motion } from "motion/react";
import { cx } from "../lib/cx";

type ScoreBadgeProps = {
  score: number;
  size?: "sm" | "lg";
};

function scoreTier(score: number): "zero" | "low" | "mid" | "high" {
  if (score <= 0) return "zero";
  if (score <= 3) return "low";
  if (score <= 6) return "mid";
  return "high";
}

/** A nota do ativo. Muda virando, como placar. */
export function ScoreBadge({ score, size = "sm" }: ScoreBadgeProps) {
  return (
    <span className={cx("score", `score--${scoreTier(score)}`, `score--${size}`)}>
      <span className="sr-only">Nota </span>
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span
          key={score}
          className="score__value"
          initial={{ rotateX: -90, opacity: 0 }}
          animate={{ rotateX: 0, opacity: 1 }}
          exit={{ rotateX: 90, opacity: 0 }}
          transition={{ type: "spring", stiffness: 420, damping: 26 }}
        >
          {score}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
