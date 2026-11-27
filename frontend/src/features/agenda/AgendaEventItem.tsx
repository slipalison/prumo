import type { AgendaEvent, AgendaIncome, UnusualMark } from "../../api/agendaTypes";
import { AssetIcon } from "../../components/AssetIcon";
import { cx } from "../../lib/cx";
import { KIND_LABEL } from "../../lib/income";
import { useMoney } from "../../lib/privacy";
import {
  assetNames,
  badgeLabel,
  categoryLabel,
  CLOSURE_LABEL,
  resultsTitle,
  shortDate,
  unitText,
  unusualTitle,
  usualText,
} from "./agendaText";

/** O selo de fora do padrão na data com e no pagamento. */
export function UnusualBadge({ mark }: { mark: UnusualMark }) {
  const money = useMoney();
  return (
    <span className={cx("agenda-unusual", `agenda-unusual--${mark.direction}`)}>
      {unusualTitle(mark.direction, mark.recent.change)}
      <span className="sr-only"> do habitual, que é de {usualText(money, mark)}</span>
    </span>
  );
}

function incomeLine(income: AgendaIncome, type: AgendaEvent["type"]): string {
  if (type === "income_record") {
    const ex = income.ex_date ? `data ex ${shortDate(income.ex_date)}` : null;
    const pay = income.pay_date ? `paga em ${shortDate(income.pay_date)}` : "pagamento a definir";
    return [ex, pay].filter(Boolean).join(" · ");
  }
  const state = income.state === "received" ? "Recebido" : "A receber";
  return income.record_date ? `${state} · data com ${shortDate(income.record_date)}` : state;
}

function EventBody({ event }: { event: AgendaEvent }) {
  const money = useMoney();
  const names = assetNames(event);
  const { income, unusual, document } = event;
  if (income) {
    return (
      <>
        <span className="agenda-item__text">
          <strong>{names}</strong>
          <span className="muted"> · {KIND_LABEL[income.kind]}</span>
          {income.unusual && (
            <>
              {" "}
              <UnusualBadge mark={income.unusual} />
            </>
          )}
          <span className="agenda-item__detail">{incomeLine(income, event.type)}</span>
        </span>
        <strong className="agenda-item__value">
          {income.state === "undated" ? `~${money.brl(income.net)}` : money.brl(income.net)}
        </strong>
      </>
    );
  }
  if (unusual) {
    const mark = { recent: unusual.recent, long: unusual.long, years: unusual.years };
    return (
      <span className="agenda-item__text">
        <strong>
          {unusualTitle(unusual.direction, unusual.recent.change)} no provento de {names}
        </strong>
        <span className="agenda-item__detail">
          {unitText(money, unusual.per_unit)} por cota; habitual de {usualText(money, mark)} · data com{" "}
          {shortDate(unusual.record_date)}
        </span>
      </span>
    );
  }
  if (document) {
    const label = event.type === "results" ? resultsTitle(document) : categoryLabel(document);
    const where = document.source === "fundos_net" ? "Fundos.NET" : "CVM";
    const detail =
      event.type === "results"
        ? `entregue ${document.source === "fundos_net" ? "no" : "à"} ${where}`
        : (document.subject ?? document.title ?? where);
    return (
      <>
        <span className="agenda-item__text">
          <strong>{names}</strong>
          <span className="muted"> · {label}</span>
          <span className="agenda-item__detail">{detail}</span>
        </span>
        {document.url && (
          <a
            className="link-btn agenda-item__link"
            href={document.url}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={`Abrir o documento: ${label} de ${names} (abre em nova aba)`}
          >
            Abrir
          </a>
        )}
      </>
    );
  }
  return (
    <span className="agenda-item__text">
      <strong>Sem pregão na B3</strong>
      <span className="agenda-item__detail">{event.closure ? CLOSURE_LABEL[event.closure] : ""}</span>
    </span>
  );
}

/** Uma linha da Agenda: o tipo, quem, o quê e (no provento) quanto. */
export function AgendaEventItem({ event, past }: { event: AgendaEvent; past: boolean }) {
  const first = event.assets[0];
  return (
    <li className={cx("agenda-item", `agenda-item--${event.type}`, past && "is-past")}>
      <span className={cx("agenda-type", `agenda-type--${event.type}`)}>{badgeLabel(event)}</span>
      {first ? (
        <AssetIcon symbol={first.symbol} name={first.display_name} assetClass={first.asset_class} size="sm" />
      ) : (
        <span className="agenda-item__icon" aria-hidden="true" />
      )}
      <EventBody event={event} />
    </li>
  );
}
