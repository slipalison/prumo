import type { PlanIncome } from "../../api/types";
import { useMoney } from "../../lib/privacy";

/**
 * O que o aporte soma de renda, no topo do resultado: "este aporte soma
 * ~R$ X/mês" e "sua renda vai de R$ A para R$ B". É estimativa, pelo que cada
 * cota pagou em 12 meses, líquida do IR retido na fonte (a mesma da Renda e da meta),
 * e o rótulo diz isso. O plano não espera o provedor:
 * sem histórico em cache ainda, a estimativa fica indisponível.
 */
export function PlanIncomeLine({ income }: Readonly<{ income: PlanIncome }>) {
  const money = useMoney();
  if (!income.available) {
    return (
      <p className="plan__income">
        Estimativa de renda indisponível por enquanto: o histórico de proventos ainda está sendo buscado.{" "}
        <span className="income-tag">estimativa</span>
      </p>
    );
  }
  if (income.added_monthly <= 0) return null;
  return (
    <p className="plan__income">
      Este aporte soma <strong className="gold">~{money.brl(income.added_monthly)}/mês</strong> de renda: sua renda vai de{" "}
      {money.brl(income.before_monthly)} para <strong>{money.brl(income.after_monthly)}</strong> por mês.{" "}
      <span className="income-tag">estimativa</span>
      {income.missing.length > 0 && (
        <span className="field__hint"> Sem histórico: {income.missing.join(", ")}.</span>
      )}
    </p>
  );
}

/** A renda mensal estimada de um item do plano, no cartão dele. */
export function ItemIncome({ monthly }: Readonly<{ monthly: number | null }>) {
  const money = useMoney();
  if (monthly === null || monthly <= 0) return null;
  return (
    <span className="plan-card__income" title="Estimativa pelo que a cota pagou nos últimos 12 meses, líquida do IR retido na fonte">
      +~{money.brl(monthly)}/mês de renda
    </span>
  );
}
