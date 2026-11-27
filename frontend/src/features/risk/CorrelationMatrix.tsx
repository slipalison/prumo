import type { CSSProperties } from "react";
import type { RiskPair } from "../../api/riskTypes";
import { cx } from "../../lib/cx";
import { describeCoefficient, formatCoefficient } from "./riskFormat";

type CorrelationMatrixProps = {
  /** O nome de cada linha e coluna, na ordem da matriz. */
  labels: string[];
  ids: string[];
  values: Array<Array<number | null>>;
  /** Os pares destacados (os que mais andam juntos). */
  pairs: RiskPair[];
  caption: string;
};

/** A tinta da célula: a força da correlação vira opacidade, o sinal vira o tom. */
function heat(value: number | null): { className?: string; style?: CSSProperties } {
  if (value === null) return { className: "is-empty" };
  // No máximo 45% de tinta: o número continua com contraste de texto nos dois temas.
  const strength = `${Math.round(Math.min(Math.abs(value), 1) * 45)}%`;
  return {
    className: value >= 0 ? "is-pos" : "is-neg",
    style: { ["--heat" as string]: strength },
  };
}

/**
 * A matriz de correlação é uma tabela de verdade (`<th scope>` nas duas
 * pontas): o leitor de tela anda por linha e coluna e ouve o número; quem vê
 * tem a cor por trás de cada número (laranja quando andam juntos, azul quando
 * andam em sentidos opostos, mais forte quanto mais perto de 1 ou de −1). O
 * número está sempre escrito, então a cor nunca é a única pista. Os pares de
 * destaque levam um contorno e estão listados ao lado, em texto.
 */
export function CorrelationMatrix({ labels, ids, values, pairs, caption }: CorrelationMatrixProps) {
  const highlighted = new Set(pairs.flatMap((p) => [`${p.a}|${p.b}`, `${p.b}|${p.a}`]));
  return (
    <div className="corr__scroll">
      <table className="corr">
        <caption className="sr-only">{caption}</caption>
        <thead>
          <tr>
            <td className="corr__corner" />
            {labels.map((label, j) => (
              <th key={ids[j]} scope="col" className="corr__col">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {labels.map((label, i) => (
            <tr key={ids[i]}>
              <th scope="row" className="corr__row">
                {label}
              </th>
              {labels.map((other, j) => {
                const value = values[i]?.[j] ?? null;
                const tint = heat(value);
                const marked = i !== j && highlighted.has(`${ids[i]}|${ids[j]}`);
                return (
                  <td
                    key={ids[j]}
                    className={cx("corr__cell", tint.className, i === j && "is-self", marked && "is-top")}
                    style={tint.style}
                    title={
                      value === null || i === j
                        ? undefined
                        : `${label} e ${other}: ${formatCoefficient(value)}, ${describeCoefficient(value)}`
                    }
                  >
                    {value === null ? (
                      <>
                        <span aria-hidden="true">—</span>
                        <span className="sr-only">poucos pregões em comum</span>
                      </>
                    ) : (
                      formatCoefficient(value)
                    )}
                    {marked && <span className="sr-only"> (entre os que mais andam juntos)</span>}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** A régua da cor: −1, 0 e 1 com o que cada ponta quer dizer. */
export function CorrelationScale() {
  return (
    <div className="corr-scale" aria-hidden="true">
      <span className="corr-scale__bar" />
      <span className="corr-scale__labels">
        <span>−1 sentidos opostos</span>
        <span>0 sem relação</span>
        <span>1 juntos</span>
      </span>
    </div>
  );
}
