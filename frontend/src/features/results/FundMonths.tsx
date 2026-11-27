import type { FundResults } from "../../api/resultsTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { Missing } from "../fundamentals/Missing";
import { MiniBars } from "./MiniBars";

export type FundEntry = { assetId: string; symbol: string; fund: FundResults };

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

/** `2026-10-01` → "out/26". */
function monthShort(iso: string): string {
  const [year, month] = iso.split("-").map(Number) as [number, number];
  return `${MONTHS[month - 1]}/${String(year).slice(2)}`;
}

/** `2026-10-01` → "out/2026". */
function monthLong(iso: string): string {
  const [year, month] = iso.split("-").map(Number) as [number, number];
  return `${MONTHS[month - 1]}/${year}`;
}

const yieldText = (value: number) => formatPercent(value * 100, 2);

/** Média dos meses com número; `null` sem nenhum. */
function average(values: Array<number | null>): number | null {
  const known = values.filter((v): v is number => v !== null);
  return known.length === 0 ? null : known.reduce((a, b) => a + b, 0) / known.length;
}

function columns(money: Money): DataColumn<FundEntry>[] {
  return [
    {
      key: "fund",
      label: "Fundo",
      rowHeader: true,
      cell: (row) => (
        <span className="results__who">
          <span className="results__symbol">{row.symbol}</span>
          <span className="results__name">{row.fund.name}</span>
        </span>
      ),
    },
    {
      key: "month",
      label: "Informe",
      cell: (row) => {
        const last = row.fund.months.at(-1);
        return last ? monthLong(last.reference_date) : "sem informe";
      },
    },
    {
      key: "yield",
      label: "Rendimento do mês",
      align: "end",
      cell: (row) => {
        const last = row.fund.months.at(-1);
        if (!last) return <Missing gap="not_reported" />;
        return last.dividend_yield.value === null ? <Missing gap={last.dividend_yield.gap} /> : yieldText(last.dividend_yield.value);
      },
    },
    {
      key: "average",
      label: "Média do período",
      align: "end",
      wideOnly: true,
      cell: (row) => {
        const mean = average(row.fund.months.map((m) => m.dividend_yield.value));
        return mean === null ? <Missing gap="not_reported" /> : yieldText(mean);
      },
    },
    {
      key: "nav",
      label: "Valor patrimonial por cota",
      align: "end",
      cell: (row) => {
        const last = row.fund.months.at(-1);
        if (!last) return <Missing gap="not_reported" />;
        return last.nav_per_share.value === null ? <Missing gap={last.nav_per_share.gap} /> : money.brl(last.nav_per_share.value);
      },
    },
    {
      key: "series",
      label: "Rendimento mês a mês",
      cell: (row) => (
        <MiniBars
          highlightLast
          label={`Rendimento do mês sobre o valor patrimonial, ${row.fund.months
            .map((m) => `${monthShort(m.reference_date)}: ${m.dividend_yield.value === null ? "sem número" : yieldText(m.dividend_yield.value)}`)
            .join("; ")}`}
          bars={row.fund.months.map((m) => ({
            key: m.reference_date,
            label: monthShort(m.reference_date),
            value: m.dividend_yield.value,
            title: `${monthLong(m.reference_date)}: ${m.dividend_yield.value === null ? "sem número" : yieldText(m.dividend_yield.value)}`,
          }))}
        />
      ),
    },
  ];
}

/**
 * Os FIIs da carteira mês a mês, pelo informe mensal da CVM: o rendimento do
 * mês sobre o valor patrimonial da cota (como o fundo informou) e o valor
 * patrimonial. O informe não traz o resultado do fundo nem o rendimento em
 * reais: a tela diz isso no ⓘ do painel.
 */
export function FundMonths({ funds }: { funds: FundEntry[] }) {
  const money = useMoney();
  return (
    <DataTable
      caption="FIIs mês a mês, pelo informe mensal da CVM"
      columns={columns(money)}
      rows={funds}
      rowKey={(row) => row.assetId}
      className="results__funds"
    />
  );
}
