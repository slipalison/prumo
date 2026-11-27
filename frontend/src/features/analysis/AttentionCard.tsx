import type { Attention } from "../../api/analysisTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { IconAlert } from "../../components/icons";
import { Panel } from "../../components/Layout";
import { pointText } from "../../lib/analysisText";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { plural } from "../../lib/format";

/** Sem ponto de atenção: nenhum nos ativos avaliados, ou ainda sem dado para avaliar. */
function NoAttention({ assessed }: Readonly<{ assessed: number }>) {
  if (assessed > 0) {
    return (
      <output>
        Nenhum ponto de atenção nos {assessed} {plural(assessed, ["ativo com fundamentos", "ativos com fundamentos"])}.
      </output>
    );
  }
  return (
    <output>
      Ainda sem dados de fundamentos para avaliar: nenhum ativo seu tem balanço ou informe da CVM guardado, então não
      há o que dizer, nem “tudo certo”.
    </output>
  );
}

/**
 * Pontos de atenção por ativo: avisos curtos e factuais tirados dos balanços da
 * CVM (ROE e margem baixos, crescimento fraco, dívida maior que o patrimônio,
 * recuperação judicial) e dos proventos (queda em 12 meses). Não recomendam
 * nada. Sem dado de fundamentos, diz isso em vez de "tudo certo".
 */
export function AttentionCard({ attention }: Readonly<{ attention: Attention }>) {
  const { assessed, without_data: without, items } = attention;

  return (
    <Panel
      className="attention-card"
      title="Pontos de atenção dos ativos"
      info="Fatos dos balanços da CVM e dos seus proventos, ativo por ativo. Os indicadores de cada ativo estão na folha dele, em Fundamentos."
    >
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
                    <IconAlert width={14} height={14} className="attention__icon" />
                    <span>{pointText(point)}</span>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      ) : (
        <NoAttention assessed={assessed} />
      )}

      <p className="attention-card__foot">
        {assessed > 0 && (
          <>
            {assessed} {plural(assessed, ["ativo avaliado", "ativos avaliados"])}
            {without > 0 && (
              <>
                ; {without} {plural(without, ["ação ou FII ainda sem dados", "ações ou FIIs ainda sem dados"])} (a busca
                na CVM roda em segundo plano)
              </>
            )}
            .{" "}
          </>
        )}
        Não são recomendação: o Prumo não manda comprar nem vender.
      </p>
    </Panel>
  );
}
