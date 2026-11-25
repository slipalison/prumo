import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useDeleteIncome } from "../../api/incomeQueries";
import type { IncomeEvent } from "../../api/incomeTypes";
import { IconTrash } from "../../components/icons";
import { useToast } from "../../components/toastContext";
import { cx } from "../../lib/cx";
import { formatIsoDate, formatIsoDay, KIND_LABEL, STATUS_LABEL } from "../../lib/income";
import { useMoney } from "../../lib/privacy";

const payer = (event: IncomeEvent) => event.symbol ?? event.display_name;

/** Os próximos pagamentos, do mais perto para o mais longe. */
export function UpcomingList({ events }: { events: IncomeEvent[] }) {
  const money = useMoney();
  if (events.length === 0) {
    return <p className="field__hint">Nenhum pagamento a receber lançado. Lance os proventos anunciados para vê-los aqui.</p>;
  }
  return (
    <ol className="income-list">
      {events.map((event, index) => (
        <motion.li
          key={event.id}
          className="income-row"
          initial={{ opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ delay: Math.min(index, 8) * 0.04 }}
        >
          <span className="income-row__date">{formatIsoDay(event.pay_date)}</span>
          <span className="income-row__who">
            <strong>{payer(event)}</strong>
            <span className="muted"> · {KIND_LABEL[event.kind]}</span>
          </span>
          <strong className="income-row__value">{money.brl(event.net)}</strong>
        </motion.li>
      ))}
    </ol>
  );
}

/** Os proventos lançados, do mais novo para o mais velho; o lançado à mão se apaga. */
export function EventList({ events }: { events: IncomeEvent[] }) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const remove = useDeleteIncome();
  const money = useMoney();
  const toast = useToast();

  if (events.length === 0) {
    return <p className="field__hint">Nenhum provento lançado ainda.</p>;
  }

  const confirmRemove = (event: IncomeEvent) =>
    remove.mutate(event.id, {
      onSuccess: () => {
        setConfirming(null);
        toast({ tone: "success", message: `Provento de ${payer(event)} apagado.` });
      },
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });

  return (
    <ul className="income-list">
      {events.map((event) => {
        const asking = confirming === event.id;
        return (
          <li key={event.id} className="income-row income-row--event">
            <span className="income-row__date">{formatIsoDate(event.pay_date)}</span>
            <span className="income-row__who">
              <strong>{payer(event)}</strong>
              <span className="muted">
                {" "}
                · {KIND_LABEL[event.kind]}
                {event.withholding > 0 && ` · IR ${money.brl(event.withholding)}`}
              </span>
            </span>
            <span className={cx("income-tag", event.status === "upcoming" && "income-tag--upcoming")}>
              {STATUS_LABEL[event.status]}
            </span>
            <strong className="income-row__value">{money.brl(event.net)}</strong>
            {asking ? (
              <span className="income-row__confirm" role="group" aria-label={`Apagar provento de ${payer(event)}?`}>
                <button type="button" className="link-btn" onClick={() => confirmRemove(event)}>
                  Apagar
                </button>
                <button type="button" className="link-btn" onClick={() => setConfirming(null)}>
                  Manter
                </button>
              </span>
            ) : (
              <button
                type="button"
                className="icon-btn icon-btn--sm icon-btn--danger"
                aria-label={`Apagar provento de ${payer(event)} de ${formatIsoDate(event.pay_date)}`}
                onClick={() => setConfirming(event.id)}
              >
                <IconTrash />
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
