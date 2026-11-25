import { QueryClient } from "@tanstack/react-query";
import { ApiError } from "../api/client";

/**
 * Erro de quem pediu (4xx) não melhora tentando de novo; rede e 5xx, sim, até
 * duas vezes.
 */
export function shouldRetry(failureCount: number, error: unknown): boolean {
  if (error instanceof ApiError && error.status >= 400 && error.status < 500) return false;
  return failureCount < 2;
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { staleTime: 30_000, retry: shouldRetry, refetchOnWindowFocus: true },
      mutations: { retry: false },
    },
  });
}
