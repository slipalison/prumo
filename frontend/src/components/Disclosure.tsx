import type { ReactNode } from "react";
import { cx } from "../lib/cx";

type DisclosureProps = {
  /** O que se vê fechado: uma frase que diz o que tem dentro. */
  summary: ReactNode;
  /** Quantos itens estão dentro ("8 regras ok"), à direita da frase. */
  count?: ReactNode;
  children: ReactNode;
  /** Abre já aberto (o estado depois fica com a pessoa). */
  defaultOpen?: boolean;
  className?: string;
};

/**
 * Recolher/expandir com `<details>` nativo: teclado, leitor de tela e busca
 * da página funcionam sem código. Para o que é secundário mas tem várias
 * linhas (as regras que estão ok, o detalhe de uma conta); explicação curta
 * vai num ⓘ, formulário vai numa folha (docs/layout.md).
 */
export function Disclosure({ summary, count, children, defaultOpen = false, className }: DisclosureProps) {
  return (
    <details className={cx("disclosure", className)} open={defaultOpen || undefined}>
      <summary className="disclosure__summary">
        <span>{summary}</span>
        {count !== undefined && <span className="disclosure__count">{count}</span>}
      </summary>
      <div className="disclosure__body">{children}</div>
    </details>
  );
}
