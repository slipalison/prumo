import { useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { analysisKeys } from "../../api/analysis";
import { ceilingKeys } from "../../api/ceiling";
import type { Ceiling, FundReference, References, RefValue, StockReference } from "../../api/ceilingTypes";
import { errorMessage } from "../../api/client";
import { useUpdateAsset } from "../../api/queries";
import { AssetIcon } from "../../components/AssetIcon";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { IconChevronDown } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Sheet } from "../../components/Sheet";
import { useToast } from "../../components/toastContext";
import { iconSubjectOfLabel } from "../../lib/assetIcon";
import {
  cdiText,
  choiceOf,
  draftChanges,
  draftOf,
  FUND_TYPE_LABEL,
  GAP_TEXT,
  METHOD_LABEL,
  realRateText,
  type CeilingDraft,
} from "../../lib/ceilingText";
import { formatPercent } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { CeilingPicker } from "../ceiling/CeilingPicker";
import { methodTip } from "../ceiling/methodTip";

type Item = StockReference | FundReference;

/** Célula sem valor: o traço à vista, o motivo para o leitor de tela e no ⓘ. */
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

/** O valor de uma referência, com a conta no ⓘ; sem valor, o motivo. */
function RefCell({ found, name, tip }: { found: RefValue; name: string; tip: string | null }) {
  const money = useMoney();
  if (found.value === null) return <Missing why={`sem ${name}: ${GAP_TEXT[found.gap ?? "no_reference"]}`} />;
  return (
    <span className="reference__value">
      {money.brl(found.value)}
      {tip && (
        <InfoTip label={`Como sai: ${name}`} align="end">
          {tip}
        </InfoTip>
      )}
    </span>
  );
}

/** "Seu teto": o valor e o método, "Sem teto", ou o método sem valor com o motivo. */
function CeilingCell({ ceiling, label }: { ceiling: Ceiling | null; label: string }) {
  const money = useMoney();
  if (!ceiling) return <span className="references__muted">Sem teto</span>;
  if (ceiling.value === null) {
    return (
      <span className="reference__ceiling reference__ceiling--off">
        <span>{METHOD_LABEL[ceiling.method]}: sem valor</span>
        <InfoTip label={`Por que o teto de ${label} está sem valor`} align="end">
          Sem valor hoje ({GAP_TEXT[ceiling.unavailable_reason ?? "no_reference"]}): {label} fica fora do aporte até o método
          voltar a ter valor, ou até você trocar o teto.
        </InfoTip>
      </span>
    );
  }
  return (
    <span className="reference__ceiling">
      <span className="reference__ceiling-value">{money.brl(ceiling.value)}</span>
      <span className="reference__ceiling-method">{METHOD_LABEL[ceiling.method]}</span>
    </span>
  );
}

function assetColumn(): DataColumn<Item> {
  return {
    key: "asset",
    label: "Ativo",
    rowHeader: true,
    cell: (item) => (
      <span className="reference__asset">
        <AssetIcon size="sm" assetClass={item.asset_class} {...iconSubjectOfLabel(item.label)} />
        {item.label}
      </span>
    ),
  };
}

function tailColumns(onChoose: (item: Item) => void): DataColumn<Item>[] {
  return [
    {
      key: "mine",
      label: "Seu teto",
      align: "end",
      cell: (item) => <CeilingCell ceiling={item.ceiling} label={item.label} />,
    },
    {
      key: "today",
      label: "Hoje",
      align: "end",
      cell: (item) => <TodayCell price={item.unit_price} />,
    },
    {
      key: "choose",
      label: "Teto",
      header: <span className="sr-only">Escolher o teto</span>,
      align: "end",
      cell: (item) => (
        <button
          type="button"
          className="btn btn--ghost reference__choose"
          aria-haspopup="dialog"
          aria-label={`Escolher o teto de ${item.label}`}
          onClick={() => onChoose(item)}
        >
          Teto <IconChevronDown width={14} height={14} />
        </button>
      ),
    },
  ];
}

function TodayCell({ price }: { price: number | null }) {
  const money = useMoney();
  return price === null ? <span className="references__muted">Sem preço</span> : <>{money.brl(price)}</>;
}

function stockColumns(references: References, money: Money, onChoose: (item: Item) => void): DataColumn<StockReference>[] {
  const tip = (item: StockReference, method: "bazin" | "graham" | "combined") =>
    methodTip(item, method, references.market, references.bazin_yield, money);
  return [
    assetColumn(),
    { key: "bazin", label: "Bazin", align: "end", cell: (item) => <RefCell found={item.bazin} name={`Bazin de ${item.label}`} tip={tip(item, "bazin")} /> },
    { key: "graham", label: "Graham", align: "end", cell: (item) => <RefCell found={item.graham} name={`Graham de ${item.label}`} tip={null} /> },
    {
      key: "both",
      label: "Pelas duas",
      align: "end",
      cell: (item) =>
        item.combined.value === null ? (
          <Missing why="sem nenhuma das duas" />
        ) : (
          <strong className="reference__both">{money.brl(item.combined.value)}</strong>
        ),
    },
    ...tailColumns(onChoose),
  ];
}

function fundColumns(references: References, money: Money, onChoose: (item: Item) => void): DataColumn<FundReference>[] {
  const tip = (item: FundReference, method: "nav" | "fund_yield") =>
    methodTip(item, method, references.market, references.bazin_yield, money);
  return [
    assetColumn(),
    {
      key: "type",
      label: "Tipo",
      cell: (item) =>
        item.fund_type ? FUND_TYPE_LABEL[item.fund_type] : <span className="references__muted">Sem tipo</span>,
    },
    { key: "nav", label: "VP", align: "end", cell: (item) => <RefCell found={item.nav} name={`VP de ${item.label}`} tip={tip(item, "nav")} /> },
    {
      key: "yield",
      label: "Rendimento",
      align: "end",
      cell: (item) => (
        <RefCell found={item.fund_yield} name={`Rendimento de ${item.label}`} tip={tip(item, "fund_yield")} />
      ),
    },
    {
      key: "both",
      label: "Pelas duas",
      align: "end",
      cell: (item) =>
        item.combined.value === null ? (
          <Missing why="sem nenhuma das duas" />
        ) : (
          <strong className="reference__both">{money.brl(item.combined.value)}</strong>
        ),
    },
    ...tailColumns(onChoose),
  ];
}

/** A explicação das fórmulas, com as taxas de hoje. */
function Formulas({ references }: { references: References }): ReactNode {
  const bazinYield = `${formatPercent(references.bazin_yield, Number.isInteger(references.bazin_yield) ? 0 : 2)} ao ano`;
  const market = references.market;
  const pp = (value: number) => `${value.toLocaleString("pt-BR", { maximumFractionDigits: 2 })} p.p.`;
  return (
    <>
      <span className="references__formula">
        Ações. Bazin: a média anual de dividendos por cota (até 5 anos) dividida pelo seu rendimento-alvo, hoje {bazinYield}.
        Graham: a raiz de 22,5 vezes o lucro por ação vezes o patrimônio por ação, com os dois positivos (balanços da CVM).
      </span>
      <span className="references__formula">
        FIIs. VP: o patrimônio por cota do informe mensal da CVM. Rendimento: os proventos por cota de 12 meses divididos
        pelo alvo do tipo. Tijolo: {realRateText(market)} + {pp(market.brick_premium)}. Papel: {cdiText(market)}, +{" "}
        {pp(market.paper_premium)}. Híbrido: os dois, pela composição da carteira. Fundo de fundos: a média dos dois.
      </span>
      <span className="references__formula">
        Pelas duas: o menor valor entre as duas referências; com uma só, é ela. O teto por método acompanha o método: o valor
        muda com o balanço, os proventos e as taxas.
      </span>
    </>
  );
}

/**
 * Referências de preço: Bazin e Graham nas ações, VP e Rendimento nos FIIs.
 * São fórmulas públicas, mostradas como referência: não são preço justo, nem
 * alvo, nem recomendação. "Teto ▾" abre a escolha (Sem teto, cada método com o
 * valor de hoje, Valor próprio); confirmar grava, e "Sem teto" tira o teto.
 */
export function ReferencesCard({ references }: { references: References }) {
  const money = useMoney();
  const toast = useToast();
  const client = useQueryClient();
  const update = useUpdateAsset();
  const [choosing, setChoosing] = useState<{ item: Item; draft: CeilingDraft } | null>(null);
  const [open, setOpen] = useState(false);

  const choose = (item: Item) => {
    setChoosing({ item, draft: draftOf(item.ceiling, item.unit_price ?? 0) });
    setOpen(true);
  };

  const confirm = () => {
    if (!choosing) return;
    const { item, draft } = choosing;
    if (!draftChanges(draft, item.ceiling)) {
      setOpen(false);
      return;
    }
    update.mutate(
      { id: item.asset_id, body: { ceiling: choiceOf(draft) } },
      {
        onSuccess: () => {
          void client.invalidateQueries({ queryKey: analysisKeys.all });
          void client.invalidateQueries({ queryKey: ceilingKeys.all });
          toast({
            tone: "success",
            message: draft.kind === "none" ? `Teto de ${item.label} removido.` : `Teto de ${item.label} atualizado.`,
          });
          setOpen(false);
        },
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  const empty = references.stocks.length === 0 && references.funds.length === 0;
  const manualInvalid = choosing?.draft.kind === "manual" && choosing.draft.value <= 0;

  return (
    <Panel
      className="references"
      flush
      title="Referências de preço"
      infoLabel="Como as referências são calculadas"
      info={<Formulas references={references} />}
      actions={
        <a className="link-btn" href="#/ajustes/analise">
          Mudar o rendimento-alvo e os prêmios
        </a>
      }
    >
      <p className="references__disclaimer" role="note">
        Referência pública, não é recomendação: fórmulas abertas que não dizem se um ativo está barato, caro ou se vale
        comprar.
      </p>

      {empty ? (
        <p className="references__empty">
          Sem ações nem FIIs da B3 na carteira: as referências de preço valem para eles.
        </p>
      ) : (
        <>
          {references.stocks.length > 0 && (
            <DataTable
              caption="Ações"
              showCaption
              className="references__table"
              cozy
              columns={stockColumns(references, money, choose)}
              rows={references.stocks}
              rowKey={(item) => item.asset_id}
            />
          )}
          {references.funds.length > 0 && (
            <DataTable
              caption="FIIs"
              showCaption
              className="references__table references__table--funds"
              cozy
              columns={fundColumns(references, money, choose)}
              rows={references.funds}
              rowKey={(item) => item.asset_id}
            />
          )}
        </>
      )}

      <Sheet
        open={open}
        onClose={() => setOpen(false)}
        title={choosing ? `Teto de ${choosing.item.label}` : "Teto"}
        description="O teto é seu: escolha um método, um valor próprio ou nenhum. Acima dele o ativo fica fora do aporte."
        footer={
          <>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setOpen(false)}>
              Cancelar
            </MagneticButton>
            <MagneticButton loading={update.isPending} disabled={manualInvalid} onClick={confirm}>
              Confirmar
            </MagneticButton>
          </>
        }
      >
        {choosing && (
          <CeilingPicker
            name={choosing.item.label}
            priceNow={choosing.item.unit_price}
            family={"bazin" in choosing.item ? "stock" : "fund"}
            item={choosing.item}
            market={references.market}
            bazinYield={references.bazin_yield}
            value={choosing.draft}
            onChange={(draft) => setChoosing((current) => current && { ...current, draft })}
            manualStart={choosing.item.unit_price ?? 0}
          />
        )}
      </Sheet>
    </Panel>
  );
}
