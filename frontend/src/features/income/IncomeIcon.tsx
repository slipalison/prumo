import type { SVGProps } from "react";

/** A aba Renda: três colunas subindo, com a linha do chão. */
export function IconIncome(props: Readonly<SVGProps<SVGSVGElement>>) {
  return (
    <svg
      width="22"
      height="22"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...props}
    >
      <path d="M4 20h16" />
      <path d="M6.5 20v-5" />
      <path d="M12 20V10" />
      <path d="M17.5 20V5" />
    </svg>
  );
}
