import { useId, useMemo, useState } from "react";
import { errorMessage } from "../../api/client";
import { useIncomeHeatmap, useIncomeHistory } from "../../api/incomeHistoryQueries";
import {
  HISTORY_GROUPINGS,
  HISTORY_INTERVALS,
  type AmountKind,
  type DateBasis,
  type HistoryGrouping,
  type HistoryInterval,
  type IncomeHistory as History,
} from "../../api/incomeHistoryTypes";
import { Panel } from "../../components/Layout";
import { Skeleton } from "../../components/Skeleton";
import {
  AMOUNT_LABEL,
  BASIS_LABEL,
  bucketsInPeriod,
  GROUPING_LABEL,
  INTERVAL_LABEL,
  PERIODS,
  seriesOf,
  seriesTotals,
  type HeatMetric,
  type PeriodId,
} from "../../lib/incomeHistory";
import { useMoney } from "../../lib/privacy";
import { HistoryBars } from "./HistoryBars";
import { IncomeHeatmap } from "./IncomeHeatmap";
import { Segmented } from "./Segmented";

type View = "bars" | "heatmap";

const VIEWS = [
  { id: "bars", label: "Barras" },
  { id: "heatmap", label: "Mapa de calor" },
] as const;

const BASES = [
  { id: "payment", label: BASIS_LABEL.payment },
  { id: "record", label: BASIS_LABEL.record },
] as const;

const AMOUNTS = [
  { id: "net", label: AMOUNT_LABEL.net },
  { id: "gross", label: AMOUNT_LABEL.gross },
] as const;

const METRICS = [
  { id: "amount", label: "R$" },
  { id: "dy", label: "DY" },
  {
    id: "yoc",
    label: (
      <>
        YoC<span className="sr-only"> (yield on cost)</span>
      </>
    ),
  },
] as const;

const INTERVAL_IN_CAPTION: Record<HistoryInterval, string> = {
  month: "mês",
  quarter: "trimestre",
  half_year: "semestre",
  year: "ano",
};

/** "pela data de pagamento, líquidos": o que as escolhas de cima dizem, para as legendas. */
function contextOf(basis: DateBasis, value: AmountKind): string {
  return `pela ${basis === "payment" ? "data de pagamento" : "data com"}, ${value === "net" ? "líquidos" : "brutos"}`;
}

type SelectProps<T extends string> = {
  label: string;
  value: T;
  options: readonly T[];
  names: Record<T, string>;
  onChange: (value: T) => void;
};

function Select<T extends string>({ label, value, options, names, onChange }: SelectProps<T>) {
  const id = useId();
  return (
    <span className="income-history__field">
      <label htmlFor={id} className="income-history__label">
        {label}
      </label>
      <select id={id} className="income-history__select" value={value} onChange={(event) => onChange(event.target.value as T)}>
        {options.map((option) => (
          <option key={option} value={option}>
            {names[option]}
          </option>
        ))}
      </select>
    </span>
  );
}

const PERIOD_NAMES = Object.fromEntries(PERIODS.map((p) => [p.id, p.label])) as Record<PeriodId, string>;
const PERIOD_IDS = PERIODS.map((p) => p.id);

/**
 * O histórico de proventos para explorar: barras por mês, trimestre, semestre
 * ou ano, pela data de pagamento ou pela data com, por classe, segmento ou
 * ativo, líquido ou bruto; e o mapa de calor mês × ano em R$, DY ou yield on
 * cost. A data e o valor valem para as duas vistas.
 */
export function IncomeHistory() {
  const [view, setView] = useState<View>("bars");
  const [basis, setBasis] = useState<DateBasis>("payment");
  const [value, setValue] = useState<AmountKind>("net");
  const [interval, chooseInterval] = useState<HistoryInterval>("month");
  const [group, setGroup] = useState<HistoryGrouping>("total");
  const [period, setPeriod] = useState<PeriodId>("24m");
  const [metric, setMetric] = useState<HeatMetric>("amount");
  const history = useIncomeHistory({ interval, basis, group, value });
  const heatmap = useIncomeHeatmap({ basis, value }, view === "heatmap");
  const context = contextOf(basis, value);

  return (
    <Panel
      title="Histórico"
      className="income-history"
      info={
        <>
          Pela data de pagamento, o provento entra no mês em que caiu; pela data com, no mês em que o direito foi garantido (o último pregão antes da
          data ex; sem data ex, vale o pagamento). O líquido é o que caiu na conta: sem o IR do JCP (15% até 2025, 17,5% desde 2026) e os 30% retidos
          nos EUA; o bruto é antes disso. No mapa, o DY é o provento do mês ÷ o patrimônio médio da carteira no mês, e o yield on cost, o provento do mês
          ÷ o custo médio (o que você pagou).
        </>
      }
      infoLabel="Como ler o histórico"
      actions={<Segmented label="Vista do histórico" options={VIEWS} value={view} onChange={setView} />}
    >
      <div className="income-history__controls">
        <Segmented label="Data do provento" options={BASES} value={basis} onChange={setBasis} />
        <Segmented label="Valor" options={AMOUNTS} value={value} onChange={setValue} />
        {view === "bars" ? (
          <>
            <Select label="Intervalo" value={interval} options={HISTORY_INTERVALS} names={INTERVAL_LABEL} onChange={chooseInterval} />
            <Select label="Agrupar por" value={group} options={HISTORY_GROUPINGS} names={GROUPING_LABEL} onChange={setGroup} />
            <Select label="Período" value={period} options={PERIOD_IDS} names={PERIOD_NAMES} onChange={setPeriod} />
          </>
        ) : (
          <Segmented label="Métrica do mapa" options={METRICS} value={metric} onChange={setMetric} />
        )}
      </div>

      {view === "bars" && (
        <>
          {history.isPending && <Skeleton height={220} radius={16} />}
          {history.isError && <p className="field__hint">Não consegui carregar o histórico: {errorMessage(history.error)}</p>}
          {history.data && <HistoryView history={history.data} period={period} context={context} />}
        </>
      )}
      {view === "heatmap" && (
        <>
          {heatmap.isPending && <Skeleton height={220} radius={16} />}
          {heatmap.isError && <p className="field__hint">Não consegui carregar o mapa de calor: {errorMessage(heatmap.error)}</p>}
          {heatmap.data &&
            (heatmap.data.years.length > 0 ? (
              <IncomeHeatmap years={heatmap.data.years} metric={metric} context={context} />
            ) : (
              <p className="field__hint">Nenhum provento lançado ainda.</p>
            ))}
        </>
      )}
    </Panel>
  );
}

function HistoryView({ history, period, context }: { history: History; period: PeriodId; context: string }) {
  const money = useMoney();
  const visible = useMemo(
    () => bucketsInPeriod(history.buckets, history.interval, period, history.today),
    [history.buckets, history.interval, period, history.today],
  );
  const series = useMemo(() => seriesOf(history.groups, visible), [history.groups, visible]);
  const totals = useMemo(() => seriesTotals(visible, series), [visible, series]);
  const hasUpcoming = visible.some((b) => b.upcoming > 0);
  const sum = visible.reduce((acc, b) => acc + b.received + b.upcoming, 0);

  if (sum <= 0) {
    return <p className="field__hint">Nenhum provento nesse período.</p>;
  }
  const caption = `Proventos por ${INTERVAL_IN_CAPTION[history.interval]}, ${context}, ${PERIOD_NAMES[period].toLowerCase()}`;
  const grouped = history.group !== "total";
  return (
    <>
      <HistoryBars buckets={visible} series={series} interval={history.interval} caption={caption} />
      <ul className="income-legend history-legend" aria-label="Legenda">
        {series.map((s) => (
          <li key={s.key}>
            <span className="income-legend__swatch" style={{ background: s.color }} aria-hidden="true" />
            {grouped ? s.label : "Recebido"}
            {grouped && (
              <>
                {" "}
                <strong className="history-legend__value">{money.brl(totals.get(s.key) ?? 0)}</strong>
              </>
            )}
          </li>
        ))}
        {hasUpcoming && (
          <li>
            <span className="income-legend__swatch income-legend__swatch--upcoming" aria-hidden="true" /> A receber
          </li>
        )}
        <li className="history-legend__total">
          Total <strong className="history-legend__value">{money.brl(sum)}</strong>
        </li>
      </ul>
    </>
  );
}
