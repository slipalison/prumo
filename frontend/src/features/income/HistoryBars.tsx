import { motion } from "motion/react";
import { useId } from "react";
import type { HistoryBucket, HistoryInterval } from "../../api/incomeHistoryTypes";
import { niceCeiling } from "../../lib/income";
import { bucketLong, bucketShort, labelStep, segmentsOf, type Segment, type Series } from "../../lib/incomeHistory";
import { SPRING, STAGGER } from "../../lib/motion";
import { useMoney } from "../../lib/privacy";
import { useWidth } from "./useWidth";

type HistoryBarsProps = {
  buckets: HistoryBucket[];
  series: Series[];
  interval: HistoryInterval;
  /** O nome do gráfico, que a tabela escondida repete na legenda dela. */
  caption: string;
};

const FALLBACK_WIDTH = 640;
const HEIGHT = 220;
const NARROW = 420;
const NARROW_HEIGHT = 176;
const LEFT = 56;
const RIGHT = 8;
const TOP = 10;
const BOTTOM = 26;
const MAX_BAR = 48;
/** O vão de superfície entre duas partes empilhadas. */
const GAP = 2;
/** Uma parte nunca some de vez: o menor desenho visível. */
const MIN_PART = 1;

type Piece = { key: string; color: string; colorIndex: number; upcoming: boolean; y: number; height: number };

/**
 * As partes de uma barra de baixo para cima: em cada grupo, o recebido e depois
 * o a receber, com um vão de superfície entre duas partes.
 */
function stack(segments: Segment[], series: Series[], scale: (value: number) => number, baseY: number): Piece[] {
  const pieces: Piece[] = [];
  let top = baseY;
  for (const segment of segments) {
    const colorIndex = series.indexOf(segment.series);
    for (const [upcoming, value] of [
      [false, segment.received],
      [true, segment.upcoming],
    ] as const) {
      if (!(value > 0)) continue;
      const full = Math.max(scale(value), MIN_PART + GAP);
      top -= full;
      pieces.push({
        key: `${segment.series.key}-${upcoming ? "upcoming" : "received"}`,
        color: segment.series.color,
        colorIndex,
        upcoming,
        y: top + GAP,
        height: full - GAP,
      });
    }
  }
  return pieces;
}

/**
 * Barras empilhadas do histórico: uma parte por grupo, de baixo para cima na
 * ordem da legenda (a do histórico inteiro), e dentro de cada parte o recebido
 * cheio e o a receber listrado na mesma cor. A cor nunca é a única pista: a
 * legenda dá o nome e o total de cada grupo, o balão de cada barra lista as
 * partes e uma tabela escondida traz todos os números para o leitor de tela.
 */
export function HistoryBars({ buckets, series, interval, caption }: HistoryBarsProps) {
  const patternBase = useId();
  const money = useMoney();
  const { ref, width } = useWidth(FALLBACK_WIDTH);
  const plotW = Math.max(width - LEFT - RIGHT, 1);
  const height = width < NARROW ? NARROW_HEIGHT : HEIGHT;
  const plotH = height - TOP - BOTTOM;
  const baseY = TOP + plotH;
  const tallest = Math.max(...buckets.map((b) => b.received + b.upcoming), 0);
  const ceiling = niceCeiling(tallest);
  const slot = plotW / Math.max(buckets.length, 1);
  const barWidth = Math.max(Math.min(slot * 0.62, MAX_BAR), 2);
  const step = labelStep(slot);
  const ticks = [0, ceiling / 2, ceiling];
  const patternOf = (index: number) => `${patternBase}-hatch-${index}`;
  const scale = (value: number) => (value / ceiling) * plotH;

  return (
    <figure className="history-bars">
      <div ref={ref} className="history-bars__plot">
        <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`${caption}. Os números estão na tabela a seguir.`} className="history-bars__svg">
          <defs>
            {series.map((s, index) => (
              <pattern key={s.key} id={patternOf(index)} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                <rect width="2.4" height="6" fill={s.color} />
              </pattern>
            ))}
          </defs>
          {ticks.map((tick) => {
            const y = baseY - scale(tick);
            return (
              <g key={tick}>
                <line x1={LEFT} x2={width - RIGHT} y1={y} y2={y} className="history-bars__grid" />
                <text x={LEFT - 8} y={y + 4} textAnchor="end" className="history-bars__axis">
                  {tick === 0 ? "0" : money.compact(tick)}
                </text>
              </g>
            );
          })}
          {buckets.map((bucket, index) => {
            const x = LEFT + slot * index + (slot - barWidth) / 2;
            const segments = segmentsOf(bucket, series);
            const parts = segments.map((s) => `${s.series.label} ${money.brl(s.received + s.upcoming)}`).join("; ");
            const upcoming = bucket.upcoming > 0 ? `, ${money.brl(bucket.upcoming)} a receber` : "";
            const showLabel = (buckets.length - 1 - index) % step === 0;
            return (
              <g key={bucket.start} className="history-bars__bar">
                <title>
                  {`${bucketLong(bucket.start, interval)}: ${money.brl(bucket.received + bucket.upcoming)}${upcoming}${segments.length > 1 ? ` (${parts})` : ""}`}
                </title>
                {/* A área do balão é a coluna inteira, não só a barra. */}
                <rect x={LEFT + slot * index} y={TOP} width={slot} height={plotH} className="history-bars__hit" />
                {stack(segments, series, scale, baseY).map((piece) => (
                  <motion.rect
                    key={piece.key}
                    x={x}
                    width={barWidth}
                    rx={3}
                    fill={piece.upcoming ? `url(#${patternOf(piece.colorIndex)})` : piece.color}
                    stroke={piece.upcoming ? piece.color : undefined}
                    strokeWidth={piece.upcoming ? 1.2 : undefined}
                    className={piece.upcoming ? "history-bars__upcoming" : "history-bars__received"}
                    initial={{ height: 0, y: baseY }}
                    animate={{ height: piece.height, y: piece.y }}
                    transition={{ ...SPRING.value, delay: Math.min(index, 24) * STAGGER }}
                  />
                ))}
                {showLabel && (
                  <text x={x + barWidth / 2} y={height - 8} textAnchor="middle" className="history-bars__axis">
                    {bucketShort(bucket.start, interval)}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      </div>
      {/* A tabela fica dentro de um bloco escondido: uma <table> não encolhe a 1 px
          e, sozinha, alargaria a página. */}
      <div className="sr-only">
        <table>
          <caption>{caption}</caption>
          <thead>
            <tr>
              <th scope="col">Período</th>
              {series.map((s) => (
                <th key={s.key} scope="col">
                  {s.label}
                </th>
              ))}
              <th scope="col">Total</th>
              <th scope="col">A receber</th>
            </tr>
          </thead>
          <tbody>
            {buckets.map((bucket) => {
              const byKey = new Map(segmentsOf(bucket, series).map((s) => [s.series.key, s.received + s.upcoming]));
              return (
                <tr key={bucket.start}>
                  <th scope="row">{bucketLong(bucket.start, interval)}</th>
                  {series.map((s) => (
                    <td key={s.key}>{money.brl(byKey.get(s.key) ?? 0)}</td>
                  ))}
                  <td>{money.brl(bucket.received + bucket.upcoming)}</td>
                  <td>{money.brl(bucket.upcoming)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </figure>
  );
}
