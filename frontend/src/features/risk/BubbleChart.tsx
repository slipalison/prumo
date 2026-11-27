import { useLayoutEffect, useRef, useState } from "react";
import { formatPercent } from "../../lib/format";
import { cx } from "../../lib/cx";
import { formatSignedPercent } from "../performance/performanceFormat";
import { placeLabels, type Side } from "./labels";
import { formatVolatility, niceTicks } from "./riskFormat";

export type Bubble = {
  key: string;
  label: string;
  /** Volatilidade anualizada, em %. */
  x: number;
  /** Retorno no período, em %. */
  y: number;
  /** Peso na carteira, em % (só os ativos; as referências não têm). */
  weight?: number;
  kind: "asset" | "portfolio" | "benchmark";
};

type BubbleChartProps = {
  bubbles: Bubble[];
  /** Resumo para o leitor de tela; os números estão na tabela por ativo. */
  label: string;
};

const W = 640;
const H = 280;
const R_MIN = 6;
const R_MAX = 24;
const MARK = 14;
/** Sem a medida do gráfico (teste, primeira pintura), os de maior peso levam o nome. */
const FALLBACK_LABELS = 6;

/** O raio pela área: a bolha com o dobro do peso tem o dobro da área. */
function radius(weight: number, largest: number): number {
  if (largest <= 0) return R_MIN;
  return R_MIN + (R_MAX - R_MIN) * Math.sqrt(Math.max(weight, 0) / largest);
}

/**
 * Risco × retorno: cada ativo é uma bolha (x = volatilidade anualizada, y =
 * retorno na janela, área = peso na carteira); a carteira (losango) e o
 * Ibovespa (quadrado) ficam de referência, com forma própria além da cor. As
 * bolhas são HTML posicionado sobre a grade em SVG: continuam redondas e com
 * texto legível em qualquer largura. O desenho é `role="img"` com um resumo;
 * quem lê a tela tem a tabela por ativo, com os mesmos números.
 */
export function BubbleChart({ bubbles, label }: BubbleChartProps) {
  const [hover, setHover] = useState<string | null>(null);
  const plot = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  useLayoutEffect(() => {
    const element = plot.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const measure = () => {
      const rect = element.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) setSize({ width: rect.width, height: rect.height });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  if (bubbles.length === 0) return null;
  const xs = bubbles.map((b) => b.x);
  const ys = bubbles.map((b) => b.y);
  const xMax = Math.max(...xs) * 1.12 || 1;
  const yLow = Math.min(...ys, 0);
  const yHigh = Math.max(...ys, 0);
  const yPad = (yHigh - yLow) * 0.12 || 1;
  const [y0, y1] = [yLow - yPad, yHigh + yPad];
  const xTicks = niceTicks(0, xMax, 4);
  const yTicks = niceTicks(y0, y1, 4);
  const left = (x: number) => (x / xMax) * 100;
  const top = (y: number) => (1 - (y - y0) / (y1 - y0)) * 100;
  const largest = Math.max(...bubbles.map((b) => b.weight ?? 0));
  const diameter = (b: Bubble) => (b.kind === "asset" ? radius(b.weight ?? 0, largest) * 2 : MARK);

  let sides: Map<string, Side>;
  if (size) {
    const marks = bubbles.map((bubble) => ({
      key: bubble.key,
      label: bubble.label,
      kind: bubble.kind,
      weight: bubble.weight,
      cx: (left(bubble.x) / 100) * size.width,
      cy: (top(bubble.y) / 100) * size.height,
      r: diameter(bubble) / 2,
    }));
    sides = placeLabels(marks, size.width, size.height);
  } else {
    const named = bubbles
      .filter((b) => b.kind !== "asset")
      .concat(
        bubbles
          .filter((b) => b.kind === "asset")
          .sort((a, b) => (b.weight ?? 0) - (a.weight ?? 0))
          .slice(0, FALLBACK_LABELS),
      );
    sides = new Map(named.map((b) => [b.key, left(b.x) > 70 ? "left" : "right"]));
  }
  // As bolhas maiores atrás, para as pequenas não sumirem embaixo delas.
  const order = [...bubbles].sort((a, b) => (b.weight ?? 0) - (a.weight ?? 0));
  const focused = bubbles.find((b) => b.key === hover);

  return (
    <div className="bubble-chart">
      <div className="bubble-chart__y" aria-hidden="true">
        {yTicks.map((tick) => (
          <span key={tick} className="chart__tick" style={{ top: `${top(tick)}%` }}>
            {formatSignedPercent(tick, 0)}
          </span>
        ))}
      </div>
      <div ref={plot} className="bubble-chart__plot" role="img" aria-label={label} onPointerLeave={() => setHover(null)}>
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
          {yTicks.map((tick) => (
            <line key={`y${tick}`} className="chart__grid" x1={0} x2={W} y1={(top(tick) / 100) * H} y2={(top(tick) / 100) * H} />
          ))}
          {xTicks.map((tick) => (
            <line key={`x${tick}`} className="chart__grid" x1={(left(tick) / 100) * W} x2={(left(tick) / 100) * W} y1={0} y2={H} />
          ))}
          <line className="chart__zero" x1={0} x2={W} y1={(top(0) / 100) * H} y2={(top(0) / 100) * H} />
        </svg>
        {order.map((b) => {
          const d = diameter(b);
          const side = sides.get(b.key) ?? (hover === b.key ? (left(b.x) > 70 ? "left" : "right") : null);
          return (
            <span
              key={b.key}
              className={cx("bubble", `bubble--${b.kind}`, hover === b.key && "is-hover")}
              style={{ left: `${left(b.x)}%`, top: `${top(b.y)}%`, width: d, height: d }}
              onPointerEnter={() => setHover(b.key)}
              onPointerDown={() => setHover(b.key)}
              aria-hidden="true"
              data-key={b.key}
            >
              {side && <span className={cx("bubble__label", `is-${side}`)}>{b.label}</span>}
            </span>
          );
        })}
        {focused && (
          <div
            className={cx("chart__tooltip bubble-chart__tooltip", left(focused.x) > 55 && "is-left")}
            style={{ left: `${left(focused.x)}%`, top: `${Math.min(Math.max(top(focused.y), 20), 70)}%` }}
            aria-hidden="true"
          >
            <strong className="chart__tooltip-date">{focused.label}</strong>
            <span className="chart__tooltip-row">
              <span />
              <span>Volatilidade</span>
              <strong>{formatVolatility(focused.x)}</strong>
            </span>
            <span className="chart__tooltip-row">
              <span />
              <span>Retorno</span>
              <strong>{formatSignedPercent(focused.y)}</strong>
            </span>
            {focused.weight !== undefined && (
              <span className="chart__tooltip-row">
                <span />
                <span>Peso</span>
                <strong>{formatPercent(focused.weight, 1)}</strong>
              </span>
            )}
          </div>
        )}
      </div>
      <div className="bubble-chart__x" aria-hidden="true">
        {xTicks.map((tick) => (
          <span key={tick} className="bubble-chart__xtick" style={{ left: `${left(tick)}%` }}>
            {formatPercent(tick, 0)}
          </span>
        ))}
      </div>
      <span className="bubble-chart__axis-name bubble-chart__axis-name--x" aria-hidden="true">
        Volatilidade ao ano →
      </span>
    </div>
  );
}
