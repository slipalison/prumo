import { motion } from "motion/react";
import { useId, useState } from "react";
import type { IncomeMonth } from "../../api/incomeTypes";
import { useMoney } from "../../lib/privacy";
import { barGeometry, barsSummary, monthLabel, monthShort, niceCeiling } from "../../lib/income";
import { DURATION, SPRING, STAGGER } from "../../lib/motion";
import { useWidth } from "./useWidth";
import "./monthlyBars.css";

/** `2025-12` → "dezembro de 2025". */
function monthTitle(month: string): string {
  const text = new Date(`${month}-15T12:00:00Z`).toLocaleDateString("pt-BR", { month: "long", year: "numeric", timeZone: "UTC" });
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A caixinha do mês sob o ponteiro: o recebido e, se houver, o a receber. */
function MonthTooltip({ month, left, top }: Readonly<{ month: IncomeMonth; left: number; top: number }>) {
  const money = useMoney();
  // Perto das bordas a caixa se apoia para dentro, para não sair do painel.
  let side = "";
  if (left < 20) side = " is-start";
  else if (left > 80) side = " is-end";
  return (
    <div className={`income-bars__tooltip${side}`} style={{ left: `${left}%`, top: `${top}%` }} aria-hidden="true">
      <strong className="income-bars__tooltip-title">{monthTitle(month.month)}</strong>
      <span className="income-bars__tooltip-row">
        <span className="income-legend__swatch income-legend__swatch--received" />
        <span>Recebido</span>
        <strong>{money.brl(month.received)}</strong>
      </span>
      {month.upcoming > 0 && (
        <span className="income-bars__tooltip-row">
          <span className="income-legend__swatch income-legend__swatch--upcoming" />
          <span>A receber</span>
          <strong>{money.brl(month.upcoming)}</strong>
        </span>
      )}
    </div>
  );
}

type MonthlyBarsProps = {
  months: IncomeMonth[];
  /** Meta de renda mensal: uma linha tracejada atravessa o gráfico. */
  goal?: number | null;
};

/** Largura de partida (e a de quem não mede, como o teste); depois vale a do painel. */
const FALLBACK_WIDTH = 640;
const HEIGHT = 220;
/** No celular o gráfico é mais baixo: a altura da tela pesa mais que a do desenho. */
const NARROW = 420;
const NARROW_HEIGHT = 176;
const LEFT = 56;
const RIGHT = 8;
const TOP = 14;
const BOTTOM = 26;
/** A barra não passa disto, para não virar um bloco quando o gráfico é largo. */
const MAX_BAR = 48;

/**
 * Barras mensais empilhadas: o recebido (cheio) embaixo e o a receber
 * (listrado) em cima. A cor nunca é a única pista: o listrado muda o desenho,
 * a legenda diz o nome e uma tabela escondida traz todos os números para o
 * leitor de tela.
 */
export function MonthlyBars({ months, goal }: Readonly<MonthlyBarsProps>) {
  const patternId = useId();
  const money = useMoney();
  const { ref, width } = useWidth(FALLBACK_WIDTH);
  const plotW = width - LEFT - RIGHT;
  const height = width < NARROW ? NARROW_HEIGHT : HEIGHT;
  const plotH = height - TOP - BOTTOM;
  const baseY = TOP + plotH;
  const tallest = Math.max(...months.map((m) => m.received + m.upcoming), goal ?? 0, 0);
  const ceiling = niceCeiling(tallest);
  // A coluna inteira do mês responde ao ponteiro (não só a barra, que num mês
  // fraco é um fio); a barra desenhada fica centrada nela.
  const bars = barGeometry(months, plotW, plotH, ceiling).map((bar) => {
    const barWidth = Math.min((plotW / Math.max(months.length, 1)) * 0.62, MAX_BAR);
    return { ...bar, columnX: bar.x, columnWidth: bar.width, x: bar.x + (bar.width - barWidth) / 2, width: barWidth };
  });
  const [hover, setHover] = useState<number | null>(null);
  const hovered = hover === null ? undefined : bars[hover];
  const hoveredMonth = hover === null ? undefined : months[hover];
  const ticks = [0, ceiling / 2, ceiling];
  const goalY = goal ? baseY - (goal / ceiling) * plotH : null;

  return (
    <figure className="income-bars">
      <div ref={ref} className="income-bars__plot">
        <svg // NOSONAR: gráfico em SVG; role="img" com aria-label é o que o nomeia, não há tag nativa equivalente
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={barsSummary(months, money.brl)}
          className="income-bars__svg"
          onPointerLeave={() => setHover(null)}
        >
          <defs>
            <pattern id={patternId} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
              <rect width="6" height="6" className="income-bars__hatch-bg" />
              <rect width="2.4" height="6" className="income-bars__hatch" />
            </pattern>
          </defs>
          {ticks.map((tick) => {
            const y = baseY - (tick / ceiling) * plotH;
            return (
              <g key={tick}>
                <line x1={LEFT} x2={width - RIGHT} y1={y} y2={y} className="income-bars__grid" />
                <text x={LEFT - 8} y={y + 4} textAnchor="end" className="income-bars__axis">
                  {tick === 0 ? "0" : money.compact(tick)}
                </text>
              </g>
            );
          })}
          {bars.map((bar, index) => {
            const month = months[index];
            if (!month) return null;
            const x = LEFT + bar.x;
            return (
              <g key={bar.month}>
                <rect
                  x={LEFT + bar.columnX}
                  y={TOP}
                  width={bar.columnWidth}
                  height={plotH}
                  className="income-bars__hit"
                  data-testid={`income-bars-hit-${bar.month}`}
                  onPointerEnter={() => setHover(index)}
                  onPointerDown={() => setHover(index)}
                />
                <motion.rect
                  x={x}
                  width={bar.width}
                  rx={4}
                  className="income-bars__received"
                  initial={{ height: 0, y: baseY }}
                  animate={{ height: bar.receivedHeight, y: baseY - bar.receivedHeight }}
                  transition={{ ...SPRING.value, delay: index * STAGGER }}
                />
                {bar.upcomingHeight > 0 && (
                  <motion.rect
                    x={x}
                    width={bar.width}
                    rx={4}
                    fill={`url(#${patternId})`}
                    className="income-bars__upcoming"
                    initial={{ height: 0, y: baseY }}
                    animate={{
                      height: bar.upcomingHeight,
                      y: baseY - bar.receivedHeight - bar.upcomingHeight,
                    }}
                    transition={{ ...SPRING.value, delay: DURATION.fast + index * STAGGER }}
                  />
                )}
                <text x={x + bar.width / 2} y={height - 8} textAnchor="middle" className="income-bars__axis">
                  {monthShort(bar.month)}
                </text>
              </g>
            );
          })}
          {goalY !== null && goal ? (
            <g>
              <line x1={LEFT} x2={width - RIGHT} y1={goalY} y2={goalY} className="income-bars__goal" />
              <text x={width - RIGHT} y={goalY - 6} textAnchor="end" className="income-bars__goal-label">
                {`meta ${money.compact(goal)}`}
              </text>
            </g>
          ) : null}
        </svg>
        {hovered && hoveredMonth && (
          <MonthTooltip
            month={hoveredMonth}
            left={((LEFT + hovered.x + hovered.width / 2) / width) * 100}
            top={((baseY - hovered.receivedHeight - hovered.upcomingHeight) / height) * 100}
          />
        )}
      </div>
      <ul className="income-legend" aria-label="Legenda">
        <li>
          <span className="income-legend__swatch income-legend__swatch--received" aria-hidden="true" /> Recebido
        </li>
        <li>
          <span className="income-legend__swatch income-legend__swatch--upcoming" aria-hidden="true" /> A receber
        </li>
        {goal ? (
          <li>
            <span className="income-legend__swatch income-legend__swatch--goal" aria-hidden="true" /> Meta mensal
          </li>
        ) : null}
      </ul>
      <table className="sr-only">
        <caption>Renda líquida por mês, últimos 12 meses</caption>
        <thead>
          <tr>
            <th scope="col">Mês</th>
            <th scope="col">Recebido</th>
            <th scope="col">A receber</th>
          </tr>
        </thead>
        <tbody>
          {months.map((m) => (
            <tr key={m.month}>
              <th scope="row">{monthLabel(m.month)}</th>
              <td>{money.brl(m.received)}</td>
              <td>{money.brl(m.upcoming)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}
