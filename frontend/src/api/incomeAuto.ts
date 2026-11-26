import { request } from "./client";
import type { IncomeAuto } from "./incomeAutoTypes";

export const incomeAutoKeys = {
  all: ["income-auto"] as const,
};

export const incomeAutoApi = {
  get: () => request<IncomeAuto>("GET", "/income/auto"),
  save: (enabled: boolean) => request<IncomeAuto>("PUT", "/income/auto", { enabled }),
};
