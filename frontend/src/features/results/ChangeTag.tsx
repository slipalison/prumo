import type { Change } from "../../api/resultsTypes";
import { cx } from "../../lib/cx";
import { changeView } from "./resultsText";

type ChangeTagProps = {
  /** `null`: não há o trimestre para comparar. */
  change: Change | null;
  line: "profit" | "top";
  /** Contra o mesmo trimestre do ano anterior (a/a) ou contra o anterior (t/t). */
  against: "year_ago" | "previous_quarter";
};

const AGAINST = {
  year_ago: { short: "a/a", long: "contra um ano antes", missing: "sem o trimestre de um ano antes" },
  previous_quarter: { short: "t/t", long: "contra o trimestre anterior", missing: "sem o trimestre anterior" },
} as const;

const ARROW = { up: "▲", down: "▼", neutral: "", missing: "" } as const;

/**
 * A variação de uma linha: o texto diz tudo (o percentual, ou "saiu do
 * prejuízo"); a seta e a cor só reforçam. Sem o trimestre comparado, um traço
 * com o motivo.
 */
export function ChangeTag({ change, line, against }: Readonly<ChangeTagProps>) {
  const vs = AGAINST[against];
  const prefix = (
    <>
      <span className="change__vs" aria-hidden="true">
        {vs.short}
      </span>
      <span className="sr-only">{vs.long}:</span>
    </>
  );
  if (!change) {
    return (
      <span className="change change--missing" title={vs.missing}>
        {prefix}
        <span aria-hidden="true">—</span>
        <span className="sr-only">{vs.missing}</span>
      </span>
    );
  }
  const view = changeView(change, line);
  return (
    <span className={cx("change", `change--${view.tone}`)} title={view.hint ?? undefined}>
      {prefix}
      {ARROW[view.tone] && (
        <span className="change__arrow" aria-hidden="true">
          {ARROW[view.tone]}
        </span>
      )}
      <span aria-hidden={view.tone === "missing" || undefined}>{view.text}</span>
      {view.tone === "missing" && <span className="sr-only">{view.hint}</span>}
    </span>
  );
}
