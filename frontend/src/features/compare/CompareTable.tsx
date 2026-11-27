import { errorMessage } from "../../api/client";
import type {
  CompareAsset,
  CompareCell,
  CompareGroup,
  CompareReason,
  CompareRow,
  CompareTable as Table,
} from "../../api/compareTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { InfoTip } from "../../components/InfoTip";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { FINANCIAL_TEMPLATE_TEXT, NOT_APPLICABLE_TEXT } from "../fundamentals/fundamentalsText";
import { DIRECTION_TEXT, REASON_TEXT, ROW_TEXT } from "./compareText";

const GROUP_TITLE: Record<CompareGroup, string> = {
  general: "Em comum",
  company: "Empresas",
  fund: "Fundos imobiliários",
};

const multiple = (value: number) =>
  `${value.toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}×`;

const usd = new Intl.NumberFormat("pt-BR", { style: "currency", currency: "USD" });

/** O valor de uma célula na unidade da linha. Reais passam pelo olho de ocultar valores. */
function formatValue(row: CompareRow, value: number, asset: CompareAsset, money: Money): string {
  switch (row.unit) {
    case "multiple":
      return multiple(value);
    case "percent":
      return formatPercent(value, 2);
    case "money":
      if (asset.currency === "BRL") return money.brl(value);
      return asset.currency === "USD" ? usd.format(value) : `${value.toLocaleString("pt-BR")} ${asset.currency}`;
    case "money_compact":
      return money.compact(value);
    case "text":
      return String(value);
  }
}

/** Motivos de "não se aplica": o tipo de ativo errado e, nas empresas, banco, seguradora e holding. */
const NOT_APPLICABLE: ReadonlySet<CompareReason> = new Set(["not_applicable", "financial_template", "holding"]);

/**
 * "—" com o motivo para quem passa o mouse e para o leitor de tela; o que não
 * se aplica diz "não se aplica" escrito (a mesma regra da folha do ativo e dos
 * resultados), e fica fora do destaque, que só olha número.
 */
function Missing({ reason }: { reason: CompareReason | null }) {
  const text = reason ? REASON_TEXT[reason] : "Sem número.";
  if (reason && NOT_APPLICABLE.has(reason)) {
    return (
      <span className="not-applicable">
        <span aria-hidden="true" title={text}>
          não se aplica
        </span>
        <span className="sr-only">{text}</span>
      </span>
    );
  }
  return (
    <>
      <span aria-hidden="true" title={text}>
        —
      </span>
      <span className="sr-only">{text}</span>
    </>
  );
}

function Cell({ row, cell, asset, money }: { row: CompareRow; cell: CompareCell; asset: CompareAsset; money: Money }) {
  if (cell.text !== null) return <span>{cell.text}</span>;
  if (cell.value === null) return <Missing reason={cell.reason} />;
  const text = formatValue(row, cell.value, asset, money);
  if (!cell.mark) return <span>{text}</span>;
  const word = cell.mark === "max" ? "maior valor da linha" : "menor valor da linha";
  return (
    <span className="compare-mark" data-mark={cell.mark}>
      <strong>{text}</strong>
      <span className="compare-mark__tag" aria-hidden="true">
        {cell.mark === "max" ? "maior" : "menor"}
      </span>
      <span className="sr-only">({word})</span>
    </span>
  );
}

function columnsOf(table: Table, money: Money): Array<DataColumn<CompareRow>> {
  return [
    {
      key: "indicator",
      label: "Indicador",
      rowHeader: true,
      width: "15rem",
      cell: (row) => (
        <span className="compare-row-label">
          {ROW_TEXT[row.id].label}
          <InfoTip label={`Sobre ${ROW_TEXT[row.id].label}`} align="start">
            {ROW_TEXT[row.id].hint} {DIRECTION_TEXT[row.direction]}
          </InfoTip>
        </span>
      ),
    },
    ...table.assets.map(
      (asset, index): DataColumn<CompareRow> => ({
        key: asset.key,
        label: asset.symbol,
        align: "end",
        cell: (row) => {
          const cell = row.cells[index];
          return cell ? <Cell row={row} cell={cell} asset={asset} money={money} /> : null;
        },
      }),
    ),
  ];
}

/**
 * Por que o indicador de empresa não se aplica a cada ativo que tem o caso:
 * holding (o lucro vem das participadas) ou instituição financeira. Sai dos
 * motivos das células, que vêm da mesma regra dos fundamentos.
 */
function notApplicableNotes(table: Table, rows: CompareRow[]): Array<{ symbol: string; text: string }> {
  return table.assets.flatMap((asset, index) => {
    const reasons = new Set(rows.map((row) => row.cells[index]?.reason));
    if (reasons.has("holding")) return [{ symbol: asset.symbol, text: NOT_APPLICABLE_TEXT.holding }];
    if (reasons.has("financial_template")) return [{ symbol: asset.symbol, text: FINANCIAL_TEMPLATE_TEXT }];
    return [];
  });
}

type CompareTableProps = {
  table: Table | undefined;
  pending: boolean;
  error: unknown;
  onRetry: () => void;
};

function NotApplicableNotes({ notes }: { notes: Array<{ symbol: string; text: string }> }) {
  if (notes.length === 0) return null;
  return (
    <ul className="compare-table__notes">
      {notes.map((note) => (
        <li key={note.symbol}>
          <strong>{note.symbol}:</strong> <span>{note.text}</span>
        </li>
      ))}
    </ul>
  );
}

/**
 * A tabela lado a lado: uma coluna por ativo, uma linha por indicador, em
 * blocos (em comum, empresas, fundos) que só aparecem se algum ativo os tem.
 * O destaque é o maior ou o menor valor da linha, entre ativos do mesmo tipo.
 */
export function CompareTable({ table, pending, error, onRetry }: CompareTableProps) {
  const money = useMoney();
  if (error && !table) {
    return (
      <div role="alert" className="compare__error">
        <p>{errorMessage(error)}</p>
        <MagneticButton variant="ghost" magnetic={false} onClick={onRetry}>
          Tentar de novo
        </MagneticButton>
      </div>
    );
  }
  if (!table) return pending ? <Skeleton height={220} /> : null;
  const columns = columnsOf(table, money);
  const groups = (["general", "company", "fund"] as const)
    .map((group) => ({ group, rows: table.rows.filter((row) => row.group === group) }))
    .filter((entry) => entry.rows.length > 0);
  return (
    <div className={cx("compare-table", pending && "is-refreshing")}>
      {groups.map(({ group, rows }) => (
        <section key={group} aria-label={GROUP_TITLE[group]}>
          <h3 className="section-title compare-table__group">{GROUP_TITLE[group]}</h3>
          <DataTable
            caption={`Indicadores: ${GROUP_TITLE[group]}`}
            columns={columns}
            rows={rows}
            rowKey={(row) => row.id}
            className="compare-table__table"
          />
          {group === "company" && <NotApplicableNotes notes={notApplicableNotes(table, rows)} />}
        </section>
      ))}
    </div>
  );
}
