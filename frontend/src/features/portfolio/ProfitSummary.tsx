import type { Profit, ProfitGap } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { InfoTip } from "../../components/InfoTip";
import { cx } from "../../lib/cx";
import { useMoney } from "../../lib/privacy";
import { signedBRL } from "../ledger/format";

const MISSING: Record<ProfitGap, string> = {
  unknown_cost: "falta o preço médio de",
  unknown_realized: "falta o preço médio da venda de",
  missing_price: "sem cotação:",
};

/** "falta o preço médio de A e B; sem cotação: C" — um trecho por motivo. */
function partialNote(profit: Profit): string {
  const groups = new Map<ProfitGap, string[]>();
  for (const asset of profit.partial_assets) {
    groups.set(asset.reason, [...(groups.get(asset.reason) ?? []), asset.label]);
  }
  return [...groups.entries()].map(([reason, labels]) => `${MISSING[reason]} ${labels.join(", ")}`).join("; ");
}

/**
 * "Lucro total = ganho de capital (realizado + não realizado) + proventos
 * recebidos". O que o Prumo não sabe fica de fora e é dito: o rótulo vira
 * "(parcial)", sem nenhum número inventado. As três parcelas e o que falta
 * moram no ⓘ. Com o olho fechado, todo valor é mascarado, inclusive no ⓘ.
 */
export function ProfitSummary({ profit }: { profit: Profit }) {
  const money = useMoney();
  const capital = profit.realized + profit.unrealized;
  return (
    <GlassCard as="section" className="stat-card" aria-label="Lucro total">
      <span className="stat-card__label">
        Lucro total{profit.partial ? " (parcial)" : ""}
        <InfoTip label="Como se chega no lucro total" align="start">
          <dl className="profit__parts">
            <div>
              <dt>Ganho de capital</dt>
              <dd>{signedBRL(money, capital)}</dd>
            </div>
            <div>
              <dt>não realizado</dt>
              <dd>{signedBRL(money, profit.unrealized)}</dd>
            </div>
            <div>
              <dt>realizado (vendas)</dt>
              <dd>{signedBRL(money, profit.realized)}</dd>
            </div>
            <div>
              <dt>Proventos recebidos</dt>
              <dd>{signedBRL(money, profit.income_received)}</dd>
            </div>
          </dl>
          {profit.partial && (
            <p className="profit__note">
              Parcial: {partialNote(profit)}. Esses ativos ficam fora da soma até você informar; nada aqui é estimado.
            </p>
          )}
          {profit.removed_assets && (
            <p className="profit__note">Não inclui o resultado de vendas de ativos que já saíram da carteira.</p>
          )}
        </InfoTip>
      </span>
      <strong className={cx("stat-card__value", !money.hidden && profit.total > 0 && "is-up", !money.hidden && profit.total < 0 && "is-down")}>
        {signedBRL(money, profit.total)}
      </strong>
      <span className="stat-card__detail">capital e proventos{profit.partial ? ", sem o que falta" : ""}</span>
    </GlassCard>
  );
}
