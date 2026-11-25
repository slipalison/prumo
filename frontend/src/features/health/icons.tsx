import type { SVGProps } from "react";
import { Svg } from "../../components/icons";

/** Batimento: o ícone da Saúde na navegação. */
export function IconPulse(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <path d="M3 12h4l2.5-6 4 12 2.5-6H21" />
    </Svg>
  );
}
