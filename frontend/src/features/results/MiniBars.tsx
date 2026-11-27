import { cx } from "../../lib/cx";

export type MiniBar = {
  key: string;
  /** O rótulo curto do eixo ("3T26", "out/26"). */
  label: string;
  /** `null`: sem número (a coluna fica vazia, com o motivo no `title`). */
  value: number | null;
  /** O que aparece ao passar o mouse: o período e o número (já mascarado). */
  title: string;
};

type MiniBarsProps = {
  bars: MiniBar[];
  /** O resumo para o leitor de tela (com os números já mascarados). */
  label: string;
  /** Destaca a última barra (o período mais novo). */
  highlightLast?: boolean;
};

/**
 * Barras pequenas de uma série curta (oito trimestres, doze meses). Barra
 * negativa desce da linha do zero, com outra cor **e** outro desenho
 * (listrada), para a cor não ser a única pista; o número de cada uma está no
 * `title` e no resumo do leitor de tela.
 */
export function MiniBars({ bars, label, highlightLast = true }: MiniBarsProps) {
  const known = bars.map((b) => b.value).filter((v): v is number => v !== null);
  const top = Math.max(0, ...known);
  const bottom = Math.max(0, ...known.map((v) => -v));
  const span = top + bottom || 1;
  const upShare = (top / span) * 100;
  const downShare = 100 - upShare;
  return (
    <div className="mini-bars">
      <div className="mini-bars__plot" role="img" aria-label={label}>
        {bars.map((bar, index) => {
          const value = bar.value;
          const height = value === null ? 0 : (Math.abs(value) / span) * 100;
          const last = highlightLast && index === bars.length - 1;
          return (
            <div key={bar.key} className="mini-bars__col" title={bar.title}>
              <div className="mini-bars__up" style={{ height: `${upShare}%` }}>
                {value !== null && value > 0 && (
                  <span className={cx("mini-bars__bar", last && "is-last")} style={{ height: `${(height / upShare) * 100}%` }} />
                )}
              </div>
              {downShare > 0 && (
                <div className="mini-bars__down" style={{ height: `${downShare}%` }}>
                  {value !== null && value < 0 && (
                    <span
                      className={cx("mini-bars__bar", "is-negative", last && "is-last")}
                      style={{ height: `${(height / downShare) * 100}%` }}
                    />
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {bars.length > 0 && (
        <div className="mini-bars__axis" aria-hidden="true">
          <span>{bars[0]?.label}</span>
          <span>{bars.at(-1)?.label}</span>
        </div>
      )}
    </div>
  );
}
