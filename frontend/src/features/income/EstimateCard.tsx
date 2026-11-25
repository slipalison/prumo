import { useId } from "react";
import type { IncomeEstimate } from "../../api/incomeTypes";
import { CountUp } from "../../components/CountUp";
import { GlassCard } from "../../components/GlassCard";
import { formatRelative } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

const SHOWN = 5;

/**
 * A renda mensal que a carteira de hoje renderia se pagasse o que cada cota
 * pagou nos últimos 12 meses. É estimativa: o histórico do Yahoo traz o valor
 * bruto na data ex, sem separar JCP de dividendo; o que a pessoa recebeu e
 * lançou (ou importou) sempre vale mais.
 */
export function EstimateCard({ estimate }: { estimate: IncomeEstimate }) {
  const titleId = useId();
  const money = useMoney();
  const top = estimate.assets.filter((a) => a.monthly > 0).slice(0, SHOWN);
  const rest = estimate.assets.filter((a) => a.monthly > 0).length - top.length;
  // O histórico chega em segundo plano: logo depois de cadastrar o ativo, ou com
  // o provedor fora do ar, ainda não há estimativa de nenhum papel.
  const unavailable = estimate.assets.length === 0 && estimate.missing.length > 0;

  return (
    <GlassCard as="section" className="income-estimate" aria-labelledby={titleId}>
      <header className="income-estimate__head">
        <div>
          <p className="eyebrow" id={titleId}>
            Estimativa da carteira de hoje
          </p>
          {unavailable ? (
            <p className="income-goal__now">Estimativa indisponível por enquanto.</p>
          ) : (
            <p className="income-goal__now">
              <span aria-hidden="true">~</span>
              <CountUp value={estimate.monthly} className="income-goal__value" />
              <span className="income-goal__per"> por mês</span>
            </p>
          )}
        </div>
        <span className="income-tag">estimativa</span>
      </header>
      <p className="field__hint">
        Pelo que cada cota pagou nos últimos 12 meses (valor bruto, data ex; o JCP vem junto com o dividendo). Não é extrato:
        {estimate.updated_at ? ` histórico de ${formatRelative(estimate.updated_at)}.` : " ainda sem histórico."}
      </p>
      {top.length > 0 && (
        <ul className="income-list">
          {top.map((asset) => (
            <li key={asset.asset_id} className="income-row">
              <span className="income-row__who">
                <strong>{asset.symbol ?? asset.display_name}</strong>
                <span className="muted"> · {money.brl(asset.per_unit_12m)} por cota em 12 meses</span>
              </span>
              <strong className="income-row__value">~{money.brl(asset.monthly)}/mês</strong>
            </li>
          ))}
          {rest > 0 && <li className="field__hint">e mais {rest}</li>}
        </ul>
      )}
      {estimate.missing.length > 0 && (
        <p className="field__hint">
          Sem histórico ainda (ficam fora da conta; ele é buscado em segundo plano): {estimate.missing.join(", ")}.
        </p>
      )}
    </GlassCard>
  );
}
