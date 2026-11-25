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
