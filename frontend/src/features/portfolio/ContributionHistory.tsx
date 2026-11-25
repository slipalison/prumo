import { motion } from "motion/react";
import { useContributions } from "../../api/queries";
import { GlassCard } from "../../components/GlassCard";
import { IconHistory } from "../../components/icons";
import { Skeleton } from "../../components/Skeleton";
import { formatDateTime } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

export function ContributionHistory() {
  const contributions = useContributions();
  const money = useMoney();

  return (
    <GlassCard as="section" className="history" aria-labelledby="history-title">
      <header className="section-head">
        <IconHistory className="section-head__icon" />
        <h2 id="history-title" className="section-title">
          Histórico de aportes
        </h2>
      </header>
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
              transition={{ delay: Math.min(index, 8) * 0.04 }}
            >
              <div className="history__head">
                <span className="history__date">{formatDateTime(contribution.created_at)}</span>
                <strong className="history__value">{money.brl(contribution.invested)}</strong>
              </div>
              <p className="history__items">
                {contribution.items
                  .map((item) => `${money.quantity(item.quantity, !Number.isInteger(item.quantity))} ${item.symbol ?? item.display_name}`)
                  .join(" · ")}
              </p>
            </motion.li>
          ))}
        </ol>
      )}
    </GlassCard>
  );
}
