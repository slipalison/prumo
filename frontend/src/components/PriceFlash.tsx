import type { ReactNode } from "react";
import { cx } from "../lib/cx";
import { useMoney } from "../lib/privacy";
import { usePriceTrend, type Trend } from "../lib/usePriceTrend";

function Arrow({ trend }: Readonly<{ trend: Trend }>) {
  // Cor nunca sozinha: a seta diz o sentido para quem não distingue verde de
  // vermelho. Fora do leitor de tela — o preço muda o tempo todo.
  return (
    <span className={cx("trend-mark", `trend-mark--${trend}`)} aria-hidden="true">
      {trend === "up" ? "▲" : "▼"}
    </span>
  );
}

/**
 * Seta que aparece por um instante quando `value` sobe ou desce. Com os
 * valores ocultos ela some: o sentido em que o patrimônio andou também é dado.
 */
export function TrendMark({ value }: Readonly<{ value: number }>) {
  const { trend, version } = usePriceTrend(value);
  const { hidden } = useMoney();
  if (!trend || hidden) return null;
  return <Arrow key={version} trend={trend} />;
}

/** O número pisca na cor do sentido em que andou, com a seta ao lado. */
export function PriceFlash({ value, children }: Readonly<{ value: number; children: ReactNode }>) {
  const { trend: moved, version } = usePriceTrend(value);
  const { hidden } = useMoney();
  // Oculto, não pisca nem mostra seta: a cor e a seta entregariam o sentido.
  const trend = hidden ? null : moved;
  return (
    <span className="price-flash">
      {trend && <Arrow key={`arrow-${version}`} trend={trend} />}
      <span key={version} className={cx("price-flash__value", trend && `price-flash__value--${trend}`)}>
        {children}
      </span>
    </span>
  );
}
