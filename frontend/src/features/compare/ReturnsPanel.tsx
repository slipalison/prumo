import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useCompareReturns } from "../../api/compare";
import type { CompareCurve, CompareIndex, ComparePeriod, CompareReturns } from "../../api/compareTypes";
import { InfoTip } from "../../components/InfoTip";
import { Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { LineChart, type ChartSeries } from "../performance/LineChart";
import { formatDay, formatSignedPercent, spansYears } from "../performance/performanceFormat";
import { CURVE_REASON_TEXT } from "./compareText";

const PERIODS: Array<{ id: ComparePeriod; label: string }> = [
  { id: "1m", label: "1 mês" },
  { id: "3m", label: "3 meses" },
  { id: "6m", label: "6 meses" },
  { id: "ytd", label: "No ano" },
  { id: "12m", label: "12 meses" },
  { id: "3y", label: "3 anos" },
  { id: "5y", label: "5 anos" },
  { id: "max", label: "Máximo" },
];

const INDEXES: Array<{ id: CompareIndex; label: string }> = [
  { id: "cdi", label: "CDI" },
  { id: "ibov", label: "Ibovespa" },
];

/** A cor e o tracejado de cada ativo, na ordem da escolha: a cor nunca é a única pista. */
const ASSET_LINES: ReadonlyArray<{ color: string; dash?: string }> = [
  { color: "var(--cmp-1)" },
  { color: "var(--cmp-2)", dash: "9 3" },
  { color: "var(--cmp-3)", dash: "4 2" },
  { color: "var(--cmp-4)", dash: "1.5 2.5" },
  { color: "var(--cmp-5)", dash: "16 3" },
];

const OTHER_LINES: Record<string, { color: string; dash?: string }> = {
  portfolio: { color: "var(--perf-portfolio)" },
  cdi: { color: "var(--perf-cdi)", dash: "7 5" },
  ibov: { color: "var(--perf-ibov)", dash: "12 5 2 5" },
};

function lineOf(curve: CompareCurve, assetIndex: number): { color: string; dash?: string } {
  if (curve.kind === "asset") return ASSET_LINES[assetIndex % ASSET_LINES.length] ?? { color: "var(--text-muted)" };
  return OTHER_LINES[curve.key] ?? { color: "var(--text-muted)" };
}

function Swatch({ color, dash }: { color: string; dash?: string | undefined }) {
  return (
    <svg className="legend__swatch" width="26" height="10" viewBox="0 0 26 10" aria-hidden="true">
      <line x1="1" x2="25" y1="5" y2="5" stroke={color} strokeWidth="2.5" strokeDasharray={dash} strokeLinecap="round" />
    </svg>
  );
}

type Chosen = { period: ComparePeriod; totalReturn: boolean; alignStart: boolean; portfolio: boolean; benchmarks: CompareIndex[] };

type ReturnsPanelProps = { keys: readonly string[] };

/**
 * A rentabilidade comparada: cada linha parte de 0% no começo do período.
 * Ativos, a própria carteira e os índices; com ou sem proventos; com o início
 * igualado (desde a data em que todos existem) ou cada um do seu começo.
 */
export function ReturnsPanel({ keys }: ReturnsPanelProps) {
  const [chosen, setChosen] = useState<Chosen>({
    period: "12m",
    totalReturn: false,
    alignStart: false,
    portfolio: true,
    benchmarks: ["cdi", "ibov"],
  });
  const query = useCompareReturns(keys, {
    period: chosen.period,
    totalReturn: chosen.totalReturn,
    alignStart: chosen.alignStart,
    portfolio: chosen.portfolio,
    benchmarks: chosen.benchmarks,
  });
  const set = (patch: Partial<Chosen>) => setChosen((current) => ({ ...current, ...patch }));
  const toggleIndex = (id: CompareIndex) =>
    set({ benchmarks: chosen.benchmarks.includes(id) ? chosen.benchmarks.filter((b) => b !== id) : [...chosen.benchmarks, id] });

  return (
    <Panel
      title="Rentabilidade comparada"
      info={
        <>
          Cada linha é o retorno acumulado desde o começo do período, a partir de 0%. Ativo em dólar ou cripto vai para reais pelo câmbio de cada dia. “Com
          proventos” reinveste o provento bruto na data ex (retorno total); “Igualar o início” parte da data em que todos os ativos existem.
        </>
      }
      infoLabel="Como ler a rentabilidade comparada"
    >
      <div className="compare-controls">
        <div className="chips" role="group" aria-label="Período">
          {PERIODS.map((p) => (
            <button
              key={p.id}
              type="button"
              className={cx("chip chip--quick", chosen.period === p.id && "is-active")}
              aria-pressed={chosen.period === p.id}
              onClick={() => set({ period: p.id })}
            >
              {p.label}
            </button>
          ))}
        </div>
        <div className="compare-controls__row">
          <label className="switch-row compare-switch">
            <span className="switch-row__label">Com proventos</span>
            <input
              type="checkbox"
              role="switch"
              className="switch"
              checked={chosen.totalReturn}
              onChange={(event) => set({ totalReturn: event.target.checked })}
            />
          </label>
          <label className="switch-row compare-switch">
            <span className="switch-row__label">Igualar o início</span>
            <input
              type="checkbox"
              role="switch"
              className="switch"
              checked={chosen.alignStart}
              onChange={(event) => set({ alignStart: event.target.checked })}
            />
          </label>
          <div className="chips" role="group" aria-label="Comparar também com">
            <button
              type="button"
              className={cx("chip", chosen.portfolio && "is-active")}
              aria-pressed={chosen.portfolio}
              onClick={() => set({ portfolio: !chosen.portfolio })}
            >
              Sua carteira
            </button>
            {INDEXES.map((index) => (
              <button
                key={index.id}
                type="button"
                className={cx("chip", chosen.benchmarks.includes(index.id) && "is-active")}
                aria-pressed={chosen.benchmarks.includes(index.id)}
                onClick={() => toggleIndex(index.id)}
              >
                {index.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {query.isPending && <Skeleton height={260} />}
      {query.isError && !query.data && (
        <div role="alert" className="compare__error">
          <p>{errorMessage(query.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void query.refetch()}>
            Tentar de novo
          </MagneticButton>
        </div>
      )}
      {query.data && <Curves data={query.data} refreshing={query.isFetching} />}
    </Panel>
  );
}

function Curves({ data, refreshing }: { data: CompareReturns; refreshing: boolean }) {
  const withYear = spansYears(data.from, data.to);
  let assetIndex = -1;
  const lines = data.series.map((curve) => {
    if (curve.kind === "asset") assetIndex += 1;
    return { curve, ...lineOf(curve, assetIndex) };
  });
  const drawn = lines.filter(({ curve }) => curve.reason === null && curve.starts_on !== null);
  const chartSeries: ChartSeries[] = drawn.map(({ curve, color, dash }) => ({
    key: curve.key,
    label: curve.name,
    color,
    dash,
    values: curve.values,
  }));
  const summary = drawn
    .map(({ curve }) => `${curve.name} ${curve.return_percent === null ? "sem dado" : formatSignedPercent(curve.return_percent)}`)
    .join(", ");
  const converted = data.series.some((curve) => curve.converted_to_brl);
  const portfolioOn = data.series.some((curve) => curve.kind === "portfolio" && curve.reason === null);
  const noIncome = data.total_return
    ? data.series.filter((curve) => curve.kind === "asset" && curve.reason === null && !curve.includes_income).map((curve) => curve.name)
    : [];

  return (
    <div className={cx("compare-curves", refreshing && "is-refreshing")}>
      <ul className="legend" aria-label="Linhas do gráfico">
        {lines.map(({ curve, color, dash }) => {
          const late = curve.starts_on !== null && curve.starts_on > data.from;
          return (
            <li key={curve.key} className="legend__item compare-legend__item">
              <Swatch color={color} dash={dash} />
              <span>{curve.name}</span>
              {curve.reason ? (
                <span className="field__hint">{CURVE_REASON_TEXT[curve.reason]}</span>
              ) : (
                <>
                  <strong>{curve.return_percent === null ? "—" : formatSignedPercent(curve.return_percent)}</strong>
                  {late && curve.starts_on && <span className="field__hint">começa em {formatDay(curve.starts_on, true)}</span>}
                </>
              )}
            </li>
          );
        })}
      </ul>
      {data.dates.length > 1 && chartSeries.length > 0 ? (
        <LineChart
          dates={data.dates}
          series={chartSeries}
          format={(value) => formatSignedPercent(value, 1)}
          zeroLine
          label={`Rentabilidade de ${formatDay(data.from, withYear)} a ${formatDay(data.to, withYear)}: ${summary}.`}
        />
      ) : (
        <p className="data-table__empty">Ainda não há dois fechamentos para desenhar a comparação.</p>
      )}
      <p className="field__hint compare-curves__range">
        De {formatDay(data.from, true)} a {formatDay(data.to, true)}
        {data.align_start && ", com o início igualado"}
        {data.total_return ? ", com proventos brutos reinvestidos na data ex" : ", só o preço dos ativos"}.
      </p>
      {converted && <p className="field__hint">Ativos em dólar e cripto estão em reais, pelo câmbio de cada dia.</p>}
      {portfolioOn && !data.total_return && (
        <p className="field__hint">A carteira já inclui os proventos que você recebeu: é a cota do Desempenho.</p>
      )}
      {noIncome.length > 0 && (
        <p className="field__hint">Sem histórico de proventos na fonte (contam só o preço): {noIncome.join(", ")}.</p>
      )}
      <InfoTip label="O que esta comparação não diz" align="start">
        Retorno passado de cada linha, nada além disso: não prevê o que vem depois nem diz qual ativo escolher.
      </InfoTip>
    </div>
  );
}
