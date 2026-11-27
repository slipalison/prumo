import { useEffect, useState } from "react";
import { formatQuantity, parseDecimal } from "../lib/format";
import { useMoney } from "../lib/privacy";
import { useFocusOnMount } from "../lib/useFocusOnMount";
import { IconPlus } from "./icons";

type QuantityInputProps = {
  id: string;
  label: string;
  value: number;
  onChange: (value: number) => void;
  /** Cripto aceita fração; cota e ação, só inteiro. */
  fractional?: boolean;
  min?: number;
  focusOnMount?: boolean;
  /** Uma linha de ajuda sob o campo, lida junto pelo leitor de tela. */
  hint?: string;
};

/**
 * Quantidade com − e + para inteiros; para fração, campo livre que aceita
 * vírgula ("0,015"). O texto digitado fica como a pessoa escreveu até sair do
 * campo — "0," não vira "0" no meio da digitação.
 *
 * Com os valores ocultos o campo segue editável: mostra a quantidade só
 * enquanto tem o foco, e a máscara fora dele (e para o leitor de tela).
 */
export function QuantityInput({
  id,
  label,
  value,
  onChange,
  fractional = false,
  min = 0,
  focusOnMount,
  hint,
}: Readonly<QuantityInputProps>) {
  const hintId = `${id}-hint`;
  const inputRef = useFocusOnMount<HTMLInputElement>(focusOnMount);
  const [text, setText] = useState(() => formatQuantity(value, fractional));
  const money = useMoney();
  const [focused, setFocused] = useState(false);

  useEffect(() => {
    setText((current) => {
      const parsed = parseDecimal(current);
      return parsed === value ? current : formatQuantity(value, fractional);
    });
  }, [value, fractional]);

  const commit = (next: number) => {
    const clean = Math.max(min, fractional ? next : Math.round(next));
    onChange(clean);
    setText(formatQuantity(clean, fractional));
  };

  return (
    <div className="quantity">
      <label htmlFor={id} className="field__label">
        {label}
      </label>
      <div className="quantity__row">
        {!fractional && (
          <button
            type="button"
            className="quantity__step"
            aria-label={`Diminuir ${label.toLowerCase()}`}
            onClick={() => commit(value - 1)}
            disabled={value <= min}
          >
            <span aria-hidden="true">−</span>
          </button>
        )}
        <input
          id={id}
          className="quantity__input"
          type="text"
          inputMode={fractional ? "decimal" : "numeric"}
          autoComplete="off"
          ref={inputRef}
          data-autofocus={focusOnMount ? "" : undefined}
          aria-describedby={hint ? hintId : undefined}
          value={money.hidden && !focused ? money.quantity(value, fractional) : text}
          onFocus={() => setFocused(true)}
          onChange={(event) => {
            setText(event.target.value);
            const parsed = parseDecimal(event.target.value.replaceAll(".", fractional ? "." : ""));
            if (parsed !== null && parsed >= min) onChange(fractional ? parsed : Math.round(parsed));
          }}
          onBlur={() => {
            setFocused(false);
            setText(formatQuantity(value, fractional));
          }}
        />
        {!fractional && (
          <button
            type="button"
            className="quantity__step"
            aria-label={`Aumentar ${label.toLowerCase()}`}
            onClick={() => commit(value + 1)}
          >
            <IconPlus />
          </button>
        )}
      </div>
      {hint && (
        <p id={hintId} className="field__hint">
          {hint}
        </p>
      )}
    </div>
  );
}
