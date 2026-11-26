import { errorMessage } from "../../api/client";
import type { Balance, CapitalGain, Darf, Pool, TaxMonth, TaxMonths } from "../../api/taxTypes";
import { DataTable, StatusPill, type DataColumn } from "../../components/DataTable";
import { Panel } from "../../components/Layout";
import { useToast } from "../../components/toastContext";
import { useMoney } from "../../lib/privacy";
import { DoubtTip } from "./DoubtTip";
import { isoDay, monthName, POOL_LABEL, rateText, SHORT_INSTRUMENT } from "./taxText";
import { useDarfPaid } from "./useTax";

const stockSales = (month: TaxMonth) => {
  const line = month.sales.find((s) => s.instrument === "stock");
  return line ? line.common_sales + line.day_trade_sales : 0;
};

const monthTax = (month: TaxMonth) =>
  month.pools.reduce((sum, pool) => sum + pool.tax, 0) + month.capital_gain.tax;

/** Mês sem venda, imposto nem DARF: só o nome, no celular. */
const isQuiet = (month: TaxMonth) =>
  stockSales(month) === 0 && monthTax(month) === 0 && month.darfs.every((d) => !d.payable && d.pending_after <= 0);

/** Zero é traço: o número que não existe não ocupa a vista. */
function Amount({ value }: { value: number }) {
  const money = useMoney();
  return value === 0 ? <span className="tax-empty">–</span> : <>{money.brl(value)}</>;
}

/**
 * Os doze meses do ano numa tabela: vendas de ações, imposto, DARF, vencimento
 * e a situação (a pagar ou pago, com o botão que muda). O que fica depois da
 * tabela é o saldo que passa para o ano seguinte.
 */
export function MonthsPanel({ data }: { data: TaxMonths }) {
  const money = useMoney();
  const pendingDarf = (data.closing.pending_darf.exchange_net_gain ?? 0) + (data.closing.pending_darf.capital_gain ?? 0);

  const columns: DataColumn<TaxMonth>[] = [
    {
      key: "month",
      label: "Mês",
      rowHeader: true,
      cell: (month) => <span className="tax-month-name">{monthName(month.month)}</span>,
    },
    { key: "sales", label: "Vendas de ações", align: "end", cell: (month) => <Amount value={stockSales(month)} /> },
    { key: "tax", label: "Imposto do mês", align: "end", cell: (month) => <Amount value={monthTax(month)} /> },
    { key: "darf", label: "DARF", cell: (month) => <DarfCell month={month} /> },
    {
      key: "due",
      label: "Vencimento",
      cell: (month) => {
        const payable = month.darfs.filter((d) => d.payable);
        return payable.length === 0 ? (
          <span className="tax-empty">–</span>
        ) : (
          payable.map((d) => (
            <span key={d.code} className="tax-line">
              {isoDay(d.due_date)}
            </span>
          ))
        );
      },
    },
    {
      key: "status",
      label: "Situação",
      cell: (month) => month.darfs.filter((d) => d.payable).map((darf) => <PaidToggle key={darf.code} month={month} darf={darf} />),
    },
  ];

  return (
    <Panel
      title={`Meses de ${data.year}`}
      info="DARF 6015 (bolsa) e 4600 (cripto), com vencimento no último dia útil do mês seguinte. Abaixo de dez reais o DARF não se paga: o valor soma ao do mês seguinte."
      infoLabel="Como o DARF é calculado"
    >
      <DataTable
        caption={`DARF por mês em ${data.year}`}
        columns={columns}
        rows={data.months}
        rowKey={(month) => String(month.month)}
        rowClassName={(month) => (isQuiet(month) ? "tax-row--quiet" : undefined)}
        className="tax-months"
      />
      <p className="tax-closing">
        Prejuízo a compensar em 31/12: <BalanceText balance={data.closing} empty="nenhum" />
        {pendingDarf > 0 && <> · DARF abaixo do mínimo ainda não pago: {money.brl(pendingDarf)}</>}
      </p>
    </Panel>
  );
}

function DarfCell({ month }: { month: TaxMonth }) {
  const money = useMoney();
  const payable = month.darfs.filter((d) => d.payable);
  const pending = month.darfs.filter((d) => !d.payable && d.pending_after > 0);
  if (payable.length === 0 && pending.length === 0) return <span className="tax-empty">–</span>;
  return (
    <>
      {payable.map((darf) => (
        <span key={darf.code} className="tax-line">
          {darf.code} · {money.brl(darf.amount)}
        </span>
      ))}
      {pending.map((darf) => (
        <span key={darf.code} className="tax-line tax-line--muted">
          {darf.code}: abaixo do mínimo, soma no mês seguinte
        </span>
      ))}
    </>
  );
}

function PaidToggle({ month, darf }: { month: TaxMonth; darf: Darf }) {
  const change = useDarfPaid();
  const toast = useToast();
  const name = monthName(month.month);
  const paid = darf.paid !== null;
  const toggle = () =>
    change.mutate(
      { year: month.year, month: month.month, code: darf.code, paid: !paid },
      {
        onSuccess: () =>
          toast({
            tone: paid ? "info" : "success",
            message: paid ? `DARF ${darf.code} de ${name} voltou para a pagar.` : `DARF ${darf.code} de ${name} marcado como pago.`,
          }),
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );

  return (
    <span className="tax-paid">
      <StatusPill status={paid ? "ok" : "attention"}>{paid ? "Pago" : "A pagar"}</StatusPill>
      <button
        type="button"
        className="link-btn tax-paid__button"
        onClick={toggle}
        disabled={change.isPending}
        aria-label={paid ? `Desmarcar o DARF ${darf.code} de ${name}` : `Marcar o DARF ${darf.code} de ${name} como pago`}
      >
        {paid ? "Desmarcar" : "Marcar como pago"}
      </button>
      {darf.paid && !darf.paid.same_amount && <span className="tax-line--muted">o valor mudou depois de marcar: confira</span>}
    </span>
  );
}

/** O prejuízo que sobra de cada grupo, em uma frase. */
export function BalanceText({ balance, empty }: { balance: Balance; empty: string }) {
  const money = useMoney();
  const parts = (Object.entries(balance.losses) as Array<[Pool, number]>).filter(([, value]) => value > 0);
  if (parts.length === 0) return <>{empty}</>;
  return <>{parts.map(([pool, value]) => `${POOL_LABEL[pool]} ${money.brl(value)}`).join(" · ")}</>;
}

/** A conta de um mês: vendas, grupos com a compensação, cripto e as dúvidas que tocam o número. */
export function MonthDetails({ month }: { month: TaxMonth }) {
  const money = useMoney();
  const name = monthName(month.month);
  const pools = month.pools.filter((p) => p.result !== 0 || p.loss_before !== 0);
  const sales = month.sales.filter((s) => s.common_sales > 0 || s.day_trade_sales > 0);
  const exempt = month.exemptions.find((e) => e.id === "stock_sales");

  const columns: DataColumn<(typeof pools)[number]>[] = [
    { key: "pool", label: "Grupo", rowHeader: true, cell: (p) => POOL_LABEL[p.pool] },
    { key: "result", label: "Resultado", align: "end", cell: (p) => money.brl(p.result) },
    { key: "before", label: "Prejuízo antes", align: "end", cell: (p) => money.brl(p.loss_before) },
    { key: "used", label: "Compensado", align: "end", cell: (p) => money.brl(p.loss_used) },
    { key: "base", label: "Base", align: "end", cell: (p) => money.brl(p.base) },
    { key: "rate", label: "Alíquota", align: "end", cell: (p) => rateText(p.rate) },
    { key: "tax", label: "Imposto", align: "end", cell: (p) => money.brl(p.tax) },
    { key: "after", label: "Prejuízo que fica", align: "end", cell: (p) => money.brl(p.loss_after) },
  ];

  return (
    <details className="tax-month">
      <summary>{name.charAt(0).toUpperCase() + name.slice(1)}</summary>
      {sales.length > 0 && (
        <ul className="tax-facts" aria-label={`Vendas de ${name}`}>
          {sales.map((s) => (
            <li key={s.instrument}>
              {SHORT_INSTRUMENT[s.instrument]}: vendas de {money.brl(s.common_sales)}
              {s.day_trade_sales > 0 && <> (mais {money.brl(s.day_trade_sales)} em day trade)</>}, resultado de{" "}
              {money.brl(s.common_result + s.day_trade_result)}
            </li>
          ))}
        </ul>
      )}
      {pools.length > 0 && (
        <div role="region" aria-label={`Apuração de ${name}`}>
          <DataTable caption={`Apuração de ${name}`} columns={columns} rows={pools} rowKey={(p) => p.pool} />
        </div>
      )}
      <CryptoLine gain={month.capital_gain} name={name} />
      {exempt && exempt.exempt_gain > 0 && <p className="tax-facts">Lucro isento das ações no mês: {money.brl(exempt.exempt_gain)}</p>}
      {month.doubts.length > 0 && (
        <p className="tax-doubts">
          {month.doubts.map((doubt) => (
            <DoubtTip key={doubt} doubt={doubt} text />
          ))}
        </p>
      )}
    </details>
  );
}

function CryptoLine({ gain, name }: { gain: CapitalGain; name: string }) {
  const money = useMoney();
  if (gain.gains === 0 && gain.losses === 0) return null;
  return (
    <p className="tax-facts" aria-label={`Cripto em ${name}`}>
      Cripto: lucro de {money.brl(gain.gains)} e prejuízo de {money.brl(gain.losses)} nas vendas;{" "}
      {gain.exempt ? "mês isento." : `imposto de ${money.brl(gain.tax)}.`}
    </p>
  );
}
