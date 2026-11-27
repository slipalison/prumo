import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useDeleteObjective, useObjectives } from "../../api/objectivesQueries";
import type { Objective, ObjectivesDefaults, ObjectivesResponse } from "../../api/objectivesTypes";
import { GlassCard } from "../../components/GlassCard";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader } from "../../components/Layout";
import { IconCheck, IconEdit, IconPlus, IconTrash } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { cx } from "../../lib/cx";
import { formatDay, formatPercent } from "../../lib/format";
import { baseText, objectiveName, premiseRows, rangeText, shortDate, targetText } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";
import { SPRING } from "../../lib/motion";
import { ObjectiveSheet } from "./ObjectiveSheet";
import { ProjectionChart, type CurveBasis } from "./ProjectionChart";

const FALLBACK_DEFAULTS: ObjectivesDefaults = {
  real_return_percent: 0,
  return_source: "fallback",
  inflation_percent: 0,
  inflation_source: "target",
  inflation_years: null,
  reinvest_income: true,
  income_growth_percent: 0,
  income_yield_percent: 0,
  scenario_spread_percent: 2,
  focus: null,
};

/** Ajustes › Objetivos: patrimônio, valor numa classe e renda mensal, com a projeção em três cenários. */
export function ObjectivesScreen() {
  const objectives = useObjectives();
  const [sheet, setSheet] = useState<{ objective: Objective | null } | null>(null);
  const hasIncomeGoal = objectives.data?.objectives.some((o) => o.kind === "monthly_income") ?? false;

  return (
    <Page className="objectives">
      <PageHeader
        title="Para onde a carteira vai?"
        summary={
          <>
            Cenários com premissas à vista, não promessa. A meta de renda também aparece em <a href="#/renda">Renda</a>.
          </>
        }
        infoLabel="Como a data de cada objetivo é estimada"
        info="Uma conta mês a mês, em reais de hoje, com as premissas de cada objetivo sempre à vista: aporte, rentabilidade acima da inflação, inflação (Focus do Banco Central), proventos reinvestidos ou não, crescimento e rendimento dos proventos. O pessimista e o otimista tiram e somam a amplitude por ano. A data sai de duas leituras da meta: o número como você digitou (reais da época) e com o poder de compra de hoje. É um cenário, não uma promessa."
        actions={
          <MagneticButton onClick={() => setSheet({ objective: null })}>
            <IconPlus /> Novo objetivo
          </MagneticButton>
        }
      />

      {objectives.isPending && <Skeleton height={140} radius={22} />}
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
        defaults={objectives.data?.defaults ?? FALLBACK_DEFAULTS}
        hasIncomeGoal={hasIncomeGoal}
      />
    </Page>
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
          Defina, por exemplo, quanto você quer ter no total, em Fundos Imobiliários ou de renda por mês. Cada objetivo
          mostra quando chega em três cenários, com a conta à vista, e o Aportar passa a dizer quanto cada aporte aproxima
          você da meta.
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
          <Grid as="ul" cols="auto" minWidth="340px" alignStart className="objective-list">
            {active.map((objective) => (
              <ObjectiveCard key={objective.id} objective={objective} defaults={data.defaults} onEdit={onEdit} />
            ))}
          </Grid>
        </section>
      )}
      {achieved.length > 0 && (
        <section aria-labelledby="objectives-achieved">
          <h2 id="objectives-achieved" className="section-title">
            Concluídos
          </h2>
          <Grid as="ul" cols="auto" minWidth="340px" alignStart className="objective-list">
            {achieved.map((objective) => (
              <ObjectiveCard key={objective.id} objective={objective} defaults={data.defaults} onEdit={onEdit} />
            ))}
          </Grid>
        </section>
      )}
    </>
  );
}

type CardProps = { objective: Objective; defaults: ObjectivesDefaults; onEdit: (objective: Objective) => void };

function ObjectiveCard({ objective, defaults, onEdit }: CardProps) {
  const money = useMoney();
  const toast = useToast();
  const remove = useDeleteObjective();
  const [asking, setAsking] = useState(false);
  const [basis, setBasis] = useState<CurveBasis>("nominal");
  const achieved = objective.status === "achieved";
  const income = objective.kind === "monthly_income";
  const name = objectiveName(objective);
  const target = targetText(objective, money);
  const shown = Math.min(Math.max(objective.percent, 0), 100);
  const estimate = objective.basis === "estimate";

  const confirmRemove = () =>
    remove.mutate(objective.id, {
      onSuccess: () => toast({ tone: "success", message: "Objetivo removido." }),
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });

  const current = income ? `${money.brl(objective.current)} por mês` : money.brl(objective.current);

  return (
    <li>
      <GlassCard as="article" className={cx("objective-card", achieved && "objective-card--achieved")} aria-label={name}>
        <header className="objective-card__head">
          <div>
            <h3 className="objective-card__name">{name}</h3>
            <p className="objective-card__target">
              Meta de <strong>{target}</strong>
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
          aria-valuetext={`${current} de ${target}, ${formatPercent(objective.percent, 1)}${estimate ? ", pela estimativa" : ""}`}
        >
          <motion.div
            className="objective-progress__fill"
            initial={{ width: "0%" }}
            animate={{ width: `${shown}%` }}
            transition={SPRING.value}
          />
        </div>
        <p className="objective-card__legend">
          <strong>{formatPercent(objective.percent, 1)}</strong> · {current} de {target}
          {income && <span className="income-tag"> {estimate ? "estimativa" : "recebido"}</span>}
        </p>

        {achieved ? (
          <p className="objective-card__done" role="status">
            <IconCheck width={18} height={18} /> Concluído
            {objective.achieved_at ? ` em ${formatDay(objective.achieved_at)}` : ""}.
          </p>
        ) : (
          <Projection objective={objective} defaults={defaults} basis={basis} onBasis={setBasis} />
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

type ProjectionProps = {
  objective: Objective;
  defaults: ObjectivesDefaults;
  basis: CurveBasis;
  onBasis: (basis: CurveBasis) => void;
};

/** As datas dos três cenários, a curva e as premissas de um objetivo em andamento. */
function Projection({ objective, defaults, basis, onBasis }: ProjectionProps) {
  const money = useMoney();
  const { scenarios, curve, assumptions } = objective;
  const name = objectiveName(objective);
  const income = objective.kind === "monthly_income";
  const reachedByEstimate = income && objective.projection.state === "reached";
  const rows = assumptions ? premiseRows(objective, assumptions, money, defaults) : [];

  return (
    <>
      <p className="objective-card__eta">
        {reachedByEstimate
          ? "Pela estimativa, a renda de hoje já cobre a meta; ela é marcada quando o recebido chegar lá."
          : baseText(objective.projection)}
      </p>
      {scenarios && (
        <div className="objective-card__scenarios">
          <p>{rangeText(scenarios, "nominal")}</p>
          <p>
            Com o poder de compra de hoje (a meta corrigida pela inflação): {shortDate(scenarios.base.today)} no base.
          </p>
        </div>
      )}
      {curve && curve.points.length > 1 && (
        <>
          <div className="proj-toggle" role="group" aria-label={`Leitura da curva: ${name}`}>
            <button type="button" aria-pressed={basis === "nominal"} onClick={() => onBasis("nominal")}>
              Reais da época
            </button>
            <button type="button" aria-pressed={basis === "today"} onClick={() => onBasis("today")}>
              Reais de hoje
            </button>
          </div>
          <ProjectionChart curve={curve} target={objective.target} basis={basis} perMonth={income} name={name} />
        </>
      )}
      {rows.length > 0 && (
        <dl className="objective-premises" aria-label={`Premissas da conta: ${name}`}>
          <div className="objective-premises__head">
            Premissas
            <InfoTip label={`De onde vem cada premissa: ${name}`} align="start">
              <ul className="objective-premises__sources">
                {rows.map((row) => (
                  <li key={row.id}>
                    <strong>{row.label}:</strong> {row.source}.
                  </li>
                ))}
              </ul>
            </InfoTip>
          </div>
          {rows.map((row) => (
            <div key={row.id} className="objective-premises__row">
              <dt>{row.label}</dt>
              <dd>
                {row.value}
                {row.informed && <span className="objective-premises__mine"> · sua</span>}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </>
  );
}
