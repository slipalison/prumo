import { errorMessage } from "../../api/client";
import { usePerformanceClasses } from "../../api/performance";
import type { ClassPerformance, ClassesPerformance, PerformancePeriod } from "../../api/performanceTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { Panel } from "../../components/Layout";
import { Skeleton } from "../../components/Skeleton";
import { CLASS_META } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { formatDay, formatSignedPercent, spansYears, trend } from "./performanceFormat";

const INFO =
  "Cada classe pelo mesmo método de cotas da carteira, como se fosse uma carteira só dela: o aporte não conta como ganho e os proventos contam como retorno. O peso é a parte do patrimônio no fim do período. Ao lado, o índice da classe na mesma janela: IFIX para FIIs, Ibovespa para Ações Nacionais, CDI para Renda Fixa.";

/**
 * Por classe: o peso de cada classe ao lado da rentabilidade dela no período
 * da tela, e o índice de referência na mesma janela (docs/desempenho.md, "Por
 * classe").
 */
export function ClassReturnsPanel({ period }: { period: PerformancePeriod }) {
  const query = usePerformanceClasses(period);
  return (
    <Panel title="Por classe" info={INFO} infoLabel="Como se calcula o desempenho por classe">
      {query.isPending && <Skeleton height={160} radius={14} />}
      {query.isError && (
        <div className="perf-classes__error" role="alert">
          <p className="field__hint">Não consegui carregar as classes: {errorMessage(query.error)}</p>
          <button type="button" className="chip" onClick={() => void query.refetch()}>
            Tentar de novo
          </button>
        </div>
      )}
      {query.data && <ClassTable data={query.data} />}
    </Panel>
  );
}

function ClassTable({ data }: { data: ClassesPerformance }) {
  const money = useMoney();
  const range = data.window;
  const withYear = spansYears(range?.from, range?.to);
  return (
    <DataTable
      caption="Peso e rentabilidade de cada classe no período"
      columns={columns(money, range, withYear)}
      rows={data.classes}
      rowKey={(row) => row.asset_class}
      className="perf-classes"
      empty="Nenhuma classe teve patrimônio no período."
    />
  );
}

/** Quando a classe não mede o período inteiro: entrou ou zerou no meio dele. */
function partialNote(row: ClassPerformance, range: ClassesPerformance["window"], withYear: boolean): string | null {
  if (row.full_window || !range) return null;
  if (row.from > range.from) return `desde ${formatDay(row.from, withYear)}`;
  if (row.to < range.to) return `até ${formatDay(row.to, withYear)}`;
  return null;
}

function columns(
  money: Money,
  range: ClassesPerformance["window"],
  withYear: boolean,
): DataColumn<ClassPerformance>[] {
  return [
    {
      key: "class",
      label: "Classe",
      rowHeader: true,
      cell: (row) => {
        const note = partialNote(row, range, withYear);
        return (
          <span className="perf-classes__name">
            {CLASS_META[row.asset_class].label}
            {note && <span className="perf-classes__note">{note}</span>}
          </span>
        );
      },
    },
    {
      key: "weight",
      label: "Peso",
      align: "end",
      cell: (row) => (
        <span className="perf-classes__weight">
          <span className="perf-classes__bar" aria-hidden="true">
            <span style={{ width: `${Math.min(Math.max(row.weight_percent, 0), 100)}%` }} />
          </span>
          {formatPercent(row.weight_percent, 1)}
        </span>
      ),
    },
    {
      key: "return",
      label: "Rentabilidade",
      align: "end",
      cell: (row) => (
        <strong className={cx("perf-classes__return", `is-${trend(row.return_percent)}`)}>
          {formatSignedPercent(row.return_percent)}
        </strong>
      ),
    },
    {
      key: "benchmark",
      label: "Índice",
      align: "end",
      cell: (row) =>
        row.benchmark ? (
          <span className="perf-classes__index">
            {row.benchmark.label}{" "}
            {row.benchmark.return_percent === null ? "sem dado" : formatSignedPercent(row.benchmark.return_percent)}
          </span>
        ) : (
          <>
            <span aria-hidden="true">—</span>
            <span className="sr-only">sem índice para a classe</span>
          </>
        ),
    },
    {
      key: "value",
      label: "Patrimônio",
      align: "end",
      wideOnly: true,
      cell: (row) => money.brl(row.market_value),
    },
  ];
}
