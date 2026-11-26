import type { Skipped } from "../../api/taxTypes";
import { useMoney } from "../../lib/privacy";
import { FoldPanel } from "../income/FoldPanel";
import { isoDay, NEVER_CALCULATED, SKIP_TEXT } from "./taxText";

/** O que ficou fora da conta, com o motivo de cada item; fechado, só a contagem. */
export function SkippedPanel({ skipped }: { skipped: Skipped[] }) {
  const money = useMoney();
  return (
    <FoldPanel title="O que o Prumo não calculou" count={skipped.length}>
      {skipped.length === 0 ? (
        <p className="field__hint tax-fold-text">Nada do seu livro ficou de fora da conta.</p>
      ) : (
        <ul className="tax-skipped" aria-label="Fora da conta">
          {skipped.map((item) => (
            <li key={`${item.subject}-${item.date ?? ""}-${item.reason}`}>
              <strong>{item.subject}</strong>
              {item.date && <> · {isoDay(item.date)}</>}
              {item.amount !== null && <> · {money.brl(item.amount)}</>}
              <span className="field__hint">{SKIP_TEXT[item.reason]}</span>
            </li>
          ))}
        </ul>
      )}
    </FoldPanel>
  );
}

/** O que o Prumo nunca calcula (docs/ir.md, seção 3), sempre listado, recolhido. */
export function NeverPanel() {
  return (
    <FoldPanel title="Nunca entra na conta" count={NEVER_CALCULATED.length}>
      <ul className="tax-never">
        {NEVER_CALCULATED.map((text) => (
          <li key={text}>{text}</li>
        ))}
      </ul>
    </FoldPanel>
  );
}
