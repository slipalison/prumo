import { http, HttpResponse } from "msw";
import type { UnusualIncome } from "../api/agendaTypes";
import { todayInBrazil } from "../features/agenda/agendaText";
import { agendaOf, sampleMonth } from "../test/agendaHandlers";

/*
 * A Agenda da demonstração: o mês pedido com proventos, um corte, documentos e
 * um feriado, e dois selos de "fora do padrão" nos próximos pagamentos da
 * Renda (um corte e um aumento). Tudo FICTÍCIO.
 */

function demoUnusual(): UnusualIncome[] {
  return [
    {
      income_ids: ["e-demo-up-1"],
      installments: 1,
      basis: "similar",
      kind: "dividend",
      asset_id: "a-demo-2",
      record_date: "2026-11-27",
      ex_date: "2026-11-30",
      per_unit: 0.3,
      direction: "cut",
      recent: { usual: 0.45, change: -0.3333, payments: 4 },
      long: { usual: 0.47, change: -0.3617, payments: 19 },
      years: 5,
    },
    {
      income_ids: ["e-demo-up-4"],
      installments: 3,
      basis: "largest",
      kind: null,
      asset_id: "a-demo-0",
      record_date: "2026-12-01",
      ex_date: "2026-12-02",
      per_unit: 1.2,
      direction: "increase",
      recent: { usual: 0.7, change: 0.7143, payments: 4 },
      long: null,
      years: 0,
    },
  ];
}

export const demoAgendaHandlers = [
  http.get("/api/v1/agenda", ({ request }) => {
    const url = new URL(request.url);
    const from = url.searchParams.get("from") ?? "2026-11-01";
    const to = url.searchParams.get("to") ?? "2026-11-30";
    return HttpResponse.json(agendaOf(from, to, sampleMonth(from), todayInBrazil()));
  }),
  http.get("/api/v1/agenda/unusual-income", () => HttpResponse.json({ items: demoUnusual() })),
];
