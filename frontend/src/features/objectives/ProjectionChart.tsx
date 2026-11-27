import { motion } from "motion/react";
import { useState, type PointerEvent } from "react";
import type { Curve, CurvePoint } from "../../api/objectivesTypes";
import { formatMonth } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";
import { SPRING } from "../../lib/motion";
import "./projection.css";

export type CurveBasis = "nominal" | "today";

type ProjectionChartProps = {
  curve: Curve;
  /** A meta, no número que a pessoa digitou. */
  target: number;
  /** Em reais da época ou de hoje: cada leitura tem a própria curva. */
  basis: CurveBasis;
  /** Meta de renda: os valores são por mês. */
  perMonth: boolean;
  /** O nome do objetivo, para o resumo do leitor de tela. */
  name: string;
};

const W = 640;
const H = 160;
const PAD_Y = 10;

type Values = { low: number[]; base: number[]; high: number[] };

function valuesOf(points: CurvePoint[], basis: CurveBasis): Values {
  return basis === "nominal"
    ? { low: points.map((p) => p.low), base: points.map((p) => p.base), high: points.map((p) => p.high) }
    : { low: points.map((p) => p.low_today), base: points.map((p) => p.base_today), high: points.map((p) => p.high_today) };
}

const xAt = (index: number, count: number) => (count <= 1 ? W / 2 : (index / (count - 1)) * W);

function scaleMax(values: Values, target: number): number {
  const top = Math.max(...values.high, target, 0);
  return top > 0 ? top * 1.08 : 1;
}

const yAt = (value: number, max: number) => PAD_Y + (1 - Math.max(value, 0) / max) * (H - 2 * PAD_Y);

function line(values: number[], max: number): string {
  return values
    .map((v, i) => `${i === 0 ? "M" : "L"}${xAt(i, values.length).toFixed(2)} ${yAt(v, max).toFixed(2)}`)
    .join(" ");
}

/** A faixa: o otimista da esquerda para a direita, o pessimista de volta. */
function band(values: Values, max: number): string {
  const n = values.high.length;
  if (n < 2) return "";
  const top = values.high.map((v, i) => `${i === 0 ? "M" : "L"}${xAt(i, n).toFixed(2)} ${yAt(v, max).toFixed(2)}`);
  const bottom = values.low
    .map((v, i) => ({ v, i }))
    .reverse()
    .map(({ v, i }) => `L${xAt(i, n).toFixed(2)} ${yAt(v, max).toFixed(2)}`);
  return `${top.join(" ")} ${bottom.join(" ")} Z`;
}

/** Os pontos de ano em ano (e o último), para a tabela do leitor de tela. */
function yearly(points: CurvePoint[]): number[] {
  const indexes = points.map((p, i) => ({ p, i })).filter(({ p }) => p.months % 12 === 0).map(({ i }) => i);
  const last = points.length - 1;
  if (last >= 0 && !indexes.includes(last)) indexes.push(last);
  return indexes;
}

/**
 * A curva projetada do objetivo: a linha do cenário base, a faixa entre o
 * pessimista e o otimista e a linha tracejada da meta. Passar o ponteiro
 * mostra o mês e os três cenários. Quem lê a tela tem o resumo e uma tabela
 * de ano em ano; todo valor passa pelo "ocultar valores".
 */
export function ProjectionChart({ curve, target, basis, perMonth, name }: Readonly<ProjectionChartProps>) {
  const money = useMoney();
  const [hover, setHover] = useState<number | null>(null);
  const points = curve.points;
  const values = valuesOf(points, basis);
  const max = scaleMax(values, target);
  const count = points.length;
  const suffix = perMonth ? " por mês" : "";
  const fmt = (v: number) => `${money.compact(v)}${suffix}`;
  const targetY = yAt(target, max);
  const first = points[0];
  const last = points[count - 1];

  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || count === 0) return;
    const ratio = Math.min(Math.max((event.clientX - rect.left) / rect.width, 0), 1);
    setHover(Math.round(ratio * (count - 1)));
  };

  const hovered = hover === null ? undefined : points[hover];
  const left = hover === null ? 0 : (xAt(hover, count) / W) * 100;
  const readingLabel = basis === "nominal" ? "em reais da época" : "em reais de hoje";
  const summary = first && last
    ? `${name}, ${readingLabel}: no cenário base, de ${fmt(values.base[0] ?? 0)} hoje a ${fmt(values.base[count - 1] ?? 0)} em ${formatMonth(last.month)}, entre ${fmt(values.low[count - 1] ?? 0)} e ${fmt(values.high[count - 1] ?? 0)} nos outros cenários. Meta: ${fmt(target)}.`
    : `${name}: sem curva.`;

  return (
    <figure className="proj-chart">
      <div className="proj-chart__grid">
        <div className="proj-chart__axis" aria-hidden="true">
          <span className="proj-chart__tick" style={{ top: `${(yAt(max / 1.08, max) / H) * 100}%` }}>
            {money.compact(max / 1.08)}
          </span>
          <span className="proj-chart__tick" style={{ top: `${(yAt(0, max) / H) * 100}%` }}>
            {money.compact(0)}
          </span>
        </div>
        <div
          className="proj-chart__plot"
          onPointerMove={onMove}
          onPointerDown={onMove}
          onPointerLeave={() => setHover(null)}
          data-testid="projection-plot"
        >
          <svg // NOSONAR: gráfico em SVG não vira <img>; role="img" com o aria-label dá o resumo ao leitor de tela.
            viewBox={`0 0 ${W} ${H}`}
            preserveAspectRatio="none"
            role="img"
            aria-label={summary}
          >
            <line className="proj-chart__baseline" x1={0} x2={W} y1={yAt(0, max)} y2={yAt(0, max)} />
            <motion.path
              d={band(values, max)}
              className="proj-chart__band"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={SPRING.value}
            />
            <line className="proj-chart__target" x1={0} x2={W} y1={targetY} y2={targetY} />
            <motion.path
              key={basis}
              d={line(values.base, max)}
              className="proj-chart__base"
              initial={{ pathLength: 0 }}
              animate={{ pathLength: 1 }}
              transition={SPRING.value}
            />
          </svg>
          <span className="proj-chart__target-label" style={{ top: `${(targetY / H) * 100}%` }} aria-hidden="true">
            Meta
          </span>
          {hovered && hover !== null && (
            <>
              <span className="proj-chart__crosshair" style={{ left: `${left}%` }} aria-hidden="true" />
              <span
                className="proj-chart__dot"
                style={{ left: `${left}%`, top: `${(yAt(values.base[hover] ?? 0, max) / H) * 100}%` }}
                aria-hidden="true"
              />
              <div className={`proj-chart__tooltip${left > 55 ? " is-left" : ""}`} style={{ left: `${left}%` }} aria-hidden="true">
                <strong>{formatMonth(hovered.month)}</strong>
                <span>
                  Otimista <b>{fmt(values.high[hover] ?? 0)}</b>
                </span>
                <span>
                  Base <b>{fmt(values.base[hover] ?? 0)}</b>
                </span>
                <span>
                  Pessimista <b>{fmt(values.low[hover] ?? 0)}</b>
                </span>
              </div>
            </>
          )}
        </div>
        {first && last && (
          <div className="proj-chart__dates" aria-hidden="true">
            <span>hoje</span>
            <span>{formatMonth(last.month)}</span>
          </div>
        )}
      </div>
      <figcaption className="proj-chart__legend">
        <span className="proj-chart__key proj-chart__key--base">Cenário base</span>
        <span className="proj-chart__key proj-chart__key--band">Pessimista a otimista</span>
        <span className="proj-chart__key proj-chart__key--target">Meta</span>
      </figcaption>
      <div className="sr-only">
        <table>
          <caption>{`${name}: projeção ${readingLabel}, de ano em ano`}</caption>
          <thead>
            <tr>
              <th scope="col">Mês</th>
              <th scope="col">Pessimista</th>
              <th scope="col">Base</th>
              <th scope="col">Otimista</th>
            </tr>
          </thead>
          <tbody>
            {yearly(points).map((i) => (
              <tr key={points[i]?.month ?? i}>
                <th scope="row">{formatMonth(points[i]?.month ?? null)}</th>
                <td>{fmt(values.low[i] ?? 0)}</td>
                <td>{fmt(values.base[i] ?? 0)}</td>
                <td>{fmt(values.high[i] ?? 0)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </figure>
  );
}
