import { request } from "./client";
import type { Alerts, AlertsRequest, LinkCode } from "./alertsTypes";

export const alertsKeys = {
  all: ["alerts"] as const,
};

export const alertsApi = {
  get: () => request<Alerts>("GET", "/alerts"),
  save: (body: AlertsRequest) => request<Alerts>("PUT", "/alerts", body),
  link: () => request<LinkCode>("POST", "/alerts/telegram"),
  unlink: () => request<undefined>("DELETE", "/alerts/telegram"),
  test: () => request<{ sent: boolean }>("POST", "/alerts/test"),
};
