import { Fragment, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { cx } from "../lib/cx";
import { IconAlert, IconCheck, IconClose, IconInfo } from "./icons";

export type DataColumn<T> = {
  key: string;
  /** O nome da coluna: vai no cabeçalho e, na tabela empilhada (celular), antes de cada valor. */
  label: string;
  /** Um cabeçalho mais rico que o rótulo (com ⓘ, por exemplo). */
  header?: ReactNode;
  cell: (row: T) => ReactNode;
  /** Números e ações à direita. */
  align?: "start" | "end" | "center";
  /** A coluna que nomeia a linha (`<th scope="row">`): o ativo, a regra. Uma por tabela. */
  rowHeader?: boolean;
  /** Some na tabela empilhada (o detalhe que no celular fica na folha). */
  wideOnly?: boolean;
  /** Largura fixa da coluna (CSS), quando o conteúdo é previsível. */
  width?: string;
};

type DataTableProps<T> = {
  /** O nome da tabela (`<caption>`); o leitor de tela o anuncia ao entrar. */
  caption: string;
  /** Mostra a legenda; sem isto ela fica só para o leitor de tela (o painel já tem título). */
  showCaption?: boolean;
  columns: DataColumn<T>[];
  rows: readonly T[];
  rowKey: (row: T) => string;
  /** O que dizer quando não há linha (nunca uma tabela vazia muda). */
  empty?: ReactNode;
  /** Linhas de 44 px (alvo de toque) quando a linha tem botão. */
  cozy?: boolean;
  /** Empilha as linhas quando a largura do conteúdo é pequena (padrão: sim). */
  stack?: boolean;
  /** Classe por linha (para o estado da linha: atenção, desligada). */
  rowClassName?: (row: T) => string | undefined;
  /**
   * A linha aberta logo abaixo da linha (o detalhe que não cabe numa coluna):
   * `null` quando a linha está fechada. Quem abre é um botão do chamador, com
   * `aria-expanded` e `aria-controls` apontando para `detailId`.
   */
  detail?: (row: T) => ReactNode | null;
  detailId?: (row: T) => string;
  className?: string;
};

const ALIGN = { start: undefined, end: "is-end", center: "is-center" } as const;

/**
 * A tabela compacta das listas longas (regras da Análise, referências de
 * preço, ativos): uma linha de 40 px por item, cabeçalho pequeno, números
 * tabulares à direita. É uma `<table>` de verdade (o leitor de tela navega por
 * linha e coluna); larga demais, rola de lado; estreita, empilha.
 *
 * O bloco só vira rolagem lateral (`data-scrolls`) quando a tabela de fato não
 * cabe: tabela que cabe não tem `overflow`, e a área de toque do ⓘ no canto não
 * cria uma rolagem de 2 px. O balão do ⓘ vai para a camada de topo (`Toggletip`),
 * então nem a tabela que rola o corta.
 */
export function DataTable<T>({
  caption,
  showCaption = false,
  columns,
  rows,
  rowKey,
  empty,
  cozy = false,
  stack = true,
  rowClassName,
  detail,
  detailId,
  className,
}: Readonly<DataTableProps<T>>) {
  const isEmpty = rows.length === 0 && empty !== undefined;
  const wrap = useRef<HTMLDivElement>(null);
  const [scrolls, setScrolls] = useState(false);
  useLayoutEffect(() => {
    const block = wrap.current;
    const table = block?.firstElementChild;
    if (!block || !(table instanceof HTMLElement) || typeof ResizeObserver === "undefined") return;
    const measure = () => setScrolls(table.offsetWidth > block.clientWidth + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(block);
    observer.observe(table);
    return () => observer.disconnect();
  }, [isEmpty]);
  if (isEmpty) {
    return <p className="data-table__empty">{empty}</p>;
  }
  return (
    <div
      ref={wrap}
      data-scrolls={scrolls ? "" : undefined}
      className={cx("data-table", cozy && "data-table--cozy", stack && "data-table--stack", className)}
    >
      <table className="data-table__table">
        <caption className={showCaption ? "data-table__caption" : "sr-only"}>{caption}</caption>
        <thead>
          <tr>
            {columns.map((column) => (
              <th
                key={column.key}
                scope="col"
                className={cx(ALIGN[column.align ?? "start"], column.wideOnly && "is-wide-only")}
                style={column.width ? { width: column.width } : undefined}
              >
                {column.header ?? column.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const opened = detail?.(row) ?? null;
            return (
              <Fragment key={rowKey(row)}>
                <tr className={cx(rowClassName?.(row), opened !== null && "is-open")}>
                  {columns.map((column) => {
                    const classes = cx(ALIGN[column.align ?? "start"], column.wideOnly && "is-wide-only");
                    return column.rowHeader ? (
                      <th key={column.key} scope="row" className={classes}>
                        {column.cell(row)}
                      </th>
                    ) : (
                      <td key={column.key} className={classes} data-label={column.label}>
                        {column.cell(row)}
                      </td>
                    );
                  })}
                </tr>
                {opened !== null && (
                  <tr className="data-table__detail" id={detailId?.(row)}>
                    <td colSpan={columns.length}>{opened}</td>
                  </tr>
                )}
              </Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export type Status = "ok" | "attention" | "bad" | "off" | "nodata" | "info";

/** O texto de cada estado: o estado é dito, a cor e o ícone só reforçam. */
const STATUS_LABEL: Record<Status, string> = {
  ok: "Ok",
  attention: "Ponto a ver",
  bad: "Fora do limite",
  off: "Desligada",
  nodata: "Sem dado",
  info: "Informação",
};

const STATUS_ICON: Record<Status, ReactNode> = {
  ok: <IconCheck width={14} height={14} />,
  attention: <IconAlert width={14} height={14} />,
  bad: <IconClose width={14} height={14} />,
  off: null,
  nodata: <IconInfo width={14} height={14} />,
  info: <IconInfo width={14} height={14} />,
};

type StatusPillProps = {
  status: Status;
  /** Troca o texto padrão ("Pago", "A pagar"); o texto é obrigatório na tela, nunca só a cor. */
  children?: ReactNode;
};

/** O selo de estado de uma linha: ícone + texto, borda tracejada no que não conta. */
export function StatusPill({ status, children }: Readonly<StatusPillProps>) {
  return (
    <span className={cx("status-pill", `status-pill--${status}`)}>
      {STATUS_ICON[status]}
      {children ?? STATUS_LABEL[status]}
    </span>
  );
}
