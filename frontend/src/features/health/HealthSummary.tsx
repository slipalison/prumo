import { GlassCard } from "../../components/GlassCard";
import { IconAlert, IconCheck } from "../../components/icons";
import { plural } from "../../lib/format";
import { warningTitle } from "../../lib/healthText";
import { useHealth } from "./useHealth";

const SHOWN = 2;

/**
 * Resumo curto dos avisos para a Carteira, com o caminho para o painel. Nunca
 * atrapalha a carteira: enquanto carrega ou se falhar, não aparece.
 */
export function HealthSummary() {
  const health = useHealth();
  if (!health.data) return null;
  const { warnings, rules } = health.data;
  const measured = rules.some((r) => r.status === "ok" || r.status === "warning");
  const extra = warnings.length - SHOWN;

  return (
    <GlassCard as="section" className="health-summary" aria-label="Saúde da carteira">
      <div className="health-summary__head">
        {warnings.length > 0 ? (
          <IconAlert className="health-summary__icon health-summary__icon--warn" />
        ) : (
          <IconCheck className="health-summary__icon health-summary__icon--ok" />
        )}
        <h2 className="health-summary__title">
          {warnings.length > 0
            ? `Saúde da carteira: ${warnings.length} ${plural(warnings.length, ["aviso", "avisos"])}`
            : measured
              ? "Saúde da carteira: nenhum aviso"
              : "Saúde da carteira: ainda sem o que medir"}
        </h2>
        <a className="link-btn" href="#/ajustes/saude">
          Ver painel
        </a>
      </div>
      {warnings.length > 0 && (
        <ul className="health-summary__list">
          {warnings.slice(0, SHOWN).map((warning, index) => (
            <li key={`${warning.rule}-${index}`}>{warningTitle(warning)}</li>
          ))}
          {extra > 0 && <li className="muted">e mais {extra}</li>}
        </ul>
      )}
    </GlassCard>
  );
}
