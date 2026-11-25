import { useId, useState, type FormEvent } from "react";
import { errorMessage } from "../../api/client";
import { useCreateAsset } from "../../api/queries";
import type { Asset, AssetClass, CreateAssetRequest, SymbolSuggestion } from "../../api/types";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { QuantityInput } from "../../components/QuantityInput";
import { ScoreSlider } from "../../components/ScoreSlider";
import {
  acceptsB3Listing,
  ASSET_CLASSES,
  CLASS_META,
  classColor,
  guessClassFromSymbol,
  looksLikeB3Ticker,
  unitOf,
} from "../../lib/assetClasses";
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
 *
 * Quantidade 0 vale: o ativo que a pessoa ainda não comprou entra na lista com
 * a nota, e o aporte já o considera na divisão da classe.
 */
export function AssetForm({ initialClass, submitLabel = "Adicionar à carteira", onCreated }: AssetFormProps) {
  const ids = { symbol: useId(), quantity: useId(), name: useId(), amount: useId(), score: useId(), b3: useId() };
  const [assetClass, setAssetClass] = useState<AssetClass>(initialClass ?? "br_stocks");
  const [classTouched, setClassTouched] = useState(Boolean(initialClass));
  const [symbol, setSymbol] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [quantity, setQuantity] = useState(0);
  const [amount, setAmount] = useState(0);
  const [score, setScore] = useState(5);
  // "Negociado na B3": segue o formato do ticker até a pessoa mexer na chave.
  const [listedOnB3, setListedOnB3] = useState(false);
  const [b3Touched, setB3Touched] = useState(false);
  const create = useCreateAsset();

  const meta = CLASS_META[assetClass];
  const b3Choice = acceptsB3Listing(assetClass);
  const onB3 = b3Choice ? listedOnB3 : true;
  const unit = unitOf(assetClass, onB3);
  const valueBased = unit === "value";
  const needsManualScore = meta.checklist === null;
  // O campo de cota inteira mostra o número arredondado; é ele que vai.
  const units = unit === "fraction" ? quantity : Math.round(quantity);
  const valid = valueBased ? displayName.trim().length > 0 : cleanSymbol(symbol).length > 0;

  const handleSymbolChange = (text: string) => {
    setSymbol(text);
    setDisplayName("");
    if (!classTouched) {
      const guess = guessClassFromSymbol(text);
      if (guess) setAssetClass(guess);
    }
    if (!b3Touched) setListedOnB3(looksLikeB3Ticker(text));
  };

  const handleSelect = (suggestion: SymbolSuggestion) => {
    setSymbol(suggestion.symbol);
    setDisplayName(suggestion.display_name);
    setAssetClass(suggestion.suggested_class);
    setListedOnB3(suggestion.listed_on_b3);
    setB3Touched(false);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid || create.isPending) return;
    const body: CreateAssetRequest = valueBased
      ? { asset_class: assetClass, display_name: displayName.trim(), amount, score_override: score }
      : {
          asset_class: assetClass,
          symbol: cleanSymbol(symbol),
          quantity: units,
          ...(displayName ? { display_name: displayName } : {}),
          ...(needsManualScore ? { score_override: score } : {}),
          ...(b3Choice && onB3 ? { listed_on_b3: true } : {}),
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

      {b3Choice && (
        <div className="switch-row">
          <label htmlFor={ids.b3} className="switch-row__label">
            Negociado na B3
            <span className="field__hint">
              ETF ou BDR com ticker da B3, como um ETF de bitcoin em Cripto: cotado em reais, cota inteira.
            </span>
          </label>
          <input
            id={ids.b3}
            type="checkbox"
            role="switch"
            className="switch"
            checked={listedOnB3}
            onChange={(event) => {
              setListedOnB3(event.target.checked);
              setB3Touched(true);
            }}
          />
        </div>
      )}

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
          <p className="field__hint">Ainda não aplicou? Deixe zero: ele entra na divisão do aporte pela nota.</p>
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
            fractional={unit === "fraction"}
            hint="Ainda não comprou? Deixe 0: ele entra na divisão do aporte pela nota."
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
