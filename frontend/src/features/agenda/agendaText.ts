import type {
  AgendaDocument,
  AgendaEvent,
  AgendaEventType,
  DocumentCategory,
  MarketClosure,
  UnusualDirection,
  UnusualMark,
} from "../../api/agendaTypes";
import type { IncomeKind } from "../../api/incomeTypes";
import type { AssetClass } from "../../api/types";
import type { Money } from "../../lib/money";

/** O nome de cada tipo de evento, no filtro e no selo da linha. */
export const EVENT_TYPE_LABEL: Record<AgendaEventType, string> = {
  income_record: "Data com",
  income_payment: "Pagamento",
  unusual_income: "Fora do padrão",
  results: "Resultado",
  material_fact: "Fato relevante",
  notice: "Comunicados e avisos",
  market_closed: "Sem pregão",
};

/** A ordem dos filtros: do provento ao calendário. */
export const FILTER_ORDER: AgendaEventType[] = [
  "income_record",
  "income_payment",
  "unusual_income",
  "results",
  "material_fact",
  "notice",
  "market_closed",
];

const CATEGORY_LABEL: Record<DocumentCategory, string> = {
  material_fact: "Fato relevante",
  market_notice: "Comunicado ao mercado",
  shareholder_notice: "Aviso aos acionistas",
  meeting: "Assembleia",
  results: "Resultado",
  management_report: "Relatório gerencial",
};

const NOTICE_BADGE: Partial<Record<DocumentCategory, string>> = {
  market_notice: "Comunicado",
  shareholder_notice: "Aviso",
  meeting: "Assembleia",
  management_report: "Relatório",
};

/** O selo da linha: o tipo, e nos comunicados e avisos o nome curto do documento. */
export function badgeLabel(event: AgendaEvent): string {
  const short = event.type === "notice" && event.document ? NOTICE_BADGE[event.document.category] : undefined;
  return short ?? EVENT_TYPE_LABEL[event.type];
}

/** O nome do documento; no FII o aviso é "aos cotistas". */
export function categoryLabel(document: AgendaDocument): string {
  if (document.category === "shareholder_notice" && document.source === "fundos_net") return "Aviso aos cotistas";
  return CATEGORY_LABEL[document.category];
}

export const CLOSURE_LABEL: Record<MarketClosure, string> = {
  new_year: "Confraternização Universal",
  carnival: "Carnaval",
  good_friday: "Paixão de Cristo",
  tiradentes: "Tiradentes",
  labour_day: "Dia do Trabalho",
  corpus_christi: "Corpus Christi",
  independence: "Independência",
  aparecida: "Nossa Senhora Aparecida",
  all_souls: "Finados",
  republic: "Proclamação da República",
  black_consciousness: "Consciência Negra",
  christmas_eve: "Véspera de Natal",
  christmas: "Natal",
  year_end: "Último dia do ano",
};

// ------------------------------------------------------------------ datas --

/** Um mês do calendário (`month` de 1 a 12). */
export type YearMonth = { year: number; month: number };

const pad = (n: number) => String(n).padStart(2, "0");

/** `2026-11-05` a partir de ano, mês e dia (sem passar pelo fuso). */
export function isoOf(year: number, month: number, day: number): string {
  return `${year}-${pad(month)}-${pad(day)}`;
}

/** `2026-11-05` → `{ year: 2026, month: 11 }`. */
export function monthOf(iso: string): YearMonth {
  return { year: Number(iso.slice(0, 4)), month: Number(iso.slice(5, 7)) };
}

export function addMonths({ year, month }: YearMonth, delta: number): YearMonth {
  const index = year * 12 + (month - 1) + delta;
  return { year: Math.floor(index / 12), month: (index % 12) + 1 };
}

export function daysIn({ year, month }: YearMonth): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

/** O primeiro e o último dia do mês, para a consulta. */
export function monthRange(ym: YearMonth): { from: string; to: string } {
  return { from: isoOf(ym.year, ym.month, 1), to: isoOf(ym.year, ym.month, daysIn(ym)) };
}

const utc = (iso: string) => new Date(`${iso}T12:00:00Z`);

const monthTitleFormat = new Intl.DateTimeFormat("pt-BR", { month: "long", year: "numeric", timeZone: "UTC" });
const dayHeadingFormat = new Intl.DateTimeFormat("pt-BR", { weekday: "short", day: "numeric", month: "short", timeZone: "UTC" });
const dayLongFormat = new Intl.DateTimeFormat("pt-BR", { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" });

/** `novembro de 2026`. */
export function monthTitle(ym: YearMonth): string {
  return monthTitleFormat.format(utc(isoOf(ym.year, ym.month, 1)));
}

/** `qua., 2 de dez.` */
export function dayHeading(iso: string): string {
  return dayHeadingFormat.format(utc(iso));
}

/** `quarta-feira, 2 de dezembro`. */
export function dayLong(iso: string): string {
  return dayLongFormat.format(utc(iso));
}

/** `2026-11-25` → `25/11`. */
export function shortDate(iso: string): string {
  return `${iso.slice(8, 10)}/${iso.slice(5, 7)}`;
}

/** `2026-11-25` → `25/11/2026`. */
export function fullDate(iso: string): string {
  return `${iso.slice(8, 10)}/${iso.slice(5, 7)}/${iso.slice(0, 4)}`;
}

/** Domingo primeiro, como os calendários brasileiros. */
export const WEEKDAYS = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"] as const;

/**
 * As semanas do mês, de domingo a sábado: cada dia é o ISO dele, e `null` nos
 * dias de fora do mês que completam a primeira e a última semana.
 */
export function calendarWeeks(ym: YearMonth): (string | null)[][] {
  const first = new Date(Date.UTC(ym.year, ym.month - 1, 1)).getUTCDay();
  const cells: (string | null)[] = Array.from({ length: first }, () => null);
  for (let day = 1; day <= daysIn(ym); day += 1) cells.push(isoOf(ym.year, ym.month, day));
  while (cells.length % 7 !== 0) cells.push(null);
  const weeks: (string | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
  return weeks;
}

/** O dia de hoje no Brasil (UTC−3), antes de o servidor responder. */
export function todayInBrazil(now: Date = new Date()): string {
  return new Date(now.getTime() - 3 * 3_600_000).toISOString().slice(0, 10);
}

// ------------------------------------------------------------- proventos --

const percentFormat = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 0 });

/** `-0.25` → `25%` (o sentido vem no texto). */
export function changePercent(change: number): string {
  return `${percentFormat.format(Math.abs(change) * 100)}%`;
}

/** `Corte de 25%` / `Aumento de 40%`. */
export function unusualTitle(direction: UnusualDirection, change: number): string {
  return `${direction === "cut" ? "Corte" : "Aumento"} de ${changePercent(change)}`;
}

const unitFormat = new Intl.NumberFormat("pt-BR", {
  style: "currency",
  currency: "BRL",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

/** Valor por cota com até 4 casas (`R$ 0,0182`): o JCP mensal de banco tem centavos de centavo. */
export function formatUnit(value: number): string {
  return unitFormat.format(Number.isFinite(value) ? value : 0);
}

/** O valor por cota, mascarado com o olho fechado como qualquer R$. */
export function unitText(money: Money, value: number): string {
  return money.hidden ? money.brl(value) : formatUnit(value);
}

/**
 * A referência em uma frase, pela base da comparação: o habitual (a média dos
 * parecidos), o pagamento regular (a mediana do tipo) ou o maior pagamento.
 */
export function referenceText(money: Money, mark: Pick<UnusualMark, "basis" | "recent" | "long" | "years">): string {
  const recent = unitText(money, mark.recent.usual);
  const long = mark.long ? unitText(money, mark.long.usual) : null;
  if (mark.basis === "largest") {
    const head = `o maior pagamento dos 12 meses foi de ${recent}`;
    return long ? `${head}, e o de ${mark.years} anos, de ${long}` : head;
  }
  const head = mark.basis === "regular" ? `o pagamento regular é de ${recent} nos 12 meses` : `habitual de ${recent} nos 12 meses`;
  return long ? `${head} e ${long} em ${mark.years} anos` : head;
}

/** "no rendimento", "no JCP", "no dividendo"; sem tipo (o total), "no provento". */
export function kindPhrase(kind: IncomeKind | null): string {
  if (kind === "jcp") return "no JCP";
  if (kind === "dividend") return "no dividendo";
  if (kind === "reit_income") return "no rendimento";
  return "no provento";
}

/** ` em 3 parcelas` quando a data com paga em mais de uma. */
export function installmentsText(installments: number): string {
  return installments > 1 ? ` em ${installments} parcelas` : "";
}

/** "DFP de 2025" / "ITR do trimestre até 30/09/2026" / o nome do documento do FII. */
export function resultsTitle(document: AgendaDocument): string {
  const reference = document.reference_date;
  if (document.filing === "annual") return reference ? `DFP de ${reference.slice(0, 4)}` : "DFP";
  if (document.filing === "quarterly") return reference ? `ITR do trimestre até ${fullDate(reference)}` : "ITR";
  return document.title ?? "Demonstrações do fundo";
}

// --------------------------------------------------------------- filtros --

export type AgendaFilter = {
  types: ReadonlySet<AgendaEventType>;
  assetClass: AssetClass | null;
  assetId: string | null;
};

export const ALL_TYPES: ReadonlySet<AgendaEventType> = new Set(FILTER_ORDER);

/**
 * Se o evento passa no filtro. O dia sem pregão não é de nenhum ativo: só o
 * filtro de tipo o tira.
 */
export function passes(event: AgendaEvent, filter: AgendaFilter): boolean {
  if (!filter.types.has(event.type)) return false;
  if (event.type === "market_closed") return true;
  if (filter.assetClass && !event.assets.some((a) => a.asset_class === filter.assetClass)) return false;
  if (filter.assetId && !event.assets.some((a) => a.asset_id === filter.assetId)) return false;
  return true;
}

/** Os eventos por dia, na ordem do dia. */
export function byDay(events: AgendaEvent[]): Map<string, AgendaEvent[]> {
  const days = new Map<string, AgendaEvent[]>();
  for (const event of [...events].sort((a, b) => a.date.localeCompare(b.date))) {
    const list = days.get(event.date) ?? [];
    list.push(event);
    days.set(event.date, list);
  }
  return days;
}

/** O que distingue um evento dos outros do mesmo dia: o tipo, os ativos e o que ele traz. */
function eventSignature(event: AgendaEvent): string {
  const { income, unusual, document } = event;
  const parts = [
    event.type,
    event.closure,
    event.assets.map((a) => a.asset_id).join(","),
    income?.income_id,
    income?.kind,
    income?.record_date,
    income?.pay_date,
    unusual?.kind,
    unusual?.record_date,
    document?.category,
    document?.url,
    document?.title,
    document?.version,
  ];
  return parts.map((part) => part ?? "").join("|");
}

/**
 * Os eventos de uma lista com a chave do React: o conteúdo do evento e, se dois
 * saírem iguais, a ordem entre os iguais (é só o que os distingue).
 */
export function keyedEvents(events: AgendaEvent[]): { key: string; event: AgendaEvent }[] {
  const seen = new Map<string, number>();
  return events.map((event) => {
    const signature = eventSignature(event);
    const repeat = seen.get(signature) ?? 0;
    seen.set(signature, repeat + 1);
    return { key: repeat === 0 ? signature : `${signature}#${repeat}`, event };
  });
}

/** Quem o evento cita, como a linha mostra: `ITSA3, ITSA4`. */
export function assetNames(event: AgendaEvent): string {
  return event.assets.map((a) => a.symbol ?? a.display_name).join(", ");
}
