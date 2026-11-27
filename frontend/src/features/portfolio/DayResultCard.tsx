import { useMemo } from "react";
import { useDayReferences } from "../../api/panel";
import type { Asset } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { InfoTip } from "../../components/InfoTip";
import { cx } from "../../lib/cx";
import { dayMovers, dayTotal, signDirection } from "../../lib/panel";
import { useMoney } from "../../lib/privacy";
import { formatSignedPercent, signedBRL } from "../ledger/format";
import { formatDay } from "../performance/performanceFormat";
import { DeltaArrow } from "./DeltaArrow";
import "./panel.css";

const INFO =
  "Quanto a carteira ganhou ou perdeu desde o fechamento do pregão anterior, no preço ao vivo. A compra feita depois do fechamento conta a partir do preço que você pagou, com as taxas, e a venda, até o preço de venda: dinheiro que entra ou sai não é ganho. Fora do pregão, vale a última cotação contra o fechamento antes dela. Ativo com preço manual e renda fixa por valor não entram, nem proventos.";

/** O percentual com duas casas, como a tela mostra: `+0,00%` não existe. */
const shownPercent = (percent: number) => Math.round(percent * 100) / 100;

/**
 * "Resultado do dia": o ganho ou a perda da carteira desde o fechamento
 * anterior, em reais e em %, ao lado do patrimônio. Anda com o preço ao vivo
 * (o stream atualiza os ativos); o fechamento e os negócios depois dele vêm de
 * `GET /portfolio/day-reference`. Com os valores ocultos, os reais viram
 * máscara e o percentual fica. Enquanto carrega, ou se a API falhar, não
 * aparece.
 */
export function DayResultCard({ assets }: { assets: Asset[] }) {
  const references = useDayReferences();
  const money = useMoney();
  const total = useMemo(() => dayTotal(dayMovers(assets, references.data?.assets ?? [])), [assets, references.data]);

  if (references.isPending || references.isError) return null;

  const label = (
    <span className="stat-card__label">
      Resultado do dia
      <InfoTip label="Como se chega no resultado do dia" align="start">
        {INFO}
      </InfoTip>
    </span>
  );

  if (total === null) {
    return (
      <GlassCard as="section" className="stat-card day-result" aria-label="Resultado do dia">
        {label}
        <strong className="stat-card__value">—</strong>
        <span className="stat-card__detail">sem fechamento anterior ainda</span>
      </GlassCard>
    );
  }

  const direction = signDirection(total.delta);
  const percent = shownPercent(total.percent);
  return (
    <GlassCard as="section" className="stat-card day-result" aria-label="Resultado do dia">
      {label}
      <strong
        className={cx(
          "stat-card__value",
          !money.hidden && direction === "above" && "is-up",
          !money.hidden && direction === "below" && "is-down",
        )}
      >
        {!money.hidden && <DeltaArrow direction={direction} />}
        {signedBRL(money, total.delta)}
      </strong>
      <span className="stat-card__detail">
        <span className={cx("day-change", `day-change--${signDirection(percent, 0.005)}`)}>{formatSignedPercent(percent, 2)}</span>{" "}
        <span className="day-result__since">desde o fechamento de {formatDay(total.since)}</span>
      </span>
    </GlassCard>
  );
}
