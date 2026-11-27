import { useObjectives } from "../../api/objectivesQueries";
import { rangeText, shortDate } from "../../lib/objectives";
import "./incomeProjection.css";

/**
 * A data estimada da meta de renda, em uma linha, sob a barra da Renda. A
 * conta inteira (premissas, curva, cenários) mora em Ajustes › Objetivos; se a
 * consulta falha, a linha some e o cartão segue.
 */
export function IncomeGoalProjection({ goalId }: Readonly<{ goalId: string }>) {
  const objectives = useObjectives();
  const goal = objectives.data?.objectives.find((o) => o.id === goalId);
  if (goal?.status !== "active" || !goal.scenarios) return null;
  const base = goal.scenarios.base.nominal;
  const lead =
    base.state === "reached"
      ? "Pela estimativa, a renda de hoje já cobre a meta."
      : `Cenário base: ${shortDate(base)}. ${rangeText(goal.scenarios, "nominal")}`;
  return (
    <p className="income-goal__projection">
      {lead}{" "}
      <a className="link-btn" href="#/ajustes/objetivos">
        Ver projeção e premissas
      </a>
    </p>
  );
}
