import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { taxApi, taxKeys } from "../../api/tax";
import type { CarryforwardRequest, DarfCode } from "../../api/taxTypes";

/** Os meses do ano. O livro muda a cada lançamento: nunca fica "fresco". */
export function useTaxMonths(year: number | null) {
  return useQuery({
    queryKey: taxKeys.months(year ?? 0),
    queryFn: () => taxApi.months(year ?? 0),
    enabled: year !== null,
    staleTime: 0,
    retry: false,
  });
}

export function useIrpf(year: number | null) {
  return useQuery({
    queryKey: taxKeys.irpf(year ?? 0),
    queryFn: () => taxApi.irpf(year ?? 0),
    enabled: year !== null,
    staleTime: 0,
    retry: false,
  });
}

/** A tabela de regras do ano, com as fontes: muda só com versão nova do servidor. */
export function useTaxRules(year: number | null) {
  return useQuery({
    queryKey: taxKeys.rules(year ?? 0),
    queryFn: () => taxApi.rules(year ?? 0),
    enabled: year !== null,
    staleTime: Infinity,
    retry: false,
  });
}

type LateArgs = { year: number; month: number; code: DarfCode; payOn: string };

/** A multa e os juros do DARF se pago em `payOn` (só com a folha aberta). */
export function useLateCharge(args: LateArgs | null) {
  return useQuery({
    queryKey: args ? taxKeys.late(args.year, args.month, args.code, args.payOn) : ["tax", "late", "none"],
    queryFn: () => (args ? taxApi.late(args.year, args.month, args.code, args.payOn) : Promise.reject(new Error("sem DARF"))),
    enabled: args !== null && /^\d{4}-\d{2}-\d{2}$/.test(args.payOn),
    staleTime: 0,
    retry: false,
  });
}

type WithholdingChange = { year: number; month: number; amount: number | null };

/** Grava o IR retido do mês (da nota) ou, com `amount: null`, volta para a regra. */
export function useInformedWithholding() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ year, month, amount }: WithholdingChange) =>
      amount === null
        ? taxApi.clearWithholding(year, month)
        : taxApi.saveWithholding(year, month, amount).then(() => undefined),
    // O retido muda o DARF do mês, a sobra dos meses seguintes e a declaração.
    onSuccess: () => client.invalidateQueries({ queryKey: taxKeys.all }),
  });
}

export function useCarryforward() {
  return useQuery({ queryKey: taxKeys.carryforward, queryFn: taxApi.carryforward, staleTime: 0, retry: false });
}

type PaidChange = { year: number; month: number; code: DarfCode; paid: boolean };

/** Marca ou desmarca um DARF como pago. */
export function useDarfPaid() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ year, month, code, paid }: PaidChange) =>
      paid ? taxApi.markPaid(year, month, code).then(() => undefined) : taxApi.unmarkPaid(year, month, code),
    onSuccess: (_, { year }) => client.invalidateQueries({ queryKey: taxKeys.months(year) }),
  });
}

/** O saldo de antes do Prumo muda a conta de todos os anos. */
export function useSaveCarryforward() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: CarryforwardRequest) => taxApi.saveCarryforward(body),
    onSuccess: () => client.invalidateQueries({ queryKey: taxKeys.all }),
  });
}
