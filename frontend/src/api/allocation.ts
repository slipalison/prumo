import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { AllocationSettings } from "./allocationTypes";
import { request } from "./client";

export const allocationKeys = {
  settings: ["allocation-settings"] as const,
};

export const allocationApi = {
  settings: () => request<AllocationSettings>("GET", "/settings/allocation"),
  saveSettings: (body: AllocationSettings) => request<AllocationSettings>("PUT", "/settings/allocation", body),
};

export function useAllocationSettings() {
  return useQuery({ queryKey: allocationKeys.settings, queryFn: allocationApi.settings });
}

/** Grava o limite; a tela passa a mostrar o valor salvo sem buscar de novo. */
export function useSaveAllocationSettings() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: allocationApi.saveSettings,
    onSuccess: (saved) => client.setQueryData(allocationKeys.settings, saved),
  });
}
