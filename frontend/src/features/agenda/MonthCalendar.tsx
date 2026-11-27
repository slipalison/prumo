import type { AgendaEvent, AgendaEventType } from "../../api/agendaTypes";
import { cx } from "../../lib/cx";
import { calendarWeeks, dayLong, EVENT_TYPE_LABEL, WEEKDAYS, type YearMonth } from "./agendaText";

/** Quantos pontos cabem num dia antes do "+N". */
const DOTS = 3;

const WEEKDAY_NAMES = ["domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"];

function summary(list: AgendaEvent[]): string {
  if (list.length === 0) return "nenhum evento";
  const types = [...new Set(list.map((e) => e.type))].map((t: AgendaEventType) => EVENT_TYPE_LABEL[t]);
  return `${list.length} ${list.length === 1 ? "evento" : "eventos"}: ${types.join(", ")}`;
}

/** A semana se chama pelo primeiro dia do mês que ela tem (toda semana da grade tem ao menos um). */
function weekKey(week: (string | null)[]): string {
  return week.find((day) => day !== null) ?? "";
}

type Props = {
  month: YearMonth;
  days: Map<string, AgendaEvent[]>;
  today: string;
  selected: string;
  onSelect: (day: string) => void;
};

/**
 * O mês em grade, de domingo a sábado. Cada dia é um botão: mostra os tipos de
 * evento em pontos coloridos (com o nome no leitor de tela) e abre a lista do
 * dia ao lado.
 */
export function MonthCalendar({ month, days, today, selected, onSelect }: Readonly<Props>) {
  return (
    <table className="agenda-calendar">
      <thead>
        <tr>
          {WEEKDAYS.map((day, index) => (
            <th key={day} scope="col" abbr={WEEKDAY_NAMES[index]}>
              {day}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {calendarWeeks(month).map((week) => (
          <tr key={weekKey(week)}>
            {WEEKDAYS.map((weekday, column) => {
              const day = week[column];
              // A casa vazia (antes do dia 1 ou depois do último) é a coluna do dia da semana.
              if (!day) return <td key={weekday} className="agenda-calendar__blank" />;
              const list = days.get(day) ?? [];
              const closed = list.some((e) => e.type === "market_closed");
              const types = [...new Set(list.filter((e) => e.type !== "market_closed").map((e) => e.type))];
              return (
                <td key={day}>
                  <button
                    type="button"
                    className={cx(
                      "agenda-calendar__day",
                      day === today && "is-today",
                      day === selected && "is-selected",
                      closed && "is-closed",
                      day < today && "is-past",
                    )}
                    aria-pressed={day === selected}
                    aria-label={`${dayLong(day)}${day === today ? " (hoje)" : ""}: ${summary(list)}`}
                    onClick={() => onSelect(day)}
                  >
                    <span className="agenda-calendar__number">{Number(day.slice(8, 10))}</span>
                    <span className="agenda-calendar__dots" aria-hidden="true">
                      {types.slice(0, DOTS).map((type) => (
                        <span key={type} className={cx("agenda-dot", `agenda-dot--${type}`)} />
                      ))}
                      {types.length > DOTS && <span className="agenda-calendar__more">+{types.length - DOTS}</span>}
                    </span>
                  </button>
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
