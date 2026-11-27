import { useId, useState } from "react";
import { errorMessage } from "../../api/client";
import type { Balance, CapitalGain, Darf, Pool, PoolResult, TaxMonth, TaxMonths } from "../../api/taxTypes";
import { DataTable, StatusPill, type DataColumn } from "../../components/DataTable";
import { Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { useToast } from "../../components/toastContext";
import { useMoney } from "../../lib/privacy";
import { DoubtTip } from "./DoubtTip";
import { LateChargeSheet } from "./LateChargeSheet";
import { isoDay, monthName, POOL_LABEL, rateText, SHORT_INSTRUMENT } from "./taxText";
import { useDarfPaid, useInformedWithholding } from "./useTax";

const stockSales = (month: TaxMonth) => {
  const line = month.sales.find((s) => s.instrument === "stock");
  return line ? line.common_sales + line.day_trade_sales : 0;
};

const monthTax = (month: TaxMonth) =>
  month.pools.reduce((sum, pool) => sum + pool.tax, 0) + month.capital_gain.tax;

/** O grupo teve prejuízo a compensar no mês (antes, depois ou usado). */
const hasLoss = (p: PoolResult) => p.loss_before > 0 || p.loss_after > 0 || p.loss_used > 0;

/** Mês sem venda, imposto, DARF nem prejuízo: só o nome, no celular. */
const isQuiet = (month: TaxMonth) =>
  stockSales(month) === 0 &&
  monthTax(month) === 0 &&
  month.darfs.every((d) => !d.payable && d.pending_after <= 0) &&
  !month.pools.some(hasLoss);

/** Zero é traço: o número que não existe não ocupa a vista. */
function Amount({ value }: { value: number }) {
  const money = useMoney();
  return value === 0 ? <span className="tax-empty">–</span> : <>{money.brl(value)}</>;
}

/**
 * O prejuízo a compensar de cada grupo no fim do mês, com o que compensou
 * lucro e o que entrou no mês. Só o grupo que teve prejuízo aparece.
 */
function LossCell({ month }: { month: TaxMonth }) {
  const money = useMoney();
  const pools = month.pools.filter(hasLoss);
  if (pools.length === 0) return <span className="tax-empty">–</span>;
  return (
    <>
      {pools.map((p) => (
        <span key={p.pool} className="tax-loss">
          {POOL_LABEL[p.pool]} {p.loss_after > 0 ? money.brl(p.loss_after) : "zerado"}
          {p.loss_used > 0 && <span className="tax-line tax-line--muted">compensou {money.brl(p.loss_used)}</span>}
          {p.result < 0 && <span className="tax-line tax-line--muted">mais {money.brl(-p.result)} no mês</span>}
        </span>
      ))}
    </>
  );
}

/**
 * Os doze meses do ano numa tabela: vendas de ações, imposto, DARF (com o
 * vencimento embaixo), o prejuízo a compensar de cada grupo e a situação (a
 * pagar, em atraso ou pago, com o botão que muda e a calculadora do atraso).
 * O que fica depois da tabela é o saldo de 1º de janeiro e o que passa para o
 * ano seguinte.
 */
export function MonthsPanel({ data }: { data: TaxMonths }) {
  const money = useMoney();
  const pendingDarf = (data.closing.pending_darf.exchange_net_gain ?? 0) + (data.closing.pending_darf.capital_gain ?? 0);
  const opening = (data.months[0]?.pools ?? []).filter((p) => p.loss_before > 0);

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
    { key: "loss", label: "Prejuízo a compensar", cell: (month) => <LossCell month={month} /> },
    {
      key: "status",
      label: "Situação",
      cell: (month) =>
        month.darfs
          .filter((d) => d.payable)
          .map((darf) => <PaidToggle key={darf.code} month={month} darf={darf} today={data.today} />),
    },
  ];

  return (
    <Panel
      title={`Meses de ${data.year}`}
      info="DARF 6015 (bolsa) e 4600 (cripto), com vencimento no último dia útil do mês seguinte. Abaixo de dez reais o DARF não se paga: o valor soma ao do mês seguinte. O prejuízo só compensa lucro do mesmo grupo (ações, ETF e BDR; day trade; FII; Fiagro) e passa de um mês e de um ano para o outro, sem prazo; cripto não compensa."
      infoLabel="Como o DARF e o prejuízo são calculados"
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
        {opening.length > 0 && (
          <>
            Prejuízo a compensar em 1º de janeiro: {opening.map((p) => `${POOL_LABEL[p.pool]} ${money.brl(p.loss_before)}`).join(" · ")}
            {" · "}
          </>
        )}
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
          <span className="tax-line tax-line--muted">vence em {isoDay(darf.due_date)}</span>
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

function PaidToggle({ month, darf, today }: { month: TaxMonth; darf: Darf; today: string }) {
  const change = useDarfPaid();
  const toast = useToast();
  const [lateOpen, setLateOpen] = useState(false);
  const name = monthName(month.month);
  const paid = darf.paid !== null;
  // Datas ISO comparam como texto. Pago, o atraso é história: sem calculadora.
  const overdue = !paid && darf.due_date < today;
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
      <StatusPill status={paid ? "ok" : overdue ? "bad" : "attention"}>{paid ? "Pago" : overdue ? "Em atraso" : "A pagar"}</StatusPill>
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
      {overdue && (
        <>
          <button
            type="button"
            className="link-btn tax-paid__button"
            onClick={() => setLateOpen(true)}
            aria-label={`Calcular multa e juros do DARF ${darf.code} de ${name}`}
          >
            Multa e juros
          </button>
          <LateChargeSheet month={month} darf={darf} today={today} open={lateOpen} onClose={() => setLateOpen(false)} />
        </>
      )}
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
      <WithholdingBlock month={month} />
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

/** Há retenção a mostrar: venda em bolsa, valor informado ou sobra que chegou. */
const hasWithholding = (month: TaxMonth) => {
  const w = month.withholding;
  return w.common_sales > 0 || w.day_trade_gains > 0 || w.informed !== null || w.carried_in > 0;
};

/**
 * O IR retido na fonte do mês: pela regra (0,005% das vendas comuns e 1% do
 * lucro do dia em day trade) ou o que a pessoa leu na nota. Dá para informar o
 * da nota e voltar para a regra.
 */
function WithholdingBlock({ month }: { month: TaxMonth }) {
  const money = useMoney();
  const inputId = useId();
  const toast = useToast();
  const change = useInformedWithholding();
  const [editing, setEditing] = useState(false);
  const w = month.withholding;
  const [value, setValue] = useState(w.informed ?? w.withheld);
  if (!hasWithholding(month)) return null;
  const name = monthName(month.month);
  const byRule = w.informed === null;
  const ruleParts = [
    ...(w.common_sales > 0
      ? [w.common_rule > 0 ? `0,005% de ${money.brl(w.common_sales)} vendidos` : `0,005% de ${money.brl(w.common_sales)} vendidos fica abaixo de 1 real e é dispensado`]
      : []),
    ...(w.day_trade_gains > 0 ? [`1% de ${money.brl(w.day_trade_gains)} de lucro em day trade`] : []),
  ];
  const save = (amount: number | null) =>
    change.mutate(
      { year: month.year, month: month.month, amount },
      {
        onSuccess: () => {
          setEditing(false);
          toast({
            tone: "success",
            message: amount === null ? `IR retido de ${name} voltou para a regra.` : `IR retido de ${name} salvo.`,
          });
        },
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );

  return (
    <div className="tax-withholding" role="group" aria-label={`IR retido na fonte em ${name}`}>
      <p className="tax-facts">
        IR retido na fonte: {money.brl(w.withheld)} {byRule ? "pela regra" : "informado da nota"}
        {byRule && ruleParts.length > 0 && <> ({ruleParts.join("; ")})</>}.
        {w.carried_in > 0 && <> Sobra de {money.brl(w.carried_in)} dos meses anteriores.</>}
        {w.used > 0 && <> Abateu {money.brl(w.used)} do imposto.</>}
        {w.carried_out > 0 && <> Passam {money.brl(w.carried_out)} para o mês seguinte.</>}
      </p>
      {month.notices.includes("withholding_by_rule") && (
        <p className="field__hint">Sem o valor da nota, o Prumo usou o que a corretora deve reter: confira na nota de corretagem.</p>
      )}
      {editing ? (
        <div className="tax-withholding__edit">
          <MoneyInput id={inputId} label={`IR retido em ${name}, como na nota`} value={value} onChange={setValue} focusOnMount />
          <MagneticButton onClick={() => save(value)} loading={change.isPending}>
            Salvar
          </MagneticButton>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => setEditing(false)}>
            Cancelar
          </MagneticButton>
        </div>
      ) : (
        <p className="tax-withholding__actions">
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              setValue(w.informed ?? w.withheld);
              setEditing(true);
            }}
          >
            {byRule ? "Informar o valor da nota" : "Trocar o valor da nota"}
          </button>
          {!byRule && (
            <button type="button" className="link-btn" onClick={() => save(null)} disabled={change.isPending}>
              Voltar para a regra
            </button>
          )}
        </p>
      )}
    </div>
  );
}
