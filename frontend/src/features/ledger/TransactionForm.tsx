import { useId, useState, type FormEvent, type InputHTMLAttributes, type Ref } from "react";
import { errorMessage } from "../../api/client";
import { useCreateTransaction, useUpdateTransaction } from "../../api/ledger";
import type { Transaction, TransactionKind, UpdateTransactionRequest } from "../../api/ledgerTypes";
import type { Asset } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { useToast } from "../../components/toastContext";
import { unitOf } from "../../lib/assetClasses";
import { parseDecimal } from "../../lib/format";
import { MASK_BRL, MASK_QUANTITY } from "../../lib/money";
import { useMoney } from "../../lib/privacy";
import { useFocusOnMount } from "../../lib/useFocusOnMount";
import { numberText } from "./format";
import { kindLabel, kindsFor, PRICE_RULE, priceLabel, quantityHint, takesFees, todayIso } from "./kinds";

type TransactionFormProps = {
  asset: Asset;
  /** Com ele, edita; sem ele, cria. */
  initial?: Transaction | undefined;
  /** Abre com o foco no preço — o "informe o preço médio". */
  focusPrice?: boolean;
  onDone: () => void;
};

/**
 * Um lançamento: tipo, data, quantidade e, conforme o tipo, preço, taxa e
 * câmbio. O servidor refaz a posição ao salvar e recusa o que deixaria a
 * quantidade negativa em alguma data.
 */
export function TransactionForm({ asset, initial, focusPrice = false, onDone }: TransactionFormProps) {
  const ids = {
    kind: useId(),
    date: useId(),
    quantity: useId(),
    quantityHint: useId(),
    price: useId(),
    fees: useId(),
    currency: useId(),
    fx: useId(),
    costKnown: useId(),
  };
  const unit = unitOf(asset.asset_class, asset.listed_on_b3, asset.treasury);
  const valueBased = unit === "value";
  // Preço em outra moeda só faz sentido onde a cotação já é em outra moeda.
  const foreign = !valueBased && asset.currency !== "BRL" ? asset.currency : null;

  const [kind, setKind] = useState<TransactionKind>(initial?.kind ?? "buy");
  const [date, setDate] = useState(initial?.trade_date ?? todayIso());
  const [quantity, setQuantity] = useState(initial ? numberText(initial.quantity) : "");
  const [price, setPrice] = useState(initial?.unit_price != null ? numberText(initial.unit_price) : "");
  const [fees, setFees] = useState(initial?.fees ? numberText(initial.fees) : "");
  const [currency, setCurrency] = useState(initial?.currency ?? foreign ?? "BRL");
  const [fx, setFx] = useState(initial?.fx_rate != null ? numberText(initial.fx_rate) : "");
  const [costKnown, setCostKnown] = useState(initial?.unit_price != null);
  const [problem, setProblem] = useState<string | null>(null);
  const priceRef = useFocusOnMount<HTMLInputElement>(focusPrice);

  const create = useCreateTransaction();
  const update = useUpdateTransaction();
  const toast = useToast();
  const pending = create.isPending || update.isPending;
  const error = create.error ?? update.error;

  const rule = PRICE_RULE[kind];
  const showPrice = !valueBased && rule !== "forbidden";
  const showFees = takesFees(kind);
  const showCurrency = foreign !== null && showPrice;
  const hint = quantityHint(kind, unit);
  const quantityLabel = valueBased ? "Valor (R$)" : "Quantidade";

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (pending) return;
    setProblem(null);
    const q = parseDecimal(quantity);
    if (q === null || q === 0 || (kind !== "adjustment" && q < 0)) {
      setProblem(valueBased ? "Informe o valor." : "Informe a quantidade.");
      return;
    }
    let unitPrice: number | null = null;
    if (valueBased) {
      // Renda fixa por valor: o preço é 1 (o servidor preenche); no saldo
      // inicial, 1 quer dizer "esse saldo é o que apliquei".
      unitPrice = kind === "opening" && costKnown ? 1 : null;
    } else if (rule !== "forbidden") {
      unitPrice = parseDecimal(price);
      if (unitPrice === null && rule === "required") {
        setProblem("Informe o preço.");
        return;
      }
    }
    const chosenCurrency = showCurrency ? currency : "BRL";
    const rate = chosenCurrency === "BRL" ? null : parseDecimal(fx);
    if (chosenCurrency !== "BRL" && unitPrice !== null && rate === null) {
      setProblem(`Informe o câmbio: quantos reais valia 1 ${chosenCurrency} no dia.`);
      return;
    }
    const body: UpdateTransactionRequest = {
      kind,
      trade_date: date,
      quantity: q,
      unit_price: unitPrice,
      fees: showFees ? (parseDecimal(fees) ?? 0) : 0,
      currency: chosenCurrency,
      fx_rate: rate,
    };
    const done = () => {
      toast({ tone: "success", message: initial ? "Lançamento atualizado." : "Lançamento salvo." });
      onDone();
    };
    if (initial) {
      update.mutate({ id: initial.id, body }, { onSuccess: done });
    } else {
      create.mutate({ asset_id: asset.id, ...body, kind, trade_date: date, quantity: q }, { onSuccess: done });
    }
  };

  return (
    <form className="ledger-form" onSubmit={submit} noValidate aria-label={initial ? "Editar lançamento" : "Novo lançamento"}>
      <div className="ledger-form__grid">
        <div className="field">
          <label htmlFor={ids.kind} className="field__label">
            Tipo
          </label>
          <select id={ids.kind} className="text-input" value={kind} onChange={(event) => setKind(event.target.value as TransactionKind)}>
            {kindsFor(unit).map((k) => (
              <option key={k} value={k}>
                {kindLabel(k, unit)}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor={ids.date} className="field__label">
            Data
          </label>
          <input
            id={ids.date}
            className="text-input"
            type="date"
            max={todayIso()}
            required
            value={date}
            onChange={(event) => setDate(event.target.value)}
          />
        </div>
        <div className="field">
          <label htmlFor={ids.quantity} className="field__label">
            {quantityLabel}
          </label>
          <MaskedInput
            id={ids.quantity}
            mask={valueBased ? MASK_BRL : MASK_QUANTITY}
            aria-describedby={hint ? ids.quantityHint : undefined}
            value={quantity}
            onChange={setQuantity}
          />
        </div>
        {showPrice && (
          <div className="field">
            <label htmlFor={ids.price} className="field__label">
              {priceLabel(kind)}
            </label>
            <MaskedInput id={ids.price} ref={priceRef} mask={MASK_BRL} value={price} onChange={setPrice} />
          </div>
        )}
        {showFees && (
          <div className="field">
            <label htmlFor={ids.fees} className="field__label">
              Taxas (corretagem, emolumentos)
            </label>
            <MaskedInput id={ids.fees} mask={MASK_BRL} placeholder="0" value={fees} onChange={setFees} />
          </div>
        )}
        {showCurrency && (
          <div className="field">
            <label htmlFor={ids.currency} className="field__label">
              Moeda do preço
            </label>
            <select id={ids.currency} className="text-input" value={currency} onChange={(event) => setCurrency(event.target.value)}>
              <option value={foreign ?? "USD"}>{foreign}</option>
              <option value="BRL">BRL (já em reais)</option>
            </select>
          </div>
        )}
        {showCurrency && currency !== "BRL" && (
          <div className="field">
            <label htmlFor={ids.fx} className="field__label">
              Câmbio (R$ por 1 {currency})
            </label>
            <MaskedInput id={ids.fx} mask={null} value={fx} onChange={setFx} />
          </div>
        )}
      </div>
      {hint && (
        <p id={ids.quantityHint} className="field__hint">
          {hint}
        </p>
      )}
      {valueBased && kind === "opening" && (
        <div className="switch-row">
          <label htmlFor={ids.costKnown} className="switch-row__label">
            Esse saldo é o que você aplicou
            <span className="field__hint">Sem isso, o custo fica desconhecido e a rentabilidade não aparece.</span>
          </label>
          <input
            id={ids.costKnown}
            type="checkbox"
            role="switch"
            className="switch"
            checked={costKnown}
            onChange={(event) => setCostKnown(event.target.checked)}
          />
        </div>
      )}
      {(problem || error) && (
        <p className="form-error" role="alert">
          {problem ?? errorMessage(error)}
        </p>
      )}
      <div className="ledger-form__actions">
        <MagneticButton type="submit" magnetic={false} loading={pending}>
          {initial ? "Salvar lançamento" : "Adicionar lançamento"}
        </MagneticButton>
        <MagneticButton variant="ghost" magnetic={false} onClick={onDone}>
          Cancelar
        </MagneticButton>
      </div>
    </form>
  );
}

type MaskedInputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> & {
  value: string;
  onChange: (text: string) => void;
  /** O que aparece com os valores ocultos enquanto o campo não tem foco; `null` nunca mascara. */
  mask: string | null;
  ref?: Ref<HTMLInputElement>;
};

/**
 * Campo de número do lançamento. Com o olho fechado, fora do foco ele mostra
 * a máscara (é o que o leitor de tela lê também), como o `MoneyInput`; no
 * foco, o número para editar.
 */
function MaskedInput({ value, onChange, mask, ref, ...rest }: MaskedInputProps) {
  const money = useMoney();
  const [focused, setFocused] = useState(false);
  const masked = money.hidden && !focused && mask !== null && value !== "";
  return (
    <input
      {...rest}
      ref={ref}
      className="text-input"
      type="text"
      inputMode="decimal"
      autoComplete="off"
      value={masked ? mask : value}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}
