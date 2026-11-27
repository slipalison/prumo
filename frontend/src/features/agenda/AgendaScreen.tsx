import { motion } from "motion/react";
import { useId, useMemo, useState } from "react";
import { useAgenda } from "../../api/agenda";
import type { Agenda, AgendaEvent, AgendaEventType } from "../../api/agendaTypes";
import { errorMessage } from "../../api/client";
import type { AssetClass, Portfolio } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { CLASS_META } from "../../lib/assetClasses";
import { cx } from "../../lib/cx";
import { formatDateTime } from "../../lib/format";
import { AgendaEventItem } from "./AgendaEventItem";
import {
  addMonths,
  ALL_TYPES,
  byDay,
  dayHeading,
  dayLong,
  EVENT_TYPE_LABEL,
  FILTER_ORDER,
  keyedEvents,
  monthOf,
  monthRange,
  monthTitle,
  passes,
  todayInBrazil,
  type AgendaFilter,
  type YearMonth,
} from "./agendaText";
import { MonthCalendar } from "./MonthCalendar";
import "./nativeTags.css";

type View = "timeline" | "calendar";

const VIEWS: { id: View; label: string }[] = [
  { id: "timeline", label: "Linha do tempo" },
  { id: "calendar", label: "Calendário" },
];

const capitalize = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/**
 * A Agenda (`#/agenda`): os eventos dos ativos da carteira num mês, em linha do
 * tempo ou calendário, com filtro por tipo, classe e ativo. Informa; não
 * recomenda.
 */
export function AgendaScreen({ portfolio }: Readonly<{ portfolio: Portfolio }>) {
  const [view, setView] = useState<View>("timeline");
  const [month, setMonth] = useState<YearMonth>(() => monthOf(todayInBrazil()));
  const [filter, setFilter] = useState<AgendaFilter>({ types: ALL_TYPES, assetClass: null, assetId: null });
  const { from, to } = monthRange(month);
  const agenda = useAgenda(from, to);

  return (
    <Page className="agenda">
      <PageHeader
        title="Agenda da carteira"
        infoLabel="Sobre a Agenda"
        info={
          <>
            Só os ativos da sua carteira: data com e pagamento dos proventos (o líquido, como na Renda), o provento anunciado
            fora do padrão, o resultado entregue à CVM, fatos relevantes, comunicados e avisos, e os dias sem pregão da B3.
            Informa, não recomenda.
          </>
        }
        actions={<ViewToggle view={view} onChange={setView} />}
      />

      <MonthNav month={month} onChange={setMonth} today={agenda.data?.today ?? todayInBrazil()} />

      {agenda.isPending && <Skeleton height={240} radius={22} />}
      {agenda.isError && (
        <GlassCard as="section" className="state-card" role="alert">
          <h2 className="state-card__title">Não consegui carregar a agenda</h2>
          <p className="state-card__text">{errorMessage(agenda.error)}</p>
          <MagneticButton onClick={() => void agenda.refetch()}>Tentar de novo</MagneticButton>
        </GlassCard>
      )}
      {agenda.data && (
        <Loaded agenda={agenda.data} portfolio={portfolio} filter={filter} onFilter={setFilter} view={view} month={month} />
      )}
    </Page>
  );
}

function ViewToggle({ view, onChange }: Readonly<{ view: View; onChange: (view: View) => void }>) {
  return (
    <fieldset className="segmented" aria-label="Como ver a agenda">
      {VIEWS.map((option) => (
        <button
          key={option.id}
          type="button"
          className={cx("segmented__option", view === option.id && "is-active")}
          aria-pressed={view === option.id}
          onClick={() => onChange(option.id)}
        >
          {view === option.id && <motion.span layoutId="agenda-view" className="segmented__knob" />}
          <span className="segmented__label">{option.label}</span>
        </button>
      ))}
    </fieldset>
  );
}

function MonthNav({ month, onChange, today }: Readonly<{ month: YearMonth; onChange: (month: YearMonth) => void; today: string }>) {
  const current = monthOf(today);
  const isCurrent = current.year === month.year && current.month === month.month;
  return (
    <nav className="agenda-month" aria-label="Mês da agenda">
      <button type="button" className="icon-btn" aria-label="Mês anterior" onClick={() => onChange(addMonths(month, -1))}>
        ‹
      </button>
      <h2 className="agenda-month__title" aria-live="polite">
        {capitalize(monthTitle(month))}
      </h2>
      <button type="button" className="icon-btn" aria-label="Próximo mês" onClick={() => onChange(addMonths(month, 1))}>
        ›
      </button>
      {!isCurrent && (
        <button type="button" className="link-btn" onClick={() => onChange(current)}>
          Voltar para hoje
        </button>
      )}
    </nav>
  );
}

type LoadedProps = {
  agenda: Agenda;
  portfolio: Portfolio;
  filter: AgendaFilter;
  onFilter: (filter: AgendaFilter) => void;
  view: View;
  month: YearMonth;
};

function Loaded({ agenda, portfolio, filter, onFilter, view, month }: Readonly<LoadedProps>) {
  const shown = useMemo(() => agenda.events.filter((e) => passes(e, filter)), [agenda.events, filter]);
  const counts = useMemo(() => {
    const byType = new Map<AgendaEventType, number>();
    const others = { ...filter, types: ALL_TYPES };
    for (const event of agenda.events.filter((e) => passes(e, others))) {
      byType.set(event.type, (byType.get(event.type) ?? 0) + 1);
    }
    return byType;
  }, [agenda.events, filter]);
  const beyondCalendar = agenda.calendar_until !== null && month.year > agenda.calendar_until;

  return (
    <>
      <AgendaFilters portfolio={portfolio} filter={filter} onFilter={onFilter} counts={counts} />
      {beyondCalendar && (
        <p className="field__hint agenda-note">
          O calendário de pregões da B3 está cadastrado até {agenda.calendar_until}: os dias sem pregão deste mês ainda não
          aparecem.
        </p>
      )}
      {view === "calendar" ? (
        <CalendarView agenda={agenda} events={shown} month={month} />
      ) : (
        <Panel
          title="Linha do tempo"
          info={<Freshness agenda={agenda} />}
          infoLabel="De onde vêm os documentos"
          className="agenda-timeline"
        >
          <Timeline events={shown} today={agenda.today} month={month} />
        </Panel>
      )}
    </>
  );
}

/** Quando cada fonte foi conferida, no ⓘ. */
function Freshness({ agenda }: Readonly<{ agenda: Agenda }>) {
  const { cvm_documents: cvm, fund_documents: funds } = agenda.updated;
  return (
    <>
      Os documentos das companhias vêm dos dados abertos da CVM, que os atualiza uma vez por semana
      {cvm ? ` (conferidos em ${formatDateTime(cvm)})` : ""}; os dos FIIs, do Fundos.NET, conferido uma vez por dia
      {funds ? ` (em ${formatDateTime(funds)})` : ""}. O resultado é a data em que o ITR ou a DFP chegou à CVM.
    </>
  );
}

type FiltersProps = {
  portfolio: Portfolio;
  filter: AgendaFilter;
  onFilter: (filter: AgendaFilter) => void;
  counts: Map<AgendaEventType, number>;
};

function AgendaFilters({ portfolio, filter, onFilter, counts }: Readonly<FiltersProps>) {
  const ids = { cls: useId(), asset: useId() };
  const classes = useMemo(
    () => [...new Set(portfolio.assets.map((a) => a.asset_class))].sort((a, b) => CLASS_META[a].label.localeCompare(CLASS_META[b].label)),
    [portfolio.assets],
  );
  const assets = useMemo(
    () =>
      portfolio.assets
        .filter((a) => filter.assetClass === null || a.asset_class === filter.assetClass)
        .map((a) => ({ id: a.id, label: a.symbol ?? a.display_name }))
        .sort((a, b) => a.label.localeCompare(b.label)),
    [portfolio.assets, filter.assetClass],
  );
  const toggle = (type: AgendaEventType) => {
    const next = new Set(filter.types);
    if (next.has(type)) next.delete(type);
    else next.add(type);
    onFilter({ ...filter, types: next });
  };
  const narrowed = filter.types.size !== ALL_TYPES.size || filter.assetClass !== null || filter.assetId !== null;

  return (
    <section className="agenda-filters" aria-label="Filtros da agenda">
      <fieldset className="chips agenda-filters__types" aria-label="Tipos de evento">
        {FILTER_ORDER.map((type) => (
          <button
            key={type}
            type="button"
            className={cx("chip", "agenda-chip", `agenda-chip--${type}`, filter.types.has(type) && "is-active")}
            aria-pressed={filter.types.has(type)}
            onClick={() => toggle(type)}
          >
            <span className="chip__dot" aria-hidden="true" />
            {EVENT_TYPE_LABEL[type]}
            <span className="agenda-chip__count">{counts.get(type) ?? 0}</span>
          </button>
        ))}
      </fieldset>
      <div className="agenda-filters__selects">
        <div className="field">
          <label htmlFor={ids.cls} className="field__label">
            Classe
          </label>
          <select
            id={ids.cls}
            className="text-input"
            value={filter.assetClass ?? ""}
            onChange={(event) => onFilter({ ...filter, assetClass: (event.target.value || null) as AssetClass | null, assetId: null })}
          >
            <option value="">Todas</option>
            {classes.map((cls) => (
              <option key={cls} value={cls}>
                {CLASS_META[cls].label}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor={ids.asset} className="field__label">
            Ativo
          </label>
          <select
            id={ids.asset}
            className="text-input"
            value={filter.assetId ?? ""}
            onChange={(event) => onFilter({ ...filter, assetId: event.target.value || null })}
          >
            <option value="">Todos</option>
            {assets.map((asset) => (
              <option key={asset.id} value={asset.id}>
                {asset.label}
              </option>
            ))}
          </select>
        </div>
        {narrowed && (
          <button type="button" className="link-btn" onClick={() => onFilter({ types: ALL_TYPES, assetClass: null, assetId: null })}>
            Limpar filtros
          </button>
        )}
      </div>
    </section>
  );
}

function EmptyMonth({ month }: Readonly<{ month: YearMonth }>) {
  return <p className="field__hint">Nada na agenda de {monthTitle(month)} com esses filtros.</p>;
}

/**
 * Os dias do mês com evento, na ordem; os que passaram mais apagados. Hoje
 * sempre aparece quando é deste mês, mesmo sem evento: é o "você está aqui".
 */
function Timeline({ events, today, month }: Readonly<{ events: AgendaEvent[]; today: string; month: YearMonth }>) {
  const grouped = byDay(events);
  if (grouped.size === 0) return <EmptyMonth month={month} />;
  const { from, to } = monthRange(month);
  if (today >= from && today <= to && !grouped.has(today)) grouped.set(today, []);
  const days = [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b));
  return (
    <ol className="agenda-days">
      {days.map(([day, list]) => (
        <li key={day} className={cx("agenda-day", day < today && "is-past", day === today && "is-today")}>
          <h3 className="agenda-day__title">
            <time dateTime={day}>{capitalize(dayHeading(day))}</time>
            {day === today && <span className="agenda-day__today">hoje</span>}
          </h3>
          {list.length === 0 ? (
            <p className="field__hint agenda-day__empty">Nada hoje.</p>
          ) : (
            <ul className="agenda-list">
              {keyedEvents(list).map(({ key, event }) => (
                <AgendaEventItem key={key} event={event} past={day < today} />
              ))}
            </ul>
          )}
        </li>
      ))}
    </ol>
  );
}

function CalendarView({ agenda, events, month }: Readonly<{ agenda: Agenda; events: AgendaEvent[]; month: YearMonth }>) {
  const days = useMemo(() => byDay(events), [events]);
  const { from, to } = monthRange(month);
  const fallback = agenda.today >= from && agenda.today <= to ? agenda.today : ([...days.keys()][0] ?? from);
  const [picked, setPicked] = useState<string | null>(null);
  const selected = picked && picked >= from && picked <= to ? picked : fallback;
  const list = days.get(selected) ?? [];

  return (
    <Grid cols="main-aside" className="agenda-calendar-grid" alignStart>
      <Panel title="Calendário" info={<Freshness agenda={agenda} />} infoLabel="De onde vêm os documentos">
        <MonthCalendar month={month} days={days} today={agenda.today} selected={selected} onSelect={setPicked} />
      </Panel>
      <Panel title={capitalize(dayLong(selected))} className="agenda-day-panel" aria-live="polite">
        {list.length === 0 ? (
          <p className="field__hint">Nada neste dia com esses filtros.</p>
        ) : (
          <ul className="agenda-list">
            {keyedEvents(list).map(({ key, event }) => (
              <AgendaEventItem key={key} event={event} past={selected < agenda.today} />
            ))}
          </ul>
        )}
      </Panel>
    </Grid>
  );
}
