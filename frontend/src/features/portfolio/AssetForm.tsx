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
  const ids = {
    symbol: useId(),
    quantity: useId(),
    name: useId(),
    amount: useId(),
    score: useId(),
    b3: useId(),
    averagePrice: useId(),
    invested: useId(),
  };
  const [assetClass, setAssetClass] = useState<AssetClass>(initialClass ?? "br_stocks");
  const [classTouched, setClassTouched] = useState(Boolean(initialClass));
  const [symbol, setSymbol] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [quantity, setQuantity] = useState(0);
  const [amount, setAmount] = useState(0);
  // Custo de entrada, opcional: sem ele o saldo inicial fica de custo
  // desconhecido e a rentabilidade não aparece até a pessoa informar.
  const [averagePrice, setAveragePrice] = useState(0);
  const [invested, setInvested] = useState(0);
  const [score, setScore] = useState(5);
  // "Negociado na B3": segue o formato do ticker até a pessoa mexer na chave.
  const [listedOnB3, setListedOnB3] = useState(false);
  const [b3Touched, setB3Touched] = useState(false);
  // Renda Fixa tem dois jeitos: por valor (CDB, LCI) ou um título do Tesouro
  // Direto, que se escolhe na busca e leva a chave dele.
  const [bondMode, setBondMode] = useState(false);
  const [treasuryKey, setTreasuryKey] = useState<string | null>(null);
  const create = useCreateAsset();

  const meta = CLASS_META[assetClass];
  const treasury = bondMode && assetClass === "fixed_income";
  const b3Choice = acceptsB3Listing(assetClass) && !treasury;
  const onB3 = treasury ? false : b3Choice ? listedOnB3 : true;
  const unit = unitOf(assetClass, onB3, treasury);
  const valueBased = unit === "value";
  const needsManualScore = meta.checklist === null;
  // O campo de cota inteira mostra o número arredondado; é ele que vai.
  const units = unit === "fraction" ? quantity : Math.round(quantity);
  const valid = valueBased
    ? displayName.trim().length > 0
    : treasury
      ? treasuryKey !== null
      : cleanSymbol(symbol).length > 0;

  const handleSymbolChange = (text: string) => {
    setSymbol(text);
    setDisplayName("");
    setTreasuryKey(null);
    if (!classTouched && !bondMode) {
      const guess = guessClassFromSymbol(text);
      if (guess) setAssetClass(guess);
    }
    if (!b3Touched) setListedOnB3(looksLikeB3Ticker(text));
  };

  const handleSelect = (suggestion: SymbolSuggestion) => {
    // O campo mostra o nome do título; a chave (TD-IPCA-…) vai só no cadastro.
    setSymbol(suggestion.treasury ? suggestion.display_name : suggestion.symbol);
    setDisplayName(suggestion.display_name);
    setAssetClass(suggestion.suggested_class);
    setListedOnB3(suggestion.listed_on_b3);
    setB3Touched(false);
    setBondMode(suggestion.treasury);
    setTreasuryKey(suggestion.treasury ? suggestion.symbol : null);
  };

  const chooseClass = (next: AssetClass) => {
    setAssetClass(next);
    setClassTouched(true);
    if (next !== "fixed_income") {
      setBondMode(false);
      setTreasuryKey(null);
    }
  };

  const chooseFixedIncomeMode = (bond: boolean) => {
    setBondMode(bond);
    setTreasuryKey(null);
    setSymbol("");
    setDisplayName("");
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!valid || create.isPending) return;
    const body: CreateAssetRequest = valueBased
      ? {
          asset_class: assetClass,
          display_name: displayName.trim(),
          amount,
          score_override: score,
          ...(amount > 0 && invested > 0 ? { invested } : {}),
        }
      : treasury && treasuryKey
        ? {
            asset_class: assetClass,
            symbol: treasuryKey,
            treasury: true,
            quantity: units,
            score_override: score,
            ...(units > 0 && averagePrice > 0 ? { average_price: averagePrice } : {}),
          }
        : {
            asset_class: assetClass,
            symbol: cleanSymbol(symbol),
            quantity: units,
            ...(displayName ? { display_name: displayName } : {}),
            ...(needsManualScore ? { score_override: score } : {}),
            ...(b3Choice && onB3 ? { listed_on_b3: true } : {}),
            ...(units > 0 && averagePrice > 0 ? { average_price: averagePrice } : {}),
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
              onClick={() => chooseClass(c)}
            >
              <span className="chip__dot" aria-hidden="true" />
              {CLASS_META[c].shortLabel}
            </button>
          ))}
        </div>
      </fieldset>

      {assetClass === "fixed_income" && (
        <fieldset className="chips-field">
          <legend className="field__label">Como cadastrar</legend>
          <div className="chips" role="radiogroup" aria-label="Como cadastrar a renda fixa">
            {[
              { bond: false, label: "Por valor (CDB, LCI…)" },
              { bond: true, label: "Tesouro Direto" },
            ].map(({ bond, label }) => (
              <button
                key={label}
                type="button"
                role="radio"
                aria-checked={bond === bondMode}
                className={cx("chip", bond === bondMode && "is-active")}
                style={{ ["--chip-color" as string]: classColor("fixed_income") }}
                onClick={() => chooseFixedIncomeMode(bond)}
              >
                {label}
              </button>
            ))}
          </div>
        </fieldset>
      )}

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
              placeholder="CDB Banco X, LCI Banco Y…"
              data-autofocus=""
              value={displayName}
              onChange={(event) => setDisplayName(event.target.value)}
            />
          </div>
          <MoneyInput id={ids.amount} label="Quanto você tem hoje" value={amount} onChange={setAmount} />
          <p className="field__hint">Ainda não aplicou? Deixe zero: ele entra na divisão do aporte pela nota.</p>
          {amount > 0 && (
            <>
              <MoneyInput id={ids.invested} label="Quanto você aplicou (opcional)" value={invested} onChange={setInvested} />
              <p className="field__hint">Com ele, o Prumo mostra o rendimento. Dá para informar depois, em Lançamentos.</p>
            </>
          )}
        </>
      ) : (
        <>
          <SymbolCombobox
            id={ids.symbol}
            value={symbol}
            onChange={handleSymbolChange}
            onSelect={handleSelect}
            focusOnMount
            {...(treasury
              ? { label: "Título do Tesouro Direto", placeholder: "Tesouro IPCA+, Selic, Prefixado…" }
              : {})}
          />
          {treasury && treasuryKey === null && (
            <p className="field__hint">Digite o tipo ou o ano e escolha o título na lista: o preço vem sozinho.</p>
          )}
          {displayName && !treasury && <p className="asset-form__picked">{displayName}</p>}
          <QuantityInput
            id={ids.quantity}
            label={treasury ? "Quantidade de títulos" : "Quantidade"}
            value={quantity}
            onChange={setQuantity}
            fractional={unit === "fraction"}
            hint={
              treasury
                ? "Pode ser fração, como 0,35. Ainda não comprou? Deixe 0: ele entra na divisão do aporte pela nota."
                : "Ainda não comprou? Deixe 0: ele entra na divisão do aporte pela nota."
            }
          />
          {units > 0 && (
            <>
              <MoneyInput id={ids.averagePrice} label="Preço médio (opcional)" value={averagePrice} onChange={setAveragePrice} />
              <p className="field__hint">Em reais, por unidade. Sem ele, a rentabilidade aparece quando você informar.</p>
            </>
          )}
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
