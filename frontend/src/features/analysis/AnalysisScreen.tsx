import { useId } from "react";
import { errorMessage } from "../../api/client";
import type { Analysis } from "../../api/analysisTypes";
import { GlassCard } from "../../components/GlassCard";
import { MagneticButton } from "../../components/MagneticButton";
import { Plumb } from "../../components/Plumb";
import { Skeleton } from "../../components/Skeleton";
import { CLASS_META } from "../../lib/assetClasses";
import { attentionText } from "../../lib/analysisText";
import { formatRelative, plural } from "../../lib/format";
import { AttentionCard } from "./AttentionCard";
import { DimensionCard } from "./DimensionCard";
import { HighlightsCard } from "./HighlightsCard";
import { ProfileCard } from "./ProfileCard";
import { ReferencesCard } from "./ReferencesCard";
import { useAnalysis } from "./useAnalysis";

/**
 * Análise da carteira: o que olhar além da meta por classe. Quatro dimensões
 * (diversificação, dividendos, rentabilidade e qualidade), cada uma pontuada
 * pelo percentual de regras ligadas, com dado, que estão ok. As regras e os
 * limites são seus, em Ajustes. Nada aqui manda comprar ou vender.
 */
export function AnalysisScreen() {
  const analysis = useAnalysis();

  return (
    <div className="screen analysis">
      <header className="screen__header">
        <p className="eyebrow">Análise</p>
        <h1 className="screen__title">Análise da carteira</h1>
        <p className="screen__lead">
          O que olhar além da meta por classe: diversificação, dividendos e rentabilidade. Cada dimensão mostra quantas
          das suas regras estão ok, com o número exato. Regra sem dado para medir fica de fora da conta; ela não conta
          como falha.
        </p>
      </header>

      {analysis.isPending && (
        <GlassCard as="section" aria-label="Carregando a análise" className="analysis__loading">
          <Skeleton height={28} width="55%" />
          <Skeleton height={64} />
          <Skeleton height={140} />
        </GlassCard>
      )}
      {analysis.isError && (
        <GlassCard as="section" role="alert" className="analysis__error">
          <p>{errorMessage(analysis.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void analysis.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}
      {analysis.data && <Report analysis={analysis.data} />}
    </div>
  );
}

function Report({ analysis }: { analysis: Analysis }) {
  return (
    <>
      <Verdict analysis={analysis} />
      <ProfileCard profile={analysis.profile} />
      {analysis.dimensions.map((dimension) => (
        <DimensionCard key={dimension.id} dimension={dimension} />
      ))}
      <AttentionCard attention={analysis.attention} />
      <HighlightsCard highlights={analysis.highlights} />
      <ReferencesCard references={analysis.references} />
      <SourcesCard analysis={analysis} />
    </>
  );
}

function Verdict({ analysis }: { analysis: Analysis }) {
  const measured = analysis.dimensions.reduce((sum, d) => sum + (d.score?.measured ?? 0), 0);
  const calm = analysis.attention_count === 0 && measured > 0;

  return (
    <GlassCard as="section" className="analysis__verdict" aria-label="Resumo">
      <Plumb state={calm ? "settled" : "idle"} size={64} label={calm ? "Prumo no eixo" : "Prumo"} />
      <div role="status">
        {analysis.attention_count > 0 ? (
          <>
            <p className="analysis__count">{attentionText(analysis.attention_count)}</p>
            <p className="field__hint">
              Das {measured} {plural(measured, ["regra que tinha", "regras que tinham"])} o que medir,{" "}
              {analysis.attention_count} {plural(analysis.attention_count, ["passou", "passaram"])} do seu limite.
            </p>
          </>
        ) : calm ? (
          <>
            <p className="analysis__count">Nenhum ponto a ver</p>
            <p className="field__hint">
              A carteira está dentro dos seus limites em {measured} {plural(measured, ["regra", "regras"])}.
            </p>
          </>
        ) : (
          <>
            <p className="analysis__count">Ainda sem o que medir</p>
            <p className="field__hint">
              Cadastre ativos, defina as metas e registre proventos para as regras terem o que olhar. Ligadas e sem
              dado, elas aparecem como “Sem dado”.
            </p>
          </>
        )}
        <a className="link-btn" href="#/ajustes/analise">
          Ajustar as regras
        </a>
      </div>
    </GlassCard>
  );
}

function SourcesCard({ analysis }: { analysis: Analysis }) {
  const titleId = useId();
  const origin: Record<string, string> = {
    b3_stocks: "Classificação Setorial da B3",
    cvm_reits: "informe mensal de FII da CVM (tipo pelo balanço do fundo)",
  };

  return (
    <GlassCard as="section" className="dimension" aria-labelledby={titleId}>
      <h2 id={titleId} className="section-title">
        De onde vêm os dados
      </h2>
      <ul className="source-list">
        {analysis.sources.map((source) => (
          <li key={source.source}>
            <strong>Setor de {CLASS_META[source.asset_class].label}</strong>: {origin[source.source]}.{" "}
            {source.updated_at ? `Atualizado ${formatRelative(source.updated_at)}.` : "Ainda não buscado."}
            {source.failing && (
              <span className="source-list__warn">
                {" "}
                A última busca falhou{source.updated_at ? "; vale a anterior" : ""}. O Prumo tenta de novo sozinho.
              </span>
            )}
          </li>
        ))}
        <li>
          <strong>Ações e REITs dos EUA, cripto e renda fixa</strong>: sem setor. A fonte pública dos EUA (SEC) exige
          identificar quem consulta com nome e e-mail, e o Prumo não envia dado de ninguém.
        </li>
        <li>
          <strong>Rentabilidade</strong>: a série de cotas do Desempenho contra CDI e IPCA (Banco Central) e o
          Ibovespa. Janela que a série não cobre fica sem dado.
        </li>
      </ul>
      {analysis.unclassified_count > 0 && (
        <p className="field__hint">
          {analysis.unclassified_count} {plural(analysis.unclassified_count, ["ativo", "ativos"])} em Ações Nacionais ou
          FIIs sem setor na fonte (comum em papel novo ou fundo sem informe).{" "}
          {plural(analysis.unclassified_count, ["Ele", "Eles"])} não entra{analysis.unclassified_count === 1 ? "" : "m"}{" "}
          nas regras de setor.
        </p>
      )}
    </GlassCard>
  );
}
