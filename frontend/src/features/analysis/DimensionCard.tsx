import type { Dimension } from "../../api/analysisTypes";
import { Disclosure } from "../../components/Disclosure";
import { Panel } from "../../components/Layout";
import { DIMENSION_TEXT, dimensionAnchor, restText, scorePercent, scoreText } from "../../lib/analysisText";
import { RuleTable } from "./RuleTable";

/** Depois dos pontos a ver: o que está bem, o que não tem dado e o que está desligado. */
const ORDER = { warning: 0, ok: 1, no_data: 2, off: 3 } as const;

/**
 * Uma dimensão: os pontos a ver numa tabela compacta e, recolhidas no fim, as
 * regras ok, sem dado e desligadas. O cabeçalho repete a pontuação, com o
 * número exato.
 */
export function DimensionCard({ dimension }: Readonly<{ dimension: Dimension }>) {
  const text = DIMENSION_TEXT[dimension.id];
  const { score } = dimension;
  const rules = [...dimension.rules].sort((a, b) => ORDER[a.status] - ORDER[b.status]);
  const warnings = rules.filter((r) => r.status === "warning");
  const rest = rules.filter((r) => r.status !== "warning");
  const percent = scorePercent(score);

  return (
    <Panel
      id={dimensionAnchor(dimension.id)}
      tabIndex={-1}
      className="dimension"
      flush
      title={text.title}
      info={text.lead}
      actions={
        <p className="dimension__score">
          {score.measured > 0 ? scoreText(score) : "Nenhuma regra com dado ainda"}
          {percent && <strong className="dimension__percent">{percent}</strong>}
        </p>
      }
    >
      {warnings.length > 0 ? (
        <RuleTable caption={`Pontos a ver em ${text.title}`} rules={warnings} withAction />
      ) : (
        <p className="dimension__calm">Nenhum ponto a ver nesta dimensão.</p>
      )}
      {rest.length > 0 && (
        <Disclosure className="dimension__rest" summary={restText(rest)}>
          <RuleTable caption={`Outras regras de ${text.title}`} rules={rest} />
        </Disclosure>
      )}
    </Panel>
  );
}
