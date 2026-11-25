import { useId, useState } from "react";
import { errorMessage } from "../../api/client";
import type {
  Balance,
  CapitalGain,
  Carryforward,
  CarryforwardValues,
  Darf,
  Doubt,
  Irpf,
  IrpfAsset,
  IrpfIncome,
  Pool,
  Skipped,
  TaxMonth,
  TaxMonths,
} from "../../api/taxTypes";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert } from "../../components/icons";
import { LimitBar } from "../../components/LimitBar";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import {
  DISCLAIMER,
  DOUBT_TEXT,
  INSTRUMENT_LABEL,
  irpfLabel,
  isoDay,
  monthName,
  NEVER_CALCULATED,
  POOL_LABEL,
  rateText,
  SHORT_INSTRUMENT,
  SKIP_TEXT,
} from "./taxText";
import { useCarryforward, useDarfPaid, useIrpf, useSaveCarryforward, useTaxMonths } from "./useTax";
import "./tax.css";

/** O ano que abre: o de hoje, se tem regra; senão o último com regra. */
function initialYear(covered: number[], today: Date): number | null {
  if (covered.length === 0) return null;
  const year = today.getFullYear();
  return covered.includes(year) ? year : Math.max(...covered.filter((y) => y <= year), Math.min(...covered));
}

/**
 * Impostos: o isentômetro do mês, o DARF de cada mês com vencimento e a marca
 * de pago, a declaração do ano para copiar, o que ficou fora da conta e o
 * saldo de prejuízo que a pessoa traz de antes. A conta é do servidor
 * (docs/ir.md); a tela só mostra, e o aviso de responsabilidade fica sempre à
 * vista.
 */
export function TaxScreen() {
  const carry = useCarryforward();
  const covered = carry.data?.covered_years ?? [];
  const [chosen, setChosen] = useState<number | null>(null);
  const year = chosen ?? initialYear(covered, new Date());
  const months = useTaxMonths(year);
  const irpf = useIrpf(year);

  const skipped = mergeSkipped(months.data?.skipped ?? [], irpf.data?.skipped ?? []);

  return (
    <div className="screen tax">
      <header className="screen__header">
        <p className="eyebrow">Impostos</p>
        <h1 className="screen__title">Imposto de renda</h1>
        <p className="screen__lead">
          Isentômetro, DARF de cada mês e o que vai para a declaração, a partir do seu livro de lançamentos e dos
          proventos.
        </p>
      </header>

      <aside className="tax-disclaimer" role="note" aria-label="Aviso de responsabilidade">
        <IconAlert width={18} height={18} className="tax-disclaimer__icon" />
        <p>{DISCLAIMER}</p>
      </aside>

      {carry.isError && (
        <GlassCard as="section" role="alert" className="state-card">
          <p className="state-card__text">{errorMessage(carry.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void carry.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}

      {covered.length > 0 && (
        <div className="chips" role="group" aria-label="Ano-calendário">
          {covered.map((option) => (
            <button
              key={option}
              type="button"
              className={cx("chip chip--quick", option === year && "is-active")}
              aria-pressed={option === year}
              onClick={() => setChosen(option)}
            >
              {option}
            </button>
          ))}
        </div>
      )}

      {(carry.isPending || months.isPending) && !carry.isError && (
        <div className="tax__loading" role="status" aria-label="Carregando os impostos">
          <Skeleton height={120} radius={22} />
          <Skeleton height={260} radius={22} />
        </div>
      )}
      {months.isError && (
        <GlassCard as="section" role="alert" className="state-card">
          <h2 className="state-card__title">Não consegui apurar {year}</h2>
          <p className="state-card__text">{errorMessage(months.error)}</p>
        </GlassCard>
      )}
      {months.data && (
        <>
          <ExemptionCard data={months.data} />
          <MonthsCard data={months.data} />
        </>
      )}
      {irpf.data && <IrpfCard irpf={irpf.data} />}
      {(months.data || irpf.data) && <SkippedCard skipped={skipped} />}
      {carry.data && <CarryforwardCard carry={carry.data} />}
    </div>
  );
}

function mergeSkipped(...lists: Skipped[][]): Skipped[] {
  const seen = new Set<string>();
  const out: Skipped[] = [];
  for (const item of lists.flat()) {
    const key = `${item.subject}|${item.date ?? ""}|${item.reason}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  return out;
}

function DoubtNote({ doubt }: { doubt: Doubt }) {
  return (
    <p className="tax-doubt">
      <strong>Ponto a confirmar com contador:</strong> {DOUBT_TEXT[doubt]}
    </p>
  );
}

// ------------------------------------------------------------ isentômetro --

/** O mês de hoje, se o ano é o de hoje; senão dezembro. */
function currentMonth(data: TaxMonths): TaxMonth | undefined {
  const [todayYear, todayMonth] = data.today.split("-").map(Number);
  const month = todayYear === data.year ? todayMonth : 12;
  return data.months.find((m) => m.month === month);
}

function ExemptionCard({ data }: { data: TaxMonths }) {
  const money = useMoney();
  const month = currentMonth(data);
  const stocks = month?.exemptions.find((e) => e.id === "stock_sales");
  const crypto = month?.exemptions.find((e) => e.id === "crypto_sales");
  if (!month || !stocks) return null;
  const name = monthName(month.month);
  const percent = stocks.limit > 0 ? (stocks.sales / stocks.limit) * 100 : 0;
  const cryptoSales = crypto?.sales ?? 0;

  return (
    <GlassCard as="section" className="tax-card tax-meter" aria-label="Isentômetro">
      <p className="eyebrow">
        Isentômetro · {name} de {month.year}
      </p>
      <p className="tax-meter__value">
        <strong>{money.brl(stocks.sales)}</strong> em vendas de ações, de {money.brl(stocks.limit)} isentos
      </p>
      <LimitBar
        current={percent}
        limit={100}
        label={`Vendas de ações em ${name}: ${formatPercent(percent)} do limite de isenção`}
      />
      <p className="tax-meter__percent">{formatPercent(percent)} do limite</p>
      {stocks.exempt ? (
        <p>
          Dentro do limite: o lucro das ações em operação comum neste mês é isento. Ainda cabem{" "}
          {money.brl(stocks.remaining)} em vendas.
        </p>
      ) : (
        <p className="tax-meter__over">Passou do limite: o lucro das ações neste mês paga 15%.</p>
      )}
      <p className="field__hint">
        Só a venda de ações e units soma aqui. ETF, BDR e FII não têm essa isenção.
        {cryptoSales > 0 && crypto && (
          <>
            {" "}
            Cripto tem a própria: {money.brl(cryptoSales)} vendidos de {money.brl(crypto.limit)} no mês.
          </>
        )}
      </p>
      {month.doubts.includes("day_trade_in_stock_exemption") && <DoubtNote doubt="day_trade_in_stock_exemption" />}
    </GlassCard>
  );
}

// ------------------------------------------------------------------ meses --

const stockSales = (month: TaxMonth) => {
  const line = month.sales.find((s) => s.instrument === "stock");
  return line ? line.common_sales + line.day_trade_sales : 0;
};

const monthTax = (month: TaxMonth) =>
  month.pools.reduce((sum, pool) => sum + pool.tax, 0) + month.capital_gain.tax;

function hasActivity(month: TaxMonth): boolean {
  return (
    month.sales.some((s) => s.common_sales > 0 || s.day_trade_sales > 0) ||
    month.pools.some((p) => p.result !== 0) ||
    month.darfs.some((d) => d.total > 0) ||
    month.doubts.length > 0
  );
}

function MonthsCard({ data }: { data: TaxMonths }) {
  const money = useMoney();
  const titleId = useId();
  const active = data.months.filter(hasActivity);

  return (
    <GlassCard as="section" className="tax-card" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        Meses de {data.year}
      </h2>
      <p className="field__hint">
        DARF 6015 (bolsa) e 4600 (cripto), com vencimento no último dia útil do mês seguinte. Abaixo de dez reais o
        DARF não se paga: o valor soma ao do mês seguinte.
      </p>
      <div className="tax-table-wrap" role="region" aria-label={`DARF por mês em ${data.year}`}>
        <table className="tax-table">
          <thead>
            <tr>
              <th scope="col">Mês</th>
              <th scope="col">Vendas de ações</th>
              <th scope="col">Imposto do mês</th>
              <th scope="col">DARF</th>
              <th scope="col">Vencimento</th>
              <th scope="col">Situação</th>
            </tr>
          </thead>
          <tbody>
            {data.months.map((month) => (
              <MonthRow key={month.month} month={month} />
            ))}
          </tbody>
        </table>
      </div>
      <p className="tax-closing">
        Prejuízo a compensar em 31/12: <BalanceText balance={data.closing} empty="nenhum" />
        {(data.closing.pending_darf.exchange_net_gain ?? 0) + (data.closing.pending_darf.capital_gain ?? 0) > 0 && (
          <>
            {" "}
            · DARF abaixo do mínimo ainda não pago:{" "}
            {money.brl(
              (data.closing.pending_darf.exchange_net_gain ?? 0) + (data.closing.pending_darf.capital_gain ?? 0),
            )}
          </>
        )}
      </p>
      {active.length > 0 && (
        <div className="tax-details">
          <h3 className="tax-subtitle">Como cada mês foi apurado</h3>
          {active.map((month) => (
            <MonthDetails key={month.month} month={month} />
          ))}
        </div>
      )}
    </GlassCard>
  );
}

function MonthRow({ month }: { month: TaxMonth }) {
  const money = useMoney();
  const sales = stockSales(month);
  const tax = monthTax(month);
  const payable = month.darfs.filter((d) => d.payable);
  const pending = month.darfs.filter((d) => !d.payable && d.pending_after > 0);
  const name = monthName(month.month);

  return (
    <tr>
      <th scope="row">{name}</th>
      <td className={sales === 0 ? "is-empty" : undefined}>{sales === 0 ? "–" : money.brl(sales)}</td>
      <td className={tax === 0 ? "is-empty" : undefined}>{tax === 0 ? "–" : money.brl(tax)}</td>
      <td>
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
        {payable.length === 0 && pending.length === 0 && <span className="is-empty">–</span>}
      </td>
      <td>
        {payable.map((darf) => (
          <span key={darf.code} className="tax-line">
            {isoDay(darf.due_date)}
          </span>
        ))}
      </td>
      <td>
        {payable.map((darf) => (
          <PaidToggle key={darf.code} month={month} darf={darf} />
        ))}
      </td>
    </tr>
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
    <span className="tax-line tax-paid">
      <span className={cx("tax-status", paid ? "tax-status--paid" : "tax-status--due")}>
        {paid ? "Pago" : "A pagar"}
      </span>
      {darf.paid && !darf.paid.same_amount && (
        <span className="tax-line--muted">o valor mudou depois de marcar: confira</span>
      )}
      <button
        type="button"
        className="link-btn"
        onClick={toggle}
        disabled={change.isPending}
        aria-label={paid ? `Desmarcar o DARF ${darf.code} de ${name}` : `Marcar o DARF ${darf.code} de ${name} como pago`}
      >
        {paid ? "Desmarcar" : "Marcar como pago"}
      </button>
    </span>
  );
}

function BalanceText({ balance, empty }: { balance: Balance; empty: string }) {
  const money = useMoney();
  const parts = (Object.entries(balance.losses) as Array<[Pool, number]>).filter(([, value]) => value > 0);
  if (parts.length === 0) return <>{empty}</>;
  return <>{parts.map(([pool, value]) => `${POOL_LABEL[pool]} ${money.brl(value)}`).join(" · ")}</>;
}

function MonthDetails({ month }: { month: TaxMonth }) {
  const money = useMoney();
  const name = monthName(month.month);
  const pools = month.pools.filter((p) => p.result !== 0 || p.loss_before !== 0);
  const sales = month.sales.filter((s) => s.common_sales > 0 || s.day_trade_sales > 0);

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
        <div className="tax-table-wrap" role="region" aria-label={`Apuração de ${name}`}>
          <table className="tax-table tax-table--compact">
            <thead>
              <tr>
                <th scope="col">Grupo</th>
                <th scope="col">Resultado</th>
                <th scope="col">Prejuízo antes</th>
                <th scope="col">Compensado</th>
                <th scope="col">Base</th>
                <th scope="col">Alíquota</th>
                <th scope="col">Imposto</th>
                <th scope="col">Prejuízo que fica</th>
              </tr>
            </thead>
            <tbody>
              {pools.map((p) => (
                <tr key={p.pool}>
                  <th scope="row">{POOL_LABEL[p.pool]}</th>
                  <td>{money.brl(p.result)}</td>
                  <td>{money.brl(p.loss_before)}</td>
                  <td>{money.brl(p.loss_used)}</td>
                  <td>{money.brl(p.base)}</td>
                  <td>{rateText(p.rate)}</td>
                  <td>{money.brl(p.tax)}</td>
                  <td>{money.brl(p.loss_after)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <CryptoLine gain={month.capital_gain} name={name} />
      {month.exemptions.some((e) => e.id === "stock_sales" && e.exempt_gain > 0) && (
        <p className="tax-facts">
          Lucro isento das ações no mês:{" "}
          {money.brl(month.exemptions.find((e) => e.id === "stock_sales")?.exempt_gain ?? 0)}
        </p>
      )}
      {month.doubts.map((doubt) => (
        <DoubtNote key={doubt} doubt={doubt} />
      ))}
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

// ------------------------------------------------------------------- IRPF --

function IrpfCard({ irpf }: { irpf: Irpf }) {
  const money = useMoney();
  const titleId = useId();
  const pending = (irpf.year_end.pending_darf.exchange_net_gain ?? 0) + (irpf.year_end.pending_darf.capital_gain ?? 0);

  return (
    <GlassCard as="section" className="tax-card" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        Declaração de {irpf.year}
      </h2>
      <p className="field__hint">
        O ano-calendário {irpf.year}, na declaração entregue em {irpf.year + 1}. Copie cada linha para o programa da
        Receita.
      </p>

      <h3 className="tax-subtitle">Bens e Direitos (situação em 31/12)</h3>
      {irpf.doubts.includes("irpf_codes") && <DoubtNote doubt="irpf_codes" />}
      {irpf.assets.length === 0 ? (
        <p className="field__hint">Nenhuma posição em 31/12 dentro do IR do Prumo.</p>
      ) : (
        <ul className="tax-irpf" aria-label="Bens e Direitos">
          {irpf.assets.map((asset) => (
            <AssetRow key={asset.symbol} asset={asset} />
          ))}
        </ul>
      )}

      <h3 className="tax-subtitle">Rendimentos isentos e não tributáveis</h3>
      <IncomeList lines={irpf.exempt_income} label="Rendimentos isentos" />
      {irpf.doubts.includes("dividend_withholding") && <DoubtNote doubt="dividend_withholding" />}

      <h3 className="tax-subtitle">Rendimentos sujeitos à tributação exclusiva</h3>
      <IncomeList lines={irpf.exclusive_income} label="Tributação exclusiva" net />
      {irpf.doubts.includes("interest_on_equity_net") && <DoubtNote doubt="interest_on_equity_net" />}

      <h3 className="tax-subtitle">Renda variável</h3>
      <p className="tax-facts">
        DARF do ano: {money.brl(irpf.darf_total)}. Prejuízo que passa para {irpf.year + 1}:{" "}
        <BalanceText balance={irpf.year_end} empty="nenhum" />.
        {pending > 0 && <> DARF abaixo do mínimo que fica para janeiro: {money.brl(pending)}.</>}
        {irpf.withholding_left > 0 && <> IR retido que sobrou: {money.brl(irpf.withholding_left)}.</>}
      </p>
      {irpf.doubts.includes("withholding_leftover") && <DoubtNote doubt="withholding_leftover" />}
    </GlassCard>
  );
}

function assetText(asset: IrpfAsset, quantity: string, cost: string, average: string): string {
  return `${asset.symbol} (${INSTRUMENT_LABEL[asset.instrument]}): ${quantity} unidades. Grupo ${asset.group}, código ${asset.code}. Situação em 31/12: ${cost} (custo de aquisição, preço médio ${average}).`;
}

function AssetRow({ asset }: { asset: IrpfAsset }) {
  const money = useMoney();
  const toast = useToast();
  const fractional = asset.instrument === "crypto";
  const quantity = money.quantity(asset.quantity, fractional);
  const copy = async () => {
    // O que vai para a área de transferência é o valor de verdade: copiar é
    // o que a pessoa pediu, e a tela continua mascarada.
    const real = assetText(
      asset,
      asset.quantity.toLocaleString("pt-BR", { maximumFractionDigits: 8 }),
      asset.total_cost.toLocaleString("pt-BR", { style: "currency", currency: "BRL" }),
      asset.average_price.toLocaleString("pt-BR", { style: "currency", currency: "BRL", maximumFractionDigits: 8 }),
    );
    try {
      await navigator.clipboard.writeText(real);
      toast({ tone: "success", message: `Linha de ${asset.symbol} copiada.` });
    } catch {
      toast({ tone: "error", message: "Não consegui copiar. Selecione o texto e copie à mão." });
    }
  };

  return (
    <li className="tax-irpf__item">
      <div className="tax-irpf__head">
        <strong>{asset.symbol}</strong>
        <span className="field__hint">
          Grupo {asset.group} · Código {asset.code} · {irpfLabel(asset.group, asset.code, asset.label)}
        </span>
      </div>
      <p className="tax-irpf__text">
        {quantity} unidades · situação em 31/12: {money.brl(asset.total_cost)} · preço médio{" "}
        {money.brl(asset.average_price)}
      </p>
      <button type="button" className="link-btn" onClick={() => void copy()} aria-label={`Copiar a linha de ${asset.symbol}`}>
        Copiar
      </button>
    </li>
  );
}

function IncomeList({ lines, label, net = false }: { lines: IrpfIncome[]; label: string; net?: boolean }) {
  const money = useMoney();
  return (
    <ul className="tax-irpf" aria-label={label}>
      {lines.map((line) => (
        <li key={`${line.group}-${line.code}`} className="tax-irpf__item">
          <div className="tax-irpf__head">
            <strong>Código {line.code}</strong>
            <span className="field__hint">{irpfLabel(line.group, line.code, line.label)}</span>
          </div>
          <p className="tax-irpf__text">
            {net ? (
              <>
                Líquido {money.brl(line.net)} (bruto {money.brl(line.gross)}, retido {money.brl(line.withheld)})
              </>
            ) : (
              money.brl(line.net)
            )}
          </p>
          {line.by_payer.length > 0 && (
            <p className="field__hint">
              {line.by_payer.map((payer) => `${payer.symbol} ${money.brl(payer.gross - payer.withheld)}`).join(" · ")}
            </p>
          )}
        </li>
      ))}
    </ul>
  );
}

// --------------------------------------------------------- não calculado --

function SkippedCard({ skipped }: { skipped: Skipped[] }) {
  const money = useMoney();
  const titleId = useId();
  return (
    <GlassCard as="section" className="tax-card" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        O que o Prumo não calculou
      </h2>
      {skipped.length === 0 ? (
        <p className="field__hint">Nada do seu livro ficou de fora da conta.</p>
      ) : (
        <ul className="tax-skipped" aria-label="Fora da conta">
          {skipped.map((item) => (
            <li key={`${item.subject}-${item.date ?? ""}-${item.reason}`}>
              <strong>{item.subject}</strong>
              {item.date && <> · {isoDay(item.date)}</>}
              {item.amount !== null && <> · {money.brl(item.amount)}</>}
              <span className="field__hint">{SKIP_TEXT[item.reason]}</span>
            </li>
          ))}
        </ul>
      )}
      <h3 className="tax-subtitle">Nunca entra na conta</h3>
      <ul className="tax-never">
        {NEVER_CALCULATED.map((text) => (
          <li key={text}>{text}</li>
        ))}
      </ul>
    </GlassCard>
  );
}

// ------------------------------------------------------- saldo anterior --

const CARRY_FIELDS: Array<{ key: keyof CarryforwardValues; label: string }> = [
  { key: "common_loss", label: "Prejuízo em ações, ETF e BDR" },
  { key: "day_trade_loss", label: "Prejuízo em day trade" },
  { key: "real_estate_fund_loss", label: "Prejuízo em FII" },
  { key: "agribusiness_fund_loss", label: "Prejuízo em Fiagro" },
  { key: "pending_exchange_darf", label: "DARF 6015 abaixo do mínimo, não pago" },
  { key: "pending_capital_gain_darf", label: "DARF 4600 abaixo do mínimo, não pago" },
];

function valuesOf(carry: Carryforward): CarryforwardValues {
  return {
    common_loss: carry.common_loss,
    day_trade_loss: carry.day_trade_loss,
    real_estate_fund_loss: carry.real_estate_fund_loss,
    agribusiness_fund_loss: carry.agribusiness_fund_loss,
    pending_exchange_darf: carry.pending_exchange_darf,
    pending_capital_gain_darf: carry.pending_capital_gain_darf,
  };
}

function CarryforwardCard({ carry }: { carry: Carryforward }) {
  const money = useMoney();
  const titleId = useId();
  const baseId = useId();
  const [editing, setEditing] = useState(false);
  const [year, setYear] = useState(carry.year);
  const [values, setValues] = useState<CarryforwardValues>(() => valuesOf(carry));
  const save = useSaveCarryforward();
  const toast = useToast();
  const filled = CARRY_FIELDS.filter((field) => carry[field.key] > 0);

  const open = () => {
    setYear(carry.year);
    setValues(valuesOf(carry));
    setEditing(true);
  };
  const submit = () =>
    save.mutate(
      { year, ...values },
      {
        onSuccess: () => {
          toast({ tone: "success", message: "Saldo de antes do Prumo salvo." });
          setEditing(false);
        },
      },
    );

  return (
    <GlassCard as="section" className="tax-card" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        Saldo de antes do Prumo
      </h2>
      <p className="field__hint">
        O prejuízo a compensar e o DARF abaixo do mínimo ainda não pago, como estavam em 1º de janeiro do primeiro ano
        apurado. Estão na sua última declaração (Renda Variável) ou no controle do contador.
      </p>
      {!editing ? (
        <>
          {carry.informed && filled.length > 0 ? (
            <ul className="tax-facts" aria-label={`Saldo em 1º de janeiro de ${carry.year}`}>
              {filled.map((field) => (
                <li key={field.key}>
                  {field.label}: {money.brl(carry[field.key])}
                </li>
              ))}
            </ul>
          ) : (
            <p className="tax-facts">
              {carry.informed ? "Saldo informado: nada a compensar" : "Nada informado: a conta começa do zero"} em 1º de
              janeiro de {carry.year}.
            </p>
          )}
          <MagneticButton variant="ghost" magnetic={false} onClick={open}>
            {carry.informed ? "Editar saldo" : "Informar saldo"}
          </MagneticButton>
        </>
      ) : (
        <div className="tax-carry">
          <div className="chips" role="radiogroup" aria-label="O saldo vale em 1º de janeiro de">
            {carry.covered_years.map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={option === year}
                className={cx("chip chip--quick", option === year && "is-active")}
                onClick={() => setYear(option)}
              >
                1º/1/{option}
              </button>
            ))}
          </div>
          {CARRY_FIELDS.map((field) => (
            <MoneyInput
              key={field.key}
              id={`${baseId}-${field.key}`}
              label={field.label}
              value={values[field.key]}
              onChange={(value) => setValues((current) => ({ ...current, [field.key]: value }))}
            />
          ))}
          {save.isError && (
            <p className="form-error" role="alert">
              {errorMessage(save.error)}
            </p>
          )}
          <div className="tax-carry__actions">
            <MagneticButton onClick={submit} loading={save.isPending}>
              Salvar saldo
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setEditing(false)}>
              Cancelar
            </MagneticButton>
          </div>
        </div>
      )}
    </GlassCard>
  );
}
