import type { RuleResult } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconCheck } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
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

/** O título do resumo: os pontos a ver, nenhum, ou ainda sem regra com o que medir. */
function summaryTitle(attention: number, measured: boolean): string {
  if (attention > 0) return attentionText(attention);
  return measured ? "Nenhum ponto a ver" : "Ainda sem o que medir";
}

/**
 * Resumo curto da Análise para a faixa de números da Carteira: quantos pontos
 * a ver e o caminho para a tela; os dois primeiros ficam no ⓘ. Nunca atrapalha
 * a carteira: enquanto carrega ou se falhar, não aparece.
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
    <GlassCard as="section" className="stat-card health-summary" aria-label="Análise da carteira">
      <span className="stat-card__label">
        {attention > 0 ? (
          <IconAlert className="health-summary__icon health-summary__icon--warn" width={14} height={14} />
        ) : (
          <IconCheck className="health-summary__icon health-summary__icon--ok" width={14} height={14} />
        )}
        Análise
        {warnings.length > 0 && (
          <InfoTip label="Os primeiros pontos a ver" align="start">
            <ul className="health-summary__list">
              {warnings.slice(0, SHOWN).map((result) => (
                <li key={result.rule}>{attentionLine(result)}</li>
              ))}
              {extra > 0 && <li className="muted">e mais {extra}</li>}
            </ul>
          </InfoTip>
        )}
      </span>
      <h2 className="stat-card__value health-summary__title">
        {summaryTitle(attention, measured)}
      </h2>
      <a className="stat-card__detail health-summary__link" href="#/analise">
        Ver análise
      </a>
    </GlassCard>
  );
}
