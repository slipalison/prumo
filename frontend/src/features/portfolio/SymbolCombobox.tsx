import { useId, useState, type KeyboardEvent } from "react";
import { useSymbolSearch } from "../../api/queries";
import type { SymbolSuggestion } from "../../api/types";
import { IconSearch } from "../../components/icons";
import { acceptsB3Listing, CLASS_META } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { useFocusOnMount } from "../../lib/useFocusOnMount";

type SymbolComboboxProps = {
  id: string;
  value: string;
  onChange: (text: string) => void;
  onSelect: (suggestion: SymbolSuggestion) => void;
  focusOnMount?: boolean;
  label?: string;
  placeholder?: string;
};

/**
 * Um campo só: digita o ticker ou o nome, a lista aparece, setas escolhem,
 * Enter confirma. Se a busca cair, o que foi digitado continua valendo.
 */
export function SymbolCombobox({
  id,
  value,
  onChange,
  onSelect,
  focusOnMount,
  label = "Ticker ou nome",
  placeholder = "KNRI11, PETR4, BTC…",
}: SymbolComboboxProps) {
  const listId = useId();
  const inputRef = useFocusOnMount<HTMLInputElement>(focusOnMount);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const search = useSymbolSearch(value);
  const results = search.data ?? [];
  const showList = open && value.trim().length >= 2 && (results.length > 0 || search.isFetching || search.isError);

  const choose = (suggestion: SymbolSuggestion) => {
    onSelect(suggestion);
    setOpen(false);
    setActive(-1);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setOpen(true);
      setActive((current) => (results.length ? (current + 1) % results.length : -1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) => (results.length ? (current <= 0 ? results.length - 1 : current - 1) : -1));
    } else if (event.key === "Enter" && showList && active >= 0) {
      const picked = results[active];
      if (picked) {
        event.preventDefault();
        choose(picked);
      }
    } else if (event.key === "Escape" && showList) {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    }
  };

  const optionId = (index: number) => `${listId}-option-${index}`;

  return (
    <div className="combo">
      <label htmlFor={id} className="field__label">
        {label}
      </label>
      <div className="combo__field">
        <IconSearch className="combo__icon" />
        <input
          id={id}
          className="combo__input"
          type="text"
          role="combobox"
          aria-expanded={showList}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={showList && active >= 0 ? optionId(active) : undefined}
          autoComplete="off"
          autoCapitalize="characters"
          spellCheck={false}
          placeholder={placeholder}
          ref={inputRef}
          data-autofocus={focusOnMount ? "" : undefined}
          value={value}
          onChange={(event) => {
            onChange(event.target.value);
            setOpen(true);
            setActive(-1);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onKeyDown={handleKeyDown}
        />
      </div>
      {showList && (
        <ul id={listId} role="listbox" className="combo__list glass" aria-label="Sugestões">
          {search.isError && <li className="combo__status">Busca indisponível agora — pode seguir com o que digitou.</li>}
          {!search.isError && results.length === 0 && <li className="combo__status">Buscando…</li>}
          {results.map((suggestion, index) => (
            <li
              key={`${suggestion.symbol}-${suggestion.exchange}`}
              id={optionId(index)}
              role="option"
              tabIndex={-1}
              aria-selected={index === active}
              className={cx("combo__option", index === active && "is-active")}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(suggestion)}
              onKeyDown={(event) => {
                if (event.key === "Enter") choose(suggestion);
              }}
            >
              {/* A chave do título do Tesouro não é um ticker: a pessoa lê o nome. */}
              <span className="combo__symbol">{suggestion.treasury ? suggestion.display_name : suggestion.symbol}</span>
              <span className="combo__name">{suggestion.treasury ? suggestion.exchange : suggestion.display_name}</span>
              <span className="combo__class">
                {CLASS_META[suggestion.suggested_class].shortLabel}
                {suggestion.listed_on_b3 && acceptsB3Listing(suggestion.suggested_class) && " · B3"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
