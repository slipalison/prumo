import type { Doubt, IncomePayer, IrpfIncome } from "../../api/taxTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { useToast } from "../../components/toastContext";
import { formatBRL } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { DoubtTip } from "./DoubtTip";
import { irpfLabel, monthName } from "./taxText";

/**
 * Uma linha da ficha: uma fonte pagadora dentro de um código, ou o código
 * inteiro quando não há fonte (o lucro isento das próprias vendas, 20 e 05).
 */
type IncomeRow = { line: IrpfIncome; payer: IncomePayer | null; exclusive: boolean };

const SHEET: Record<string, string> = {
  isentos: "Rendimentos isentos e não tributáveis",
  exclusiva: "Rendimentos sujeitos à tributação exclusiva",
};

const SHEET_SHORT: Record<string, string> = { isentos: "Isentos", exclusiva: "Exclusiva" };

/** O texto que vai para a área de transferência: com o valor de verdade. */
function rowText({ line, payer, exclusive }: IncomeRow): string {
  const head = `${SHEET[line.group] ?? line.group}, código ${line.code} (${irpfLabel(line.group, line.code, line.label)})`;
  if (!payer) return `${head}: ${formatBRL(line.net)}.`;
  const who = payer.cnpj ? `CNPJ ${payer.cnpj}, ${payer.name ?? ""}` : `CNPJ não encontrado (${payer.symbols.join(", ")})`;
  const value = exclusive
    ? `${formatBRL(payer.net)} (líquido; bruto ${formatBRL(payer.gross)}, IR retido ${formatBRL(payer.withheld)})`
    : formatBRL(payer.net);
  return `${head}: ${who}, ${value}.`;
}

function rowsOf(lines: IrpfIncome[], exclusive: boolean): IncomeRow[] {
  return lines.flatMap((line): IncomeRow[] => {
    if (line.by_payer.length > 0) return line.by_payer.map((payer) => ({ line, payer, exclusive }));
    return line.gross > 0 ? [{ line, payer: null, exclusive }] : [];
  });
}

const rowName = (row: IncomeRow) => (row.payer ? (row.payer.name ?? row.payer.symbols.join(", ")) : `código ${row.line.code}`);

type IncomeReportProps = { exempt: IrpfIncome[]; exclusive: IrpfIncome[]; doubts: Doubt[] };

/** As colunas da ficha de rendimentos; `onCopy` leva o texto da linha para a área de transferência. */
function incomeColumns(money: Money, onCopy: (text: string, done: string) => Promise<void>): DataColumn<IncomeRow>[] {
  return [
    {
      key: "sheet",
      label: "Ficha",
      rowHeader: true,
      cell: ({ line, payer }) => (
        <>
          <span className="tax-code">
            {SHEET_SHORT[line.group] ?? line.group} · Código {line.code}
          </span>
          <span className="tax-sheet">{irpfLabel(line.group, line.code, line.label)}</span>
          {payer && line.by_payer.length > 1 && <span className="tax-payers">total do código {money.brl(line.net)}</span>}
        </>
      ),
    },
    {
      key: "payer",
      label: "Fonte pagadora",
      cell: ({ payer }) =>
        payer ? (
          <>
            <span className="tax-sheet">{payer.name ?? payer.symbols.join(", ")}</span>
            <span className="tax-payers">
              {payer.cnpj ? `CNPJ ${payer.cnpj}` : "CNPJ não encontrado na CVM: confira no informe de rendimentos"}
              {payer.name && ` · ${payer.symbols.join(", ")}`}
            </span>
            {payer.above_threshold.map((m) => (
              <span key={m.month} className="tax-payers">
                Em {monthName(m.month)}: {money.brl(m.gross)} da mesma empresa, acima do limite; retenção de 10% pela regra{" "}
                {money.brl(m.withholding_by_rule)}, retido informado {money.brl(m.withheld)}.
              </span>
            ))}
          </>
        ) : (
          <span className="tax-sheet">Lucro das suas vendas no ano</span>
        ),
    },
    {
      key: "value",
      label: "Valor",
      align: "end",
      cell: ({ line, payer, exclusive: isExclusive }) => {
        const amount = payer ?? line;
        return isExclusive ? (
          <>
            Líquido {money.brl(amount.net)}
            <span className="tax-payers">
              (bruto {money.brl(amount.gross)}, retido {money.brl(amount.withheld)})
            </span>
          </>
        ) : (
          money.brl(amount.net)
        );
      },
    },
    {
      key: "copy",
      label: "Copiar",
      align: "end",
      cell: (row) => (
        <button
          type="button"
          className="link-btn"
          onClick={() => void onCopy(rowText(row), `Linha de ${rowName(row)} copiada.`)}
          aria-label={`Copiar a linha de ${rowName(row)}, código ${row.line.code}`}
        >
          Copiar
        </button>
      ),
    },
  ];
}

/**
 * Os rendimentos do ano como a declaração pede: uma linha por fonte pagadora
 * em cada código (CNPJ e nome do cadastro da CVM, os tickers, o valor), e o
 * lucro isento das próprias vendas; "Copiar" por linha e o relatório inteiro
 * de uma vez. As dúvidas jurídicas ficam no ⓘ azul do título.
 */
export function IncomeReport({ exempt, exclusive, doubts }: Readonly<IncomeReportProps>) {
  const money = useMoney();
  const toast = useToast();
  const rows = [...rowsOf(exempt, false), ...rowsOf(exclusive, true)];

  const copy = async (text: string, done: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast({ tone: "success", message: done });
    } catch {
      toast({ tone: "error", message: "Não consegui copiar. Selecione o texto e copie à mão." });
    }
  };

  return (
    <section className="tax-block">
      <h3 className="tax-subtitle">
        Rendimentos
        {doubts.includes("dividend_withholding") && <DoubtTip doubt="dividend_withholding" />}
        {doubts.includes("interest_on_equity_net") && <DoubtTip doubt="interest_on_equity_net" />}
        {rows.length > 0 && (
          <button
            type="button"
            className="link-btn tax-subtitle__action"
            onClick={() => void copy(rows.map(rowText).join("\n"), "Relatório de rendimentos copiado.")}
          >
            Copiar o relatório
          </button>
        )}
      </h3>
      <DataTable
        caption="Rendimentos por fonte pagadora"
        columns={incomeColumns(money, copy)}
        rows={rows}
        rowKey={({ line, payer }) => `${line.group}-${line.code}-${payer ? (payer.cnpj ?? payer.symbols.join(",")) : "own"}`}
        empty="Nenhum rendimento neste ano."
      />
      <p className="field__hint">
        Uma linha por fonte pagadora, como o programa da Receita pede. O CNPJ e o nome vêm do cadastro de companhias abertas e do
        informe mensal de FII da CVM: confira com o informe de rendimentos da empresa, do administrador do fundo ou da corretora.
      </p>
    </section>
  );
}
