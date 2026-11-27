import { errorMessage } from "../../api/client";
import { FoldPanel } from "../income/FoldPanel";
import { coveredItems, NOT_COVERED } from "./coverage";
import { useTaxRules } from "./useTax";

/**
 * O que o IR do Prumo cobre no ano, com a regra e a fonte de cada linha (da
 * tabela do servidor), e o que fica fora, com o porquê. Recolhido: se lê uma
 * vez.
 */
export function CoveragePanel({ year }: { year: number }) {
  const rules = useTaxRules(year);
  const covered = rules.data ? coveredItems(rules.data) : [];
  return (
    <FoldPanel
      title="O que o Prumo cobre"
      count={rules.data ? `${covered.length} calcula · ${NOT_COVERED.length} fora` : `${NOT_COVERED.length} fora`}
    >
      <h3 className="tax-subtitle">Calcula em {year}</h3>
      {rules.isError && <p className="field__hint tax-fold-text">{errorMessage(rules.error)}</p>}
      <ul className="tax-coverage" aria-label={`O que o Prumo calcula em ${year}`}>
        {covered.map((item) => (
          <li key={item.id}>
            <strong>{item.title}</strong>
            <span>{item.detail}</span>
            <span className="tax-source">Fonte: {item.source}</span>
          </li>
        ))}
      </ul>
      <h3 className="tax-subtitle">Fica fora da conta</h3>
      <ul className="tax-coverage" aria-label="O que o Prumo não calcula">
        {NOT_COVERED.map((item) => (
          <li key={item.id}>
            <strong>{item.title}</strong>
            <span className="field__hint">{item.why}</span>
          </li>
        ))}
      </ul>
    </FoldPanel>
  );
}
