import { motion } from "motion/react";
import { useEffect, useId, useState } from "react";
import { errorMessage } from "../../api/client";
import { useDeleteIncomeGoal, useSaveIncomeGoal } from "../../api/incomeQueries";
import type { IncomeGoal } from "../../api/incomeTypes";
import { CountUp } from "../../components/CountUp";
import { GlassCard } from "../../components/GlassCard";
import { IconEdit, IconTarget } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { formatPercent } from "../../lib/format";
import { formatDay } from "../../lib/income";
import { useMoney } from "../../lib/privacy";
import { SPRING } from "../../lib/motion";

type IncomeGoalCardProps = {
  goal: IncomeGoal | null;
  /** Média mensal do que foi recebido nos 12 meses; vale sem meta definida. */
  monthlyAverage: number;
  /** Estimativa mensal da carteira (histórico por cota). */
  estimatedMonthly: number;
};

/**
 * A meta de renda mensal contra a renda de 12 meses. A barra usa o recebido
 * (média mensal); sem nada recebido ainda, usa a estimativa e diz isso.
 */
export function IncomeGoalCard({ goal, monthlyAverage, estimatedMonthly }: IncomeGoalCardProps) {
  const titleId = useId();
  const money = useMoney();
  const [editing, setEditing] = useState(false);
  const basis = goal?.basis ?? (monthlyAverage > 0 ? "received" : "estimate");
  const current = goal?.current_monthly ?? (monthlyAverage > 0 ? monthlyAverage : estimatedMonthly);
  const percent = goal?.percent ?? 0;
  const shown = Math.min(Math.max(percent, 0), 100);

  return (
    <GlassCard as="section" className="income-goal" aria-labelledby={titleId}>
      <header className="income-goal__head">
        <div>
          <p className="eyebrow" id={titleId}>
            Meta de renda mensal
          </p>
          <p className="income-goal__now">
            <CountUp value={current} className="income-goal__value" />
            <span className="income-goal__per"> por mês</span>
          </p>
          <p className="field__hint">
            {basis === "received"
              ? "Média do que você recebeu nos últimos 12 meses."
              : "Estimativa pelo que cada cota pagou nos últimos 12 meses. Ainda não há proventos recebidos lançados."}
          </p>
        </div>
        <MagneticButton variant="ghost" magnetic={false} onClick={() => setEditing(true)}>
          {goal ? <IconEdit /> : <IconTarget />}
          {goal ? "Editar meta" : "Definir meta"}
        </MagneticButton>
      </header>

      {goal ? (
        <>
          <div
            className="income-progress"
            role="progressbar"
            aria-label="Progresso da meta de renda mensal"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(shown)}
            aria-valuetext={`${money.brl(goal.current_monthly)} de ${money.brl(goal.target)} por mês, ${formatPercent(percent, 0)}${basis === "estimate" ? ", pela estimativa" : ""}`}
          >
            <motion.div
              className={basis === "estimate" ? "income-progress__fill income-progress__fill--estimate" : "income-progress__fill"}
              initial={{ width: "0%" }}
              animate={{ width: `${shown}%` }}
              transition={SPRING.value}
            />
          </div>
          <p className="income-goal__legend">
            <strong>{formatPercent(percent, 0)}</strong> de {money.brl(goal.target)} por mês
            {basis === "estimate" && <span className="income-tag"> estimativa</span>}
          </p>
          {goal.achieved_at && (
            <p className="income-goal__done" role="status">
              Meta atingida em {formatDay(goal.achieved_at)}.
            </p>
          )}
        </>
      ) : (
        <p className="field__hint">
          Quanto você quer receber de proventos por mês? Defina e acompanhe a barra encher a cada pagamento.
        </p>
      )}

      <GoalSheet open={editing} goal={goal} onClose={() => setEditing(false)} />
    </GlassCard>
  );
}

function GoalSheet({ open, goal, onClose }: { open: boolean; goal: IncomeGoal | null; onClose: () => void }) {
  const inputId = useId();
  const [target, setTarget] = useState(goal?.target ?? 0);
  const [removing, setRemoving] = useState(false);
  const save = useSaveIncomeGoal();
  const remove = useDeleteIncomeGoal();
  const toast = useToast();
  const money = useMoney();

  // Cada abertura parte do que está salvo, não do que ficou digitado na anterior.
  useEffect(() => {
    if (open) {
      setTarget(goal?.target ?? 0);
      setRemoving(false);
      save.reset();
      remove.reset();
    }
    // `save` e `remove` mudam de identidade a cada render; só a abertura importa.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, goal?.target]);

  const submit = () => {
    if (target <= 0 || save.isPending) return;
    save.mutate(target, {
      onSuccess: () => {
        toast({ tone: "success", message: "Meta de renda salva." });
        onClose();
      },
    });
  };

  const confirmRemove = () =>
    remove.mutate(undefined, {
      onSuccess: () => {
        toast({ tone: "success", message: "Meta de renda removida." });
        setRemoving(false);
        onClose();
      },
    });

  const error = save.error ?? remove.error;

  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Meta de renda mensal"
      description="Quanto você quer receber de proventos, em média, por mês."
    >
      <form
        className="asset-form"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <MoneyInput id={inputId} label="Meta por mês" value={target} onChange={setTarget} focusOnMount={!money.hidden} onEnter={submit} />
        {error && (
          <p className="form-error" role="alert">
            {errorMessage(error)}
          </p>
        )}
        <MagneticButton type="submit" disabled={target <= 0} loading={save.isPending}>
          Salvar meta
        </MagneticButton>
        {goal &&
          (removing ? (
            <div className="income-confirm" role="group" aria-label="Confirmar remoção da meta">
              <span>Remover a meta de renda?</span>
              <MagneticButton variant="danger" magnetic={false} loading={remove.isPending} onClick={confirmRemove}>
                Remover
              </MagneticButton>
              <MagneticButton variant="ghost" magnetic={false} onClick={() => setRemoving(false)}>
                Manter
              </MagneticButton>
            </div>
          ) : (
            <button type="button" className="link-btn income-remove" onClick={() => setRemoving(true)}>
              Remover meta
            </button>
          ))}
      </form>
    </Sheet>
  );
}
