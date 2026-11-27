import { useMemo } from "react";
import { useCompareTable } from "../../api/compare";
import { usePortfolio } from "../../api/queries";
import { Page, PageHeader, Panel } from "../../components/Layout";
import { useHash } from "../../lib/route";
import { AssetPicker } from "./AssetPicker";
import { CompareTable } from "./CompareTable";
import { ReturnsPanel } from "./ReturnsPanel";
import { readSelection, selectionHash } from "./selection";
import "./compare.css";

/**
 * Comparar ativos (`#/analise/comparar?ativos=PETR4,VALE3`): até cinco, da
 * carteira ou da busca, lado a lado nos indicadores que o Prumo já calcula e
 * numa rentabilidade comparada (ativo × ativo × carteira × índices). A escolha
 * mora no endereço. Nada aqui manda comprar ou vender: o destaque é só o maior
 * ou o menor valor da linha.
 */
export function CompareScreen() {
  const hash = useHash();
  const keys = useMemo(() => readSelection(hash), [hash]);
  const portfolio = usePortfolio();
  const table = useCompareTable(keys);

  const names = useMemo(
    () => new Map((table.data?.assets ?? []).map((asset) => [asset.key, { name: asset.name, assetClass: asset.asset_class }])),
    [table.data],
  );
  const change = (next: string[]) => {
    window.location.hash = selectionHash(next);
  };

  return (
    <Page className="compare">
      <PageHeader
        title="Comparar ativos"
        info={
          <>
            Escolha até cinco ativos, da sua carteira ou da busca, e veja os indicadores lado a lado. O destaque marca só o maior ou o menor valor da
            linha, entre ativos do mesmo tipo: não é ranking nem recomendação. Balanços só existem para os ativos da carteira; para os outros, o Prumo
            busca o preço e os proventos na hora.
          </>
        }
        infoLabel="Sobre a comparação"
        actions={
          <a className="link-btn" href="#/analise">
            Voltar à análise
          </a>
        }
      />
      <Panel title="Ativos" aria-label="Ativos da comparação">
        <AssetPicker keys={keys} portfolio={portfolio.data?.assets ?? []} names={names} onChange={change} />
      </Panel>
      {keys.length === 0 ? (
        <Panel title="Lado a lado" compact>
          <p className="data-table__empty">
            Escolha ao menos um ativo para ver os indicadores e a rentabilidade. Para comparar, a escolha de dois ou mais mostra o maior e o menor valor de cada
            linha.
          </p>
        </Panel>
      ) : (
        <>
          <Panel
            title="Indicadores lado a lado"
            info="Indicador que não existe para o ativo aparece como “—”, com o motivo. O destaque marca o maior ou o menor valor da linha só entre ativos do mesmo tipo (empresa com empresa, fundo com fundo)."
            infoLabel="Como ler a tabela"
          >
            <CompareTable table={table.data} pending={table.isFetching} error={table.error} onRetry={() => void table.refetch()} />
          </Panel>
          <ReturnsPanel keys={keys} />
        </>
      )}
    </Page>
  );
}
