// Espelho do livro de lançamentos (docs/api.md § Lançamentos).

import type { AssetClass } from "./types";

export type TransactionKind =
  | "opening"
  | "buy"
  | "sell"
  | "split"
  | "reverse_split"
  | "bonus"
  | "amortization"
  | "redemption"
  | "adjustment";

export type TransactionSource = "manual" | "plan" | "b3_import" | "migration";

export type Transaction = {
  id: string;
  /** `null` depois que o ativo foi removido: o lançamento fica de histórico. */
  asset_id: string | null;
  symbol: string | null;
  display_name: string;
  asset_class: AssetClass;
  kind: TransactionKind;
  /** `AAAA-MM-DD`, a data do negócio. */
  trade_date: string;
  /** Com sinal só no ajuste. Em renda fixa por valor, o valor em reais. */
  quantity: number;
  /** Na moeda do lançamento; `null` onde o tipo não tem preço ou o custo é desconhecido. */
  unit_price: number | null;
  fees: number;
  currency: string;
  /** Reais por unidade da moeda; `null` em reais. */
  fx_rate: number | null;
  source: TransactionSource;
  external_ref: string | null;
  contribution_id: string | null;
  created_at: string;
};

export type CreateTransactionRequest = {
  asset_id: string;
  kind: TransactionKind;
  trade_date: string;
  quantity: number;
  unit_price?: number | null;
  fees?: number;
  currency?: string;
  fx_rate?: number | null;
};

export type UpdateTransactionRequest = Partial<Omit<CreateTransactionRequest, "asset_id">>;

export type TransactionFilter = { asset_id?: string; from?: string; to?: string };
