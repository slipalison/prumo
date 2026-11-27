import { useId } from "react";
import type { CeilingMethod, FundReference, MarketRates, RefValue, StockReference } from "../../api/ceilingTypes";
import { IconAlert, IconCheck, IconClose } from "../../components/icons";
import { InfoTip } from "../../components/InfoTip";
import { MoneyInput } from "../../components/MoneyInput";
import {
  FAMILY_METHODS,
  GAP_TEXT,
  METHOD_LABEL,
  methodPhrase,
  methodValue,
  type CeilingDraft,
  type ReferenceFamily,
} from "../../lib/ceilingText";
import { cx } from "../../lib/cx";
import { priceCheck } from "../../lib/planText";
import { useMoney } from "../../lib/privacy";
import { methodTip } from "./methodTip";

export type CeilingPickerProps = {
  /** O ativo, como a pessoa o chama ("ITSA4"), para os nomes acessíveis. */
  name: string;
  /** O preço de hoje, para o chip aprova / não aprova; `null` sem preço. */
  priceNow: number | null;
  /** A família do ativo; `null` (exterior, cripto, renda fixa) só oferece Sem teto e Valor próprio. */
  family: ReferenceFamily | null;
  /** As referências de hoje do ativo; `null` enquanto carregam ou quando não há. */
  item: StockReference | FundReference | null;
  market: MarketRates | null;
  /** O rendimento-alvo do Bazin, para a conta do ⓘ. */
  bazinYield: number | null;
  value: CeilingDraft;
  onChange: (draft: CeilingDraft) => void;
  /** O valor que o campo de valor próprio mostra ao ser escolhido. */
  manualStart: number;
  /** As referências ainda estão chegando. */
  loading?: boolean;
};

/**
 * A escolha do teto de preço, a mesma na Análise e na folha do ativo: um grupo
 * de rádio com Sem teto, cada método da família do ativo com o valor de hoje e
 * o chip aprova / não aprova contra o preço de hoje, e Valor próprio (o campo
 * abre ao escolher). O teto é da pessoa: nada vem marcado além do que está
 * gravado, e a tela nunca diz "compre" ou "barato".
 */
export function CeilingPicker({
  name,
  priceNow,
  family,
  item,
  market,
  bazinYield,
  value,
  onChange,
  manualStart,
  loading = false,
}: Readonly<CeilingPickerProps>) {
  const group = useId();
  const manualId = useId();
  const methods: CeilingMethod[] = family ? FAMILY_METHODS[family] : [];
  const chosen = value.kind === "method" ? value.method : null;
  const chosenValue = chosen && item ? methodValue(item, chosen) : null;

  return (
    <fieldset className="ceiling-picker">
      <legend className="sr-only">Teto de preço de {name}</legend>
      <div className="ceiling-option">
        <input
          id={`${group}-none`}
          type="radio"
          name={group}
          className="ceiling-option__radio"
          checked={value.kind === "none"}
          onChange={() => onChange({ kind: "none" })}
        />
        <label htmlFor={`${group}-none`} className="ceiling-option__label">
          <span className="ceiling-option__name">Sem teto</span>
          <span className="ceiling-option__hint">o aporte segue sem teto</span>
        </label>
      </div>

      {methods.map((method) => (
        <MethodOption
          key={method}
          group={group}
          method={method}
          name={name}
          chosen={chosen}
          priceNow={priceNow}
          item={item}
          market={market}
          bazinYield={bazinYield}
          loading={loading}
          onChange={onChange}
        />
      ))}

      <div className="ceiling-option">
        <input
          id={`${group}-manual`}
          type="radio"
          name={group}
          className="ceiling-option__radio"
          checked={value.kind === "manual"}
          onChange={() => onChange({ kind: "manual", value: value.kind === "manual" ? value.value : manualStart })}
        />
        <label htmlFor={`${group}-manual`} className="ceiling-option__label">
          <span className="ceiling-option__name">Valor próprio</span>
          <span className="ceiling-option__hint">um preço fixo, que você escolhe</span>
        </label>
      </div>
      {value.kind === "manual" && (
        <div className="ceiling-picker__manual">
          <MoneyInput
            id={manualId}
            label="Pagar no máximo, por unidade"
            value={value.value}
            onChange={(reais) => onChange({ kind: "manual", value: reais })}
          />
        </div>
      )}

      {chosen && chosenValue?.value === null && !loading && (
        <p className="inline-notice ceiling-picker__notice" role="note">
          <IconAlert width={16} height={16} style={{ verticalAlign: "-3px" }} /> O teto {methodPhrase(chosen)} está sem
          valor hoje ({GAP_TEXT[chosenValue.gap ?? "no_reference"]}): com ele, {name} fica fora do aporte até o método voltar
          a ter valor.
        </p>
      )}
    </fieldset>
  );
}

type MethodOptionProps = Pick<CeilingPickerProps, "name" | "priceNow" | "item" | "market" | "bazinYield" | "onChange"> & {
  /** O nome do grupo de rádio (o mesmo de todas as opções). */
  group: string;
  method: CeilingMethod;
  /** O método escolhido hoje; `null` em Sem teto e em Valor próprio. */
  chosen: CeilingMethod | null;
  loading: boolean;
};

/** Uma opção de método: o rádio, o nome, o valor de hoje com o chip e o ⓘ com a conta. */
function MethodOption({ group, method, name, chosen, priceNow, item, market, bazinYield, loading, onChange }: Readonly<MethodOptionProps>) {
  const money = useMoney();
  const id = `${group}-${method}`;
  const found = item ? methodValue(item, method) : null;
  const tip = item && market ? methodTip(item, method, market, bazinYield, money) : null;
  return (
    <div className="ceiling-option">
      <input
        id={id}
        type="radio"
        name={group}
        className="ceiling-option__radio"
        checked={chosen === method}
        onChange={() => onChange({ kind: "method", method })}
      />
      <label htmlFor={id} className="ceiling-option__label">
        <span className="ceiling-option__name">{METHOD_LABEL[method]}</span>
        <span className="ceiling-option__state">
          <MethodState found={found} priceNow={priceNow} loading={loading} />
        </span>
      </label>
      {tip && (
        <InfoTip label={`Como sai o ${METHOD_LABEL[method]} de ${name}`} align="end">
          {tip}
        </InfoTip>
      )}
    </div>
  );
}

/**
 * O valor de hoje do método e o chip aprova / não aprova contra o preço de
 * hoje; carregando, sem referência ou sem valor, o porquê.
 */
function MethodState({ found, priceNow, loading }: Readonly<{ found: RefValue | null; priceNow: number | null; loading: boolean }>) {
  const money = useMoney();
  if (loading || !found) return <span className="ceiling-option__hint">{loading ? "carregando…" : "sem referência"}</span>;
  if (found.value === null) {
    return <span className="ceiling-option__hint">sem valor hoje: {GAP_TEXT[found.gap ?? "no_reference"]}</span>;
  }
  const check = priceNow === null ? null : priceCheck(priceNow, found.value);
  return (
    <>
      <span className="ceiling-option__value">{money.brl(found.value)}</span>
      {check && (
        <span className={cx("price-ref", "ceiling-option__chip", check.approves ? "price-ref--approves" : "price-ref--rejects")}>
          {check.approves ? <IconCheck width={14} height={14} /> : <IconClose width={14} height={14} />}
          <span>{check.verdict}</span>
        </span>
      )}
    </>
  );
}
