import { useState } from "react";
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
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
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
  const [period, setPeriod] = useState<PerformancePeriod>("12m");
  const worth = useNetWorth(period);
  const perf = usePerformance(period);

  return (
    <div className="screen performance">
      <header className="screen__header">
        <p className="eyebrow">Desempenho</p>
        <h1 className="screen__title">Você está ganhando do CDI?</h1>
        <p className="screen__lead">
          Rentabilidade pelo método de cotas: o aporte não conta como ganho, só o que o preço e os proventos fizeram.
        </p>
      </header>

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

      {(worth.isPending || perf.isPending) && (
        <div className="performance__loading" role="status" aria-label="Carregando o desempenho">
          <Skeleton height={120} radius={22} />
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
    </div>
  );
}

function Loaded({ worth, perf }: { worth: NetWorth; perf: Performance }) {
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
      <div className="performance__cards">
        <VersusCard title="Últimos 12 meses" data={perf.summary.twelve_months} twelveMonths />
        <VersusCard title="Desde o início" data={since} />
      </div>
      <NetWorthCard worth={worth} />
      <ReturnCard perf={perf} />
      <MonthTable years={perf.years} />
    </>
  );
}

function VersusCard({ title, data, twelveMonths = false }: { title: string; data: VersusCdi | null; twelveMonths?: boolean }) {
  if (!data) return null;
  const started = data.from !== data.to;
  return (
    <GlassCard as="section" className="versus-card" aria-label={title}>
      <h2 className="versus-card__title">{title}</h2>
      <p className={cx("versus-card__value", `is-${trend(data.return_percent)}`)}>
        {started ? formatSignedPercent(data.return_percent) : "—"}
      </p>
      {started && data.excess_over_cdi !== null && data.cdi_percent !== null ? (
        <p className="versus-card__text">
          <strong>{versusText(data.excess_over_cdi)}</strong>
          {data.percent_of_cdi !== null && ` (${formatPercent(data.percent_of_cdi, 0)} do CDI)`}. O CDI rendeu{" "}
          {formatSignedPercent(data.cdi_percent)}.
        </p>
      ) : (
        <p className="versus-card__text">
          {started ? "O CDI do período ainda não foi publicado." : "Ainda não há dois fechamentos para comparar."}
        </p>
      )}
      <p className="versus-card__meta">
        {started ? `De ${formatDay(data.from, true)} a ${formatDay(data.to, true)}` : `Começa em ${formatDay(data.from, true)}`}
        {twelveMonths && started && !data.full_window && " · a série ainda não tem 12 meses"}
      </p>
    </GlassCard>
  );
}

/**
 * Com o olho fechado, todo valor em R$ (números, eixo, régua e o resumo do
 * leitor de tela) sai pela máscara de `useMoney()`; o sinal do ganho também
 * some, para não dizer o sentido. Os percentuais ficam, como na Carteira.
 */
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
    <GlassCard as="section" className="perf-card" aria-labelledby="net-worth-title">
      <header className="perf-card__head">
        <h2 id="net-worth-title" className="section-title">
          Patrimônio no tempo
        </h2>
        {last.partial && <span className="perf-card__badge">Hoje é parcial</span>}
      </header>
      <dl className="perf-stats">
        <div>
          <dt>Patrimônio</dt>
          <dd>
            <CountUp value={last.market_value} />
          </dd>
        </div>
        <div>
          <dt>Aplicado</dt>
          <dd>{money.brl(last.invested)}</dd>
        </div>
        <div>
          <dt>Ganho</dt>
          <dd className={cx(!money.hidden && `is-${trend(last.gain)}`)}>
            {!money.hidden && (last.gain > 0 ? "+" : last.gain < 0 ? "−" : "")}
            {money.brl(Math.abs(last.gain))}
          </dd>
        </div>
        <div>
          <dt>Proventos</dt>
          <dd>{money.brl(last.income)}</dd>
        </div>
      </dl>
      <p className="field__hint">
        O ganho é o do preço; os proventos recebidos ficam ao lado e entram na rentabilidade.
      </p>
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
      {last.partial && (
        <p className="field__hint">
          O ponto de hoje ({formatDay(last.date)}) usa o preço do momento; a foto do dia é tirada depois do fechamento.
        </p>
      )}
    </GlassCard>
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
  return (
    <GlassCard as="section" className="perf-card" aria-labelledby="return-title">
      <header className="perf-card__head">
        <h2 id="return-title" className="section-title">
          Rentabilidade contra os índices
        </h2>
      </header>
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
      <IndexNotes perf={perf} />
    </GlassCard>
  );
}

/** De quando é cada índice: o IPCA sai uma vez por mês, o CDI no dia seguinte. */
function IndexNotes({ perf }: { perf: Performance }) {
  const notes = perf.benchmarks
    .filter((b) => b.as_of && perf.window && b.as_of < perf.window.to)
    .map((b) => `${b.label} até ${formatDay(b.as_of ?? "", true)}`);
  const missing = perf.benchmarks.filter((b) => b.return_percent === null).map((b) => b.label);
  if (notes.length === 0 && missing.length === 0) return null;
  return (
    <p className="field__hint">
      {notes.length > 0 && `Índices publicados: ${notes.join(" · ")}. `}
      {missing.length > 0 && `Ainda sem dado no período: ${missing.join(", ")}.`}
    </p>
  );
}

function MonthTable({ years }: { years: PerformanceYear[] }) {
  return (
    <GlassCard as="section" className="perf-card" aria-labelledby="months-title">
      <header className="perf-card__head">
        <h2 id="months-title" className="section-title">
          Mês a mês
        </h2>
      </header>
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
    </GlassCard>
  );
}

