import { request } from "./client";
import type {
  Carryforward,
  CarryforwardRequest,
  Darf,
  DarfCode,
  InformedWithholding,
  Irpf,
  LateCharge,
  TaxMonths,
  TaxRules,
} from "./taxTypes";

export const taxKeys = {
  all: ["tax"] as const,
  months: (year: number) => ["tax", "months", year] as const,
  irpf: (year: number) => ["tax", "irpf", year] as const,
  rules: (year: number) => ["tax", "rules", year] as const,
  late: (year: number, month: number, code: DarfCode, payOn: string) => ["tax", "late", year, month, code, payOn] as const,
  carryforward: ["tax", "carryforward"] as const,
};

export const taxApi = {
  months: (year: number) => request<TaxMonths>("GET", `/tax/months?year=${year}`),
  irpf: (year: number) => request<Irpf>("GET", `/tax/irpf/${year}`),
  rules: (year: number) => request<TaxRules>("GET", `/tax/rules/${year}`),
  late: (year: number, month: number, code: DarfCode, payOn: string) =>
    request<LateCharge>("GET", `/tax/darfs/${year}/${month}/late?code=${code}&pay_on=${payOn}`),
  markPaid: (year: number, month: number, code: DarfCode) =>
    request<Darf>("POST", `/tax/darfs/${year}/${month}/paid`, { code }),
  unmarkPaid: (year: number, month: number, code: DarfCode) =>
    request<undefined>("DELETE", `/tax/darfs/${year}/${month}/paid?code=${code}`),
  saveWithholding: (year: number, month: number, amount: number) =>
    request<InformedWithholding>("PUT", `/tax/withholding/${year}/${month}`, { amount }),
  clearWithholding: (year: number, month: number) =>
    request<undefined>("DELETE", `/tax/withholding/${year}/${month}`),
  carryforward: () => request<Carryforward>("GET", "/tax/carryforward"),
  saveCarryforward: (body: CarryforwardRequest) => request<Carryforward>("PUT", "/tax/carryforward", body),
};
