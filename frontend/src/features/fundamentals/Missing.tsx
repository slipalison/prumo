import type { FundamentalsGap } from "../../api/fundamentalsTypes";
import { GAP_TEXT, isNotApplicable } from "./fundamentalsText";

/**
 * O lugar de um número que não há. Indicador que não se aplica (banco,
 * seguradora, holding) diz "não se aplica" escrito; o resto é "—". Nos dois, o
 * motivo vai no `title` e para o leitor de tela. Nunca um zero.
 */
export function Missing({ gap }: Readonly<{ gap: FundamentalsGap | null }>) {
  const text = gap ? GAP_TEXT[gap] : "sem número";
  if (isNotApplicable(gap)) {
    return (
      <span className="not-applicable">
        <span aria-hidden="true" title={text}>
          não se aplica
        </span>
        <span className="sr-only">{text}</span>
      </span>
    );
  }
  return (
    <>
      <span aria-hidden="true" title={text}>
        —
      </span>
      <span className="sr-only">{text}</span>
    </>
  );
}
