import { request } from "./client";
import type {
  CreateIncomeRequest,
  Entitlement,
  IncomeEvent,
  IncomeFilters,
  IncomeGoal,
  IncomeSummary,
  YieldReport,
} from "./incomeTypes";

function queryString(filters: IncomeFilters): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filters)) {
    if (value !== undefined) params.set(key, String(value));
  }
  const text = params.toString();
  return text ? `?${text}` : "";
}

export const incomeApi = {
  events: (filters: IncomeFilters = {}) => request<IncomeEvent[]>("GET", `/income${queryString(filters)}`),
  createEvent: (body: CreateIncomeRequest) => request<IncomeEvent>("POST", "/income", body),
  deleteEvent: (id: string) => request<undefined>("DELETE", `/income/${encodeURIComponent(id)}`),
  summary: () => request<IncomeSummary>("GET", "/income/summary"),
  yieldOnCost: () => request<YieldReport>("GET", "/income/yield-on-cost"),
  entitlement: (asset: string, exDate: string, payDate: string) => {
    const params = new URLSearchParams({ asset });
    if (exDate) params.set("ex_date", exDate);
    if (payDate) params.set("pay_date", payDate);
    return request<Entitlement>("GET", `/income/entitlement?${params.toString()}`);
  },
  saveGoal: (target: number) => request<IncomeGoal>("PUT", "/income/goal", { target }),
  deleteGoal: () => request<undefined>("DELETE", "/income/goal"),
};
