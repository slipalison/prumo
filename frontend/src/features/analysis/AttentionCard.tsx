import { useId } from "react";
import type { Attention } from "../../api/analysisTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert } from "../../components/icons";
import { pointText } from "../../lib/analysisText";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { plural } from "../../lib/format";

/**
 * Pontos de atenção por ativo: avisos curtos e factuais tirados dos balanços da
 * CVM (ROE e margem baixos, crescimento fraco, dívida maior que o patrimônio,
 * recuperação judicial) e dos proventos (queda em 12 meses). Não recomendam
 * nada. Sem dado de fundamentos, diz isso em vez de "tudo certo".
 */
export function AttentionCard({ attention }: { attention: Attention }) {
  const titleId = useId();
  const { assessed, without_data: without, items } = attention;

  return (
    <GlassCard as="section" className="dimension" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        Pontos de atenção dos ativos
      </h2>
      <p className="field__hint">
        Fatos dos balanços da CVM e dos seus proventos, ativo por ativo. Não são recomendação: o Prumo não manda comprar
        nem vender.
      </p>

      {items.length > 0 ? (
        <ul className="attention-list" aria-label="Ativos com pontos de atenção">
          {items.map((item) => (
            <li key={item.asset_id} className="attention">
              <div className="attention__head">
                <AssetIcon size="sm" assetClass={item.asset_class} {...iconSubjectOfLabel(item.label)} />
                <h3 className="attention__name">{item.label}</h3>
              </div>
              <ul className="attention__points" aria-label={`Pontos de atenção de ${item.label}`}>
                {item.points.map((point) => (
                  <li key={point.kind} className="attention__point">
                    <IconAlert width={16} height={16} className="attention__icon" />
                    <span>{pointText(point)}</span>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      ) : assessed > 0 ? (
        <p role="status">
          Nenhum ponto de atenção nos {assessed} {plural(assessed, ["ativo com fundamentos", "ativos com fundamentos"])}.
        </p>
      ) : (
        <p role="status">
          Ainda sem dados de fundamentos para avaliar: nenhum ativo seu tem balanço ou informe da CVM guardado, então não
          há o que dizer, nem “tudo certo”.
        </p>
      )}

      {assessed > 0 && (
        <p className="field__hint">
          {assessed} {plural(assessed, ["ativo avaliado", "ativos avaliados"])}
          {without > 0 && (
            <>
              ; {without} {plural(without, ["ação ou FII ainda sem dados", "ações ou FIIs ainda sem dados"])} (a busca na
              CVM roda em segundo plano)
            </>
          )}
          . Os indicadores de cada ativo estão na folha dele, em Fundamentos.
        </p>
      )}
    </GlassCard>
  );
}
