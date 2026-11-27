import { motion } from "motion/react";
import type { ReactNode } from "react";
import { SPRING } from "../lib/motion";

export type DonutSegment = {
  key: string;
  value: number;
  color: string;
  label: string;
};

type DonutProps = {
  segments: DonutSegment[];
  /** Descrição para leitor de tela ("Carteira por classe: …"). */
  label: string;
  size?: number;
  thickness?: number;
  children?: ReactNode;
};

const GAP = 3;

/**
 * Rosca em SVG. Os arcos se desenham ao entrar e, quando os valores mudam
 * (antes → depois do aporte), cada arco escorrega para a posição nova.
 */
export function Donut({ segments, label, size = 220, thickness = 18, children }: Readonly<DonutProps>) {
  const radius = (size - thickness) / 2;
  const circumference = 2 * Math.PI * radius;
  const visible = segments.filter((s) => s.value > 0);
  const total = visible.reduce((acc, s) => acc + s.value, 0);
  const gap = visible.length > 1 ? GAP : 0;

  let start = 0;
  const arcs = visible.map((segment) => {
    const length = total > 0 ? (segment.value / total) * circumference : 0;
    const arc = { segment, dash: Math.max(length - gap, 0.001), offset: -start };
    start += length;
    return arc;
  });

  return (
    <figure className="donut" style={{ width: size, height: size }}>
      <svg // NOSONAR: SVG em linha e animado, com nome; não vira <img>
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        role="img"
        aria-label={label}
      >
        <g transform={`rotate(-90 ${size / 2} ${size / 2})`}>
          <circle
            className="donut__track"
            cx={size / 2}
            cy={size / 2}
            r={radius}
            strokeWidth={thickness}
            fill="none"
          />
          {arcs.map(({ segment, dash, offset }) => (
            <motion.circle
              key={segment.key}
              data-segment={segment.key}
              cx={size / 2}
              cy={size / 2}
              r={radius}
              fill="none"
              stroke={segment.color}
              strokeWidth={thickness}
              strokeLinecap="round"
              initial={{ strokeDasharray: `0 ${circumference}`, strokeDashoffset: offset }}
              animate={{ strokeDasharray: `${dash} ${circumference}`, strokeDashoffset: offset }}
              transition={SPRING.value}
            />
          ))}
        </g>
      </svg>
      {children && <div className="donut__center">{children}</div>}
    </figure>
  );
}
