import { useMemo, useState } from "react";
import { useDayReferences } from "../../api/panel";
import type { Asset, AssetClass } from "../../api/types";
import { Panel } from "../../components/Layout";
import { ASSET_CLASSES, CLASS_META, classColor } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { classDays, dayMovers, dayTotal, signDirection, topMovers, type Direction, type PricedMover } from "../../lib/panel";
import { useMoney } from "../../lib/privacy";
import { formatSignedPercent, signedBRL } from "../ledger/format";
import { DeltaArrow } from "./DeltaArrow";
import "./panel.css";

const SHOWN = 5;

/** O sentido da variação escrito para o leitor de tela, antes do número. */
const SPOKEN_DIRECTION: Record<Direction, string> = {
  above: "alta de ",
  below: "baixa de ",
  equal: "sem variação, ",
};

function Change({ percent, direction }: Readonly<{ percent: number; direction?: Direction }>) {
  const kind = direction ?? signDirection(Math.round(percent * 10) / 10, 0.05);
  return (
    <span className={cx("day-change", `day-change--${kind}`)}>
      <DeltaArrow direction={kind} />
      <span className="sr-only">{SPOKEN_DIRECTION[kind]}</span>
      {formatSignedPercent(percent)}
    </span>
  );
}

function MoverList({ title, movers, showClass }: Readonly<{ title: string; movers: PricedMover[]; showClass: boolean }>) {
  return (
    <section className="movers" aria-label={title}>
      <h3 className="movers__title">{title}</h3>
      {movers.length === 0 ? (
        <p className="field__hint">Nenhum ativo {title === "Maiores altas" ? "em alta" : "em baixa"} agora.</p>
      ) : (
        <ol className="movers__list">
          {movers.map((m) => (
            <li key={m.id} className="movers__item">
              <span className="movers__name">
                {showClass && <span className="dot" style={{ background: classColor(m.assetClass) }} aria-hidden="true" />}
                {m.label}
                {showClass && <span className="sr-only"> ({CLASS_META[m.assetClass].label})</span>}
              </span>
              <Change percent={m.changePercent} />
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

/**
 * "Hoje na carteira": como o preço dos ativos andou contra o fechamento
 * anterior, por classe, com as maiores altas e baixas. A variação usa o preço
 * ao vivo (a tela se atualiza com o stream); o fechamento anterior vem da série
 * diária. Informa o que aconteceu; não diz o que fazer.
 */
export function TodayPanel({ assets }: Readonly<{ assets: Asset[] }>) {
  const references = useDayReferences();
  const money = useMoney();
  const [filter, setFilter] = useState<AssetClass | null>(null);

  const movers = useMemo(() => dayMovers(assets, references.data?.assets ?? []), [assets, references.data]);
  const byClass = useMemo(() => classDays(movers, ASSET_CLASSES), [movers]);
  const total = useMemo(() => dayTotal(movers), [movers]);
  // A classe escolhida some do tema se deixou de ter ativo medido.
  const active = filter !== null && byClass.some((c) => c.assetClass === filter) ? filter : null;
  const { gainers, losers } = useMemo(
    () => topMovers(active === null ? movers : movers.filter((m) => m.assetClass === active), SHOWN),
    [movers, active],
  );

  if (references.isPending || references.isError) return null;

  return (
    <Panel
      title="Hoje na carteira"
      className="today"
      info="Quanto o preço de cada ativo andou contra o fechamento do pregão anterior, no preço ao vivo. Fora do pregão, vale a última cotação contra o fechamento antes dela. O valor em reais é o que cada posição ganhou ou perdeu desde o fechamento: a compra feita depois dele conta a partir do preço pago, com as taxas, e a venda, até o preço de venda. Ativo com preço manual ou renda fixa por valor não entra. É um retrato do dia, não um sinal de compra ou venda."
      infoLabel="Sobre Hoje na carteira"
    >
      {byClass.length === 0 || total === null ? (
        <p className="field__hint">Ainda sem fechamento anterior para medir o dia. Ele aparece depois do primeiro fechamento diário da carteira.</p>
      ) : (
        <>
          <p className="today__total">
            <span className="today__total-label">Carteira no dia</span>
            <span className="today__class-numbers">
              <Change percent={total.percent} direction={signDirection(Math.round(total.percent * 10) / 10, 0.05)} />
              <span className="today__class-money">{signedBRL(money, total.delta)}</span>
            </span>
          </p>
          <ul className="today__classes" aria-label="Variação do dia por classe">
            {byClass.map((c) => {
              const direction = signDirection(Math.round(c.percent * 10) / 10, 0.05);
              return (
                <li key={c.assetClass}>
                  <button
                    type="button"
                    className={cx("today__class", active === c.assetClass && "is-active")}
                    style={{ ["--class-color" as string]: classColor(c.assetClass) }}
                    aria-pressed={active === c.assetClass}
                    onClick={() => setFilter(active === c.assetClass ? null : c.assetClass)}
                  >
                    <span className="today__class-name">
                      <span className="dot" style={{ background: "var(--class-color)" }} aria-hidden="true" />
                      {CLASS_META[c.assetClass].shortLabel}
                    </span>
                    <span className="today__class-numbers">
                      <Change percent={c.percent} direction={direction} />
                      <span className="today__class-money">{signedBRL(money, c.delta)}</span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          <div className="today__movers">
            <MoverList title="Maiores altas" movers={gainers} showClass={active === null} />
            <MoverList title="Maiores baixas" movers={losers} showClass={active === null} />
          </div>
        </>
      )}
    </Panel>
  );
}
