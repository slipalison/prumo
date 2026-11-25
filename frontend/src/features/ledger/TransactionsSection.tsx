import { useId, useState } from "react";
import { errorMessage } from "../../api/client";
import { useYieldOnCost } from "../../api/incomeQueries";
import { useDeleteTransaction, useTransactions } from "../../api/ledger";
import type { Transaction } from "../../api/ledgerTypes";
import type { Asset } from "../../api/types";
import { IconEdit, IconPlus, IconTrash } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { nounOf, unitOf, type UnitKind } from "../../lib/assetClasses";
import { formatBRL, plural } from "../../lib/format";
import type { Money } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { yieldText } from "../../lib/yieldOnCost";
import { formatSignedPercent, signedBRL } from "./format";
import { formatTradeDate, kindLabel, quantityDelta } from "./kinds";
import { TransactionForm } from "./TransactionForm";
import "./ledger.css";

/** Valor na moeda do lançamento; com os valores ocultos, a máscara. */
function inCurrency(money: Money, value: number, currency: string): string {
  if (currency === "BRL" || money.hidden) return money.brl(value);
  return `${currency} ${value.toLocaleString("pt-BR", { maximumFractionDigits: 8 })}`;
}

/**
 * O que a linha mostra depois do tipo: quantidade, preço, taxa. Quantidade e
 * dinheiro passam pelos formatadores da tela (`useMoney`): com o olho fechado,
 * a linha diz só o tipo e a data. O câmbio é cotação pública, não patrimônio.
 */
function detail(t: Transaction, asset: Asset, unit: UnitKind, money: Money): string {
  const delta = quantityDelta(t);
  if (unit === "value") {
    const amount =
      t.kind === "opening" || t.kind === "buy" || t.kind === "redemption" ? money.brl(t.quantity) : signedBRL(money, delta);
    return t.kind === "opening" && t.unit_price === null ? `${amount} · custo desconhecido` : amount;
  }
  const fractional = unit === "fraction";
  const noun =
    fractional && !asset.treasury
      ? (asset.symbol ?? "")
      : plural(Math.abs(t.quantity), nounOf(asset.asset_class, asset.listed_on_b3, asset.treasury));
  const units = `${money.quantity(Math.abs(t.quantity), fractional)} ${noun}`.trim();
  const parts: string[] = [];
  if (t.kind === "adjustment" || t.kind === "split" || t.kind === "reverse_split") {
    parts.push(money.hidden ? units : `${delta > 0 ? "+" : "−"}${units}`);
  } else if (t.unit_price === null) {
    parts.push(t.kind === "opening" ? `${units} · custo desconhecido` : units);
  } else {
    parts.push(`${units} × ${inCurrency(money, t.unit_price, t.currency)}`);
    if (t.fx_rate !== null) parts.push(`câmbio ${formatBRL(t.fx_rate)}`);
  }
  if (t.fees > 0) parts.push(`taxas ${inCurrency(money, t.fees, t.currency)}`);
  return parts.join(" · ");
}

const SOURCE_TAG: Partial<Record<Transaction["source"], string>> = {
  plan: "do aporte",
  b3_import: "da B3",
  migration: "de antes do livro",
};

type TransactionsSectionProps = { asset: Asset };

/** A parte "Lançamentos" da folha do ativo: a posição e o livro dela. */
export function TransactionsSection({ asset }: TransactionsSectionProps) {
  const headingId = useId();
  const list = useTransactions(asset.id);
  const yields = useYieldOnCost();
  // Só quem rendeu provento tem yield para mostrar; o resto ficaria em "0,0%".
  const assetYield = yields.data?.assets.find((y) => y.asset_id === asset.id);
  const showYield = assetYield !== undefined && assetYield.income_12m > 0;
  // `"new"`: o formulário de lançamento novo; um id: a edição daquele.
  const [editing, setEditing] = useState<string | null>(null);
  const [focusPrice, setFocusPrice] = useState(false);
  const [confirming, setConfirming] = useState<string | null>(null);
  const remove = useDeleteTransaction();
  const toast = useToast();
  const money = useMoney();
  const unit = unitOf(asset.asset_class, asset.listed_on_b3, asset.treasury);
  const valueBased = unit === "value";

  const unknownOpening = list.data?.find((t) => t.kind === "opening" && t.unit_price === null);
  const costMissing = asset.cost_basis === null && asset.quantity > 0;

  const close = () => {
    setEditing(null);
    setFocusPrice(false);
  };

  const confirmRemove = (id: string) => {
    remove.mutate(id, {
      onSuccess: () => {
        setConfirming(null);
        toast({ tone: "info", message: "Lançamento apagado." });
      },
    });
  };

  return (
    <section className="ledger" aria-labelledby={headingId}>
      <div className="ledger__head">
        <h3 id={headingId} className="section-title">
          Lançamentos
        </h3>
        {editing !== "new" && (
          <button
            type="button"
            className="link-btn"
            onClick={() => {
              setEditing("new");
              setFocusPrice(false);
            }}
          >
            <IconPlus width={16} height={16} /> Adicionar
          </button>
        )}
      </div>

      <dl className="ledger__summary">
        {!valueBased && (
          <div>
            <dt>Preço médio</dt>
            <dd>{asset.average_price === null ? "—" : money.brl(asset.average_price)}</dd>
          </div>
        )}
        <div>
          <dt>{valueBased ? "Aplicado" : "Custo"}</dt>
          <dd>{asset.cost_basis === null ? "—" : money.brl(asset.cost_basis)}</dd>
        </div>
        <div>
          <dt>{valueBased ? "Rendimento" : "Resultado"}</dt>
          <dd>
            {asset.unrealized_gain === null ? "—" : signedBRL(money, asset.unrealized_gain)}
            {asset.return_percent !== null && <span className="muted"> ({formatSignedPercent(asset.return_percent)})</span>}
          </dd>
        </div>
        <div>
          <dt>Lucro realizado</dt>
          <dd>{asset.realized_gain === null ? "—" : signedBRL(money, asset.realized_gain)}</dd>
        </div>
        {showYield && (
          <div>
            <dt>Yield on cost (12 meses)</dt>
            <dd>{yieldText(assetYield)}</dd>
          </div>
        )}
      </dl>

      {costMissing && (
        <div className="inline-notice ledger__missing">
          <span>
            {valueBased
              ? "Parte do saldo não tem custo. Informe quanto você aplicou para ver o rendimento."
              : "Parte da posição não tem custo. Informe o preço médio para ver a rentabilidade."}
          </span>
          {unknownOpening && editing !== unknownOpening.id && (
            <button
              type="button"
              className="link-btn"
              onClick={() => {
                setEditing(unknownOpening.id);
                setFocusPrice(true);
              }}
            >
              {valueBased ? "Informar o aplicado" : "Informar o preço médio"}
            </button>
          )}
        </div>
      )}

      {editing === "new" && <TransactionForm asset={asset} onDone={close} />}

      {list.isPending && <Skeleton height={48} radius={14} />}
      {list.isError && <p className="field__hint">Não consegui carregar os lançamentos agora.</p>}
      {list.data && list.data.length === 0 && (
        <p className="field__hint">
          Nenhum lançamento ainda. Toda compra, venda ou evento entra aqui, com data — é daqui que saem a quantidade e o preço médio.
        </p>
      )}
      {list.data && list.data.length > 0 && (
        <ol className="ledger__list">
          {list.data.map((t) => {
            const label = kindLabel(t.kind, unit);
            const date = formatTradeDate(t.trade_date);
            if (editing === t.id) {
              return (
                <li key={t.id}>
                  <TransactionForm asset={asset} initial={t} focusPrice={focusPrice} onDone={close} />
                </li>
              );
            }
            return (
              <li key={t.id} className="ledger-row">
                <div className="ledger-row__main">
                  <span className="ledger-row__kind">
                    {label}
                    {SOURCE_TAG[t.source] && <span className="ledger-row__tag">{SOURCE_TAG[t.source]}</span>}
                  </span>
                  <span className="ledger-row__detail">{detail(t, asset, unit, money)}</span>
                </div>
                <span className="ledger-row__date">{date}</span>
                {confirming === t.id ? (
                  <div className="ledger-row__confirm" role="group" aria-label={`Apagar ${label} de ${date}`}>
                    <MagneticButton variant="danger" magnetic={false} loading={remove.isPending} onClick={() => confirmRemove(t.id)}>
                      Apagar
                    </MagneticButton>
                    <MagneticButton variant="ghost" magnetic={false} onClick={() => setConfirming(null)}>
                      Cancelar
                    </MagneticButton>
                  </div>
                ) : (
                  <div className="ledger-row__actions">
                    <button
                      type="button"
                      className="icon-btn"
                      aria-label={`Editar ${label} de ${date}`}
                      onClick={() => {
                        setEditing(t.id);
                        setFocusPrice(false);
                      }}
                    >
                      <IconEdit />
                    </button>
                    <button
                      type="button"
                      className="icon-btn icon-btn--danger"
                      aria-label={`Apagar ${label} de ${date}`}
                      onClick={() => {
                        remove.reset();
                        setConfirming(t.id);
                      }}
                    >
                      <IconTrash />
                    </button>
                  </div>
                )}
              </li>
            );
          })}
        </ol>
      )}
      {remove.isError && (
        <p className="form-error" role="alert">
          {errorMessage(remove.error)}
        </p>
      )}
    </section>
  );
}
