import type { ReactNode } from "react";
import { Disclosure } from "../../components/Disclosure";
import { GlassCard } from "../../components/GlassCard";

type FoldPanelProps = {
  /** O nome do bloco: vale para a região e para o recolher. */
  title: string;
  /** Quantos itens estão dentro, à direita do nome. */
  count?: ReactNode;
  children: ReactNode;
};

/**
 * Um painel de vidro que é só um recolher: o que se lê uma vez ou raramente
 * (lista longa, a conta de cada mês) fica fechado, com a contagem à vista.
 */
export function FoldPanel({ title, count, children }: Readonly<FoldPanelProps>) {
  return (
    <GlassCard as="section" className="panel panel--compact" aria-label={title}>
      <Disclosure summary={title} count={count}>
        {children}
      </Disclosure>
    </GlassCard>
  );
}
