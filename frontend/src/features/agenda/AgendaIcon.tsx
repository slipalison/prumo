import type { SVGProps } from "react";
import { Svg } from "../../components/icons";

/** A aba Agenda: a folha do calendário com as argolas e um dia marcado. */
export function IconAgenda(props: SVGProps<SVGSVGElement>) {
  return (
    <Svg {...props}>
      <rect x="3.5" y="5" width="17" height="15.5" rx="2.5" />
      <path d="M3.5 10h17" />
      <path d="M8 3v4" />
      <path d="M16 3v4" />
      <path d="M14.5 14.5h2.5v2.5h-2.5z" />
    </Svg>
  );
}
