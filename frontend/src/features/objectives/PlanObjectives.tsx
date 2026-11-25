import { useObjectiveImpact } from "../../api/objectivesQueries";
import type { ImpactEntry } from "../../api/objectivesTypes";
import type { Plan } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { formatPercent } from "../../lib/format";
import { assumptionLines, formatMonth, objectiveName, projectionText } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";

/**
 * O que este aporte faz com os objetivos, abaixo do resultado do plano: o
 * percentual da meta com ele e o mês estimado de conclusão. Sem objetivo
 * ativo, ou se a consulta falha, o cartão não aparece e o plano segue inteiro.
 */
export function PlanObjectives({ plan }: { plan: Plan }) {
  const items = plan.items.map((item) => ({ asset_class: item.asset_class, value: item.value }));
  const impact = useObjectiveImpact(items);
  const entries = impact.data?.objectives ?? [];
  if (entries.length === 0) return null;

  return (
    <GlassCard as="section" className="plan-objectives" aria-label="Seus objetivos">
      <h3 className="section-title">Seus objetivos</h3>
      <ul className="plan-objectives__list">
        {entries.map((entry) => (
          <Entry key={entry.id} entry={entry} />
        ))}
      </ul>
      <p className="field__hint">
        A data é uma estimativa com as premissas abaixo de cada objetivo.{" "}
        <a href="#/ajustes/objetivos">Trocar as premissas em Objetivos</a>.
      </p>
    </GlassCard>
  );
}

function Entry({ entry }: { entry: ImpactEntry }) {
  const money = useMoney();
  const name = objectiveName(entry);
  const lines = assumptionLines(entry.assumptions, money);
  const { projection } = entry;
  const completes = projection.state === "reached";
  const pace =
    entry.assumptions.contribution_source === "ledger"
      ? "assim"
      : `de ${money.brl(entry.assumptions.monthly_contribution)} por mês`;

  return (
    <li className="plan-objectives__item">
      <strong className="plan-objectives__name">{name}</strong>
      {completes ? (
        <p>
          Com este aporte, você <strong className="gold">completa a meta de {money.brl(entry.target)}</strong>.
        </p>
      ) : (
        <p>
          Com este aporte, você chega a <strong className="gold">{formatPercent(entry.percent_after, 1)}</strong> da meta de{" "}
          {money.brl(entry.target)}
          <span className="muted"> (hoje, {formatPercent(entry.percent_before, 1)})</span>.
        </p>
      )}
      {!completes &&
        (projection.state === "eta" ? (
          <p>
            Com aportes {pace}, a meta chega em <strong>{formatMonth(projection.month)}</strong>.
          </p>
        ) : (
          <p>{projectionText(projection)}</p>
        ))}
      <ul className="objective-card__premises" aria-label={`Premissas: ${name}`}>
        <li>{lines.contribution}</li>
        <li>{lines.rate}</li>
      </ul>
    </li>
  );
}
