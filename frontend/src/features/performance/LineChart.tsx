import { motion } from "motion/react";
import { useId, useState, type PointerEvent } from "react";
import { formatDay, spansYears } from "./performanceFormat";
import { DURATION, SPRING } from "../../lib/motion";

export type ChartSeries = {
  key: string;
  label: string;
  /** Cor da linha (variável CSS); o texto nunca usa a cor da série. */
  color: string;
  /** Tracejado do SVG: a segunda pista, além da cor. */
  dash?: string | undefined;
  /** Preenche a área até a base (o patrimônio). */
  area?: boolean | undefined;
  values: Array<number | null>;
};

type LineChartProps = {
  dates: string[];
  series: ChartSeries[];
  format: (value: number) => string;
  /** Resumo para o leitor de tela; os números estão na tabela. */
  label: string;
  /** Inclui o zero na escala e desenha a linha dele (rentabilidade). */
  zeroLine?: boolean;
  /** A escala parte do zero (patrimônio). */
  fromZero?: boolean;
};

const W = 640;
const H = 220;
const PAD_Y = 12;

type Scale = { min: number; max: number };

function scale(series: ChartSeries[], zeroLine: boolean, fromZero: boolean): Scale {
  const values = series.flatMap((s) => s.values.filter((v): v is number => v !== null && Number.isFinite(v)));
  let min = Math.min(...values, ...(zeroLine || fromZero ? [0] : []));
  let max = Math.max(...values, ...(zeroLine ? [0] : []));
  if (!Number.isFinite(min) || !Number.isFinite(max)) return { min: 0, max: 1 };
  if (max - min < 1e-9) {
    const pad = Math.abs(max) * 0.05 || 1;
    min -= pad;
    max += pad;
  }
  const pad = (max - min) * 0.08;
  return { min: fromZero ? min : min - pad, max: max + pad };
}

const xAt = (index: number, count: number) => (count <= 1 ? W / 2 : (index / (count - 1)) * W);
const yAt = (value: number, s: Scale) => PAD_Y + (1 - (value - s.min) / (s.max - s.min)) * (H - 2 * PAD_Y);

/** Caminho da linha, interrompido onde falta valor. */
function linePath(values: Array<number | null>, s: Scale): string {
  let path = "";
  let pen = false;
  values.forEach((value, index) => {
    if (value === null) {
      pen = false;
      return;
    }
    path += `${pen ? "L" : "M"}${xAt(index, values.length).toFixed(2)} ${yAt(value, s).toFixed(2)} `;
    pen = true;
  });
  return path.trim();
}

function areaPath(values: Array<number | null>, s: Scale): string {
  const line = linePath(values, s);
  if (!line || values.some((v) => v === null)) return "";
  const base = yAt(Math.max(s.min, 0), s).toFixed(2);
  return `${line} L${xAt(values.length - 1, values.length).toFixed(2)} ${base} L${xAt(0, values.length).toFixed(2)} ${base} Z`;
}

/**
 * Linhas em SVG próprio: escala que cabe nos dados, grade discreta, a linha se
 * desenha ao entrar e, ao passar o ponteiro, uma régua vertical mostra o valor
 * de cada série no dia. O desenho é `role="img"` com um resumo; quem lê a tela
 * tem a tabela.
 */
export function LineChart({ dates, series, format, label, zeroLine = false, fromZero = false }: LineChartProps) {
  const [hover, setHover] = useState<number | null>(null);
  const gradient = useId();
  const s = scale(series, zeroLine, fromZero);
  const count = dates.length;
  const ticks = [s.max - (s.max - s.min) * 0.08, (s.max + s.min) / 2, s.min + (fromZero ? 0 : (s.max - s.min) * 0.08)];

  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || count === 0) return;
    const ratio = Math.min(Math.max((event.clientX - rect.left) / rect.width, 0), 1);
    setHover(Math.round(ratio * (count - 1)));
  };

  const hoverDate = hover === null ? undefined : dates[hover];
  const left = hover === null ? 0 : (xAt(hover, count) / W) * 100;

  return (
    <div className="chart">
      <div className="chart__axis" aria-hidden="true">
        {ticks.map((tick, i) => (
          <span key={i} className="chart__tick" style={{ top: `${(yAt(tick, s) / H) * 100}%` }}>
            {format(tick)}
          </span>
        ))}
      </div>
      <div
        className="chart__plot"
        onPointerMove={onMove}
        onPointerDown={onMove}
        onPointerLeave={() => setHover(null)}
        data-testid="chart-plot"
      >
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={label}>
          <defs>
            <linearGradient id={gradient} x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="var(--perf-portfolio)" stopOpacity="0.28" />
              <stop offset="100%" stopColor="var(--perf-portfolio)" stopOpacity="0" />
            </linearGradient>
          </defs>
          {ticks.map((tick, i) => (
            <line key={i} className="chart__grid" x1={0} x2={W} y1={yAt(tick, s)} y2={yAt(tick, s)} />
          ))}
          {zeroLine && <line className="chart__zero" x1={0} x2={W} y1={yAt(0, s)} y2={yAt(0, s)} />}
          {series.map((line) => {
            const d = linePath(line.values, s);
            const area = line.area ? areaPath(line.values, s) : "";
            const lone = count === 1 && line.values[0] !== null && line.values[0] !== undefined;
            return (
              <g key={line.key} data-series={line.key}>
                {area && (
                  <motion.path
                    d={area}
                    fill={`url(#${gradient})`}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ duration: DURATION.slow }}
                  />
                )}
                {d && !lone && (
                  <motion.path
                    d={d}
                    fill="none"
                    stroke={line.color}
                    strokeWidth={2}
                    strokeDasharray={line.dash}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    vectorEffect="non-scaling-stroke"
                    initial={{ pathLength: 0 }}
                    animate={{ pathLength: 1 }}
                    transition={SPRING.value}
                  />
                )}
              </g>
            );
          })}
        </svg>
        {count === 1 &&
          series.map((line) =>
            line.values[0] === null || line.values[0] === undefined ? null : (
              <span
                key={line.key}
                className="chart__dot"
                style={{ left: "50%", top: `${(yAt(line.values[0], s) / H) * 100}%`, background: line.color }}
                aria-hidden="true"
              />
            ),
          )}
        {hover !== null && hoverDate && (
          <>
            <span className="chart__crosshair" style={{ left: `${left}%` }} aria-hidden="true" />
            {series.map((line) => {
              const value = line.values[hover];
              return value === null || value === undefined ? null : (
                <span
                  key={line.key}
                  className="chart__dot"
                  style={{ left: `${left}%`, top: `${(yAt(value, s) / H) * 100}%`, background: line.color }}
                  aria-hidden="true"
                />
              );
            })}
            <div className={`chart__tooltip${left > 60 ? " is-left" : ""}`} style={{ left: `${left}%` }} aria-hidden="true">
              <strong className="chart__tooltip-date">{formatDay(hoverDate, true)}</strong>
              {series.map((line) => {
                const value = line.values[hover];
                return (
                  <span key={line.key} className="chart__tooltip-row">
                    <span className="chart__swatch" style={{ background: line.color }} />
                    <span>{line.label}</span>
                    <strong>{value === null || value === undefined ? "—" : format(value)}</strong>
                  </span>
                );
              })}
            </div>
          </>
        )}
      </div>
      {count > 0 && (
        <div className="chart__dates" aria-hidden="true">
          <span>{formatDay(dates[0] ?? "", spansYears(dates[0], dates[count - 1]))}</span>
          {count > 1 && <span>{formatDay(dates[count - 1] ?? "", spansYears(dates[0], dates[count - 1]))}</span>}
        </div>
      )}
    </div>
  );
}
