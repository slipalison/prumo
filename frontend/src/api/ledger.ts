import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { request } from "./client";
import type {
  CreateTransactionRequest,
  Transaction,
  TransactionFilter,
  UpdateTransactionRequest,
} from "./ledgerTypes";
import { keys } from "./queries";

export const ledgerApi = {
  list: (filter: TransactionFilter = {}) => {
    const params = new URLSearchParams(
      Object.entries(filter).filter((entry): entry is [string, string] => Boolean(entry[1])),
    ).toString();
    return request<Transaction[]>("GET", `/transactions${params ? `?${params}` : ""}`);
  },
  create: (body: CreateTransactionRequest) => request<Transaction>("POST", "/transactions", body),
  update: (id: string, body: UpdateTransactionRequest) =>
    request<Transaction>("PATCH", `/transactions/${encodeURIComponent(id)}`, body),
  remove: (id: string) => request<undefined>("DELETE", `/transactions/${encodeURIComponent(id)}`),
};

export const ledgerKeys = {
  all: ["transactions"] as const,
  ofAsset: (assetId: string) => ["transactions", assetId] as const,
};

/** Um lançamento muda a quantidade, o custo e o histórico: tudo é lido de novo. */
function refresh(client: QueryClient) {
  return Promise.all([
    client.invalidateQueries({ queryKey: ledgerKeys.all }),
    client.invalidateQueries({ queryKey: keys.portfolio }),
  ]);
}

export function useTransactions(assetId: string) {
  return useQuery({ queryKey: ledgerKeys.ofAsset(assetId), queryFn: () => ledgerApi.list({ asset_id: assetId }) });
}

export function useCreateTransaction() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTransactionRequest) => ledgerApi.create(body),
    onSuccess: () => refresh(client),
  });
}

export function useUpdateTransaction() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateTransactionRequest }) => ledgerApi.update(id, body),
    onSuccess: () => refresh(client),
  });
}

export function useDeleteTransaction() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => ledgerApi.remove(id),
    onSuccess: () => refresh(client),
  });
}
