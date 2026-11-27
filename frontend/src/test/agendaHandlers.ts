import { http, HttpResponse } from "msw";
import type {
  Agenda,
  AgendaAsset,
  AgendaEvent,
  AgendaIncome,
  UnusualIncome,
  UnusualIncomeList,
} from "../api/agendaTypes";

/*
 * O servidor falso da Agenda. Os eventos saem do mês pedido (o dia 5, o 10...),
 * para o teste e a demonstração não dependerem da data de hoje. Tudo FICTÍCIO.
 */

export const KNRI: AgendaAsset = { asset_id: "a-knri", symbol: "KNRI11", display_name: "Kinea Renda Imobiliária", asset_class: "br_reits" };
export const MXRF: AgendaAsset = { asset_id: "a-mxrf", symbol: "MXRF11", display_name: "Maxi Renda", asset_class: "br_reits" };
export const ITSA: AgendaAsset = { asset_id: "a-itsa", symbol: "ITSA4", display_name: "Itaúsa", asset_class: "br_stocks" };

const day = (from: string, n: number) => `${from.slice(0, 8)}${String(n).padStart(2, "0")}`;

export function income(fields: Partial<AgendaIncome> = {}): AgendaIncome {
  return {
    income_id: "inc-knri",
    kind: "reit_income",
    record_date: null,
    ex_date: null,
    pay_date: null,
    amount_per_unit: 1.1,
    quantity: 10,
    gross: 11,
    net: 11,
    state: "upcoming",
    source: "b3_announcement",
    unusual: null,
    ...fields,
  };
}

const empty = { income: null, unusual: null, document: null, closure: null };

/** Um mês fictício da carteira: proventos, um corte, documentos e um feriado. */
export function sampleMonth(from: string): AgendaEvent[] {
  const cut = {
    direction: "cut" as const,
    recent: { usual: 0.1, change: -0.25, payments: 12 },
    long: { usual: 0.098, change: -0.2347, payments: 58 },
    years: 5,
  };
  return [
    { ...empty, date: day(from, 2), type: "market_closed", assets: [], closure: "all_souls" },
    {
      ...empty,
      date: day(from, 4),
      type: "unusual_income",
      assets: [MXRF],
      unusual: {
        direction: "cut",
        per_unit: 0.075,
        record_date: day(from, 10),
        ex_date: day(from, 11),
        approved_on: day(from, 4),
        pay_dates: [day(from, 24)],
        recent: cut.recent,
        long: cut.long,
        years: 5,
      },
    },
    {
      ...empty,
      date: day(from, 6),
      type: "material_fact",
      assets: [ITSA],
      document: {
        category: "material_fact",
        source: "cvm_ipe",
        title: null,
        subject: "Aquisição de participação (fictício)",
        reference_date: day(from, 6),
        version: 1,
        url: "https://www.rad.cvm.gov.br/ENET/frmDownloadDocumento.aspx?numSequencia=1",
        filing: null,
      },
    },
    {
      ...empty,
      date: day(from, 10),
      type: "income_record",
      assets: [MXRF],
      income: income({
        income_id: "inc-mxrf",
        record_date: day(from, 10),
        ex_date: day(from, 11),
        pay_date: day(from, 24),
        amount_per_unit: 0.075,
        quantity: 100,
        gross: 7.5,
        net: 7.5,
        unusual: cut,
      }),
    },
    {
      ...empty,
      date: day(from, 12),
      type: "results",
      assets: [ITSA],
      document: {
        category: "results",
        source: "cvm_filing",
        title: null,
        subject: null,
        reference_date: "2026-09-30",
        version: 1,
        url: "https://www.rad.cvm.gov.br/ENETCONSULTA/frmDownloadDocumento.aspx?NumeroSequencialDocumento=1",
        filing: "quarterly",
      },
    },
    {
      ...empty,
      date: day(from, 14),
      type: "income_payment",
      assets: [KNRI],
      income: income({ record_date: day(from, 1), ex_date: day(from, 2), pay_date: day(from, 14), state: "received" }),
    },
    {
      ...empty,
      date: day(from, 18),
      type: "notice",
      assets: [KNRI],
      document: {
        category: "meeting",
        source: "fundos_net",
        title: "AGO · Edital de Convocação",
        subject: null,
        reference_date: null,
        version: 1,
        url: "https://fnet.bmfbovespa.com.br/fnet/publico/exibirDocumento?id=1",
        filing: null,
      },
    },
    {
      ...empty,
      date: day(from, 20),
      type: "income_record",
      assets: [ITSA],
      income: income({
        income_id: null,
        kind: "jcp",
        record_date: day(from, 20),
        ex_date: day(from, 21),
        pay_date: null,
        amount_per_unit: 0.02,
        quantity: 2500,
        gross: 50,
        net: 41.25,
        state: "undated",
      }),
    },
    {
      ...empty,
      date: day(from, 24),
      type: "income_payment",
      assets: [MXRF],
      income: income({
        income_id: "inc-mxrf",
        record_date: day(from, 10),
        ex_date: day(from, 11),
        pay_date: day(from, 24),
        amount_per_unit: 0.075,
        quantity: 100,
        gross: 7.5,
        net: 7.5,
        unusual: cut,
      }),
    },
  ];
}

type Build = (from: string, to: string) => AgendaEvent[];

let build: Build = () => [];
let today: string | null = null;
let unusual: UnusualIncome[] = [];
let failing = false;

/** O que `GET /agenda` devolve (cada teste começa vazio). */
export function setAgenda(next: Build, todayIs: string | null = null) {
  build = next;
  today = todayIs;
}

export function setUnusualIncome(next: UnusualIncome[]) {
  unusual = next;
}

export function failAgenda(fail: boolean) {
  failing = fail;
}

export function resetAgenda() {
  build = () => [];
  today = null;
  unusual = [];
  failing = false;
}

export function agendaOf(from: string, to: string, events: AgendaEvent[], todayIs: string): Agenda {
  return {
    today: todayIs,
    from,
    to,
    calendar_until: 2027,
    updated: { cvm_documents: "2026-11-22T10:00:00Z", fund_documents: "2026-11-25T09:00:00Z" },
    events,
  };
}

export function agendaHandlersWith(source: () => { build: Build; today: string | null }) {
  return [
    http.get("/api/v1/agenda", ({ request }) => {
      if (failing) return HttpResponse.json({ code: "internal", title: "falha" }, { status: 500 });
      const url = new URL(request.url);
      const from = url.searchParams.get("from") ?? "2026-11-01";
      const to = url.searchParams.get("to") ?? "2026-11-30";
      const current = source();
      return HttpResponse.json(agendaOf(from, to, current.build(from, to), current.today ?? day(from, 15)));
    }),
    http.get("/api/v1/agenda/unusual-income", () => HttpResponse.json({ items: unusual } satisfies UnusualIncomeList)),
  ];
}

export const agendaHandlers = agendaHandlersWith(() => ({ build, today }));
