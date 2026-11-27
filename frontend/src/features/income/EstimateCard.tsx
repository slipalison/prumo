import { useMemo, useState } from "react";
import type { IncomeEstimate, IncomeEstimateAsset } from "../../api/incomeTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { CountUp } from "../../components/CountUp";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { IconAlert, IconArrowRight } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Panel } from "../../components/Layout";
import { Sheet } from "../../components/Sheet";
import { formatRelative } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";

/**
 * Quantos ativos o painel mostra; o resto abre numa folha com a lista inteira.
 * Quatro, e não cinco: com o bruto e a linha de "acima do habitual", cinco
 * linhas deixavam o painel mais alto que o "Quem pagou" ao lado e abriam um
 * vazio nele (docs/layout.md).
 */
const SHOWN = 4;

const nameOf = (asset: IncomeEstimateAsset) => asset.symbol ?? asset.display_name;

/** O rótulo pequeno da fonte e o que ele quer dizer, para o leitor de tela. */
const SOURCE = {
  b3: { label: "B3", spoken: "fonte: anúncios da B3" },
  yahoo: { label: "Yahoo", spoken: "fonte: Yahoo" },
} as const;

/** O que o ⓘ da marca explica: o mesmo texto no painel e na folha. */
const ABOVE_USUAL_INFO =
  "Pagamentos acima do habitual, como antecipações: nos últimos 12 meses o ativo pagou mais de 1,5 vez a média anual dele em até 5 anos (os 12 meses inclusive). A estimativa pode cair quando eles saírem da janela.";

/**
 * A marca de quem está acima do habitual: ícone e texto, nunca só cor. No
 * painel, estreito, o texto visível é "acima" (a linha do total, logo acima da
 * tabela, diz "acima do habitual" por extenso) e o leitor de tela ouve tudo.
 */
function AboveUsual({ short }: Readonly<{ short: boolean }>) {
  return (
    <span className="estimate__above-tag">
      <IconAlert aria-hidden="true" width={12} height={12} />
      {short ? (
        <>
          acima<span className="sr-only"> do habitual</span>
        </>
      ) : (
        "acima do habitual"
      )}
    </span>
  );
}

/** O líquido em destaque e o bruto menor embaixo (iguais quando não há JCP). */
function NetGross({ net, gross, money, strong = false }: Readonly<{ net: number; gross: number; money: Money; strong?: boolean }>) {
  const value = `~${money.brl(net)}`;
  return (
    <span className="estimate__amount">
      {strong ? <strong className="estimate__monthly">{value}</strong> : <span>{value}</span>}
      {gross !== net && <span className="estimate__gross">bruto {money.brl(gross)}</span>}
    </span>
  );
}

/**
 * A média do habitual por mês e, embaixo, a marca de quem passou bem dela (ou,
 * sem a marca, quantos anos entraram quando são menos de cinco).
 */
function Usual({ asset, money, short }: Readonly<{ asset: IncomeEstimateAsset; money: Money; short: boolean }>) {
  if (!asset.usual) return <span className="estimate__gross">histórico curto</span>;
  const years = asset.usual.years < 5 ? `${asset.usual.years} anos` : null;
  return (
    <span className="estimate__amount">
      <span>
        ~{money.brl(asset.usual.monthly)}
        {asset.above_usual && years && <span className="estimate__gross"> · {years}</span>}
      </span>
      {asset.above_usual ? <AboveUsual short={short} /> : years && <span className="estimate__gross">{years}</span>}
    </span>
  );
}

/** As colunas da tabela: a do painel (por mês e média) e a da folha (com as cotas e o por cota). */
function useColumns(full: boolean): DataColumn<IncomeEstimateAsset>[] {
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
          <span className="estimate__source">
            <span aria-hidden="true">{SOURCE[asset.source].label}</span>
            <span className="sr-only">, {SOURCE[asset.source].spoken}</span>
          </span>
        </span>
      ),
    },
    ...(full
      ? [
          {
            key: "quantity",
            label: "Cotas",
            align: "end" as const,
            wideOnly: true,
            cell: (asset: IncomeEstimateAsset) => money.quantity(asset.quantity, !Number.isInteger(asset.quantity)),
          },
          {
            key: "per-unit",
            label: "Por cota",
            align: "end" as const,
            cell: (asset: IncomeEstimateAsset) => (
              <NetGross net={asset.per_unit_12m} gross={asset.per_unit_12m_gross} money={money} />
            ),
          },
        ]
      : []),
    {
      key: "monthly",
      label: "Por mês",
      align: "end",
      cell: (asset) => <NetGross net={asset.monthly} gross={asset.monthly_gross} money={money} strong />,
    },
    {
      key: "usual",
      label: "Média 5 anos",
      align: "end",
      cell: (asset) => <Usual asset={asset} money={money} short={!full} />,
    },
  ];
}

/** "R$ X/mês de ativos acima do habitual", com o ⓘ que explica. */
function AboveUsualTotal({ estimate, count }: Readonly<{ estimate: IncomeEstimate; count: number }>) {
  const money = useMoney();
  if (estimate.above_usual_monthly <= 0) return null;
  return (
    <p className="estimate__above">
      <IconAlert aria-hidden="true" width={14} height={14} className="estimate__above-icon" />
      <span>
        <strong>~{money.brl(estimate.above_usual_monthly)}/mês</strong> de {count === 1 ? "um ativo" : `${count} ativos`} acima
        do habitual
      </span>
      <InfoTip label="O que é acima do habitual">{ABOVE_USUAL_INFO}</InfoTip>
    </p>
  );
}

/**
 * A renda mensal que a carteira de hoje renderia se pagasse o que cada cota
 * pagou nos últimos 12 meses, **líquida** do IR retido na fonte (o do JCP e os
 * 30% dos EUA) em destaque e a bruta
 * ao lado. Ações e FIIs vêm dos anúncios da B3 (que separam o JCP); EUA, ETF,
 * BDR e o que a B3 não tem, do Yahoo (bruto). Cada ativo diz a fonte num
 * rótulo pequeno, a média de até 5 anos e a marca "acima do habitual" quando
 * os 12 meses passam bem dela. É estimativa: o que a pessoa recebeu e lançou
 * (ou importou) sempre vale mais.
 *
 * O painel mostra os cinco que mais rendem; "Ver todos" abre a lista inteira,
 * do que mais rende ao que menos, numa folha (uma lista de 30 linhas no painel
 * o alongaria e deixaria um vazio no painel ao lado).
 */
export function EstimateCard({ estimate }: Readonly<{ estimate: IncomeEstimate }>) {
  const money = useMoney();
  const [all, setAll] = useState(false);
  const paying = useMemo(
    () => estimate.assets.filter((a) => a.monthly > 0).sort((a, b) => b.monthly - a.monthly),
    [estimate.assets],
  );
  const above = paying.filter((a) => a.above_usual).length;
  const top = paying.slice(0, SHOWN);
  const columns = useColumns(false);
  const fullColumns = useColumns(true);
  // O histórico chega em segundo plano: logo depois de cadastrar o ativo, ou com
  // as fontes fora do ar, ainda não há estimativa de nenhum papel.
  const unavailable = estimate.assets.length === 0 && estimate.missing.length > 0;
  const gross = estimate.monthly_gross !== estimate.monthly;

  return (
    <Panel
      className="income-estimate"
      title="Estimativa da carteira de hoje"
      info={
        <>
          Pelo que cada cota pagou nos últimos 12 meses (pela data ex), com a quantidade de hoje, líquido do IR retido na fonte:
          o do JCP e, nas ações e REITs dos EUA, os 30% retidos lá. Ações e FIIs vêm dos anúncios da B3; EUA, ETF, BDR e o que a
          B3 não tiver, do Yahoo, que não separa o JCP (numa ação da B3, o valor dele é o bruto). O rótulo ao lado de cada ativo
          diz a fonte. Não é extrato:
          {estimate.updated_at ? ` dados de ${formatRelative(estimate.updated_at)}.` : " ainda sem histórico."}
        </>
      }
      infoLabel="Como a estimativa é feita"
      actions={<span className="income-tag">estimativa</span>}
    >
      {unavailable ? (
        <p className="income-goal__now">Estimativa indisponível por enquanto.</p>
      ) : (
        <div className="estimate__head">
          <p className="income-goal__now">
            <span aria-hidden="true">~</span>
            <CountUp value={estimate.monthly} className="income-goal__value" />
            <span className="income-goal__per"> por mês, líquido</span>
          </p>
          {gross && <p className="estimate__gross estimate__gross--total">bruto ~{money.brl(estimate.monthly_gross)} por mês</p>}
        </div>
      )}
      <AboveUsualTotal estimate={estimate} count={above} />
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
        description="Do que mais rende ao que menos rende, líquido do IR retido na fonte com o bruto embaixo. Por cota é o que ela pagou nos últimos 12 meses; por mês é a posição de hoje dividida em 12; a média é a de até 5 anos, por mês."
        footer={
          <div className="estimate__total">
            <p>
              <span className="income-tag">estimativa</span> ~{money.brl(estimate.monthly)} por mês, líquido
              {gross && ` (bruto ~${money.brl(estimate.monthly_gross)})`}, {paying.length} ativos
            </p>
            <AboveUsualTotal estimate={estimate} count={above} />
          </div>
        }
      >
        <DataTable
          caption="Estimativa de renda por ativo, do que mais rende ao que menos"
          className="estimate__table"
          columns={fullColumns}
          rows={paying}
          rowKey={(asset) => asset.asset_id}
        />
      </Sheet>
    </Panel>
  );
}
