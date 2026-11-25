import { motion } from "motion/react";
import { useId } from "react";
import type { IncomeMonth } from "../../api/incomeTypes";
import { useMoney } from "../../lib/privacy";
import { barGeometry, barsSummary, monthLabel, monthShort, niceCeiling } from "../../lib/income";

type MonthlyBarsProps = {
  months: IncomeMonth[];
  /** Meta de renda mensal: uma linha tracejada atravessa o gráfico. */
  goal?: number | null;
};

const WIDTH = 640;
const HEIGHT = 250;
const LEFT = 52;
const RIGHT = 8;
const TOP = 14;
const BOTTOM = 28;
const PLOT_W = WIDTH - LEFT - RIGHT;
const PLOT_H = HEIGHT - TOP - BOTTOM;
const BASE_Y = TOP + PLOT_H;

/**
 * Barras mensais empilhadas: o recebido (cheio) embaixo e o a receber
 * (listrado) em cima. A cor nunca é a única pista: o listrado muda o desenho,
 * a legenda diz o nome e uma tabela escondida traz todos os números para o
 * leitor de tela.
 */
export function MonthlyBars({ months, goal }: MonthlyBarsProps) {
  const patternId = useId();
  const money = useMoney();
  const tallest = Math.max(...months.map((m) => m.received + m.upcoming), goal ?? 0, 0);
  const ceiling = niceCeiling(tallest);
  const bars = barGeometry(months, PLOT_W, PLOT_H, ceiling);
  const ticks = [0, ceiling / 2, ceiling];
  const goalY = goal ? BASE_Y - (goal / ceiling) * PLOT_H : null;

  return (
    <figure className="income-bars">
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={barsSummary(months, money.brl)}
        className="income-bars__svg"
      >
        <defs>
          <pattern id={patternId} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <rect width="6" height="6" className="income-bars__hatch-bg" />
            <rect width="2.4" height="6" className="income-bars__hatch" />
          </pattern>
        </defs>
        {ticks.map((tick) => {
          const y = BASE_Y - (tick / ceiling) * PLOT_H;
          return (
            <g key={tick}>
              <line x1={LEFT} x2={WIDTH - RIGHT} y1={y} y2={y} className="income-bars__grid" />
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
              <title>
                {`${monthLabel(bar.month)}: ${money.brl(month.received)} recebidos${month.upcoming > 0 ? `, ${money.brl(month.upcoming)} a receber` : ""}`}
              </title>
              <motion.rect
                x={x}
                width={bar.width}
                rx={4}
                className="income-bars__received"
                initial={{ height: 0, y: BASE_Y }}
                animate={{ height: bar.receivedHeight, y: BASE_Y - bar.receivedHeight }}
                transition={{ type: "spring", stiffness: 90, damping: 18, delay: index * 0.025 }}
              />
              {bar.upcomingHeight > 0 && (
                <motion.rect
                  x={x}
                  width={bar.width}
                  rx={4}
                  fill={`url(#${patternId})`}
                  className="income-bars__upcoming"
                  initial={{ height: 0, y: BASE_Y }}
                  animate={{
                    height: bar.upcomingHeight,
                    y: BASE_Y - bar.receivedHeight - bar.upcomingHeight,
                  }}
                  transition={{ type: "spring", stiffness: 90, damping: 18, delay: 0.2 + index * 0.025 }}
                />
              )}
              <text x={x + bar.width / 2} y={HEIGHT - 8} textAnchor="middle" className="income-bars__axis">
                {monthShort(bar.month)}
              </text>
            </g>
          );
        })}
        {goalY !== null && goal ? (
          <g>
            <line x1={LEFT} x2={WIDTH - RIGHT} y1={goalY} y2={goalY} className="income-bars__goal" />
            <text x={WIDTH - RIGHT} y={goalY - 6} textAnchor="end" className="income-bars__goal-label">
              {`meta ${money.compact(goal)}`}
            </text>
          </g>
        ) : null}
      </svg>
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
