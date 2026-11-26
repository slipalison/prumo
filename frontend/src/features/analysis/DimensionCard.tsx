import { useId } from "react";
import type { Dimension } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { DIMENSION_TEXT, noDataText, scorePercent, scoreText } from "../../lib/analysisText";
import { RuleRow } from "./RuleRow";

/** Primeiro o que precisa de atenção; depois o que está bem, o que não tem dado e o que está desligado. */
const ORDER = { warning: 0, ok: 1, no_data: 2, off: 3 } as const;

/**
 * Uma dimensão: a pontuação (o percentual de regras ligadas, com dado, que
 * estão ok, com o número exato à mostra) e as regras. Sem regra pronta — a
 * Qualidade dos ativos, enquanto os fundamentos não chegam — diz isso.
 */
export function DimensionCard({ dimension }: { dimension: Dimension }) {
  const titleId = useId();
  const text = DIMENSION_TEXT[dimension.id];
  const { score } = dimension;
  const rules = [...dimension.rules].sort((a, b) => ORDER[a.status] - ORDER[b.status]);
  const percent = score ? scorePercent(score) : null;
  const missing = score ? noDataText(score) : null;

  return (
    <GlassCard as="section" className="dimension" aria-labelledby={titleId}>
      <header className="dimension__head">
        <div>
          <h2 id={titleId} className="section-title">
            {text.title}
          </h2>
          <p className="field__hint">{text.lead}</p>
        </div>
        {score && (
          <div className="dimension__score">
            <p className="dimension__percent">{percent ?? "Sem dado"}</p>
            <p className="field__hint">
              {score.measured > 0 ? scoreText(score) : "Nenhuma regra com dado ainda"}
              {missing && ` · ${missing}`}
            </p>
          </div>
        )}
      </header>

      {score && score.percent !== null && (
        <div
          className="dimension__meter class-bar"
          role="meter"
          aria-label={`${text.title}: ${scoreText(score)}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={score.percent}
        >
          <div className="class-bar__track">
            <div className="class-bar__fill dimension__fill" style={{ width: `${score.percent}%` }} />
          </div>
        </div>
      )}

      {dimension.state === "awaiting_fundamentals" ? (
        <div className="awaiting" role="status">
          <p className="awaiting__title">Aguardando dados de fundamentos</p>
          <p className="field__hint">
            Esta dimensão usa indicadores das empresas e dos fundos que você tem (retorno sobre o patrimônio, margens,
            dívida, lucro em 5 anos, P/VP de FII) vindos dos dados abertos da CVM. Eles ainda não estão ligados, então
            nenhuma regra daqui entra na pontuação.
          </p>
        </div>
      ) : (
        <ul className="analysis-rules" aria-label={`Regras de ${text.title}`}>
          {rules.map((result) => (
            <RuleRow key={result.rule} result={result} />
          ))}
        </ul>
      )}
    </GlassCard>
  );
}
