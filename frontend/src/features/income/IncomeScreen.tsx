import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useIncomeEvents, useIncomeSummary } from "../../api/incomeQueries";
import type { IncomeEvent, IncomeSummary } from "../../api/incomeTypes";
import type { Portfolio } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { IconPlus } from "../../components/icons";
import { Grid, Page, PageHeader, Panel, StatStrip, type Stat } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { payerKey } from "../../lib/income";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { yieldText } from "../../lib/yieldOnCost";
import { AddIncomeSheet } from "./AddIncomeSheet";
import { PendingEventsCard } from "../incomeAuto/PendingEvents";
import { EstimateCard } from "./EstimateCard";
import { FoldPanel } from "./FoldPanel";
import { IncomeGoalCard } from "./IncomeGoalCard";
import { IncomeProjectionPanel } from "./IncomeProjectionPanel";
import { EventList, UpcomingList } from "./IncomeLists";
import { ScrollRegion } from "./ScrollRegion";
import { MonthlyBars } from "./MonthlyBars";
import { FilterChip } from "./FilterChip";
import { PayerExplorer } from "./PayerExplorer";
import { YearTable } from "./YearTable";

/** Quantos proventos lançados a lista mostra. */
const RECENT = 30;

type IncomeScreenProps = { portfolio: Portfolio };

export function IncomeScreen({ portfolio }: IncomeScreenProps) {
  const summary = useIncomeSummary();
  const [adding, setAdding] = useState(false);

  return (
    <Page className="income">
      <PageHeader
        title="Quanto sua carteira paga?"
        info="Dividendos, JCP e rendimentos: o que você recebeu, o que vem aí e a meta de renda por mês."
        infoLabel="Sobre a tela Renda"
        actions={
          <MagneticButton onClick={() => setAdding(true)}>
            <IconPlus /> Lançar provento
          </MagneticButton>
        }
      />

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
    </Page>
  );
}

/** O pagador de um evento, na mesma chave de "Quem pagou". */
const eventPayerKey = (event: IncomeEvent) => event.asset_id ?? event.symbol ?? event.display_name;

function Loaded({ summary }: { summary: IncomeSummary }) {
  const money = useMoney();
  const hasLedger = summary.years.length > 0;
  const hasPayers = summary.by_payer.length > 0;
  // O pagador que filtra as listas de pagamento (escolhido em "Quem pagou"). Se ele
  // sumiu do resumo (apagaram o último provento), o filtro some junto.
  const [pick, setPick] = useState<string | null>(null);
  const payer = pick ? (summary.by_payer.find((p) => payerKey(p) === pick) ?? null) : null;
  const selected = payer ? payerKey(payer) : null;
  const clear = () => setPick(null);
  const chip = payer && <FilterChip label={payer.label} onClear={clear} />;
  const only = (events: IncomeEvent[]) => (selected === null ? events : events.filter((e) => eventPayerKey(e) === selected));

  const recent = useIncomeEvents({ limit: RECENT, ...(payer?.asset_id ? { asset: payer.asset_id } : {}) });
  const recentShown = recent.data ? only(recent.data) : undefined;
  const upcoming = only(summary.upcoming);
  const estimate = <EstimateCard estimate={summary.estimate} />;

  return (
    <>
      <Grid cols="aside-main" className="income-top">
        <IncomeGoalCard goal={summary.goal} monthlyAverage={summary.monthly_average} estimatedMonthly={summary.estimate.monthly} />
        <section className="income-top__stats" aria-label="Resumo dos últimos 12 meses">
          <StatStrip items={statsOf(summary, money)} />
        </section>
      </Grid>

      {/* O mês a mês e o que vem aí têm a mesma altura: a lista de pagamentos rola por dentro. */}
      <Grid cols="main-aside" className="income-mid">
        <Panel title="Mês a mês" className="income-chart">
          <MonthlyBars months={summary.months} goal={summary.goal?.target ?? null} />
        </Panel>
        <div className="income-side">
          <Panel title="Próximos pagamentos" className="income-upcoming" actions={chip}>
            <ScrollRegion className="income-scroll" label="Próximos pagamentos, lista com rolagem">
              <UpcomingList events={upcoming} {...(payer ? { filteredBy: payer.label } : {})} />
            </ScrollRegion>
          </Panel>
          <PendingEventsCard />
          {!hasPayers && estimate}
        </div>
      </Grid>

      {/* Quem pagou ao lado da estimativa: as duas terminam na mesma linha, sem vazio. */}
      {hasPayers && (
        <Grid cols="main-aside" className="income-explore">
          <PayerExplorer payers={summary.by_payer} total={summary.total_12m} selected={selected} onSelect={setPick} />
          {estimate}
        </Grid>
      )}

      <IncomeProjectionPanel />

      {hasLedger && (
        <Panel title="Ano por mês">
          <YearTable years={summary.years} />
        </Panel>
      )}

      <FoldPanel title="Proventos lançados" count={recentShown ? recentShown.length : undefined}>
        <div className="income-ledger">
          {payer && (
            <div className="income-ledger__filter">
              <FilterChip label={payer.label} onClear={clear} />
            </div>
          )}
          {recent.isPending && <Skeleton height={56} radius={16} />}
          {recent.isError && <p className="field__hint">Não consegui carregar os proventos agora.</p>}
          {recentShown && <EventList events={recentShown} {...(payer ? { filteredBy: payer.label } : {})} />}
        </div>
      </FoldPanel>
    </>
  );
}

/**
 * Os quatro números do topo. O yield on cost é a renda dos 12 meses sobre o que
 * foi pago; ativo de custo desconhecido sai da conta, e o número diz que é
 * parcial (o nome de quem ficou de fora está no ⓘ).
 */
function statsOf(summary: IncomeSummary, money: Money): Stat[] {
  const y = summary.yield_on_cost;
  return [
    { id: "received", label: "Recebido em 12 meses", value: money.brl(summary.total_12m), tone: "gold" },
    { id: "average", label: "Média por mês", value: money.brl(summary.monthly_average), tone: "gold" },
    { id: "upcoming", label: "A receber", value: money.brl(summary.upcoming_total) },
    {
      id: "yield",
      label: y.partial ? "Yield on cost (parcial)" : "Yield on cost",
      value: yieldText(y),
      info: (
        <>
          Renda dos últimos 12 meses ({y.basis === "estimate" ? "estimada, não é extrato" : "recebida"}) ÷ o que você pagou. Hoje:{" "}
          {money.brl(y.income_12m)} sobre {money.brl(y.cost)}.
          {y.partial && (
            <>
              {" "}
              Parcial: fica de fora o que está sem preço médio ({y.unknown_cost.join(", ")}). Informe o preço médio na folha do ativo.
            </>
          )}
        </>
      ),
    },
  ];
}
