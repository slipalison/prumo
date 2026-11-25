import { useQuery } from "@tanstack/react-query";
import { cashApi } from "./cash";

export const cashKeys = {
  all: ["cash"] as const,
};

/** O caixa da pessoa, para o campo do aporte oferecer o saldo. */
export function useCash() {
  return useQuery({ queryKey: cashKeys.all, queryFn: cashApi.cash, retry: false });
}
