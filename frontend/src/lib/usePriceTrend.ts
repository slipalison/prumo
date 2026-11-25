import { useEffect, useState } from "react";

export type Trend = "up" | "down";

/**
 * Para onde o número foi na última mudança, por `hold` ms. `version` muda a
 * cada mudança: usado como `key`, reinicia a animação mesmo quando o número
 * sobe duas vezes seguidas.
 */
export function usePriceTrend(value: number, hold = 1600): { trend: Trend | null; version: number } {
  const [previous, setPrevious] = useState(value);
  const [trend, setTrend] = useState<Trend | null>(null);
  const [version, setVersion] = useState(0);

  // Ajuste durante a renderização (o jeito do React para estado que depende
  // da prop anterior): sem efeito, sem um quadro com a cor errada.
  if (value !== previous) {
    setPrevious(value);
    setTrend(value > previous ? "up" : "down");
    setVersion((v) => v + 1);
  }

  useEffect(() => {
    if (!trend) return undefined;
    const timer = setTimeout(() => setTrend(null), hold);
    return () => clearTimeout(timer);
  }, [trend, version, hold]);

  return { trend, version };
}
