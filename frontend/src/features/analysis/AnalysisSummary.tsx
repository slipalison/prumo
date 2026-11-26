import type { RuleResult } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconCheck } from "../../components/icons";
import { attentionText, ruleSentence } from "../../lib/analysisText";
import { formatRuleValue, warningTitle } from "../../lib/healthText";
import { useAnalysis } from "./useAnalysis";

const SHOWN = 2;

/** Uma linha do resumo: o que passou do limite, com o número de agora. */
function attentionLine(result: RuleResult): string {
  if (result.detail?.kind === "findings" && result.detail.warnings[0]) return warningTitle(result.detail.warnings[0]);
  const now = result.current === null ? "" : `: agora ${formatRuleValue(result.unit, result.current)}`;
  return `${ruleSentence(result)}${now}`;
}

/**
 * Resumo curto da Análise para a Carteira, com o caminho para a tela. Nunca
 * atrapalha a carteira: enquanto carrega ou se falhar, não aparece.
 */
export function AnalysisSummary() {
  const analysis = useAnalysis();
  if (!analysis.data) return null;
  const { attention_count: attention, dimensions } = analysis.data;
  const rules = dimensions.flatMap((d) => d.rules);
  const measured = rules.some((r) => r.status === "ok" || r.status === "warning");
  const warnings = rules.filter((r) => r.status === "warning");
  const extra = warnings.length - SHOWN;

  return (
    <GlassCard as="section" className="health-summary" aria-label="Análise da carteira">
      <div className="health-summary__head">
        {attention > 0 ? (
          <IconAlert className="health-summary__icon health-summary__icon--warn" />
        ) : (
          <IconCheck className="health-summary__icon health-summary__icon--ok" />
        )}
        <h2 className="health-summary__title">
          {attention > 0
            ? `Análise: ${attentionText(attention)}`
            : measured
              ? "Análise: nenhum ponto a ver"
              : "Análise: ainda sem o que medir"}
        </h2>
        <a className="link-btn" href="#/analise">
          Ver análise
        </a>
      </div>
      {warnings.length > 0 && (
        <ul className="health-summary__list">
          {warnings.slice(0, SHOWN).map((result) => (
            <li key={result.rule}>{attentionLine(result)}</li>
          ))}
          {extra > 0 && <li className="muted">e mais {extra}</li>}
        </ul>
      )}
    </GlassCard>
  );
}
