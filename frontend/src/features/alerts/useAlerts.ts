import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { alertsApi, alertsKeys } from "../../api/alerts";
import type { AlertsRequest } from "../../api/alertsTypes";

/** O estado dos alertas. `poll` (em ms) confere de tempos em tempos, enquanto a pessoa vincula o chat. */
export function useAlerts(poll: number | false = false) {
  return useQuery({
    queryKey: alertsKeys.all,
    queryFn: alertsApi.get,
    retry: false,
    refetchInterval: poll,
  });
}

export function useSaveAlerts() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: AlertsRequest) => alertsApi.save(body),
    onSuccess: (data) => client.setQueryData(alertsKeys.all, data),
  });
}

export function useLinkTelegram() {
  return useMutation({ mutationFn: alertsApi.link });
}

export function useUnlinkTelegram() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: alertsApi.unlink,
    onSuccess: () => client.invalidateQueries({ queryKey: alertsKeys.all }),
  });
}

export function useSendTest() {
  return useMutation({ mutationFn: alertsApi.test });
}
