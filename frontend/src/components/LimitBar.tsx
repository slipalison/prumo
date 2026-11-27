import { motion } from "motion/react";
import { SPRING } from "../lib/motion";

type LimitBarProps = {
  /** Onde a carteira está (0–100). */
  current: number;
  /** O limite que a pessoa escolheu (0–100). */
  limit: number;
  /** Descrição para leitor de tela ("ITUB4: 31% da carteira, limite 15%"). */
  label: string;
};

/**
 * A barra de um aviso: quanto a carteira tem e, no fio de prumo, o limite. A
 * escala acompanha a maior das duas medidas, como a barra de classe; passar do
 * limite é dito em texto no cartão, a cor só reforça.
 */
export function LimitBar({ current, limit, label }: Readonly<LimitBarProps>) {
  const scale = Math.min(Math.max(current, limit, 1) * 1.15, 100);
  const toWidth = (value: number) => `${Math.min((value / scale) * 100, 100)}%`;
  const over = current > limit;

  return (
    <div // NOSONAR: o <meter> nativo desenha a própria barra e não mostra filhos; aqui a barra tem preenchimento e o fio do limite
      className="class-bar limit-bar"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Number(current.toFixed(2))}
    >
      <div className="class-bar__track">
        <motion.div
          className={over ? "class-bar__fill limit-bar__fill limit-bar__fill--over" : "class-bar__fill limit-bar__fill"}
          initial={{ width: "0%" }}
          animate={{ width: toWidth(current) }}
          transition={SPRING.value}
        />
        <span className="class-bar__target" style={{ left: toWidth(limit) }} aria-hidden="true">
          <svg viewBox="0 0 10 14" width="10" height="14">
            <path d="M2 5 H8 L5 13 Z" />
          </svg>
        </span>
      </div>
    </div>
  );
}
