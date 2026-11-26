import { motion } from "motion/react";
import { useContributions } from "../../api/queries";
import { Disclosure } from "../../components/Disclosure";
import { GlassCard } from "../../components/GlassCard";
import { Skeleton } from "../../components/Skeleton";
import { formatDateTime } from "../../lib/format";
import { STAGGER } from "../../lib/motion";
import { useMoney } from "../../lib/privacy";
import { assetName } from "../../lib/rating";

/** Histórico de aportes, recolhido: a frase diz quantos há; abrir mostra cada um. */
export function ContributionHistory() {
  const contributions = useContributions();
  const money = useMoney();
  const count = contributions.data?.length;

  return (
    <GlassCard as="section" className="panel panel--compact history" aria-label="Histórico de aportes">
      <Disclosure summary="Histórico de aportes" count={count === undefined ? undefined : count === 0 ? "nenhum" : count}>
        {contributions.isPending && <Skeleton height={56} radius={16} />}
        {contributions.isError && <p className="field__hint">Não consegui carregar o histórico agora.</p>}
        {contributions.data && contributions.data.length === 0 && (
          <p className="field__hint">Nenhum aporte registrado ainda. Quando você marcar &quot;Já comprei&quot;, ele aparece aqui.</p>
        )}
        {contributions.data && contributions.data.length > 0 && (
          <ol className="history__list">
            {contributions.data.map((contribution, index) => (
              <motion.li
                key={contribution.id}
                className="history__item"
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                transition={{ delay: Math.min(index, 8) * STAGGER }}
              >
                <div className="history__head">
                  <span className="history__date">{formatDateTime(contribution.created_at)}</span>
                  <strong className="history__value">{money.brl(contribution.invested)}</strong>
                </div>
                <p className="history__items">
                  {contribution.items
                    .map((item) => `${money.quantity(item.quantity, !Number.isInteger(item.quantity))} ${assetName(item)}`)
                    .join(" · ")}
                </p>
              </motion.li>
            ))}
          </ol>
        )}
      </Disclosure>
    </GlassCard>
  );
}
