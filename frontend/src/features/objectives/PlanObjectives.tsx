import { useObjectiveImpact } from "../../api/objectivesQueries";
import type { ImpactEntry } from "../../api/objectivesTypes";
import type { Plan } from "../../api/types";
import { InfoTip } from "../../components/InfoTip";
import { Panel } from "../../components/Layout";
import { formatPercent } from "../../lib/format";
import { assumptionLines, formatMonth, objectiveName, projectionText } from "../../lib/objectives";
import { useMoney } from "../../lib/privacy";

/**
 * O que este aporte faz com os objetivos, abaixo do resultado do plano: o
 * percentual da meta com ele e o mês estimado de conclusão. Sem objetivo
 * ativo, ou se a consulta falha, o painel não aparece e o plano segue inteiro.
 * As premissas da conta de cada data ficam no ⓘ do objetivo.
 */
export function PlanObjectives({ plan }: { plan: Plan }) {
  const items = plan.items.map((item) => ({ asset_class: item.asset_class, value: item.value }));
  const impact = useObjectiveImpact(items);
  const entries = impact.data?.objectives ?? [];
  if (entries.length === 0) return null;

  return (
    <Panel
      title="Seus objetivos"
      className="plan-objectives"
      infoLabel="Sobre as datas dos objetivos"
      info="A data é a do cenário base, uma conta com as premissas de cada objetivo (as principais no ⓘ dele; todas, com o pessimista e o otimista, em Ajustes › Objetivos). É um cenário, não uma promessa."
      actions={
        <a className="link-btn" href="#/ajustes/objetivos">
          Trocar premissas
        </a>
      }
    >
      <ul className="plan-objectives__list">
        {entries.map((entry) => (
          <Entry key={entry.id} entry={entry} />
        ))}
      </ul>
    </Panel>
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
  const before = Math.min(Math.max(entry.percent_before, 0), 100);
  const after = Math.min(Math.max(entry.percent_after, 0), 100);

  return (
    <li className="plan-objectives__item">
      <div className="plan-objectives__head">
        <strong className="plan-objectives__name">{name}</strong>
        <InfoTip label={`Premissas da conta: ${name}`} align="start">
          <span className="plan-objectives__lines">
            <span>{lines.contribution}</span>
            <span>{lines.rate}</span>
          </span>
        </InfoTip>
      </div>
      <div className="plan-objectives__bar" aria-hidden="true">
        <span className="plan-objectives__added" style={{ width: `${after}%` }} />
        <span className="plan-objectives__before" style={{ width: `${before}%` }} />
      </div>
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
    </li>
  );
}
