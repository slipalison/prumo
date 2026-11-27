import { useState, type ReactNode } from "react";
import { errorMessage } from "../../api/client";
import { useNetWorth, usePerformance } from "../../api/performance";
import type {
  BenchmarkCode,
  NetWorth,
  Performance,
  PerformancePeriod,
  PerformanceYear,
  VersusCdi,
} from "../../api/performanceTypes";
import { CountUp } from "../../components/CountUp";
import { GlassCard } from "../../components/GlassCard";
import { Grid, Page, PageHeader, Panel, StatStrip, type Stat } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { parseSection, useHash } from "../../lib/route";
import { RISK_SECTION, RiskLink, RiskScreen } from "../risk/RiskScreen";
import { LineChart, type ChartSeries } from "./LineChart";
import "./performance.css";
import { formatDay, formatSignedPercent, MONTH_NAMES, MONTHS, spansYears, trend, versusText } from "./performanceFormat";

const PERIODS: Array<{ id: PerformancePeriod; label: string }> = [
  { id: "month", label: "Mês" },
  { id: "ytd", label: "No ano" },
  { id: "12m", label: "12 meses" },
  { id: "all", label: "Desde o início" },
];

/** Cor e tracejado de cada linha: a cor nunca é a única pista (docs/design.md). */
const LINE: Record<"portfolio" | BenchmarkCode, { label: string; color: string; dash?: string }> = {
  portfolio: { label: "Sua carteira", color: "var(--perf-portfolio)" },
  cdi: { label: "CDI", color: "var(--perf-cdi)", dash: "7 5" },
  ipca: { label: "IPCA", color: "var(--perf-ipca)", dash: "2 5" },
  ibov: { label: "Ibovespa", color: "var(--perf-ibov)", dash: "12 5 2 5" },
  selic: { label: "Selic", color: "var(--perf-cdi)", dash: "4 4" },
};

export function PerformanceScreen() {
  return parseSection(useHash()) === RISK_SECTION ? <RiskScreen /> : <PerformanceOverview />;
}

function PerformanceOverview() {
  const [period, setPeriod] = useState<PerformancePeriod>("12m");
  const worth = useNetWorth(period);
  const perf = usePerformance(period);

  return (
    <Page className="performance">
      <PageHeader
        title="Você está ganhando do CDI?"
        info="Rentabilidade pelo método de cotas: o aporte não conta como ganho, só o que o preço e os proventos fizeram."
        infoLabel="Sobre a tela Desempenho"
        summary={<RiskLink />}
        actions={
          <div className="chips" role="group" aria-label="Período">
            {PERIODS.map((p) => (
              <button
                key={p.id}
                type="button"
                className={cx("chip chip--quick", period === p.id && "is-active")}
                aria-pressed={period === p.id}
                onClick={() => setPeriod(p.id)}
              >
                {p.label}
              </button>
            ))}
          </div>
        }
      />

      {(worth.isPending || perf.isPending) && (
        <div className="performance__loading" role="status" aria-label="Carregando o desempenho">
          <Skeleton height={96} radius={22} />
          <Skeleton height={260} radius={22} />
        </div>
      )}
      {(worth.isError || perf.isError) && (
        <GlassCard as="section" className="state-card" role="alert">
          <h2 className="state-card__title">Não consegui carregar o desempenho</h2>
          <p className="state-card__text">{errorMessage(worth.error ?? perf.error)}</p>
          <MagneticButton
            onClick={() => {
              void worth.refetch();
              void perf.refetch();
            }}
          >
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}
      {worth.data && perf.data && <Loaded worth={worth.data} perf={perf.data} />}
    </Page>
  );
}

function Loaded({ worth, perf }: { worth: NetWorth; perf: Performance }) {
  const money = useMoney();
  if (!perf.starts_on) {
    return (
      <GlassCard as="section" className="state-card">
        <h2 className="state-card__title">Ainda não há série</h2>
        <p className="state-card__text">
          A série do desempenho começa no primeiro lançamento da carteira. Cadastre seus ativos na{" "}
          <a href="#/carteira">Carteira</a> ou importe o extrato da B3 em Ajustes: o histórico aparece desde a
          primeira negociação.
        </p>
      </GlassCard>
    );
  }
  const since = perf.summary.since_start;
  const young = !since || since.from === since.to;
  const rebuilding = perf.rebuilding_since ?? worth.rebuilding_since;
  return (
    <>
      {rebuilding && (
        <p className="performance__notice" role="status">
          Refazendo a série desde {formatDay(rebuilding, true)} com os lançamentos novos. Os números se acertam em
          instantes.
        </p>
      )}
      {young && (
        <p className="performance__notice" role="note">
          A série começa em {formatDay(perf.starts_on, true)}. A rentabilidade aparece a partir do próximo fechamento:
          todo dia, às 19h, a carteira ganha uma foto.
        </p>
      )}
      <section aria-label="Resumo do desempenho">
        <StatStrip items={statsOf(perf, worth, money)} />
      </section>
      <Grid cols={2}>
        <NetWorthCard worth={worth} />
        <ReturnCard perf={perf} />
      </Grid>
      <MonthTable years={perf.years} />
    </>
  );
}

/**
 * Os seis números do topo: a rentabilidade (12 meses e desde o início, cada uma
 * com a frase do CDI) e o patrimônio de hoje. Com o olho fechado, todo R$ sai
 * pela máscara e o sinal do ganho também, para não dizer o sentido; os
 * percentuais ficam, como na Carteira.
 */
function statsOf(perf: Performance, worth: NetWorth, money: Money): Stat[] {
  const items: Stat[] = [];
  const twelve = versusStat("twelve", "Últimos 12 meses", perf.summary.twelve_months, true);
  if (twelve) items.push(twelve);
  const since = versusStat("since", "Desde o início", perf.summary.since_start, false);
  if (since) items.push(since);
  const last = worth.points[worth.points.length - 1];
  if (last) {
    items.push(
      { id: "worth", label: "Patrimônio", value: <CountUp value={last.market_value} />, tone: "gold" },
      { id: "invested", label: "Aplicado", value: money.brl(last.invested) },
      {
        id: "gain",
        label: "Ganho",
        value: `${money.hidden ? "" : last.gain > 0 ? "+" : last.gain < 0 ? "−" : ""}${money.brl(Math.abs(last.gain))}`,
        info: "O ganho é o do preço; os proventos recebidos ficam ao lado e entram na rentabilidade.",
        ...(money.hidden || trend(last.gain) === "flat" ? {} : { tone: trend(last.gain) as "up" | "down" }),
      },
      { id: "income", label: "Proventos", value: money.brl(last.income) },
    );
  }
  return items;
}

function versusStat(id: string, label: string, data: VersusCdi | null, twelveMonths: boolean): Stat | null {
  if (!data) return null;
  const started = data.from !== data.to;
  const when = started ? `De ${formatDay(data.from, true)} a ${formatDay(data.to, true)}.` : `Começa em ${formatDay(data.from, true)}.`;
  const short = twelveMonths && started && !data.full_window;
  let detail: ReactNode;
  if (started && data.excess_over_cdi !== null && data.cdi_percent !== null) {
    detail = (
      <>
        {versusText(data.excess_over_cdi)}
        {data.percent_of_cdi !== null && ` (${formatPercent(data.percent_of_cdi, 0)} do CDI)`}
        {short && <span className="stat__note">a série ainda não tem 12 meses</span>}
      </>
    );
  } else {
    detail = started ? "O CDI do período ainda não foi publicado." : "Ainda não há dois fechamentos para comparar.";
  }
  return {
    id,
    label,
    value: started ? formatSignedPercent(data.return_percent) : "—",
    detail,
    info:
      started && data.cdi_percent !== null
        ? `${when} O CDI rendeu ${formatSignedPercent(data.cdi_percent)} no período.`
        : when,
    ...(started && trend(data.return_percent) !== "flat" ? { tone: trend(data.return_percent) as "up" | "down" } : {}),
  };
}

function NetWorthCard({ worth }: { worth: NetWorth }) {
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
      label: "Aplicado",
      color: "var(--text-subtle)",
      dash: "6 5",
      values: worth.points.map((p) => p.invested),
    },
  ];
  return (
    <Panel
      title="Patrimônio no tempo"
      info={
        <>
          O ganho é o do preço; os proventos recebidos ficam ao lado e entram na rentabilidade.
          {last.partial && ` O ponto de hoje (${formatDay(last.date)}) usa o preço do momento; a foto do dia é tirada depois do fechamento.`}
        </>
      }
      infoLabel="Como ler o patrimônio no tempo"
      actions={last.partial ? <span className="perf-card__badge">Hoje é parcial</span> : undefined}
    >
      <Legend items={series.map((s) => ({ key: s.key, label: s.label, color: s.color, dash: s.dash }))} />
      <LineChart
        dates={worth.points.map((p) => p.date)}
        series={series}
        format={money.compact}
        fromZero
        label={`Patrimônio de ${formatDay(first, withYear)} a ${formatDay(last.date, withYear)}: ${money.brl(
          last.market_value,
        )}, com ${money.brl(last.invested)} aplicados.`}
      />
    </Panel>
  );
}

type LegendItem = { key: string; label: string; color: string; dash?: string | undefined; value?: string };

function Swatch({ color, dash }: { color: string; dash?: string | undefined }) {
  return (
    <svg className="legend__swatch" width="26" height="10" viewBox="0 0 26 10" aria-hidden="true">
      <line x1="1" x2="25" y1="5" y2="5" stroke={color} strokeWidth="2.5" strokeDasharray={dash} strokeLinecap="round" />
    </svg>
  );
}

function Legend({ items }: { items: LegendItem[] }) {
  return (
    <ul className="legend">
      {items.map((item) => (
        <li key={item.key} className="legend__item">
          <Swatch color={item.color} dash={item.dash} />
          <span>{item.label}</span>
          {item.value && <strong>{item.value}</strong>}
        </li>
      ))}
    </ul>
  );
}

function ReturnCard({ perf }: { perf: Performance }) {
  const [hidden, setHidden] = useState<ReadonlySet<BenchmarkCode>>(new Set());
  const toggle = (code: BenchmarkCode) =>
    setHidden((current) => {
      const next = new Set(current);
      if (next.has(code)) next.delete(code);
      else next.add(code);
      return next;
    });
  const shown = perf.benchmarks.filter((b) => !hidden.has(b.code));
  const series: ChartSeries[] = [
    { key: "portfolio", ...LINE.portfolio, values: perf.points.map((p) => p.portfolio) },
    ...shown.map((b) => ({
      key: b.code,
      ...LINE[b.code],
      label: b.label,
      values: perf.points.map((p) => p.benchmarks[b.code] ?? null),
    })),
  ];
  const withYear = spansYears(perf.window?.from, perf.window?.to);
  const summary = [
    `carteira ${perf.return_percent === null ? "sem dado" : formatSignedPercent(perf.return_percent)}`,
    ...perf.benchmarks.map(
      (b) => `${b.label} ${b.return_percent === null ? "sem dado" : formatSignedPercent(b.return_percent)}`,
    ),
  ].join(", ");
  const notes = indexNotes(perf);
  return (
    <Panel
      title="Rentabilidade contra os índices"
      info={notes ?? "Cada linha é a rentabilidade acumulada no período, a partir de zero no primeiro fechamento."}
      infoLabel="De quando é cada índice"
    >
      <ul className="legend" aria-label="Linhas do gráfico">
        <li className="legend__item">
          <Swatch color={LINE.portfolio.color} />
          <span>{LINE.portfolio.label}</span>
          <strong>{perf.return_percent === null ? "—" : formatSignedPercent(perf.return_percent)}</strong>
        </li>
        {perf.benchmarks.map((b) => {
          const on = !hidden.has(b.code);
          return (
            <li key={b.code}>
              <button
                type="button"
                className={cx("legend__item legend__toggle", !on && "is-off")}
                aria-pressed={on}
                onClick={() => toggle(b.code)}
              >
                <Swatch color={LINE[b.code].color} dash={LINE[b.code].dash} />
                <span>{b.label}</span>
                <strong>{b.return_percent === null ? "sem dado" : formatSignedPercent(b.return_percent)}</strong>
              </button>
            </li>
          );
        })}
      </ul>
      {perf.points.length > 1 ? (
        <LineChart
          dates={perf.points.map((p) => p.date)}
          series={series}
          format={(v) => formatSignedPercent(v, 1)}
          zeroLine
          label={`Rentabilidade acumulada de ${formatDay(perf.window?.from ?? "", withYear)} a ${formatDay(
            perf.window?.to ?? "",
            withYear,
          )}: ${summary}.`}
        />
      ) : (
        <p className="field__hint">A linha aparece com dois fechamentos no período.</p>
      )}
    </Panel>
  );
}

/** De quando é cada índice: o IPCA sai uma vez por mês, o CDI no dia seguinte. */
function indexNotes(perf: Performance): ReactNode {
  const notes = perf.benchmarks
    .filter((b) => b.as_of && perf.window && b.as_of < perf.window.to)
    .map((b) => `${b.label} até ${formatDay(b.as_of ?? "", true)}`);
  const missing = perf.benchmarks.filter((b) => b.return_percent === null).map((b) => b.label);
  if (notes.length === 0 && missing.length === 0) return null;
  return (
    <>
      {notes.length > 0 && `Índices publicados: ${notes.join(" · ")}. `}
      {missing.length > 0 && `Ainda sem dado no período: ${missing.join(", ")}.`}
    </>
  );
}

function MonthTable({ years }: { years: PerformanceYear[] }) {
  return (
    <Panel title="Mês a mês">
      {years.length === 0 ? (
        <p className="field__hint">A tabela aparece quando um mês tiver dois fechamentos.</p>
      ) : (
        // A tabela é o gráfico em números, para quem lê com leitor de tela. A
        // rolagem lateral (celular) é do próprio navegador: contêiner rolável
        // já recebe foco pelo teclado no Chrome e no Firefox atuais.
        <div className="month-table__scroll">
          <table className="month-table">
            <caption className="sr-only">Rentabilidade da carteira por mês, no ano e o CDI do ano</caption>
            <thead>
              <tr>
                <th scope="col">Ano</th>
                {MONTHS.map((m, i) => (
                  <th key={m} scope="col">
                    <abbr title={MONTH_NAMES[i]}>{m}</abbr>
                  </th>
                ))}
                <th scope="col">No ano</th>
                <th scope="col">CDI</th>
              </tr>
            </thead>
            <tbody>
              {[...years].reverse().map((row) => (
                <tr key={row.year}>
                  <th scope="row">{row.year}</th>
                  {row.months.map((value, i) => (
                    <td key={i} className={cx(value !== null && `is-${trend(value)}`)}>
                      {value === null ? (
                        <>
                          <span aria-hidden="true">—</span>
                          <span className="sr-only">sem dado</span>
                        </>
                      ) : (
                        formatSignedPercent(value)
                      )}
                    </td>
                  ))}
                  <td className={cx("month-table__total", `is-${trend(row.return_percent)}`)}>
                    {formatSignedPercent(row.return_percent)}
                  </td>
                  <td>{row.cdi_percent === null ? "—" : formatSignedPercent(row.cdi_percent)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  );
}

