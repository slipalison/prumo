import { motion } from "motion/react";
import { useId, useState } from "react";
import type { BenchmarkCode, Performance, PerformanceYear } from "../../api/performanceTypes";
import { Panel, StatStrip, type Stat } from "../../components/Layout";
import { cx } from "../../lib/cx";
import { formatDay, formatSignedPercent, MONTH_NAMES, MONTHS, monthLabel, trend } from "./performanceFormat";

type Mode = "year" | "accumulated";

const MODES: Array<{ id: Mode; label: string }> = [
  { id: "year", label: "Ano" },
  { id: "accumulated", label: "Acumulado" },
];

/**
 * Ano a ano: a tabela mês × ano em dois modos — "Ano" (cada mês por si) e
 * "Acumulado" (desde o começo da série até o fim de cada mês) —, uma linha do
 * índice escolhido sob cada ano, nas mesmas janelas, e o resumo dos meses:
 * melhor, pior, quantos subiram e caíram e em quantos a carteira ficou acima do
 * índice. Método em docs/desempenho.md ("Ano a ano").
 */
export function YearByYearPanel({ perf }: { perf: Performance }) {
  const [mode, setMode] = useState<Mode>("year");
  const available = perf.benchmarks.map((b) => ({ code: b.code, label: b.label }));
  const [chosen, setChosen] = useState<BenchmarkCode | "none">("cdi");
  const compare = available.find((b) => b.code === chosen) ?? null;
  const selectId = useId();
  const knob = useId();
  return (
    <Panel
      className="year-panel"
      title="Ano a ano"
      info="Cada mês mede do último fechamento do mês anterior ao último do mês; o mês corrente vai até hoje. O índice da linha de baixo é medido nas mesmas janelas da carteira."
      infoLabel="Como se mede cada mês"
      actions={
        <div className="year-table__controls">
          <div className="segmented" role="group" aria-label="Modo da tabela">
            {MODES.map((option) => (
              <button
                key={option.id}
                type="button"
                className={cx("segmented__option", mode === option.id && "is-active")}
                aria-pressed={mode === option.id}
                onClick={() => setMode(option.id)}
              >
                {mode === option.id && <motion.span layoutId={knob} className="segmented__knob" />}
                <span className="segmented__label">{option.label}</span>
              </button>
            ))}
          </div>
          <label className="year-table__compare" htmlFor={selectId}>
            <span>Comparar com</span>
            <select
              id={selectId}
              className="text-input"
              value={compare?.code ?? "none"}
              onChange={(event) => setChosen(event.target.value as BenchmarkCode | "none")}
            >
              <option value="none">Nenhum</option>
              {available.map((b) => (
                <option key={b.code} value={b.code}>
                  {b.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      }
    >
      {perf.years.length === 0 ? (
        <p className="field__hint">A tabela aparece quando um mês tiver dois fechamentos.</p>
      ) : (
        <>
          <StatStrip inset className="year-stats" items={monthStats(perf, compare?.code ?? null)} />
          <YearTable perf={perf} mode={mode} compare={compare} />
        </>
      )}
    </Panel>
  );
}

function monthStats(perf: Performance, compare: BenchmarkCode | null): Stat[] {
  const summary = perf.months_summary;
  const total = summary.positive + summary.negative + summary.flat;
  const items: Stat[] = [];
  if (summary.best) {
    items.push({
      id: "best",
      label: "Melhor mês",
      value: formatSignedPercent(summary.best.return_percent),
      detail: monthLabel(summary.best.month, summary.best.year),
    });
  }
  if (summary.worst) {
    items.push({
      id: "worst",
      label: "Pior mês",
      value: formatSignedPercent(summary.worst.return_percent),
      detail: monthLabel(summary.worst.month, summary.worst.year),
    });
  }
  items.push({
    id: "positive",
    label: "Meses positivos",
    value: `${summary.positive} de ${total}`,
    detail: `${summary.negative} ${summary.negative === 1 ? "negativo" : "negativos"}${summary.flat > 0 ? `, ${summary.flat} no zero` : ""}`,
  });
  const versus = summary.versus.find((v) => v.code === compare);
  if (versus) {
    items.push({
      id: "versus",
      label: `Acima do ${versus.label}`,
      value: versus.compared > 0 ? `${versus.above} de ${versus.compared}` : "—",
      detail: versus.compared > 0 ? "meses com dado do índice" : "sem dado do índice",
      info: "Meses em que a carteira rendeu mais que o índice na mesma janela. Mês em que o índice ainda não foi publicado não conta.",
    });
  }
  return items;
}

type Compare = { code: BenchmarkCode; label: string } | null;

/** Mês a mês com duas casas; o acumulado, maior, com uma (cabe sem rolar de lado no iPad). */
function Cell({ value, digits }: { value: number | null | undefined; digits: number }) {
  if (value === null || value === undefined) {
    return (
      <td>
        <span aria-hidden="true">—</span>
        <span className="sr-only">sem dado</span>
      </td>
    );
  }
  return <td className={`is-${trend(value)}`}>{formatSignedPercent(value, digits)}</td>;
}

function YearTable({ perf, mode, compare }: { perf: Performance; mode: Mode; compare: Compare }) {
  const accumulated = mode === "accumulated";
  const newest = perf.years[perf.years.length - 1];
  const index = compare && newest ? newest.benchmarks[compare.code] : undefined;
  const lastDay = perf.window?.to;
  const lagging = compare && index?.as_of && lastDay && index.as_of < lastDay ? index.as_of : null;
  return (
    <>
      {/* A rolagem lateral (celular) é do próprio navegador: o contêiner rolável recebe foco pelo teclado. */}
      <div className="month-table__scroll">
        <table className="month-table year-table">
          <caption className="sr-only">
            {accumulated
              ? "Rentabilidade acumulada da carteira desde o começo da série até o fim de cada mês"
              : "Rentabilidade da carteira em cada mês e no ano"}
            {compare ? `, com o ${compare.label} nas mesmas janelas` : ""}
          </caption>
          <thead>
            <tr>
              <th scope="col">Ano</th>
              {MONTHS.map((m, i) => (
                <th key={m} scope="col">
                  <abbr title={MONTH_NAMES[i]}>{m}</abbr>
                </th>
              ))}
              <th scope="col">{accumulated ? "Acumulado" : "No ano"}</th>
            </tr>
          </thead>
          <tbody>
            {[...perf.years].reverse().map((row) => (
              <YearRows key={row.year} row={row} accumulated={accumulated} compare={compare} />
            ))}
          </tbody>
        </table>
      </div>
      {lagging && compare && (
        <p className="field__hint year-table__note">
          {compare.label} publicado até {formatDay(lagging, true)}.
        </p>
      )}
    </>
  );
}

function YearRows({ row, accumulated, compare }: { row: PerformanceYear; accumulated: boolean; compare: Compare }) {
  const months = accumulated ? row.accumulated : row.months;
  const total = accumulated ? row.accumulated_percent : row.return_percent;
  const index = compare ? row.benchmarks[compare.code] : undefined;
  const digits = accumulated ? 1 : 2;
  return (
    <>
      <tr className={cx(compare && "year-table__portfolio")}>
        <th scope="row">{row.year}</th>
        {months.map((value, i) => (
          <Cell key={i} value={value} digits={digits} />
        ))}
        <td className={cx("month-table__total", `is-${trend(total)}`)}>{formatSignedPercent(total, digits)}</td>
      </tr>
      {compare && (
        <tr className="year-table__index">
          <th scope="row">
            <span className="sr-only">{row.year}, </span>
            {compare.label}
          </th>
          {Array.from({ length: 12 }, (_, i) => (
            <Cell key={i} value={(accumulated ? index?.accumulated : index?.months)?.[i]} digits={digits} />
          ))}
          <Cell value={accumulated ? index?.accumulated_percent : index?.return_percent} digits={digits} />
        </tr>
      )}
    </>
  );
}
