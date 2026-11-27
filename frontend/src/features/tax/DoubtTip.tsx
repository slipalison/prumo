import type { Doubt } from "../../api/taxTypes";
import { IconInfo } from "../../components/icons";
import { Toggletip } from "../../components/Toggletip";
import { cx } from "../../lib/cx";
import { DOUBT_SHORT, DOUBT_TEXT } from "./taxText";

type DoubtTipProps = {
  doubt: Doubt;
  /** Escreve o nome curto da dúvida ao lado do ícone (onde há espaço). */
  text?: boolean;
};

/**
 * Um ponto de docs/ir.md § 5 a confirmar com o contador: o ⓘ azul fica ao lado
 * do número que ele mexe e abre a explicação. O nome diz do que se trata, para
 * a cor não ser a única pista.
 */
export function DoubtTip({ doubt, text = false }: Readonly<DoubtTipProps>) {
  return (
    <Toggletip
      label={`Ponto a confirmar com contador: ${DOUBT_SHORT[doubt]}`}
      content={DOUBT_TEXT[doubt]}
      className={cx("tax-doubt-tip", text && "tax-doubt-tip--text")}
      align="start"
      placement="bottom"
    >
      <IconInfo width={16} height={16} />
      {text && <span>{DOUBT_SHORT[doubt]}</span>}
    </Toggletip>
  );
}
