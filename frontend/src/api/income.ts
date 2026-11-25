import { request } from "./client";
import type {
  CashSuggestion,
  CreateIncomeRequest,
  IncomeEvent,
  IncomeFilters,
  IncomeGoal,
  IncomeSummary,
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
  cashSuggestion: () => request<CashSuggestion>("GET", "/income/cash-suggestion"),
  saveGoal: (target: number) => request<IncomeGoal>("PUT", "/income/goal", { target }),
  deleteGoal: () => request<undefined>("DELETE", "/income/goal"),
};
