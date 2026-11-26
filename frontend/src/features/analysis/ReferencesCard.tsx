import { useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { analysisKeys } from "../../api/analysis";
import type { Analysis, Reference } from "../../api/analysisTypes";
import { errorMessage } from "../../api/client";
import { useUpdateAsset } from "../../api/queries";
import { AssetIcon } from "../../components/AssetIcon";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { InfoTip } from "../../components/InfoTip";
import { Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import { formatPercent, plural } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";

type References = Analysis["references"];

/** Célula sem valor: o traço à vista, o motivo para o leitor de tela. */
function Missing({ why }: { why: string }) {
  return (
    <>
      <span className="references__none" aria-hidden="true">
        —
      </span>
      <span className="sr-only">{why}</span>
    </>
  );
}

function columns(money: Money, bazinYield: string, onUse: (item: Reference, price: number) => void): DataColumn<Reference>[] {
  const price = (value: number | null, why: string): ReactNode => (value === null ? <Missing why={why} /> : money.brl(value));
  return [
    {
      key: "asset",
      label: "Ativo",
      rowHeader: true,
      cell: (item) => (
        <span className="reference__asset">
          <AssetIcon size="sm" assetClass={item.asset_class} {...iconSubjectOfLabel(item.label)} />
          {item.label}
        </span>
      ),
    },
    {
      key: "bazin",
      label: "Bazin",
      align: "end",
      cell: (item) =>
        item.bazin ? (
          <span className="reference__value">
            {money.brl(item.bazin.price)}
            <InfoTip label={`Como sai o Bazin de ${item.label}`} align="end">
              Média de {item.bazin.years} {plural(item.bazin.years, ["ano", "anos"])} de dividendos:{" "}
              {money.brl(item.bazin.annual_average)} por cota ao ano, dividida por {bazinYield}.
            </InfoTip>
          </span>
        ) : (
          <Missing why="sem Bazin" />
        ),
    },
    { key: "graham", label: "Graham", align: "end", cell: (item) => price(item.graham, "sem Graham") },
    {
      key: "both",
      label: "Pelas duas",
      align: "end",
      cell: (item) =>
        item.bazin && item.graham !== null && item.ceiling !== null ? (
          <strong className="reference__both">{money.brl(item.ceiling)}</strong>
        ) : (
          <Missing why="só uma referência" />
        ),
    },
    {
      key: "max",
      label: "Seu teto",
      align: "end",
      cell: (item) => (item.max_price === null ? <span className="references__muted">Sem teto</span> : money.brl(item.max_price)),
    },
    {
      key: "today",
      label: "Hoje",
      align: "end",
      cell: (item) => (item.unit_price === null ? <span className="references__muted">Sem preço</span> : money.brl(item.unit_price)),
    },
    {
      key: "use",
      label: "Teto",
      header: <span className="sr-only">Usar como teto</span>,
      align: "end",
      cell: (item) =>
        item.ceiling !== null && item.max_price !== item.ceiling ? (
          <MagneticButton
            variant="ghost"
            magnetic={false}
            className="reference__use"
            aria-label={`Usar a referência como teto de ${item.label}`}
            onClick={() => item.ceiling !== null && onUse(item, item.ceiling)}
          >
            Usar {money.brl(item.ceiling)} como meu teto
          </MagneticButton>
        ) : null,
    },
  ];
}

/**
 * Referências de preço: Bazin (a média de até 5 anos de dividendos por cota
 * dividida pelo rendimento-alvo) e Graham (a raiz de 22,5 vezes o lucro e o
 * patrimônio por ação, dos balanços da CVM). São fórmulas públicas, mostradas
 * como referência: não são preço justo, nem alvo, nem recomendação. O valor que
 * a tela oferece como teto é um só, pelas duas: o menor deles (abaixo dele o
 * preço passa nos dois critérios), ou o que existir. Vira o teto de preço do
 * ativo só com confirmação; o resto o Prumo não diz.
 */
export function ReferencesCard({ references }: { references: References }) {
  const money = useMoney();
  const toast = useToast();
  const client = useQueryClient();
  const update = useUpdateAsset();
  const [pending, setPending] = useState<{ item: Reference; price: number } | null>(null);
  const bazinYield = `${formatPercent(references.bazin_yield, Number.isInteger(references.bazin_yield) ? 0 : 2)} ao ano`;

  const confirm = () => {
    if (!pending) return;
    update.mutate(
      { id: pending.item.asset_id, body: { max_price: pending.price } },
      {
        onSuccess: () => {
          void client.invalidateQueries({ queryKey: analysisKeys.all });
          toast({ tone: "success", message: `Teto de ${pending.item.label} atualizado.` });
          setPending(null);
        },
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  return (
    <Panel
      className="references"
      flush
      title="Referências de preço"
      infoLabel="Como as referências são calculadas"
      info={
        <>
          <span className="references__formula">
            Bazin: a média anual de dividendos por cota (até 5 anos) dividida pelo seu rendimento-alvo, hoje {bazinYield}.
          </span>
          <span className="references__formula">
            Graham: a raiz de 22,5 vezes o lucro por ação vezes o patrimônio por ação, com os dois positivos (balanços da
            CVM e número de ações da B3). Sem eles, a referência não aparece.
          </span>
          <span className="references__formula">
            Pelas duas: o menor valor entre Bazin e Graham. Abaixo dele, o preço passa nos dois critérios; com uma
            referência só, é ela.
          </span>
        </>
      }
      actions={
        <a className="link-btn" href="#/ajustes/analise">
          Mudar o rendimento-alvo
        </a>
      }
    >
      <p className="references__disclaimer" role="note">
        Referência pública, não é recomendação: fórmulas abertas que não dizem se um ativo está barato, caro ou se vale
        comprar.
      </p>

      {references.items.length === 0 ? (
        <p className="references__empty">
          Sem referência por enquanto: o Bazin precisa do histórico de dividendos das suas ações e FIIs da B3 e o Graham,
          do lucro e do patrimônio por ação, e o Prumo busca os dois em segundo plano.
        </p>
      ) : (
        <DataTable
          caption="Referências por ativo"
          className="references__table"
          cozy
          columns={columns(money, bazinYield, (item, price) => setPending({ item, price }))}
          rows={references.items}
          rowKey={(item) => item.asset_id}
        />
      )}

      <Sheet
        open={pending !== null}
        onClose={() => setPending(null)}
        title="Usar como teto de preço?"
        description="O teto fica gravado no ativo e vale para o cálculo do aporte."
        footer={
          <>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setPending(null)}>
              Cancelar
            </MagneticButton>
            <MagneticButton loading={update.isPending} onClick={confirm}>
              Definir teto
            </MagneticButton>
          </>
        }
      >
        {pending && (
          <p>
            Definir {money.brl(pending.price)} como teto de preço de {pending.item.label}? Acima desse preço o ativo
            fica de fora do próximo aporte. Você pode mudar ou tirar o teto quando quiser, ao editar o ativo.
          </p>
        )}
      </Sheet>
    </Panel>
  );
}
