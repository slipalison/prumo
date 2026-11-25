import type { IncomePayer } from "../../api/incomeTypes";
import { Donut } from "../../components/Donut";
import { formatPercent } from "../../lib/format";
import { payerSlices } from "../../lib/income";
import { useMoney } from "../../lib/privacy";

/** Quem pagou nos últimos 12 meses: rosca e lista com o nome de cada um. */
export function PayerBreakdown({ payers, total }: { payers: IncomePayer[]; total: number }) {
  const money = useMoney();
  const slices = payerSlices(payers);
  return (
    <div className="payers">
      <Donut
        size={170}
        thickness={16}
        label={`Renda dos últimos 12 meses por pagador: ${slices.map((s) => `${s.label} ${formatPercent(s.percent)}`).join(", ")}`}
        segments={slices.map((s) => ({ key: s.key, value: s.total, color: s.color, label: s.label }))}
      >
        <span className="donut__caption">12 meses</span>
        <strong className="donut__number donut__number--sm">{money.compact(total)}</strong>
      </Donut>
      <ul className="payers__list">
        {slices.map((slice) => (
          <li key={slice.key} className="payers__item">
            <span className="payers__name">
              <span className="dot" style={{ background: slice.color }} aria-hidden="true" />
              {slice.label}
            </span>
            <span className="payers__numbers">
              <strong>{money.brl(slice.total)}</strong>
              <span className="muted"> · {formatPercent(slice.percent)}</span>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
