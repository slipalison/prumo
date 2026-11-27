import { useState } from "react";
import type { Asset, SymbolSuggestion } from "../../api/types";
import { AssetIcon } from "../../components/AssetIcon";
import { IconClose } from "../../components/icons";
import { cx } from "../../lib/cx";
import { plural } from "../../lib/format";
import { SymbolCombobox } from "../portfolio/SymbolCombobox";
import { comparable, keyForSuggestion, MAX_ASSETS, suggestionComparable, symbolOf } from "./selection";

type AssetPickerProps = {
  keys: readonly string[];
  portfolio: readonly Asset[];
  /** O nome e a classe de cada escolha, quando o servidor já respondeu. */
  names: ReadonlyMap<string, { name: string; assetClass: Asset["asset_class"] }>;
  onChange: (keys: string[]) => void;
};

/**
 * A escolha dos ativos: os que já estão na comparação (tirar um é um toque),
 * a busca de ticker (a mesma do cadastro) e os da carteira ao alcance de um
 * toque. Até cinco; a lista fica no endereço.
 */
export function AssetPicker({ keys, portfolio, names, onChange }: Readonly<AssetPickerProps>) {
  const [text, setText] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const full = keys.length >= MAX_ASSETS;
  const offered = portfolio.filter(comparable).filter((asset) => !keys.some((key) => symbolOf(key) === asset.symbol.toUpperCase()));

  const add = (key: string) => {
    setNotice(null);
    if (keys.some((k) => symbolOf(k) === symbolOf(key))) {
      setNotice(`${symbolOf(key)} já está na comparação.`);
      return;
    }
    if (full) {
      setNotice(`A comparação tem no máximo ${MAX_ASSETS} ativos. Tire um para pôr outro.`);
      return;
    }
    onChange([...keys, key]);
  };
  const choose = (suggestion: SymbolSuggestion) => {
    setText("");
    if (!suggestionComparable(suggestion)) {
      setNotice("Renda fixa e Tesouro não entram: o comparador cobre ativos com cotação em bolsa.");
      return;
    }
    add(keyForSuggestion(suggestion, portfolio));
  };

  return (
    <div className="compare-picker">
      <ul className="compare-picker__chosen" aria-label="Ativos na comparação">
        {keys.map((key) => {
          const symbol = symbolOf(key);
          const info = names.get(key);
          return (
            <li key={key} className="compare-chip">
              <AssetIcon symbol={symbol} name={info?.name ?? symbol} assetClass={info?.assetClass ?? "br_stocks"} size="sm" />
              <span className="compare-chip__symbol">{symbol}</span>
              {info && info.name !== symbol && <span className="compare-chip__name">{info.name}</span>}
              <button
                type="button"
                className="compare-chip__remove"
                aria-label={`Tirar ${symbol} da comparação`}
                onClick={() => onChange(keys.filter((k) => k !== key))}
              >
                <IconClose width={14} height={14} />
              </button>
            </li>
          );
        })}
        {keys.length === 0 && <li className="field__hint">Nenhum ativo ainda.</li>}
      </ul>
      <output className="field__hint compare-picker__count">
        {keys.length} de {MAX_ASSETS} {plural(keys.length, ["ativo", "ativos"])}
        {notice && <span className="compare-picker__notice"> · {notice}</span>}
      </output>
      <div className="compare-picker__search">
        <SymbolCombobox
          id="compare-search"
          value={text}
          onChange={setText}
          onSelect={choose}
          label="Buscar um ativo"
          placeholder="PETR4, KNRI11, AAPL, BTC…"
        />
      </div>
      {offered.length > 0 && (
        <div className="compare-picker__held">
          <span className="field__label">Da sua carteira</span>
          <ul className="compare-picker__offers" aria-label="Ativos da carteira para somar">
            {offered.map((asset) => (
              <li key={asset.id}>
                <button
                  type="button"
                  className={cx("chip", "compare-offer")}
                  disabled={full}
                  aria-label={`Somar ${asset.symbol} à comparação`}
                  onClick={() => add(asset.symbol.toUpperCase())}
                >
                  {asset.symbol}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
