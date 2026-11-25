import { request } from "./client";
import type { Carryforward, CarryforwardRequest, Darf, DarfCode, Irpf, TaxMonths } from "./taxTypes";

export const taxKeys = {
  all: ["tax"] as const,
  months: (year: number) => ["tax", "months", year] as const,
  irpf: (year: number) => ["tax", "irpf", year] as const,
  carryforward: ["tax", "carryforward"] as const,
};

export const taxApi = {
  months: (year: number) => request<TaxMonths>("GET", `/tax/months?year=${year}`),
  irpf: (year: number) => request<Irpf>("GET", `/tax/irpf/${year}`),
  markPaid: (year: number, month: number, code: DarfCode) =>
    request<Darf>("POST", `/tax/darfs/${year}/${month}/paid`, { code }),
  unmarkPaid: (year: number, month: number, code: DarfCode) =>
    request<undefined>("DELETE", `/tax/darfs/${year}/${month}/paid?code=${code}`),
  carryforward: () => request<Carryforward>("GET", "/tax/carryforward"),
  saveCarryforward: (body: CarryforwardRequest) => request<Carryforward>("PUT", "/tax/carryforward", body),
};
