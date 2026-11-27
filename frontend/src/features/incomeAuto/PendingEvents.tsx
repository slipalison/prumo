import type { PendingStockEvent } from "../../api/incomeAutoTypes";
import { Panel } from "../../components/Layout";
import { useMoney } from "../../lib/privacy";
import { pendingSentence } from "./text";
import { useIncomeAuto } from "./useIncomeAuto";
import "./incomeAuto.css";

/**
 * Os eventos em ações que a B3 anunciou e o livro ainda não tem. O Prumo não
 * os lança sozinho: falta o custo atribuído, que só o informe da empresa dá.
 */
export function PendingEventList({ events }: Readonly<{ events: PendingStockEvent[] }>) {
  const money = useMoney();
  return (
    <ul className="auto-pending">
      {events.map((event) => (
        <li key={`${event.asset_id}-${event.kind}-${event.record_date}`} className="auto-pending__item">
          <p className="auto-pending__text">{pendingSentence(event)}</p>
          <p className="field__hint">
            Você tinha {money.quantity(event.quantity, false)} {event.quantity === 1 && !money.hidden ? "cota" : "cotas"} na data com. O lançamento
            fica no livro do ativo, com o custo que a empresa informar.
          </p>
        </li>
      ))}
    </ul>
  );
}

/** O painel da tela Renda: só aparece com os automáticos ligados e alguma pendência. */
export function PendingEventsCard() {
  const auto = useIncomeAuto();
  const events = auto.data?.enabled ? auto.data.pending_events : [];
  if (events.length === 0) return null;
  return (
    <Panel title="Eventos anunciados pela B3">
      <PendingEventList events={events} />
    </Panel>
  );
}
