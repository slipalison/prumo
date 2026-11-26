import type { ReactNode } from "react";
import { IconInfo } from "./icons";
import { Toggletip, type ToggletipAlign, type ToggletipPlacement } from "./Toggletip";

type InfoTipProps = {
  /** O nome do botão: o que ele explica ("Como se calcula o yield on cost"). */
  label: string;
  /** A explicação: uma a três frases, sem valor que não esteja na tela. */
  children: ReactNode;
  placement?: ToggletipPlacement;
  align?: ToggletipAlign;
};

/**
 * O ⓘ ao lado de um título, rótulo ou número: a explicação que antes era um
 * parágrafo na tela, agora sob demanda (toque, foco ou mouse). O ícone tem
 * 16 px, mas a área de toque chega a 44 px sem empurrar o texto ao lado.
 */
export function InfoTip({ label, children, placement, align }: InfoTipProps) {
  return (
    <Toggletip label={label} content={children} className="info-tip" placement={placement} align={align}>
      <IconInfo width={16} height={16} />
    </Toggletip>
  );
}
