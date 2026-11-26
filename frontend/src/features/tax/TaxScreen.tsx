import { useState } from "react";
import { errorMessage } from "../../api/client";
import type { Skipped } from "../../api/taxTypes";
import { GlassCard } from "../../components/GlassCard";
import { IconAlert } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { FoldPanel } from "../income/FoldPanel";
import { CarryforwardPanel } from "./CarryforwardPanel";
import { DeclarationPanel } from "./DeclarationPanel";
import { ExemptionPanel } from "./ExemptionPanel";
import { hasActivity } from "./monthActivity";
import { MonthDetails, MonthsPanel } from "./MonthsPanel";
import { NeverPanel, SkippedPanel } from "./SkippedPanel";
import { DISCLAIMER, DISCLAIMER_MORE } from "./taxText";
import { useCarryforward, useIrpf, useTaxMonths } from "./useTax";
import "./tax.css";

/** O ano que abre: o de hoje, se tem regra; senão o último com regra. */
function initialYear(covered: number[], today: Date): number | null {
  if (covered.length === 0) return null;
  const year = today.getFullYear();
  return covered.includes(year) ? year : Math.max(...covered.filter((y) => y <= year), Math.min(...covered));
}

function mergeSkipped(...lists: Skipped[][]): Skipped[] {
  const seen = new Set<string>();
  const out: Skipped[] = [];
  for (const item of lists.flat()) {
    const key = `${item.subject}|${item.date ?? ""}|${item.reason}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(item);
  }
  return out;
}

/**
 * Impostos: o isentômetro do mês, o DARF de cada mês com vencimento e a marca
 * de pago, a declaração do ano para copiar, o que ficou fora da conta e o
 * saldo de prejuízo que a pessoa traz de antes. A conta é do servidor
 * (docs/ir.md); a tela só mostra, e o aviso de responsabilidade fica sempre à
 * vista, em uma linha.
 */
export function TaxScreen() {
  const carry = useCarryforward();
  const covered = carry.data?.covered_years ?? [];
  const [chosen, setChosen] = useState<number | null>(null);
  const year = chosen ?? initialYear(covered, new Date());
  const months = useTaxMonths(year);
  const irpf = useIrpf(year);

  const skipped = mergeSkipped(months.data?.skipped ?? [], irpf.data?.skipped ?? []);
  const active = months.data?.months.filter(hasActivity) ?? [];

  return (
    <Page className="tax">
      <PageHeader
        title="Imposto de renda"
        info="Isentômetro, DARF de cada mês e o que vai para a declaração, a partir do seu livro de lançamentos e dos proventos."
        infoLabel="Sobre os impostos"
        actions={
          covered.length > 0 && (
            <div className="chips" role="group" aria-label="Ano-calendário">
              {covered.map((option) => (
                <button
                  key={option}
                  type="button"
                  className={cx("chip chip--quick", option === year && "is-active")}
                  aria-pressed={option === year}
                  onClick={() => setChosen(option)}
                >
                  {option}
                </button>
              ))}
            </div>
          )
        }
      />

      <aside className="tax-disclaimer" role="note" aria-label="Aviso de responsabilidade">
        <IconAlert width={18} height={18} className="tax-disclaimer__icon" />
        <p>{DISCLAIMER}</p>
        <InfoTip label="Mais sobre o aviso de responsabilidade" align="end" placement="bottom">
          {DISCLAIMER_MORE}
        </InfoTip>
      </aside>

      {carry.isError && (
        <GlassCard as="section" role="alert" className="state-card">
          <p className="state-card__text">{errorMessage(carry.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void carry.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}

      {(carry.isPending || months.isPending) && !carry.isError && (
        <div className="tax__loading" role="status" aria-label="Carregando os impostos">
          <Skeleton height={120} radius={22} />
          <Skeleton height={260} radius={22} />
        </div>
      )}
      {months.isError && (
        <GlassCard as="section" role="alert" className="state-card">
          <h2 className="state-card__title">Não consegui apurar {year}</h2>
          <p className="state-card__text">{errorMessage(months.error)}</p>
        </GlassCard>
      )}

      {months.data ? (
        <Grid cols="main-aside" alignStart>
          <MonthsPanel data={months.data} />
          <div className="tax-side">
            <ExemptionPanel data={months.data} />
            {carry.data && <CarryforwardPanel carry={carry.data} />}
            {(months.data || irpf.data) && <SkippedPanel skipped={skipped} />}
            <NeverPanel />
          </div>
        </Grid>
      ) : (
        carry.data && <CarryforwardPanel carry={carry.data} />
      )}

      {irpf.data && <DeclarationPanel irpf={irpf.data} />}

      {active.length > 0 && (
        <FoldPanel title="Como cada mês foi apurado" count={active.length}>
          <div className="tax-details">
            {active.map((month) => (
              <MonthDetails key={month.month} month={month} />
            ))}
          </div>
        </FoldPanel>
      )}
    </Page>
  );
}
