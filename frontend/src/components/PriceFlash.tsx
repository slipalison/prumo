import type { ReactNode } from "react";
import { cx } from "../lib/cx";
import { usePriceTrend, type Trend } from "../lib/usePriceTrend";

function Arrow({ trend }: { trend: Trend }) {
  // Cor nunca sozinha: a seta diz o sentido para quem não distingue verde de
  // vermelho. Fora do leitor de tela — o preço muda o tempo todo.
  return (
    <span className={cx("trend-mark", `trend-mark--${trend}`)} aria-hidden="true">
      {trend === "up" ? "▲" : "▼"}
    </span>
  );
}

/** Seta que aparece por um instante quando `value` sobe ou desce. */
export function TrendMark({ value }: { value: number }) {
  const { trend, version } = usePriceTrend(value);
  if (!trend) return null;
  return <Arrow key={version} trend={trend} />;
}

/** O número pisca na cor do sentido em que andou, com a seta ao lado. */
export function PriceFlash({ value, children }: { value: number; children: ReactNode }) {
  const { trend, version } = usePriceTrend(value);
  return (
    <span className="price-flash">
      {trend && <Arrow key={`arrow-${version}`} trend={trend} />}
      <span key={version} className={cx("price-flash__value", trend && `price-flash__value--${trend}`)}>
        {children}
      </span>
    </span>
  );
}
