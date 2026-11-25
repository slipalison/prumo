import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useDeleteObjective, useObjectives } from "../../api/objectivesQueries";
import type { Objective, ObjectivesResponse } from "../../api/objectivesTypes";
import { GlassCard } from "../../components/GlassCard";
import { IconCheck, IconEdit, IconPlus, IconTrash } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { cx } from "../../lib/cx";
import { formatDay, formatPercent } from "../../lib/format";
import { assumptionLines, objectiveName, projectionText } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";
import { ObjectiveSheet } from "./ObjectiveSheet";

/** Ajustes › Objetivos: metas de patrimônio e de valor numa classe, com data estimada. */
export function ObjectivesScreen() {
  const objectives = useObjectives();
  const [sheet, setSheet] = useState<{ objective: Objective | null } | null>(null);

  return (
    <div className="screen objectives">
      <header className="screen__header">
        <p className="eyebrow">Objetivos</p>
        <h1 className="screen__title">Para onde a carteira vai?</h1>
        <p className="screen__lead">
          Metas com prazo: um patrimônio total ou um valor numa classe. A data estimada vem de uma conta simples, mês a
          mês, e as duas premissas dela (aporte e rentabilidade) ficam sempre à vista. É uma estimativa, não uma
          promessa. A meta de renda mensal fica em <a href="#/renda">Renda</a>.
        </p>
      </header>

      <div className="objectives__actions">
        <MagneticButton onClick={() => setSheet({ objective: null })}>
          <IconPlus /> Novo objetivo
        </MagneticButton>
      </div>

      {objectives.isPending && <Skeleton height={180} radius={22} />}
      {objectives.isError && (
        <GlassCard as="section" className="state-card" role="alert">
          <h2 className="state-card__title">Não consegui carregar os objetivos</h2>
          <p className="state-card__text">{errorMessage(objectives.error)}</p>
          <MagneticButton onClick={() => void objectives.refetch()}>Tentar de novo</MagneticButton>
        </GlassCard>
      )}
      {objectives.data && <Loaded data={objectives.data} onEdit={(objective) => setSheet({ objective })} />}

      <ObjectiveSheet
        open={sheet !== null}
        onClose={() => setSheet(null)}
        objective={sheet?.objective ?? null}
        defaults={objectives.data?.defaults ?? { annual_return_percent: 0, return_source: "fallback" }}
      />
    </div>
  );
}

function Loaded({ data, onEdit }: { data: ObjectivesResponse; onEdit: (objective: Objective) => void }) {
  const active = data.objectives.filter((o) => o.status === "active");
  const achieved = data.objectives.filter((o) => o.status === "achieved");

  if (data.objectives.length === 0) {
    return (
      <GlassCard as="section" className="state-card">
        <h2 className="state-card__title">Nenhum objetivo ainda</h2>
        <p className="state-card__text">
          Defina, por exemplo, quanto você quer ter no total ou em Fundos Imobiliários. O Aportar passa a dizer quanto
          cada aporte aproxima você da meta.
        </p>
      </GlassCard>
    );
  }

  return (
    <>
      {active.length > 0 && (
        <section aria-labelledby="objectives-active">
          <h2 id="objectives-active" className="section-title">
            Em andamento
          </h2>
          <ul className="objective-list">
            {active.map((objective) => (
              <ObjectiveCard key={objective.id} objective={objective} onEdit={onEdit} />
            ))}
          </ul>
        </section>
      )}
      {achieved.length > 0 && (
        <section aria-labelledby="objectives-achieved">
          <h2 id="objectives-achieved" className="section-title">
            Concluídos
          </h2>
          <ul className="objective-list">
            {achieved.map((objective) => (
              <ObjectiveCard key={objective.id} objective={objective} onEdit={onEdit} />
            ))}
          </ul>
        </section>
      )}
    </>
  );
}

function ObjectiveCard({ objective, onEdit }: { objective: Objective; onEdit: (objective: Objective) => void }) {
  const money = useMoney();
  const toast = useToast();
  const remove = useDeleteObjective();
  const [asking, setAsking] = useState(false);
  const achieved = objective.status === "achieved";
  const name = objectiveName(objective);
  const lines = objective.assumptions ? assumptionLines(objective.assumptions, money) : null;
  const shown = Math.min(Math.max(objective.percent, 0), 100);

  const confirmRemove = () =>
    remove.mutate(objective.id, {
      onSuccess: () => toast({ tone: "success", message: "Objetivo removido." }),
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });

  return (
    <li>
      <GlassCard as="article" className={cx("objective-card", achieved && "objective-card--achieved")} aria-label={name}>
        <header className="objective-card__head">
          <div>
            <h3 className="objective-card__name">{name}</h3>
            <p className="objective-card__target">
              Meta de <strong>{money.brl(objective.target)}</strong>
            </p>
          </div>
          <div className="objective-card__actions">
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              aria-label={`Editar objetivo: ${name}`}
              onClick={() => onEdit(objective)}
            >
              <IconEdit />
            </button>
            <button
              type="button"
              className="icon-btn icon-btn--sm icon-btn--danger"
              aria-label={`Remover objetivo: ${name}`}
              onClick={() => setAsking(true)}
            >
              <IconTrash />
            </button>
          </div>
        </header>

        <div
          className="objective-progress"
          role="progressbar"
          aria-label={`Progresso: ${name}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(shown)}
          aria-valuetext={`${money.brl(objective.current)} de ${money.brl(objective.target)}, ${formatPercent(objective.percent, 1)}`}
        >
          <motion.div
            className="objective-progress__fill"
            initial={{ width: "0%" }}
            animate={{ width: `${shown}%` }}
            transition={{ type: "spring", stiffness: 80, damping: 18 }}
          />
        </div>
        <p className="objective-card__legend">
          <strong>{formatPercent(objective.percent, 1)}</strong> · {money.brl(objective.current)} de {money.brl(objective.target)}
        </p>

        {achieved ? (
          <p className="objective-card__done" role="status">
            <IconCheck width={18} height={18} /> Concluído
            {objective.achieved_at ? ` em ${formatDay(objective.achieved_at)}` : ""}.
          </p>
        ) : (
          <>
            <p className="objective-card__eta">{projectionText(objective.projection)}</p>
            {lines && (
              <ul className="objective-card__premises" aria-label="Premissas da conta">
                <li>{lines.contribution}</li>
                <li>{lines.rate}</li>
              </ul>
            )}
          </>
        )}

        {asking && (
          <div className="income-confirm" role="group" aria-label={`Confirmar remoção: ${name}`}>
            <span>Remover este objetivo?</span>
            <MagneticButton variant="danger" magnetic={false} loading={remove.isPending} onClick={confirmRemove}>
              Remover
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setAsking(false)}>
              Manter
            </MagneticButton>
          </div>
        )}
      </GlassCard>
    </li>
  );
}
