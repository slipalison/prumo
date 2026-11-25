import { useId, useState, type FormEvent } from "react";
import { errorMessage } from "../../api/client";
import { useCreateAsset } from "../../api/queries";
import type { Asset, AssetClass, CreateAssetRequest, SymbolSuggestion } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { ScoreSlider } from "../../components/ScoreSlider";
import { ASSET_CLASSES, CLASS_META, classColor, guessClassFromSymbol } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { SymbolCombobox } from "./SymbolCombobox";

type AssetFormProps = {
  initialClass?: AssetClass | undefined;
  submitLabel?: string;
  onCreated: (asset: Asset) => void;
};

function cleanSymbol(text: string): string {
  return text.trim().toUpperCase().replace(/\.SA$/, "");
}

/**
 * Adicionar ativo com o mínimo de campos: ticker (com autocompletar) e
 * quantidade. Renda fixa troca para nome + valor. Cripto e renda fixa pedem a
 * nota de confiança, porque não têm checklist.
 */
export function AssetForm({ initialClass, submitLabel = "Adicionar à carteira", onCreated }: AssetFormProps) {
  const ids = { symbol: useId(), quantity: useId(), name: useId(), amount: useId(), score: useId() };
  const [assetClass, setAssetClass] = useState<AssetClass>(initialClass ?? "br_stocks");
  const [classTouched, setClassTouched] = useState(Boolean(initialClass));
  const [symbol, setSymbol] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [quantity, setQuantity] = useState(0);
  const [amount, setAmount] = useState(0);
  const [score, setScore] = useState(5);
  const create = useCreateAsset();

  const meta = CLASS_META[assetClass];
  const valueBased = meta.unit === "value";
  const needsManualScore = meta.checklist === null;
  const valid = valueBased ? displayName.trim().length > 0 && amount > 0 : cleanSymbol(symbol).length > 0 && quantity > 0;

  const handleSymbolChange = (text: string) => {
    setSymbol(text);
    setDisplayName("");
    if (!classTouched) {
      const guess = guessClassFromSymbol(text);
      if (guess) setAssetClass(guess);
    }
  };

  const handleSelect = (suggestion: SymbolSuggestion) => {
    setSymbol(suggestion.symbol);
    setDisplayName(suggestion.display_name);
    setAssetClass(suggestion.suggested_class);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid || create.isPending) return;
    const body: CreateAssetRequest = valueBased
      ? { asset_class: assetClass, display_name: displayName.trim(), amount, score_override: score }
      : {
          asset_class: assetClass,
          symbol: cleanSymbol(symbol),
          quantity,
          ...(displayName ? { display_name: displayName } : {}),
          ...(needsManualScore ? { score_override: score } : {}),
        };
    create.mutate(body, { onSuccess: onCreated });
  };

  return (
    <form className="asset-form" onSubmit={submit} noValidate>
      <fieldset className="chips-field">
        <legend className="field__label">Classe</legend>
        <div className="chips" role="radiogroup" aria-label="Classe do ativo">
          {ASSET_CLASSES.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={c === assetClass}
              className={cx("chip", c === assetClass && "is-active")}
              style={{ ["--chip-color" as string]: classColor(c) }}
              onClick={() => {
                setAssetClass(c);
                setClassTouched(true);
              }}
            >
              <span className="chip__dot" aria-hidden="true" />
              {CLASS_META[c].shortLabel}
            </button>
          ))}
        </div>
      </fieldset>

      {valueBased ? (
        <>
          <div className="field">
            <label htmlFor={ids.name} className="field__label">
              Nome
            </label>
            <input
              id={ids.name}
              className="text-input"
              type="text"
              placeholder="Tesouro IPCA+ 2035, CDB Banco X…"
              data-autofocus=""
              value={displayName}
              onChange={(event) => setDisplayName(event.target.value)}
            />
          </div>
          <MoneyInput id={ids.amount} label="Quanto você tem hoje" value={amount} onChange={setAmount} />
        </>
      ) : (
        <>
          <SymbolCombobox id={ids.symbol} value={symbol} onChange={handleSymbolChange} onSelect={handleSelect} focusOnMount />
          {displayName && <p className="asset-form__picked">{displayName}</p>}
          <QuantityInput
            id={ids.quantity}
            label="Quantidade"
            value={quantity}
            onChange={setQuantity}
            fractional={meta.unit === "fraction"}
          />
        </>
      )}

      {needsManualScore && (
        <ScoreSlider
          id={ids.score}
          value={score}
          onChange={setScore}
          hint="Esta classe não tem checklist. Nota 0 tira o ativo dos aportes."
        />
      )}

      {create.isError && (
        <p className="form-error" role="alert">
          {errorMessage(create.error)}
        </p>
      )}

      <MagneticButton type="submit" disabled={!valid} loading={create.isPending} className="asset-form__submit">
        {submitLabel}
      </MagneticButton>
    </form>
  );
}
