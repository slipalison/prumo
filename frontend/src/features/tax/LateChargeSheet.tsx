import { useId, useState } from "react";
import { errorMessage } from "../../api/client";
import type { Darf, LateCharge, SelicMonth, TaxMonth } from "../../api/taxTypes";
import { Sheet } from "../../components/Sheet";
import { Skeleton } from "../../components/Skeleton";
import { useMoney } from "../../lib/privacy";
import { isoDay, monthName, periodName, rateText } from "./taxText";
import { useLateCharge } from "./useTax";

type LateChargeSheetProps = {
  month: TaxMonth;
  darf: Darf;
  /** Hoje em Brasília (`2026-11-27`): a data que abre. */
  today: string;
  open: boolean;
  onClose: () => void;
};

const selicText = (m: SelicMonth) => {
  const rate = m.rate === null ? "sem taxa" : `${m.rate.toLocaleString("pt-BR", { minimumFractionDigits: 2 })}%`;
  return `${monthName(m.month)} de ${m.year} (${rate})`;
};

/**
 * A calculadora do DARF pago depois do vencimento: a pessoa escolhe o dia do
 * pagamento e vê a multa (0,33% ao dia, até 20%) e os juros (a Selic de cada
 * mês mais 1%), com a conta à vista. A conta é do servidor; mês sem Selic não
 * vira estimativa.
 */
export function LateChargeSheet({ month, darf, today, open, onClose }: Readonly<LateChargeSheetProps>) {
  const dateId = useId();
  const [payOn, setPayOn] = useState(today);
  const late = useLateCharge(open ? { year: month.year, month: month.month, code: darf.code, payOn } : null);
  const period = periodName(month.year, month.month);

  return (
    <Sheet
      open={open}
      onClose={onClose}
      title={`DARF ${darf.code} de ${period} em atraso`}
      description={`Venceu em ${isoDay(darf.due_date)}. Escolha o dia do pagamento para ver a multa e os juros.`}
    >
      <div className="tax-late">
        <div className="field">
          <label htmlFor={dateId} className="field__label">
            Dia do pagamento
          </label>
          <input
            id={dateId}
            className="text-input"
            type="date"
            value={payOn}
            min={darf.due_date}
            onChange={(event) => setPayOn(event.target.value)}
            data-autofocus
          />
        </div>
        {late.isPending && late.fetchStatus !== "idle" && <Skeleton height={160} radius={16} />}
        {late.isError && (
          <p className="form-error" role="alert">
            {errorMessage(late.error)}
          </p>
        )}
        {late.data && <LateBreakdown charge={late.data} />}
      </div>
    </Sheet>
  );
}

function LateBreakdown({ charge }: Readonly<{ charge: LateCharge }>) {
  const money = useMoney();
  const rule = charge.rule;
  if (charge.days_late === 0) {
    return (
      <output className="tax-facts">
        Pago até {isoDay(charge.due_date)}, o DARF vai sem multa nem juros: {money.brl(charge.principal)}.
      </output>
    );
  }
  const interestParts = [
    ...charge.selic_months.map(selicText),
    ...(charge.payment_month_rate > 0 ? [`${rateText(charge.payment_month_rate)} no mês do pagamento`] : []),
  ];
  return (
    <>
      <dl className="tax-late__lines" aria-label="Conta do DARF em atraso">
        <div>
          <dt>Imposto (principal)</dt>
          <dd>{money.brl(charge.principal)}</dd>
        </div>
        <div>
          <dt>
            Multa de mora: {rateText(charge.penalty_rate)}
            <span className="tax-line tax-line--muted">
              {rateText(rule.daily_penalty.value)} × {charge.days_late} dias, até {rateText(rule.penalty_cap.value)}
            </span>
          </dt>
          <dd>{money.brl(charge.penalty)}</dd>
        </div>
        <div>
          <dt>
            Juros de mora: {charge.interest_rate === null ? "falta a Selic" : rateText(charge.interest_rate)}
            <span className="tax-line tax-line--muted">
              {interestParts.length > 0
                ? `Selic de ${interestParts.join(" + ")}`
                : "Pago no mês do vencimento: os juros ainda não começaram."}
            </span>
          </dt>
          <dd>{charge.interest === null ? <span className="tax-empty">–</span> : money.brl(charge.interest)}</dd>
        </div>
        <div className="tax-late__total">
          <dt>Total a pagar em {isoDay(charge.pay_date)}</dt>
          <dd>{charge.total === null ? <span className="tax-empty">–</span> : money.brl(charge.total)}</dd>
        </div>
      </dl>
      {charge.missing_selic.length > 0 && (
        <output className="tax-facts">
          Falta a Selic de {charge.missing_selic.map((m) => periodName(m.year, m.month)).join(", ")}: o mês ainda não fechou ou a
          taxa não saiu. Os juros e o total aparecem quando o Banco Central publicar.
        </output>
      )}
      <p className="field__hint">
        No DARF: código {charge.code}, período de apuração {String(charge.month).padStart(2, "0")}/{charge.year}, vencimento{" "}
        {isoDay(charge.due_date)}, com o principal, a multa e os juros em campos próprios. É uma estimativa: confira no Sicalc da
        Receita antes de pagar.
      </p>
      <p className="tax-source">
        Fonte: {rule.daily_penalty.source}; {rule.payment_month_interest.source}; {rule.selic_series.source}
      </p>
    </>
  );
}
