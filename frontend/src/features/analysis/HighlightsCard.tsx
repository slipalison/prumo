import type { Analysis, Mover, Ranking } from "../../api/analysisTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { Panel } from "../../components/Layout";
import { formatSignedPercent } from "../../lib/analysisText";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { cx } from "../../lib/cx";

function MoverList({ title, movers, empty }: { title: string; movers: Mover[]; empty: string }) {
  return (
    <div className="movers">
      <h4 className="movers__title">{title}</h4>
      {movers.length === 0 ? (
        <p className="movers__empty">{empty}</p>
      ) : (
        <ol className="movers__list" aria-label={title}>
          {movers.map((mover) => (
            <li key={mover.asset_id} className="movers__item">
              <AssetIcon size="sm" assetClass={mover.asset_class} {...iconSubjectOfLabel(mover.label)} />
              <span className="movers__label">{mover.label}</span>
              <strong className={cx("movers__change", mover.change_percent > 0 ? "movers__change--up" : "movers__change--down")}>
                {formatSignedPercent(mover.change_percent)}
              </strong>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function Measure({ title, ranking, empty }: { title: string; ranking: Ranking; empty: string }) {
  return (
    <div className="highlights__measure">
      <h3 className="highlights__heading">{title}</h3>
      <div className="highlights__columns">
        <MoverList title="Mais subiram" movers={ranking.best} empty={empty} />
        <MoverList title="Mais caíram" movers={ranking.worst} empty={empty} />
      </div>
    </div>
  );
}

/**
 * Destaques de 12 meses: quem mais subiu e quem mais caiu pelo preço (ajustado
 * por desdobramentos) e pela rentabilidade desde a compra. São fatos do
 * passado, não recomendação: só entra quem subiu na lista de cima e quem caiu na
 * de baixo.
 */
export function HighlightsCard({ highlights }: { highlights: Analysis["highlights"] }) {
  return (
    <Panel
      className="highlights"
      title="Destaques de 12 meses"
      infoLabel="Como os destaques são medidos"
      info="O preço de 12 meses atrás vem do histórico guardado; ativo com menos de 12 meses de histórico fica de fora. A rentabilidade desde a compra é o preço de hoje contra o preço médio do livro."
    >
      <Measure
        title="Pela variação do preço em 12 meses"
        ranking={highlights.price_12m}
        empty="Nenhum, ou ainda sem 12 meses de histórico de preço."
      />
      <Measure
        title="Pela rentabilidade desde a compra"
        ranking={highlights.since_purchase}
        empty="Nenhum com custo conhecido no livro."
      />
    </Panel>
  );
}
