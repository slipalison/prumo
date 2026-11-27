import { useState, type ReactNode } from "react";
import { errorMessage } from "../../api/client";
import { useRisk } from "../../api/risk";
import type { AssetRisk, Risk, RiskMetrics, RiskWindow } from "../../api/riskTypes";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { GlassCard } from "../../components/GlassCard";
import { Grid, Page, PageHeader, Panel, StatStrip, type Stat } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { cx } from "../../lib/cx";
import { formatPercent } from "../../lib/format";
import { formatDay, formatSignedPercent } from "../performance/performanceFormat";
import { BubbleChart, type Bubble } from "./BubbleChart";
import { CorrelationMatrix, CorrelationScale } from "./CorrelationMatrix";
import {
  describeCoefficient,
  drawdownDates,
  drawdownShort,
  formatCoefficient,
  formatVolatility,
  windowLabel,
  WINDOWS,
} from "./riskFormat";
import "../performance/performance.css";
import "./risk.css";

/** A subrota do Desempenho: `#/desempenho/risco`. */
export const RISK_SECTION = "risco";

/**
 * O link do cabeçalho do Desempenho para esta tela (uma linha só na tela-mãe;
 * quem integra transforma os links das subrotas numa subnavegação).
 */
export function RiskLink() {
  return (
    <a className="risk-entry" href={`#/desempenho/${RISK_SECTION}`}>
      Risco: volatilidade, queda máxima e correlação →
    </a>
  );
}

/**
 * Risco da carteira: quanto a carteira e cada ativo oscilaram, a maior queda,
 * quanto os ativos andam juntos e o risco × retorno. É retrato da janela
 * escolhida, nunca previsão nem recomendação. Método em `docs/risco.md`.
 */
export function RiskScreen() {
  const [selected, setSelected] = useState<RiskWindow>("12m");
  const risk = useRisk(selected);

  return (
    <Page className="risk">
      <PageHeader
        eyebrow="Desempenho"
        title="Risco da carteira"
        info={
          <>
            Quanto a carteira e cada ativo oscilaram na janela, a maior queda, quanto os ativos andam juntos e de onde vem
            a oscilação da carteira de hoje. É retrato do passado, com os retornos diários em reais e os proventos dentro:
            não é previsão nem recomendação.
          </>
        }
        infoLabel="Sobre a tela Risco"
        actions={
          <>
            <a className="chip chip--quick risk-link" href="#/desempenho">
              ← Rentabilidade
            </a>
            <div className="chips" role="group" aria-label="Janela">
              {WINDOWS.map((w) => (
                <button
                  key={w.id}
                  type="button"
                  className={cx("chip chip--quick", selected === w.id && "is-active")}
                  aria-pressed={selected === w.id}
                  onClick={() => setSelected(w.id)}
                >
                  {w.label}
                </button>
              ))}
            </div>
          </>
        }
      />

      {risk.isPending && (
        <div className="risk__loading" role="status" aria-label="Carregando o risco">
          <Skeleton height={96} radius={22} />
          <Skeleton height={300} radius={22} />
        </div>
      )}
      {risk.isError && (
        <GlassCard as="section" className="state-card" role="alert">
          <h2 className="state-card__title">Não consegui carregar o risco</h2>
          <p className="state-card__text">{errorMessage(risk.error)}</p>
          <MagneticButton onClick={() => void risk.refetch()}>Tentar de novo</MagneticButton>
        </GlassCard>
      )}
      {risk.data && <Loaded risk={risk.data} />}
    </Page>
  );
}

function Loaded({ risk }: { risk: Risk }) {
  const priced = risk.assets.filter((a) => a.status === "ok");
  if (risk.assets.length === 0 && !risk.portfolio) {
    return (
      <GlassCard as="section" className="state-card">
        <h2 className="state-card__title">Ainda não há o que medir</h2>
        <p className="state-card__text">
          O risco sai da série da carteira e do histórico de preço de cada ativo. Cadastre seus ativos na{" "}
          <a href="#/carteira">Carteira</a> ou importe o extrato da B3 em Ajustes.
        </p>
      </GlassCard>
    );
  }
  return (
    <>
      {risk.rebuilding_since && (
        <p className="risk__notice" role="status">
          Refazendo a série desde {formatDay(risk.rebuilding_since, true)}. Os números se acertam em instantes.
        </p>
      )}
      {risk.from && risk.to && (
        <p className="risk__window">
          {windowLabel(risk.window)}: do fechamento de {formatDay(risk.from, true)} ao último fechamento guardado,{" "}
          {formatDay(risk.to, true)} ({risk.trading_days} pregões; o de hoje entra depois do fechamento).
        </p>
      )}
      <section aria-label="Resumo do risco">
        <StatStrip items={summary(risk)} />
      </section>
      {/* O gráfico (curto) divide a linha com os destaques, que têm altura
          parecida; a matriz, que cresce com o número de ativos, vem embaixo, na
          largura inteira: nada fica com um vão ao lado. */}
      <Grid cols="main-aside">
        <RiskReturnPanel risk={risk} priced={priced} />
        <div className="risk__aside">
          <PairsPanel risk={risk} />
          <SharesPanel risk={risk} />
        </div>
      </Grid>
      <CorrelationPanel risk={risk} />
      <AssetsPanel risk={risk} />
    </>
  );
}

/** "a série começa em 10/02/2026" quando a série não cobre a janela inteira. */
function shortNote(metrics: RiskMetrics | null): ReactNode {
  if (!metrics || metrics.full_window) return null;
  return <span className="stat__note">a série começa em {formatDay(metrics.from, true)}</span>;
}

function summary(risk: Risk): Stat[] {
  const p = risk.portfolio;
  const b = risk.benchmark;
  const ibovVol = b?.volatility_percent ?? null;
  const ibovRet = b?.return_percent ?? null;
  const short = p && p.volatility_percent === null;
  const items: Stat[] = [
    {
      id: "volatility",
      label: "Volatilidade da carteira",
      value: p?.volatility_percent != null ? formatVolatility(p.volatility_percent) : "—",
      detail: (
        <>
          {ibovVol !== null ? `Ibovespa ${formatVolatility(ibovVol)}` : "Ibovespa sem dado na janela"}
          {short && <span className="stat__note">histórico curto: {p.returns} pregões</span>}
          {shortNote(p)}
        </>
      ),
      info: "Quanto a cota da carteira sobe e desce de um pregão para o outro, levado para o ano (desvio-padrão dos retornos diários × √252). Diz o tamanho da oscilação, não o sentido.",
    },
    {
      id: "drawdown",
      label: "Queda máxima",
      value: p?.drawdown ? formatSignedPercent(p.drawdown.depth_percent) : p?.return_percent != null ? "Sem queda" : "—",
      detail: p?.drawdown ? drawdownDates(p.drawdown) : b?.drawdown ? `Ibovespa ${formatSignedPercent(b.drawdown.depth_percent)}` : undefined,
      info: "A maior queda da cota de um pico até o fundo seguinte, dentro da janela, e quando ela voltou ao pico.",
      ...(p?.drawdown ? { tone: "down" as const } : {}),
    },
    {
      id: "return",
      label: `Retorno em ${windowLabel(risk.window)}`,
      value: p?.return_percent != null ? formatSignedPercent(p.return_percent) : "—",
      detail: ibovRet !== null ? `Ibovespa ${formatSignedPercent(ibovRet)}` : undefined,
      info: "Pelo método de cotas, com os proventos recebidos: o aporte não conta como ganho.",
    },
    {
      id: "model",
      label: "Com os pesos de hoje",
      value: risk.model ? formatVolatility(risk.model.volatility_percent) : "—",
      detail: risk.model
        ? `${formatPercent(risk.model.covered_weight_percent, 1)} da carteira na conta`
        : `precisa de ${risk.limits.min_common} pregões em comum na janela inteira`,
      info: "Quanto a carteira de hoje teria oscilado na janela se os pesos fossem os de hoje o tempo todo. Renda fixa por valor e ativo sem histórico na janela inteira entram como se não oscilassem.",
    },
  ];
  return items;
}

function RiskReturnPanel({ risk, priced }: { risk: Risk; priced: AssetRisk[] }) {
  const bubbles: Bubble[] = priced
    .filter((a) => a.metrics?.volatility_percent != null && a.metrics.return_percent != null)
    .map((a) => ({
      key: a.asset_id,
      label: a.label,
      x: a.metrics?.volatility_percent ?? 0,
      y: a.metrics?.return_percent ?? 0,
      weight: a.weight_percent,
      kind: "asset" as const,
    }));
  const assetCount = bubbles.length;
  const p = risk.portfolio;
  if (p?.volatility_percent != null && p.return_percent != null) {
    bubbles.push({ key: "portfolio", label: "Sua carteira", x: p.volatility_percent, y: p.return_percent, kind: "portfolio" });
  }
  const b = risk.benchmark;
  if (b?.volatility_percent != null && b.return_percent != null) {
    bubbles.push({ key: "benchmark", label: b.label, x: b.volatility_percent, y: b.return_percent, kind: "benchmark" });
  }
  const label = `Risco × retorno em ${windowLabel(risk.window)}: ${bubbles
    .map((x) => `${x.label}, volatilidade ${formatVolatility(x.x)}, retorno ${formatSignedPercent(x.y)}`)
    .join("; ")}.`;
  return (
    <Panel
      title="Risco × retorno"
      info="Cada bolha é um ativo: mais à direita, mais ele oscilou; mais acima, mais rendeu na janela (com proventos, em reais); a área é o peso na carteira. A carteira e o Ibovespa ficam de referência. Os números estão na tabela por ativo."
      infoLabel="Como ler o risco × retorno"
    >
      <ul className="legend" aria-label="Marcas do gráfico">
        <li className="legend__item">
          <span className="risk-key risk-key--asset" aria-hidden="true" />
          <span>Ativos (área = peso)</span>
        </li>
        <li className="legend__item">
          <span className="risk-key risk-key--portfolio" aria-hidden="true" />
          <span>Sua carteira</span>
        </li>
        <li className="legend__item">
          <span className="risk-key risk-key--benchmark" aria-hidden="true" />
          <span>Ibovespa</span>
        </li>
      </ul>
      {assetCount === 0 ? (
        <p className="field__hint">
          O gráfico aparece com um ativo que tenha pelo menos {risk.limits.min_returns} pregões de preço na janela.
        </p>
      ) : (
        <>
          <p className="bubble-chart__axis-name" aria-hidden="true">
            ↑ Retorno em {windowLabel(risk.window)}
          </p>
          <BubbleChart bubbles={bubbles} label={label} />
        </>
      )}
    </Panel>
  );
}

/** Os pares que mais andam juntos, em texto (na matriz, com contorno). */
function PairsPanel({ risk }: { risk: Risk }) {
  const byId = new Map(risk.assets.map((a) => [a.asset_id, a.label]));
  const pairs = risk.correlation.top_pairs;
  return (
    <Panel
      compact
      title="Os pares que mais andam juntos"
      info="Os maiores valores da matriz de correlação, fora a diagonal: os ativos que mais subiram e caíram nos mesmos pregões da janela."
      infoLabel="De onde saem os pares"
    >
      {pairs.length === 0 ? (
        <p className="field__hint">
          Aparecem com dois ativos que tenham pelo menos {risk.limits.min_common} pregões em comum na janela.
        </p>
      ) : (
        <ol className="risk-list">
          {pairs.map((pair) => (
            <li key={`${pair.a}-${pair.b}`} className="risk-list__item">
              <span className="risk-list__name">
                {byId.get(pair.a)} e {byId.get(pair.b)}
              </span>
              <strong className="risk-list__value">{formatCoefficient(pair.value)}</strong>
              <span className="risk-list__note">{describeCoefficient(pair.value)}</span>
            </li>
          ))}
        </ol>
      )}
    </Panel>
  );
}

/** As maiores partes do risco da carteira de hoje (a lista inteira está na tabela). */
function SharesPanel({ risk }: { risk: Risk }) {
  const top = risk.assets
    .filter((a) => a.risk_share_percent !== null)
    .sort((a, b) => (b.risk_share_percent ?? 0) - (a.risk_share_percent ?? 0))
    .slice(0, 5);
  return (
    <Panel
      compact
      title="As maiores partes do risco"
      info="De onde vem a oscilação da carteira de hoje: peso × o quanto o ativo anda com a carteira, dividido pela oscilação dela. As partes de todos somam 100%; a lista inteira está na tabela por ativo."
      infoLabel="Como se calcula a parte do risco"
    >
      {top.length === 0 ? (
        <p className="field__hint">
          Aparecem com ativos que tenham histórico na janela inteira (pelo menos {risk.limits.min_common} pregões).
        </p>
      ) : (
        <ol className="risk-list">
          {top.map((asset) => (
            <li key={asset.asset_id} className="risk-list__item">
              <span className="risk-list__name">{asset.label}</span>
              <strong className="risk-list__value">{formatPercent(asset.risk_share_percent ?? 0, 1)}</strong>
              <span className="risk-list__note">peso {formatPercent(asset.weight_percent, 1)}</span>
            </li>
          ))}
        </ol>
      )}
    </Panel>
  );
}

function CorrelationPanel({ risk }: { risk: Risk }) {
  const byId = new Map(risk.assets.map((a) => [a.asset_id, a.label]));
  const ids = risk.correlation.asset_ids;
  const labels = ids.map((id) => byId.get(id) ?? "?");
  const left = risk.assets.filter((a) => a.status === "ok" && !a.in_correlation).map((a) => a.label);
  return (
    <Panel
      title="Correlação entre os ativos"
      info={`Correlação dos retornos diários (com proventos, em reais; o ativo dos EUA já convertido), só nos pregões em que os dois têm preço, com pelo menos ${risk.limits.min_common} em comum. Perto de 1: sobem e caem juntos; perto de 0: sem relação; perto de −1: um sobe quando o outro cai.`}
      infoLabel="Como se calcula a correlação"
    >
      {ids.length < 2 ? (
        <p className="field__hint">
          A correlação precisa de dois ativos com pelo menos {risk.limits.min_common} pregões em comum na janela.
        </p>
      ) : (
        <>
          <CorrelationScale />
          <CorrelationMatrix
            labels={labels}
            ids={ids}
            values={risk.correlation.values}
            pairs={risk.correlation.top_pairs}
            caption={`Correlação dos retornos diários entre os ativos, em ${windowLabel(risk.window)}`}
          />
        </>
      )}
      {left.length > 0 && (
        <p className="field__hint">
          Fora da matriz, com menos de {risk.limits.min_common} pregões na janela: {left.join(", ")}.
        </p>
      )}
    </Panel>
  );
}

type Row =
  | { kind: "reference"; key: string; label: string; metrics: RiskMetrics | null }
  | { kind: "asset"; key: string; asset: AssetRisk };

/** Por que o ativo não tem número (ou tem só parte deles). */
function statusNote(asset: AssetRisk, risk: Risk): string | null {
  switch (asset.status) {
    case "no_series":
      return "Sem preço de mercado: na carteira de hoje, conta como se não oscilasse.";
    case "pays_along_the_way":
      return "Paga juros ou renda no caminho: o preço do Tesouro cai a cada pagamento e pareceria perda, então fica fora.";
    case "short":
      return `Histórico curto: ${asset.metrics?.returns ?? 0} pregões na janela (o mínimo é ${risk.limits.min_returns}).`;
    case "ok":
      return asset.metrics && !asset.metrics.full_window
        ? `Histórico desde ${formatDay(asset.metrics.from, true)}: fica fora da conta da carteira de hoje.`
        : null;
  }
}

function volatilityText(metrics: RiskMetrics | null): string {
  return metrics?.volatility_percent != null ? formatPercent(metrics.volatility_percent, 1) : "—";
}

function DrawdownCell({ metrics }: { metrics: RiskMetrics | null }) {
  if (!metrics || metrics.return_percent === null) return <>—</>;
  if (!metrics.drawdown) return <>Sem queda</>;
  return (
    <span className="risk-dd" title={`Do pico ao fundo: ${drawdownDates(metrics.drawdown)}`}>
      <span>{formatSignedPercent(metrics.drawdown.depth_percent)}</span>
      <span className="risk-dd__dates">{drawdownShort(metrics.drawdown)}</span>
    </span>
  );
}

function AssetsPanel({ risk }: { risk: Risk }) {
  const rows: Row[] = [
    { kind: "reference", key: "portfolio", label: "Sua carteira", metrics: risk.portfolio },
    ...(risk.benchmark ? [{ kind: "reference" as const, key: "benchmark", label: risk.benchmark.label, metrics: risk.benchmark }] : []),
    ...risk.assets.map((asset) => ({ kind: "asset" as const, key: asset.asset_id, asset })),
  ];
  const metricsOf = (row: Row) => (row.kind === "asset" ? row.asset.metrics : row.metrics);
  const columns: DataColumn<Row>[] = [
    {
      key: "label",
      label: "Ativo",
      rowHeader: true,
      cell: (row) => {
        if (row.kind === "reference") return <span className="risk-ref">{row.label}</span>;
        const note = statusNote(row.asset, risk);
        return (
          <span className="risk-asset">
            <span>{row.asset.label}</span>
            {note && <span className="risk-asset__note">{note}</span>}
          </span>
        );
      },
    },
    {
      key: "weight",
      label: "Peso",
      align: "end",
      cell: (row) => (row.kind === "asset" ? formatPercent(row.asset.weight_percent, 1) : "—"),
    },
    {
      key: "volatility",
      label: "Volatilidade",
      header: "Volatilidade ao ano",
      align: "end",
      cell: (row) => volatilityText(metricsOf(row)),
    },
    {
      key: "return",
      label: "Retorno",
      align: "end",
      cell: (row) => {
        const value = metricsOf(row)?.return_percent;
        return value == null ? "—" : formatSignedPercent(value);
      },
    },
    {
      key: "drawdown",
      label: "Queda máxima",
      align: "end",
      cell: (row) => <DrawdownCell metrics={metricsOf(row)} />,
    },
    {
      key: "share",
      label: "Parte do risco",
      align: "end",
      cell: (row) => {
        if (row.kind === "reference") return row.key === "portfolio" && risk.model ? "100%" : "—";
        const share = row.asset.risk_share_percent;
        if (share === null) return "—";
        return (
          <span className="risk-share">
            <span className="risk-share__bar" aria-hidden="true">
              <span style={{ width: `${Math.min(Math.max(share, 0), 100)}%` }} />
            </span>
            {formatPercent(share, 1)}
          </span>
        );
      },
    },
  ];
  return (
    <Panel
      title="Por ativo"
      flush
      info={
        <>
          Volatilidade, retorno e queda de cada ativo na janela, com os proventos e em reais. A parte do risco é quanto da
          oscilação da carteira de hoje vem de cada ativo: peso × o quanto ele anda com a carteira, dividido pela
          oscilação dela (as partes somam 100%; negativa quando o ativo anda contra o resto).
        </>
      }
      infoLabel="Como se calcula cada coluna"
    >
      <DataTable
        className="risk-table"
        caption={`Risco por ativo em ${windowLabel(risk.window)}, com a carteira e o Ibovespa de referência`}
        columns={columns}
        rows={rows}
        rowKey={(row) => row.key}
        rowClassName={(row) => (row.kind === "reference" ? "risk-row--reference" : row.asset.status !== "ok" ? "risk-row--muted" : undefined)}
      />
    </Panel>
  );
}
