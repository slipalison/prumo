import { motion } from "motion/react";
import type { AssetClass } from "../api/types";
import { CLASS_META, classColor } from "../lib/assetClasses";
import { formatPercent } from "../lib/format";
import { DURATION, SPRING } from "../lib/motion";

type ClassBarProps = {
  assetClass: AssetClass;
  /** % da carteira hoje (0–100). */
  current: number;
  /** % da meta (0–100). */
  target: number;
  /** % depois do aporte; o pedaço entre `current` e `after` aparece dourado. */
  after?: number;
};

/**
 * A barra de uma classe. A escala acompanha a maior das três medidas, para uma
 * classe de 5% não virar um risco invisível numa régua de 0 a 100. A meta é o
 * fio de prumo: uma linha vertical com o peso no topo.
 */
export function ClassBar({ assetClass, current, target, after }: Readonly<ClassBarProps>) {
  const scale = Math.min(Math.max(target, current, after ?? 0, 1) * 1.25, 100);
  const toWidth = (value: number) => `${Math.min((value / scale) * 100, 100)}%`;
  const added = after !== undefined && after > current + 0.005;
  const label = CLASS_META[assetClass].label;
  const description = added
    ? `${label}: ${formatPercent(current)} hoje, ${formatPercent(after)} depois do aporte, meta ${formatPercent(target)}`
    : `${label}: ${formatPercent(current)} de ${formatPercent(target)} da meta`;

  return (
    <div // NOSONAR: o <meter> nativo desenha a própria barra e não mostra filhos; aqui a barra tem preenchimento, aporte e o fio da meta
      className="class-bar"
      role="meter"
      aria-label={description}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Number(current.toFixed(2))}
    >
      <div className="class-bar__track">
        <motion.div
          className="class-bar__fill"
          style={{ background: classColor(assetClass) }}
          initial={{ width: "0%" }}
          animate={{ width: toWidth(current) }}
          transition={SPRING.value}
        />
        {added && (
          <motion.div
            className="class-bar__added"
            initial={{ left: toWidth(current), width: "0%" }}
            animate={{ left: toWidth(current), width: toWidth(after - current) }}
            transition={{ ...SPRING.value, delay: DURATION.fast }}
          />
        )}
        {target > 0 && (
          <span className="class-bar__target" style={{ left: toWidth(target) }} aria-hidden="true">
            <svg viewBox="0 0 10 14" width="10" height="14">
              <path d="M2 5 H8 L5 13 Z" />
            </svg>
          </span>
        )}
      </div>
    </div>
  );
}
