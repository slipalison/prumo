import { motion } from "motion/react";
import { useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { SPRING } from "../../lib/motion";

export type PayerArc = {
  key: string;
  value: number;
  color: string;
  /** O nome acessível do arco: o pagador, o valor e a parte (já mascarado, se for o caso). */
  label: string;
  /** Marcado como filtro (`aria-pressed`); `undefined` no que não liga e desliga (o grupo "Outros"). */
  pressed?: boolean;
};

type PayerDonutProps = {
  arcs: PayerArc[];
  /** A descrição da rosca inteira, para o leitor de tela. */
  label: string;
  /** A chave do arco aceso (o ponteiro, o foco ou o filtro); os outros escurecem. */
  activeKey: string | null;
  /** O ponteiro ou o foco entrou (`key`) ou saiu (`null`) de um arco. */
  onActive: (key: string | null) => void;
  /** Clique, toque, Enter ou Espaço num arco. */
  onPick: (key: string) => void;
  size?: number;
  thickness?: number;
  /** O que fica no meio. */
  children?: ReactNode;
};

const GAP = 3;
/** Quanto o arco aceso engorda, em pixels. */
const GROW = 4;

/** Para onde a tecla leva o foco: as setas andam em roda, Home e End vão às pontas; outra tecla, a lugar nenhum. */
function nextStop(key: string, index: number, count: number): number | null {
  if (key === "ArrowRight" || key === "ArrowDown") return (index + 1) % count;
  if (key === "ArrowLeft" || key === "ArrowUp") return (index - 1 + count) % count;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}

/**
 * A rosca de "Quem pagou": cada arco é um botão (`role="button"`) que responde
 * ao ponteiro, ao toque e ao teclado. O teclado entra por UM arco (as setas
 * andam de um a outro, Home e End vão às pontas) para a rosca não ser dezenove
 * paradas de Tab; o resto da informação está na tabela ao lado, que repete cada
 * arco com o nome. O arco aceso engorda e escurece os outros: a pista é a forma
 * e o texto do meio (o nome e o valor), nunca só a cor.
 */
export function PayerDonut({ arcs, label, activeKey, onActive, onPick, size = 176, thickness = 14, children }: Readonly<PayerDonutProps>) {
  const radius = (size - thickness - GROW) / 2;
  const circumference = 2 * Math.PI * radius;
  const visible = arcs.filter((a) => a.value > 0);
  const total = visible.reduce((acc, a) => acc + a.value, 0);
  const gap = visible.length > 1 ? GAP : 0;
  const [stop, setStop] = useState(0);
  const nodes = useRef<(SVGCircleElement | null)[]>([]);

  let start = 0;
  const placed = visible.map((arc) => {
    const length = total > 0 ? (arc.value / total) * circumference : 0;
    const placedArc = { arc, dash: Math.max(length - gap, 0.001), offset: -start };
    start += length;
    return placedArc;
  });

  const move = (event: KeyboardEvent<SVGCircleElement>, index: number) => {
    const next = nextStop(event.key, index, visible.length);
    if (next === null) return;
    event.preventDefault();
    setStop(next);
    nodes.current[next]?.focus();
  };

  const pickKey = (event: KeyboardEvent<SVGCircleElement>, key: string) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    onPick(key);
  };

  return (
    <figure
      className="donut payer-donut"
      style={{ width: size, height: size, ["--arc-thickness" as string]: `${thickness}px`, ["--arc-grow" as string]: `${GROW}px` }}
      data-active={activeKey !== null ? "" : undefined}
    >
      <svg // NOSONAR: o grupo dos arcos é o próprio SVG; <fieldset> não entra num SVG
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        role="group"
        aria-label={label}
      >
        <g transform={`rotate(-90 ${size / 2} ${size / 2})`}>
          <circle className="donut__track" cx={size / 2} cy={size / 2} r={radius} strokeWidth={thickness} fill="none" />
          {placed.map(({ arc, dash, offset }, index) => (
            <motion.circle // NOSONAR: o arco da rosca é um <circle> do SVG; <button> não entra num SVG (o teclado e o Enter estão aqui)
              key={arc.key}
              ref={(node: SVGCircleElement | null) => {
                nodes.current[index] = node;
              }}
              data-segment={arc.key}
              data-state={arc.key === activeKey ? "active" : undefined}
              className="payer-donut__arc"
              role="button"
              tabIndex={index === Math.min(stop, visible.length - 1) ? 0 : -1}
              aria-label={arc.label}
              aria-pressed={arc.pressed}
              cx={size / 2}
              cy={size / 2}
              r={radius}
              fill="none"
              stroke={arc.color}
              strokeWidth={thickness}
              strokeLinecap="round"
              initial={{ strokeDasharray: `0 ${circumference}`, strokeDashoffset: offset }}
              animate={{ strokeDasharray: `${dash} ${circumference}`, strokeDashoffset: offset }}
              transition={SPRING.value}
              onPointerEnter={() => onActive(arc.key)}
              onPointerLeave={() => onActive(null)}
              onFocus={() => {
                setStop(index);
                onActive(arc.key);
              }}
              onBlur={() => onActive(null)}
              onClick={() => onPick(arc.key)}
              onKeyDown={(event) => {
                move(event, index);
                pickKey(event, arc.key);
              }}
            />
          ))}
        </g>
      </svg>
      {children && <div className="donut__center">{children}</div>}
    </figure>
  );
}
