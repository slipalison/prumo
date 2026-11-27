import { motion } from "motion/react";
import { useState } from "react";
import { errorMessage } from "../../api/client";
import { useResults } from "../../api/results";
import type { FundamentalsGap } from "../../api/fundamentalsTypes";
import type { AssetResults, Comparison, Quarter } from "../../api/resultsTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { Disclosure } from "../../components/Disclosure";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { formatDay, formatPercent } from "../../lib/format";
import { formatIsoDate } from "../../lib/income";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { Missing } from "../fundamentals/Missing";
import { NOT_APPLICABLE_TEXT, UNSUPPORTED_TEXT, notApplicableReason } from "../fundamentals/fundamentalsText";
import { ChangeTag } from "./ChangeTag";
import { FundMonths, type FundEntry } from "./FundMonths";
import { MiniBars } from "./MiniBars";
import { ResultsBubbles, type CompanyEntry } from "./ResultsBubbles";
import { MODE_LABEL, pointsText, quarterLabel, quarterShort, TOP_LINE_LABEL, type BubbleMode } from "./resultsText";
import "./results.css";

const latestOf = (entry: CompanyEntry): Quarter | undefined => entry.company.quarters.at(-1);

const amount = (money: Money, value: number | null, gap: FundamentalsGap | null) =>
  value === null ? <Missing gap={gap} /> : money.compact(value);

/** A linha de cima de banco e seguradora não é receita: a linha diz o que é. */
function TopLineValue({ entry, money }: { entry: CompanyEntry; money: Money }) {
  const latest = latestOf(entry);
  const value = latest ? amount(money, latest.top_line.value, latest.top_line.gap) : <Missing gap="no_filing" />;
  if (entry.company.top_line === "revenue") return <>{value}</>;
  return (
    <span className="results__stack">
      <span>{value}</span>
      <span className="results__note">{TOP_LINE_LABEL[entry.company.top_line].toLowerCase()}</span>
    </span>
  );
}

function Margin({ entry, comparison }: { entry: CompanyEntry; comparison: Comparison | null }) {
  const latest = latestOf(entry);
  const reason = notApplicableReason(entry.company.template, entry.company.holding);
  if (reason) {
    return (
      <span className="results__na">
        <Missing gap={reason === "holding" ? "holding" : "financial_template"} />
        <InfoTip label={`Por que a margem não se aplica a ${entry.symbol}`} align="end">
          {NOT_APPLICABLE_TEXT[reason]}
        </InfoTip>
      </span>
    );
  }
  if (!latest) return <Missing gap="no_filing" />;
  const margin = latest.net_margin;
  const points = comparison?.net_margin_points.value ?? null;
  return (
    <span className="results__stack">
      <span>{margin.value === null ? <Missing gap={margin.gap} /> : formatPercent(margin.value * 100)}</span>
      {points !== null && <span className="results__note">{pointsText(points)} a/a</span>}
    </span>
  );
}

function quarterColumns(money: Money): DataColumn<CompanyEntry>[] {
  return [
    {
      key: "company",
      label: "Empresa",
      rowHeader: true,
      cell: (row) => (
        <span className="results__who">
          <span className="results__symbol">{row.symbol}</span>
          <span className="results__name">{row.company.name}</span>
        </span>
      ),
    },
    {
      key: "quarter",
      label: "Trimestre",
      cell: (row) => {
        const latest = latestOf(row);
        if (!latest) return <span className="results__note">sem ITR nem DFP lidos</span>;
        return (
          <span className="results__stack">
            <span>{quarterLabel(latest)}</span>
            <span className="results__note">
              {latest.received_on ? `entregue em ${formatIsoDate(latest.received_on)}` : "data de entrega não lida"}
            </span>
          </span>
        );
      },
    },
    {
      key: "top",
      label: "Receita",
      header: (
        <span className="results__th">
          Receita
          <InfoTip label="O que é a receita aqui" align="end">
            Receita líquida do trimestre. Em banco, o resultado bruto da intermediação financeira; em seguradora, o
            resultado bruto de seguros: a receita deles não se compara com a de indústria.
          </InfoTip>
        </span>
      ),
      align: "end",
      cell: (row) => <TopLineValue entry={row} money={money} />,
    },
    {
      key: "top-change",
      label: "Variação da receita",
      align: "end",
      cell: (row) => (
        <span className="results__changes">
          <ChangeTag change={row.company.year_ago?.top_line ?? null} line="top" against="year_ago" />
          <ChangeTag change={row.company.previous_quarter?.top_line ?? null} line="top" against="previous_quarter" />
        </span>
      ),
    },
    {
      key: "profit",
      label: "Lucro líquido",
      align: "end",
      cell: (row) => {
        const latest = latestOf(row);
        return latest ? amount(money, latest.net_income.value, latest.net_income.gap) : <Missing gap="no_filing" />;
      },
    },
    {
      key: "profit-change",
      label: "Variação do lucro",
      align: "end",
      cell: (row) => (
        <span className="results__changes">
          <ChangeTag change={row.company.year_ago?.net_income ?? null} line="profit" against="year_ago" />
          <ChangeTag change={row.company.previous_quarter?.net_income ?? null} line="profit" against="previous_quarter" />
        </span>
      ),
    },
    {
      key: "margin",
      label: "Margem líquida",
      align: "end",
      cell: (row) => <Margin entry={row} comparison={row.company.year_ago} />,
    },
  ];
}

/** Os oito trimestres de uma empresa: o lucro de cada um, em barras. */
function SeriesCard({ entry, money }: { entry: CompanyEntry; money: Money }) {
  const quarters = entry.company.quarters;
  const text = (q: Quarter) => {
    const profit = q.net_income.value === null ? "sem número" : money.compact(q.net_income.value);
    const top = q.top_line.value === null ? "sem número" : money.compact(q.top_line.value);
    return `${quarterLabel(q)}: lucro ${profit}; ${TOP_LINE_LABEL[entry.company.top_line].toLowerCase()} ${top}`;
  };
  return (
    <li className="results-series__card">
      <div className="results-series__head">
        <span className="results__symbol">{entry.symbol}</span>
        <span className="results__note">lucro líquido por trimestre</span>
      </div>
      {quarters.length === 0 ? (
        <p className="results__note">Sem trimestre nos arquivos da CVM lidos.</p>
      ) : (
        <MiniBars
          label={`${entry.symbol}, ${quarters.map(text).join(". ")}.`}
          bars={quarters.map((q) => ({
            key: q.period_end,
            label: quarterShort(q),
            value: q.net_income.value,
            title: text(q),
          }))}
        />
      )}
    </li>
  );
}

type Split = { companies: CompanyEntry[]; funds: FundEntry[]; unsupported: AssetResults[] };

function split(assets: AssetResults[]): Split {
  const companies: CompanyEntry[] = [];
  const funds: FundEntry[] = [];
  const unsupported: AssetResults[] = [];
  for (const asset of assets) {
    if (asset.kind === "company") {
      companies.push({ assetId: asset.asset_id, symbol: asset.symbol, weight: asset.weight, company: asset.company });
    } else if (asset.kind === "fund") {
      funds.push({ assetId: asset.asset_id, symbol: asset.symbol, fund: asset.fund });
    } else {
      unsupported.push(asset);
    }
  }
  // O que pesa mais na carteira vem primeiro.
  companies.sort((a, b) => (b.weight ?? 0) - (a.weight ?? 0) || a.symbol.localeCompare(b.symbol));
  return { companies, funds, unsupported };
}

function Report({ assets }: { assets: AssetResults[] }) {
  const money = useMoney();
  const [mode, setMode] = useState<BubbleMode>("year_ago");
  const { companies, funds, unsupported } = split(assets);
  const synced = companies.map((c) => c.company.synced_at).find((s) => s !== null) ?? null;

  if (companies.length === 0 && funds.length === 0) {
    return (
      <Panel aria-label="Sem resultados">
        <p className="field__hint">
          Sem ações nacionais nem FIIs com dado da CVM na carteira. Os resultados aparecem aqui quando houver.
        </p>
        {unsupported.length > 0 && <UnsupportedList assets={unsupported} />}
      </Panel>
    );
  }

  return (
    <>
      {companies.length > 0 && (
        <Panel
          title="O trimestre mais novo"
          flush
          info={
            <>
              Receita, lucro dos controladores e margem do último trimestre entregue à CVM, contra o mesmo trimestre do
              ano anterior (a/a) e contra o trimestre anterior (t/t). O 4º trimestre é a DFP do ano menos os nove meses
              do ITR. Com prejuízo numa das pontas não há percentual: a linha diz &quot;saiu do prejuízo&quot;,
              &quot;prejuízo menor&quot; e assim por diante.
            </>
          }
        >
          <DataTable
            caption="O trimestre mais novo de cada empresa da carteira"
            columns={quarterColumns(money)}
            rows={companies}
            rowKey={(row) => row.assetId}
            className="results__table"
          />
        </Panel>
      )}
      {companies.length > 0 && (
        <Grid cols="main-aside" alignStart>
          <Panel
            title="Lucro × receita"
            info="Cada bolha é uma empresa: a variação do lucro no eixo horizontal, a da receita (em banco e seguradora, do resultado bruto) no vertical, e o tamanho é o peso na carteira. Não é ranking: mostra quem teve o quê no trimestre."
            actions={
              <div className="segmented" role="group" aria-label="Comparar o trimestre">
                {(["year_ago", "previous_quarter"] as const).map((option) => (
                  <button
                    key={option}
                    type="button"
                    className={cx("segmented__option", mode === option && "is-active")}
                    aria-pressed={mode === option}
                    title={MODE_LABEL[option]}
                    onClick={() => setMode(option)}
                  >
                    {mode === option && <motion.span layoutId="results-mode" className="segmented__knob" />}
                    <span className="segmented__label">{option === "year_ago" ? "Ano anterior" : "Trimestre anterior"}</span>
                  </button>
                ))}
              </div>
            }
          >
            <ResultsBubbles entries={companies} mode={mode} />
          </Panel>
          <Panel title="Oito trimestres" info="O lucro líquido dos controladores de cada trimestre; barra para baixo, listrada, é prejuízo.">
            <ul className="results-series">
              {companies.map((entry) => (
                <SeriesCard key={entry.assetId} entry={entry} money={money} />
              ))}
            </ul>
          </Panel>
        </Grid>
      )}
      {funds.length > 0 && (
        <Panel
          title="FIIs mês a mês"
          flush
          info={
            <>
              Do informe mensal da CVM: o rendimento do mês sobre o valor patrimonial da cota, como o fundo informou, e o
              valor patrimonial por cota. O informe não traz o resultado do fundo (ele só sai no informe trimestral, somado
              por semestre) nem o rendimento em reais por cota: esse está na Renda, pelos avisos da B3. Rendimento negativo
              ou acima de 10% no mês é erro de preenchimento e fica sem número.
            </>
          }
        >
          <FundMonths funds={funds} />
        </Panel>
      )}
      <footer className="results__footer">
        {unsupported.length > 0 && <UnsupportedList assets={unsupported} />}
        <p className="results__note">
          Fonte: CVM (ITR, DFP e informe mensal de FII){synced ? ` · conferido em ${formatDay(synced)}` : ""}. Isto mostra
          o que as companhias entregaram; não diz o que fazer.
        </p>
      </footer>
    </>
  );
}

function UnsupportedList({ assets }: { assets: AssetResults[] }) {
  return (
    <Disclosure summary="Sem resultados" count={String(assets.length)}>
      <ul className="results__unsupported">
        {assets.map((asset) => (
          <li key={asset.asset_id}>
            <strong>{asset.symbol}</strong>: {asset.kind === "unsupported" ? UNSUPPORTED_TEXT[asset.reason] : ""}
          </li>
        ))}
      </ul>
    </Disclosure>
  );
}

/**
 * Resultados trimestrais da carteira (`#/analise/resultados`): para cada
 * empresa, o trimestre mais novo contra um ano antes e contra o anterior, as
 * bolhas de lucro × receita pelo peso na carteira e os oito trimestres; para
 * cada FII, o informe mensal. Prejuízo tem rótulo, não percentual; holding,
 * banco e seguradora dizem "não se aplica" onde a margem enganaria. Nada aqui
 * manda comprar ou vender (docs/resultados.md).
 */
export function ResultsScreen() {
  const results = useResults();
  return (
    <Page className="results">
      <PageHeader
        eyebrow="Análise"
        title="Resultados trimestrais"
        infoLabel="Sobre os resultados"
        info={
          <>
            O que cada empresa da carteira entregou à CVM no último trimestre, comparado com o mesmo trimestre do ano
            anterior e com o anterior. Os números são os da CVM, em reais; a conta está na documentação do Prumo.
          </>
        }
        actions={
          <a className="link-btn" href="#/analise">
            Voltar à análise
          </a>
        }
      />
      {results.isPending && (
        <Panel aria-label="Carregando os resultados">
          <Skeleton height={28} width="40%" />
          <Skeleton height={160} />
        </Panel>
      )}
      {results.isError && (
        <Panel role="alert" aria-label="Erro ao carregar">
          <p>{errorMessage(results.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void results.refetch()}>
            Tentar de novo
          </MagneticButton>
        </Panel>
      )}
      {results.data && <Report assets={results.data.assets} />}
    </Page>
  );
}
