import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useIncomeEvents, useIncomeSummary } from "../../api/incomeQueries";
import type { IncomeSummary } from "../../api/incomeTypes";
import type { Portfolio } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { IconPlus } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useMoney } from "../../lib/privacy";
import { yieldText } from "../../lib/yieldOnCost";
import { AddIncomeSheet } from "./AddIncomeSheet";
import { PendingEventsCard } from "../incomeAuto/PendingEvents";
import { EstimateCard } from "./EstimateCard";
import { IncomeGoalCard } from "./IncomeGoalCard";
import { EventList, UpcomingList } from "./IncomeLists";
import { MonthlyBars } from "./MonthlyBars";
import { PayerBreakdown } from "./PayerBreakdown";
import { YearTable } from "./YearTable";

/** Quantos proventos lançados a lista mostra. */
const RECENT = 30;

type IncomeScreenProps = { portfolio: Portfolio };

export function IncomeScreen({ portfolio }: IncomeScreenProps) {
  const summary = useIncomeSummary();
  const [adding, setAdding] = useState(false);

  return (
    <div className="screen income">
      <header className="screen__header">
        <p className="eyebrow">Renda</p>
        <h1 className="screen__title">Quanto sua carteira paga?</h1>
        <p className="screen__lead">Dividendos, JCP e rendimentos: o que você recebeu, o que vem aí e a meta de renda por mês.</p>
      </header>

      <div className="income__actions">
        <MagneticButton onClick={() => setAdding(true)}>
          <IconPlus /> Lançar provento
        </MagneticButton>
      </div>

      {summary.isPending && <Skeleton height={180} radius={22} />}
      {summary.isError && (
        <GlassCard as="section" className="state-card" role="alert">
          <h2 className="state-card__title">Não consegui carregar a renda</h2>
          <p className="state-card__text">{errorMessage(summary.error)}</p>
          <MagneticButton onClick={() => void summary.refetch()}>Tentar de novo</MagneticButton>
        </GlassCard>
      )}
      {summary.data && <Loaded summary={summary.data} />}

      <AddIncomeSheet
        open={adding}
        onClose={() => setAdding(false)}
        assets={portfolio.assets}
        today={summary.data?.today ?? new Date().toISOString().slice(0, 10)}
      />
    </div>
  );
}

function Loaded({ summary }: { summary: IncomeSummary }) {
  const recent = useIncomeEvents({ limit: RECENT });
  const hasLedger = summary.years.length > 0;

  return (
    <>
      <IncomeGoalCard goal={summary.goal} monthlyAverage={summary.monthly_average} estimatedMonthly={summary.estimate.monthly} />

      <section className="income-stats" aria-label="Resumo dos últimos 12 meses">
        <Stat label="Recebido em 12 meses" value={summary.total_12m} />
        <Stat label="Média por mês" value={summary.monthly_average} />
        <Stat label="A receber" value={summary.upcoming_total} />
      </section>

      <PortfolioYieldCard yield={summary.yield_on_cost} />

      <GlassCard as="section" className="income-card" aria-labelledby="income-months">
        <h2 id="income-months" className="section-title">
          Mês a mês
        </h2>
        <MonthlyBars months={summary.months} goal={summary.goal?.target ?? null} />
      </GlassCard>

      <GlassCard as="section" className="income-card" aria-labelledby="income-upcoming">
        <h2 id="income-upcoming" className="section-title">
          Próximos pagamentos
        </h2>
        <UpcomingList events={summary.upcoming} />
      </GlassCard>

      <PendingEventsCard />

      <EstimateCard estimate={summary.estimate} />

      {hasLedger && (
        <GlassCard as="section" className="income-card" aria-labelledby="income-years">
          <h2 id="income-years" className="section-title">
            Ano por mês
          </h2>
          <YearTable years={summary.years} />
        </GlassCard>
      )}

      {summary.by_payer.length > 0 && (
        <GlassCard as="section" className="income-card" aria-labelledby="income-payers">
          <h2 id="income-payers" className="section-title">
            Quem pagou
          </h2>
          <PayerBreakdown payers={summary.by_payer} total={summary.total_12m} />
        </GlassCard>
      )}

      <GlassCard as="section" className="income-card" aria-labelledby="income-ledger">
        <h2 id="income-ledger" className="section-title">
          Proventos lançados
        </h2>
        {recent.isPending && <Skeleton height={56} radius={16} />}
        {recent.isError && <p className="field__hint">Não consegui carregar os proventos agora.</p>}
        {recent.data && <EventList events={recent.data} />}
      </GlassCard>
    </>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  const money = useMoney();
  return (
    <GlassCard className="income-stat">
      <span className="income-stat__label">{label}</span>
      <strong className="income-stat__value">{money.brl(value)}</strong>
    </GlassCard>
  );
}

/**
 * Yield on cost da carteira: a renda dos 12 meses sobre o que foi pago. Ativo de
 * custo desconhecido sai da conta, e a tela diz quem.
 */
function PortfolioYieldCard({ yield: y }: { yield: IncomeSummary["yield_on_cost"] }) {
  const money = useMoney();
  return (
    <GlassCard as="section" className="income-card income-yield" aria-labelledby="income-yield">
      <h2 id="income-yield" className="section-title">
        Yield on cost
      </h2>
      <p className="income-yield__value">{yieldText(y)}</p>
      <p className="field__hint">
        Renda dos últimos 12 meses ({y.basis === "estimate" ? "estimada, não é extrato" : "recebida"}) ÷ o que você pagou. Hoje: {money.brl(y.income_12m)} sobre{" "}
        {money.brl(y.cost)}.
      </p>
      {y.partial && (
        <p className="field__hint">
          Parcial: fica de fora o que está sem preço médio ({y.unknown_cost.join(", ")}). Informe o preço médio na folha do ativo.
        </p>
      )}
    </GlassCard>
  );
}
