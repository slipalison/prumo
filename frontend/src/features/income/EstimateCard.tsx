import { useMemo, useState } from "react";
import type { IncomeEstimate, IncomeEstimateAsset } from "../../api/incomeTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { CountUp } from "../../components/CountUp";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { IconArrowRight } from "../../components/icons";
import { Panel } from "../../components/Layout";
import { Sheet } from "../../components/Sheet";
import { formatRelative } from "../../lib/format";
import { useMoney } from "../../lib/privacy";

/** Quantos ativos o painel mostra; o resto abre numa folha com a lista inteira. */
const SHOWN = 5;

const nameOf = (asset: IncomeEstimateAsset) => asset.symbol ?? asset.display_name;

/** As colunas da tabela: a do painel (sem as cotas) e a da folha (com elas). */
function useColumns(withQuantity: boolean): DataColumn<IncomeEstimateAsset>[] {
  const money = useMoney();
  return [
    {
      key: "asset",
      label: "Ativo",
      rowHeader: true,
      cell: (asset) => (
        <span className="estimate__asset">
          <AssetIcon symbol={asset.symbol} name={asset.display_name} size="sm" />
          {nameOf(asset)}
        </span>
      ),
    },
    ...(withQuantity
      ? [
          {
            key: "quantity",
            label: "Cotas",
            align: "end" as const,
            wideOnly: true,
            cell: (asset: IncomeEstimateAsset) => money.quantity(asset.quantity, !Number.isInteger(asset.quantity)),
          },
        ]
      : []),
    { key: "per-unit", label: "Por cota", align: "end", cell: (asset) => money.brl(asset.per_unit_12m) },
    {
      key: "monthly",
      label: "Por mês",
      align: "end",
      cell: (asset) => <strong className="estimate__monthly">~{money.brl(asset.monthly)}</strong>,
    },
  ];
}

/**
 * A renda mensal que a carteira de hoje renderia se pagasse o que cada cota
 * pagou nos últimos 12 meses. É estimativa: o histórico do Yahoo traz o valor
 * bruto na data ex, sem separar JCP de dividendo; o que a pessoa recebeu e
 * lançou (ou importou) sempre vale mais.
 *
 * O painel mostra os cinco que mais rendem; "Ver todos" abre a lista inteira,
 * do que mais rende ao que menos, numa folha (uma lista de 30 linhas no painel
 * o alongaria e deixaria um vazio no painel ao lado).
 */
export function EstimateCard({ estimate }: { estimate: IncomeEstimate }) {
  const money = useMoney();
  const [all, setAll] = useState(false);
  const paying = useMemo(
    () => estimate.assets.filter((a) => a.monthly > 0).sort((a, b) => b.monthly - a.monthly),
    [estimate.assets],
  );
  const top = paying.slice(0, SHOWN);
  const columns = useColumns(false);
  const fullColumns = useColumns(true);
  // O histórico chega em segundo plano: logo depois de cadastrar o ativo, ou com
  // o provedor fora do ar, ainda não há estimativa de nenhum papel.
  const unavailable = estimate.assets.length === 0 && estimate.missing.length > 0;

  return (
    <Panel
      className="income-estimate"
      title="Estimativa da carteira de hoje"
      info={
        <>
          Pelo que cada cota pagou nos últimos 12 meses (valor bruto, data ex; o JCP vem junto com o dividendo). Não é extrato:
          {estimate.updated_at ? ` histórico de ${formatRelative(estimate.updated_at)}.` : " ainda sem histórico."}
        </>
      }
      infoLabel="Como a estimativa é feita"
      actions={<span className="income-tag">estimativa</span>}
    >
      {unavailable ? (
        <p className="income-goal__now">Estimativa indisponível por enquanto.</p>
      ) : (
        <p className="income-goal__now">
          <span aria-hidden="true">~</span>
          <CountUp value={estimate.monthly} className="income-goal__value" />
          <span className="income-goal__per"> por mês</span>
        </p>
      )}
      {top.length > 0 && (
        <DataTable
          caption="Os ativos que mais rendem na estimativa"
          className="estimate__table"
          columns={columns}
          rows={top}
          rowKey={(asset) => asset.asset_id}
          stack={false}
        />
      )}
      {paying.length > SHOWN && (
        <button type="button" className="link-btn estimate__all" aria-haspopup="dialog" onClick={() => setAll(true)}>
          Ver todos os {paying.length} ativos
          <IconArrowRight aria-hidden="true" width={16} height={16} />
        </button>
      )}
      {estimate.missing.length > 0 && (
        <p className="field__hint">Sem histórico ainda (ficam fora da conta): {estimate.missing.join(", ")}.</p>
      )}
      <Sheet
        open={all}
        onClose={() => setAll(false)}
        title="Estimativa por ativo"
        description="Do que mais rende ao que menos rende. Por cota é o que ela pagou nos últimos 12 meses; por mês é a posição de hoje dividida em 12."
        footer={
          <p className="estimate__total">
            <span className="income-tag">estimativa</span> ~{money.brl(estimate.monthly)} por mês, {paying.length} ativos
          </p>
        }
      >
        <DataTable
          caption="Estimativa de renda por ativo, do que mais rende ao que menos"
          columns={fullColumns}
          rows={paying}
          rowKey={(asset) => asset.asset_id}
        />
      </Sheet>
    </Panel>
  );
}
