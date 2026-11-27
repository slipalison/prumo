import { errorMessage } from "../../api/client";
import type { Analysis } from "../../api/analysisTypes";
import { Disclosure } from "../../components/Disclosure";
import { GlassCard } from "../../components/GlassCard";
import { Grid, Page, PageHeader } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Plumb } from "../../components/Plumb";
import { Skeleton } from "../../components/Skeleton";
import { CLASS_META } from "../../lib/assetClasses";
import { attentionText } from "../../lib/analysisText";
import { formatRelative, plural } from "../../lib/format";
import { parseSection, useHash } from "../../lib/route";
import { CompareScreen } from "../compare/CompareScreen";
import { ResultsScreen } from "../results/ResultsScreen";
import { AttentionCard } from "./AttentionCard";
import { DimensionCard } from "./DimensionCard";
import { DimensionScores } from "./DimensionScores";
import { HighlightsCard } from "./HighlightsCard";
import { ProfileCard } from "./ProfileCard";
import { ReferencesCard } from "./ReferencesCard";
import { useAnalysis } from "./useAnalysis";

/**
 * Análise da carteira: o que olhar além da meta por classe. Quatro dimensões
 * (diversificação, dividendos, rentabilidade e qualidade), cada uma pontuada
 * pelo percentual de regras ligadas, com dado, que estão ok. As regras e os
 * limites são seus, em Ajustes. Nada aqui manda comprar ou vender.
 *
 * O desenho (docs/layout.md § 7.5): na primeira dobra, o veredito com o
 * perfil de risco e as quatro pontuações lado a lado; embaixo, uma tabela
 * compacta por dimensão com os pontos a ver (o que está ok fica recolhido);
 * no fim, pontos de atenção, destaques, referências de preço e as fontes.
 */
export function AnalysisScreen() {
  // Subtelas no mesmo endereço-mãe: `#/analise/comparar` (o comparador de
  // ativos) e `#/analise/resultados` (os resultados trimestrais).
  const section = parseSection(useHash());
  if (section === "comparar") return <CompareScreen />;
  if (section === "resultados") return <ResultsScreen />;
  return <AnalysisReport />;
}

function AnalysisReport() {
  const analysis = useAnalysis();

  return (
    <Page className="analysis">
      <PageHeader
        title="Análise da carteira"
        infoLabel="Sobre a análise"
        info={
          <>
            O que olhar além da meta por classe: diversificação, dividendos, rentabilidade e a qualidade dos ativos. Cada
            dimensão mostra quantas das suas regras estão ok, com o número exato. Regra sem dado para medir fica de fora
            da conta; ela não conta como falha.
          </>
        }
        actions={
          <>
            <a className="link-btn" href="#/analise/comparar">
              Comparar ativos
            </a>
            <a className="link-btn" href="#/analise/resultados">
              Resultados trimestrais
            </a>
            <a className="link-btn" href="#/ajustes/analise">
              Ajustar as regras
            </a>
          </>
        }
      />

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
    </Page>
  );
}

function Report({ analysis }: Readonly<{ analysis: Analysis }>) {
  return (
    <>
      <div className="analysis__top">
        <Verdict analysis={analysis} />
        <DimensionScores dimensions={analysis.dimensions} />
      </div>
      {analysis.dimensions.map((dimension) => (
        <DimensionCard key={dimension.id} dimension={dimension} />
      ))}
      <Grid cols={2} alignStart>
        <AttentionCard attention={analysis.attention} />
        <HighlightsCard highlights={analysis.highlights} />
      </Grid>
      <ReferencesCard references={analysis.references} />
      <SourcesCard analysis={analysis} />
    </>
  );
}

/**
 * As duas linhas do veredito: os pontos a ver, a carteira nos limites ou
 * ainda sem o que medir. São `<span>` porque vão dentro do `<output>`, que só
 * aceita conteúdo de frase; a coluna do veredito (flex) as empilha.
 */
function VerdictLines({ attention, measured, calm }: Readonly<{ attention: number; measured: number; calm: boolean }>) {
  if (attention > 0) {
    return (
      <>
        <span className="analysis__count">{attentionText(attention)}</span>
        <span className="analysis__hint">
          Das {measured} {plural(measured, ["regra que tinha", "regras que tinham"])} o que medir,{" "}
          {attention} {plural(attention, ["passou", "passaram"])} do seu limite.
        </span>
      </>
    );
  }
  if (calm) {
    return (
      <>
        <span className="analysis__count">Nenhum ponto a ver</span>
        <span className="analysis__hint">
          A carteira está dentro dos seus limites em {measured} {plural(measured, ["regra", "regras"])}.
        </span>
      </>
    );
  }
  return (
    <>
      <span className="analysis__count">Ainda sem o que medir</span>
      <span className="analysis__hint">
        Cadastre ativos, defina as metas e registre proventos para as regras terem o que olhar. Ligadas e sem
        dado, elas aparecem como “Sem dado”.
      </span>
    </>
  );
}

/** O veredito em uma frase, com o perfil de risco logo abaixo. */
function Verdict({ analysis }: Readonly<{ analysis: Analysis }>) {
  const measured = analysis.dimensions.reduce((sum, d) => sum + (d.score?.measured ?? 0), 0);
  const calm = analysis.attention_count === 0 && measured > 0;

  return (
    <GlassCard as="section" className="panel analysis__verdict" aria-label="Resumo">
      <div className="analysis__verdict-head">
        <Plumb state={calm ? "settled" : "idle"} size={44} label={calm ? "Prumo no eixo" : "Prumo"} />
        <output className="analysis__verdict-text">
          <VerdictLines attention={analysis.attention_count} measured={measured} calm={calm} />
        </output>
      </div>
      <ProfileCard profile={analysis.profile} />
    </GlassCard>
  );
}

const ORIGIN: Record<string, string> = {
  b3_stocks: "Classificação Setorial da B3",
  cvm_reits: "informe mensal de FII da CVM (tipo pelo balanço do fundo)",
};

/** De onde vêm os dados: recolhido no fim, com o aviso de falha à mostra na frase fechada. */
function SourcesCard({ analysis }: Readonly<{ analysis: Analysis }>) {
  const failing = analysis.sources.filter((source) => source.failing).length;

  return (
    <GlassCard as="section" className="panel panel--compact analysis__sources" aria-label="De onde vêm os dados">
      <Disclosure
        summary="De onde vêm os dados"
        count={failing > 0 ? `${failing} ${plural(failing, ["fonte falhou", "fontes falharam"])}` : undefined}
      >
        <ul className="source-list">
          {analysis.sources.map((source) => (
            <li key={source.source}>
              <strong>Setor de {CLASS_META[source.asset_class].label}</strong>: {ORIGIN[source.source]}.{" "}
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
            {analysis.unclassified_count} {plural(analysis.unclassified_count, ["ativo", "ativos"])} em Ações Nacionais
            ou FIIs sem setor na fonte (comum em papel novo ou fundo sem informe).{" "}
            {plural(analysis.unclassified_count, ["Ele", "Eles"])} não entra
            {analysis.unclassified_count === 1 ? "" : "m"} nas regras de setor.
          </p>
        )}
      </Disclosure>
    </GlassCard>
  );
}
