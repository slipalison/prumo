import { motion } from "motion/react";
import { useId } from "react";
import type { IncomeMonth } from "../../api/incomeTypes";
import { useMoney } from "../../lib/privacy";
import { barGeometry, barsSummary, monthLabel, monthShort, niceCeiling } from "../../lib/income";
import { DURATION, SPRING, STAGGER } from "../../lib/motion";
import { useWidth } from "./useWidth";

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
export function MonthlyBars({ months, goal }: MonthlyBarsProps) {
  const patternId = useId();
  const money = useMoney();
  const { ref, width } = useWidth(FALLBACK_WIDTH);
  const plotW = width - LEFT - RIGHT;
  const height = width < NARROW ? NARROW_HEIGHT : HEIGHT;
  const plotH = height - TOP - BOTTOM;
  const baseY = TOP + plotH;
  const tallest = Math.max(...months.map((m) => m.received + m.upcoming), goal ?? 0, 0);
  const ceiling = niceCeiling(tallest);
  const bars = barGeometry(months, plotW, plotH, ceiling).map((bar) => {
    const barWidth = Math.min((plotW / Math.max(months.length, 1)) * 0.62, MAX_BAR);
    return { ...bar, x: bar.x + (bar.width - barWidth) / 2, width: barWidth };
  });
  const ticks = [0, ceiling / 2, ceiling];
  const goalY = goal ? baseY - (goal / ceiling) * plotH : null;

  return (
    <figure className="income-bars">
      <div ref={ref} className="income-bars__plot">
        <svg
          viewBox={`0 0 ${width} ${height}`}
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
                <title>
                  {`${monthLabel(bar.month)}: ${money.brl(month.received)} recebidos${month.upcoming > 0 ? `, ${money.brl(month.upcoming)} a receber` : ""}`}
                </title>
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
