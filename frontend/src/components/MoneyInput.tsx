import type { CSSProperties, FocusEvent, KeyboardEvent } from "react";
import { useRef } from "react";
import { cx } from "../lib/cx";
import { useFocusOnMount } from "../lib/useFocusOnMount";
import { formatMoneyInput, reaisFromInput } from "../lib/moneyMask";

type MoneyInputProps = {
  id: string;
  value: number;
  onChange: (reais: number) => void;
  /** Rótulo visível; use `labelledBy` quando o rótulo for um título da tela. */
  label?: string;
  labelledBy?: string;
  size?: "hero" | "md";
  focusOnMount?: boolean;
  /** O campo principal da tela: recebe o foco de volta quando uma folha fecha. */
  screenFocus?: boolean;
  onEnter?: () => void;
  disabled?: boolean;
};

/** Até este tamanho o valor cabe na fonte cheia; acima, a fonte encolhe. */
const FULL_SIZE_CHARS = 11;

/**
 * Campo de dinheiro com máscara de caixa eletrônico: cada dígito entra pela
 * direita, como centavo. Teclado numérico no celular.
 *
 * Ao ganhar foco o valor inteiro fica selecionado: o próximo dígito começa um
 * valor novo, em vez de se pendurar no anterior (R$ 3.000,00 + "3000" não pode
 * virar R$ 3.000.003.000,00). Com o campo já focado, o clique não mexe no
 * cursor: numa máscara de caixa eletrônico o dígito sempre entra pela direita,
 * e um cursor no meio do número produziria valores sem sentido.
 */
export function MoneyInput({
  id,
  value,
  onChange,
  label,
  labelledBy,
  size = "md",
  focusOnMount,
  screenFocus,
  onEnter,
  disabled,
}: MoneyInputProps) {
  const inputRef = useFocusOnMount<HTMLInputElement>(focusOnMount);
  // O clique que dá o foco termina num mouseup que reposiciona o cursor e
  // desfaz a seleção; esse primeiro mouseup é engolido.
  const justFocused = useRef(false);
  const pendingSelect = useRef<number | undefined>(undefined);
  const cancelPendingSelect = () => {
    window.clearTimeout(pendingSelect.current);
    pendingSelect.current = undefined;
  };
  const text = formatMoneyInput(value);
  const fit = Math.min(1, FULL_SIZE_CHARS / Math.max(text.length, 1));

  const handleFocus = (event: FocusEvent<HTMLInputElement>) => {
    justFocused.current = true;
    const input = event.currentTarget;
    input.select();
    // No Chrome o clique que dá o foco posiciona o cursor DEPOIS do evento de
    // foco, desfazendo a seleção acima; a segunda chamada vem depois dele.
    // Se a pessoa já começou a digitar, a seleção atrasada não pode engolir o
    // que ela digitou: `handleKeyDown` cancela.
    pendingSelect.current = window.setTimeout(() => {
      pendingSelect.current = undefined;
      if (document.activeElement === input) input.select();
    }, 0);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    cancelPendingSelect();
    if (event.key === "Enter" && onEnter) {
      event.preventDefault();
      onEnter();
    }
  };

  return (
    <div className={cx("money-field", `money-field--${size}`)}>
      {label && (
        <label htmlFor={id} className="field__label">
          {label}
        </label>
      )}
      <input
        id={id}
        className="money-field__input"
        type="text"
        inputMode="numeric"
        autoComplete="off"
        aria-labelledby={labelledBy}
        ref={inputRef}
        data-autofocus={focusOnMount ? "" : undefined}
        data-screen-focus={screenFocus ? "" : undefined}
        value={text}
        style={{ "--money-fit": fit.toFixed(3) } as CSSProperties}
        disabled={disabled}
        onFocus={handleFocus}
        onMouseDown={(event) => {
          if (document.activeElement === event.currentTarget) event.preventDefault();
        }}
        onMouseUp={(event) => {
          if (justFocused.current) {
            justFocused.current = false;
            event.preventDefault();
          }
        }}
        onChange={(event) => onChange(reaisFromInput(event.target.value))}
        onKeyDown={handleKeyDown}
      />
    </div>
  );
}
