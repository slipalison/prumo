import type { Irpf, IrpfAsset } from "../../api/taxTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { DataTable, type DataColumn } from "../../components/DataTable";
import { Panel } from "../../components/Layout";
import { useToast } from "../../components/toastContext";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { DoubtTip } from "./DoubtTip";
import { BalanceText } from "./MonthsPanel";
import { IncomeReport } from "./IncomeReport";
import { INSTRUMENT_LABEL, irpfLabel } from "./taxText";

function assetText(asset: IrpfAsset, quantity: string, cost: string, average: string): string {
  return `${asset.symbol} (${INSTRUMENT_LABEL[asset.instrument]}): ${quantity} unidades. Grupo ${asset.group}, código ${asset.code}. Situação em 31/12: ${cost} (custo de aquisição, preço médio ${average}).`;
}

/**
 * A declaração do ano para copiar: Bens e Direitos (uma linha por ativo, com o
 * botão de copiar), os rendimentos por fonte pagadora (isentos e de tributação
 * exclusiva, com CNPJ) e o saldo da renda variável. Cada dúvida jurídica fica
 * no ⓘ azul do bloco que ela toca.
 */
export function DeclarationPanel({ irpf }: Readonly<{ irpf: Irpf }>) {
  const money = useMoney();
  const pending = (irpf.year_end.pending_darf.exchange_net_gain ?? 0) + (irpf.year_end.pending_darf.capital_gain ?? 0);

  return (
    <Panel
      title={`Declaração de ${irpf.year}`}
      info={`O ano-calendário ${irpf.year}, na declaração entregue em ${irpf.year + 1}. Copie cada linha para o programa da Receita.`}
      infoLabel="Como usar a declaração"
    >
      <section className="tax-block">
        <h3 className="tax-subtitle">
          Bens e Direitos (situação em 31/12)
          {irpf.doubts.includes("irpf_codes") && <DoubtTip doubt="irpf_codes" />}
        </h3>
        <AssetsTable assets={irpf.assets} />
      </section>

      <IncomeReport exempt={irpf.exempt_income} exclusive={irpf.exclusive_income} doubts={irpf.doubts} />

      <section className="tax-block">
        <h3 className="tax-subtitle">
          Renda variável
          {irpf.doubts.includes("withholding_leftover") && <DoubtTip doubt="withholding_leftover" />}
        </h3>
        <p className="tax-facts">
          DARF do ano: {money.brl(irpf.darf_total)}. Prejuízo que passa para {irpf.year + 1}:{" "}
          <BalanceText balance={irpf.year_end} empty="nenhum" />.
          {pending > 0 && <> DARF abaixo do mínimo que fica para janeiro: {money.brl(pending)}.</>}
          {irpf.withholding_left > 0 && <> IR retido que sobrou: {money.brl(irpf.withholding_left)}.</>}
        </p>
      </section>
    </Panel>
  );
}

/** As colunas de Bens e Direitos; `onCopy` leva a linha do ativo para a área de transferência. */
function assetColumns(money: Money, onCopy: (asset: IrpfAsset) => Promise<void>): DataColumn<IrpfAsset>[] {
  return [
    {
      key: "asset",
      label: "Ativo",
      rowHeader: true,
      cell: (asset) => (
        <span className="tax-asset">
          <AssetIcon symbol={asset.symbol} assetClass={asset.instrument === "crypto" ? "crypto" : null} size="sm" />
          {asset.symbol}
        </span>
      ),
    },
    {
      key: "sheet",
      label: "Ficha",
      cell: (asset) => (
        <span className="tax-sheet">
          Grupo {asset.group} · Código {asset.code} · {irpfLabel(asset.group, asset.code, asset.label)}
        </span>
      ),
    },
    {
      key: "quantity",
      label: "Quantidade",
      align: "end",
      cell: (asset) => `${money.quantity(asset.quantity, asset.instrument === "crypto")} unidades`,
    },
    { key: "cost", label: "Situação em 31/12", align: "end", cell: (asset) => money.brl(asset.total_cost) },
    { key: "average", label: "Preço médio", align: "end", wideOnly: true, cell: (asset) => money.brl(asset.average_price) },
    {
      key: "copy",
      label: "Copiar",
      align: "end",
      cell: (asset) => (
        <button type="button" className="link-btn" onClick={() => void onCopy(asset)} aria-label={`Copiar a linha de ${asset.symbol}`}>
          Copiar
        </button>
      ),
    },
  ];
}

function AssetsTable({ assets }: Readonly<{ assets: IrpfAsset[] }>) {
  const money = useMoney();
  const toast = useToast();

  const copy = async (asset: IrpfAsset) => {
    // O que vai para a área de transferência é o valor de verdade: copiar é
    // o que a pessoa pediu, e a tela continua mascarada.
    const real = assetText(
      asset,
      asset.quantity.toLocaleString("pt-BR", { maximumFractionDigits: 8 }),
      asset.total_cost.toLocaleString("pt-BR", { style: "currency", currency: "BRL" }),
      asset.average_price.toLocaleString("pt-BR", { style: "currency", currency: "BRL", maximumFractionDigits: 8 }),
    );
    try {
      await navigator.clipboard.writeText(real);
      toast({ tone: "success", message: `Linha de ${asset.symbol} copiada.` });
    } catch {
      toast({ tone: "error", message: "Não consegui copiar. Selecione o texto e copie à mão." });
    }
  };

  return (
    <DataTable
      caption="Bens e Direitos"
      columns={assetColumns(money, copy)}
      rows={assets}
      rowKey={(asset) => asset.symbol}
      empty="Nenhuma posição em 31/12 dentro do IR do Prumo."
    />
  );
}
