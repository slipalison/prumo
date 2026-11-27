import type { NetWorth, NetWorthBridge } from "../../api/performanceTypes";
import { Panel, StatStrip, type Stat } from "../../components/Layout";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { Legend } from "./Legend";
import { LineChart, type ChartSeries } from "./LineChart";
import { formatDay, signedMoney, spansYears, trend } from "./performanceFormat";

/**
 * Patrimônio × aportado: a curva do patrimônio contra o dinheiro aportado
 * acumulado (os fluxos do livro), e a ponte do período — quanto da variação
 * veio de aporte, quanto do mercado, e os proventos que a carteira pagou.
 * Método em docs/desempenho.md ("Patrimônio × aportado").
 */
export function NetWorthPanel({ worth }: { worth: NetWorth }) {
  const money = useMoney();
  const last = worth.points[worth.points.length - 1];
  if (!last) return null;
  const first = worth.points[0]?.date ?? last.date;
  const withYear = spansYears(first, last.date);
  const series: ChartSeries[] = [
    {
      key: "market_value",
      label: "Patrimônio",
      color: "var(--perf-portfolio)",
      area: true,
      values: worth.points.map((p) => p.market_value),
    },
    {
      key: "invested",
      label: "Aportado",
      color: "var(--text-subtle)",
      dash: "6 5",
      values: worth.points.map((p) => p.invested),
    },
  ];
  return (
    <Panel
      title="Patrimônio × aportado"
      info={
        <>
          A linha tracejada é o dinheiro que você pôs menos o que tirou (compras com taxas, vendas, resgates). A
          distância até o patrimônio é o que o mercado fez. Os proventos saíram da carteira para você: ficam ao
          lado, e o que você reinvestiu entrou como aporte.
          {last.partial && ` O ponto de hoje (${formatDay(last.date)}) usa o preço do momento; a foto do dia é tirada depois do fechamento.`}
        </>
      }
      infoLabel="Como ler patrimônio × aportado"
      actions={last.partial ? <span className="perf-card__badge">Hoje é parcial</span> : undefined}
    >
      {worth.summary && (
        <StatStrip inset className="worth-bridge" items={bridgeStats(worth.summary, money)} />
      )}
      <Legend items={series.map((s) => ({ key: s.key, label: s.label, color: s.color, dash: s.dash }))} />
      <LineChart
        dates={worth.points.map((p) => p.date)}
        series={series}
        format={money.compact}
        fromZero
        label={`Patrimônio de ${formatDay(first, withYear)} a ${formatDay(last.date, withYear)}: ${money.brl(
          last.market_value,
        )}, com ${money.brl(last.invested)} aportados.`}
      />
    </Panel>
  );
}

/**
 * A ponte do período em números: a base (quando o período não começa do zero),
 * os aportes, o mercado e os proventos. Com o olho fechado, valor e sentido somem.
 */
function bridgeStats(bridge: NetWorthBridge, money: Money): Stat[] {
  const tone = (value: number): Pick<Stat, "tone"> =>
    money.hidden || trend(value) === "flat" ? {} : { tone: trend(value) as "up" | "down" };
  const since = bridge.from ? formatDay(bridge.from, true) : null;
  const items: Stat[] = [];
  if (since) {
    items.push({ id: "base", label: "Na base", value: money.brl(bridge.start_value), detail: since });
  }
  items.push(
    {
      id: "contributions",
      label: since ? "Aportes" : "Aportado",
      value: signedMoney(money, bridge.contributions),
      detail: since ? "no período" : "desde o começo",
      info: "Compras com taxas menos vendas, resgates e amortizações.",
    },
    {
      id: "market",
      label: "Mercado",
      value: signedMoney(money, bridge.market),
      detail: since ? "no período" : "desde o começo",
      info: "O que o preço fez com o dinheiro: o patrimônio menos a base e os aportes.",
      ...tone(bridge.market),
    },
    {
      id: "income",
      label: "Proventos",
      value: money.brl(bridge.income),
      detail: since ? "no período" : "desde o começo",
      info: "Dinheiro que a carteira pagou a você. Não está no patrimônio; o que você reinvestiu está nos aportes.",
    },
  );
  return items;
}
