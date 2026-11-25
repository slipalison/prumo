import { http, HttpResponse } from "msw";
import type { Alerts, AlertsRequest, AlertKind } from "../api/alertsTypes";

/** Alertas FICTÍCIOS: bot configurado, nada vinculado, os tipos de fábrica. */
export function sampleAlerts(overrides: Partial<Alerts> = {}): Alerts {
  return {
    telegram: { configured: true, bot_username: "prumo_exemplo_bot", linked: false, linked_at: null },
    send_at: "08:00",
    hide_values: false,
    types: [
      { kind: "income_tomorrow", enabled: true, available: true, threshold: null, default_threshold: null, unit: null },
      { kind: "class_drift", enabled: true, available: true, threshold: 5, default_threshold: 5, unit: "percentage_points" },
      { kind: "price_below_ceiling", enabled: true, available: true, threshold: null, default_threshold: null, unit: null },
      { kind: "objective_achieved", enabled: true, available: true, threshold: null, default_threshold: null, unit: null },
      { kind: "tax_exemption", enabled: false, available: false, threshold: 80, default_threshold: 80, unit: "percent_of_limit" },
      { kind: "health_warning", enabled: true, available: true, threshold: null, default_threshold: null, unit: null },
    ],
    ...overrides,
  };
}

export const linkedAlerts = (): Alerts =>
  sampleAlerts({
    telegram: { configured: true, bot_username: "prumo_exemplo_bot", linked: true, linked_at: "2026-11-20T12:00:00Z" },
  });

export const problem = (status: number, code: string, title: string, detail: string) =>
  HttpResponse.json(
    { type: `urn:prumo:error:${code}`, title, status, code, detail },
    { status, headers: { "Content-Type": "application/problem+json" } },
  );

export type FakeAlerts = {
  alerts: Alerts;
  /** O que a tela mandou a `PUT /alerts`. */
  saved: AlertsRequest[];
  links: number;
  tests: number;
  unlinks: number;
  /** `true`: o envio de teste volta 502, como o Telegram fora do ar. */
  failTest: boolean;
};

type Change = { enabled?: boolean; threshold?: number };

/**
 * `/alerts` com estado, como o servidor responde (docs/api.md § Alertas). Quem
 * testa vincula "do lado do Telegram" trocando `state.alerts.telegram.linked`.
 */
export function alertsHandlers(initial: Alerts = sampleAlerts()) {
  const state: FakeAlerts = { alerts: initial, saved: [], links: 0, tests: 0, unlinks: 0, failTest: false };
  const handlers = [
    http.get("/api/v1/alerts", () => HttpResponse.json(state.alerts)),
    http.put("/api/v1/alerts", async ({ request }) => {
      const body = (await request.json()) as AlertsRequest;
      const changes = Object.entries(body.types ?? {}) as Array<[AlertKind, Change]>;
      for (const [kind, change] of changes) {
        if (change.threshold !== undefined && (change.threshold <= 0 || change.threshold > 100)) {
          return problem(422, "validation_failed", "Dados inválidos", `${kind}: o limite precisa ser maior que 0 e até 100.`);
        }
      }
      state.saved.push(body);
      if (body.send_at !== undefined) state.alerts.send_at = body.send_at;
      if (body.hide_values !== undefined) state.alerts.hide_values = body.hide_values;
      for (const [kind, change] of changes) {
        const type = state.alerts.types.find((t) => t.kind === kind);
        if (!type) continue;
        if (change.enabled !== undefined) type.enabled = change.enabled;
        if (change.threshold !== undefined) type.threshold = change.threshold;
      }
      return HttpResponse.json(state.alerts);
    }),
    http.post("/api/v1/alerts/telegram", () => {
      state.links += 1;
      return HttpResponse.json(
        {
          code: "K7M2QX9PTA",
          command: "/start K7M2QX9PTA",
          bot_username: state.alerts.telegram.bot_username,
          expires_at: "2026-11-25T15:10:00Z",
        },
        { status: 201 },
      );
    }),
    http.delete("/api/v1/alerts/telegram", () => {
      state.unlinks += 1;
      state.alerts.telegram.linked = false;
      state.alerts.telegram.linked_at = null;
      return new HttpResponse(null, { status: 204 });
    }),
    http.post("/api/v1/alerts/test", () => {
      if (state.failTest) {
        return problem(502, "telegram_unavailable", "Telegram indisponivel", "O Telegram nao aceitou a mensagem agora.");
      }
      state.tests += 1;
      return HttpResponse.json({ sent: true });
    }),
  ];
  return { state, handlers };
}
