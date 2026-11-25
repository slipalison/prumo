import { request } from "./client";
import type { Health, RulesRequest, RulesResponse } from "./healthTypes";

export const healthKeys = {
  all: ["health"] as const,
};

export const healthApi = {
  health: () => request<Health>("GET", "/health"),
  saveRules: (body: RulesRequest) => request<RulesResponse>("PUT", "/health/rules", body),
};
