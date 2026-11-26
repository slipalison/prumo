// Espelho do JSON de POST /api/v1/imports/b3/preview (docs/api.md).

import type { AssetClass } from "./types";

export type B3FileKind = "trades" | "movements" | "income_received" | "income_provisioned";
export type B3FileStatus = "read" | "unknown_format" | "unreadable";

/** Por que um arquivo não foi lido. */
export type B3FileProblem =
  | "unknown_format"
  | "missing_columns"
  | "position_sheet"
  | "not_xlsx"
  | "legacy_xls"
  | "corrupt"
  | "too_large";

export type B3FileReport = {
  index: number;
  name: string;
  status: B3FileStatus;
  kinds: B3FileKind[];
  problem: B3FileProblem | null;
  /** Texto do servidor (sem acento); a tela usa o `problem`. */
  message: string | null;
  missing_columns: string[];
  rows: number;
  accepted: number;
  unrecognized: number;
  ignored: number;
};

export type B3Trade = {
  external_ref: string;
  date: string;
  side: "buy" | "sell";
  market: "spot" | "fractional";
  code: string;
  institution: string | null;
  quantity: number;
  unit_price: number;
  total: number;
  line: number;
};

export type B3EventKind =
  | "split"
  | "reverse_split"
  | "bonus"
  | "fraction_in_kind"
  | "amortization"
  | "redemption"
  | "maturity"
  | "treasury_movement";

export type B3Event = {
  external_ref: string;
  date: string;
  kind: B3EventKind;
  direction: "credit" | "debit";
  code: string | null;
  product: string;
  quantity: number | null;
  unit_price: number | null;
  value: number | null;
  institution: string | null;
  description: string;
  treasury: boolean;
  source: B3FileKind;
  line: number;
};

export type B3IncomeKind = "dividend" | "jcp" | "yield" | "reimbursement" | "fraction_auction";

export type B3Income = {
  external_ref: string;
  kind: B3IncomeKind;
  status: "received" | "provisioned";
  date: string | null;
  code: string;
  product: string;
  quantity: number | null;
  unit_price: number | null;
  net_amount: number;
  institution: string | null;
  source: B3FileKind;
  line: number;
};

export type B3ClassBasis = "name" | "code" | "treasury";

export type B3AssetTotals = {
  bought_quantity: number;
  sold_quantity: number;
  bought_value: number;
  sold_value: number;
  net_quantity: number;
  income_received: number;
  income_provisioned: number;
};

export type B3AssetGroup = {
  key: string;
  name: string | null;
  suggested_class: AssetClass;
  class_basis: B3ClassBasis;
  listed_on_b3: boolean;
  treasury: boolean;
  totals: B3AssetTotals;
  trades: B3Trade[];
  events: B3Event[];
  income: B3Income[];
};

export type B3UnrecognizedRow = {
  file: number;
  sheet: string;
  line: number;
  reason_code: string;
  reason: string;
  text: string;
};

export type B3IgnoredSummary = { reason_code: string; reason: string; count: number };

export type B3PreviewSummary = {
  assets: number;
  trades: number;
  events: number;
  income: number;
  unrecognized: number;
  ignored: number;
  first_date: string | null;
  last_date: string | null;
};

export type B3Preview = {
  files: B3FileReport[];
  assets: B3AssetGroup[];
  unrecognized: B3UnrecognizedRow[];
  ignored: B3IgnoredSummary[];
  summary: B3PreviewSummary;
};

// --- o plano (docs/api.md § Importar da B3) ---------------------------------

/** O destino de uma linha: entra, já existe, conflita ou fica de fora. */
export type B3LineState = "new" | "existing" | "conflict" | "not_imported";

export type B3LineStatus = { status: B3LineState; reason_code: string | null };

export type B3AssetAction = "create" | "update" | "skip" | "nothing";

export type B3OpeningBasis = "covers_sales" | "keeps_quantity" | "none";

export type B3OpeningPlan = {
  date: string;
  quantity: number;
  unit_price: number | null;
  suggested_quantity: number;
  suggested_unit_price: number | null;
  basis: B3OpeningBasis;
  chosen: boolean;
};

export type B3ReplacedEntry = { kind: string; date: string; quantity: number; unit_price: number | null };

export type B3AssetConflict = { reason_code: string; detail: string };

export type B3AssetCounts = {
  new_transactions: number;
  existing_transactions: number;
  new_income: number;
  replaced_income: number;
  existing_income: number;
  not_imported: number;
};

export type B3AssetPlan = {
  key: string;
  action: B3AssetAction;
  asset_id: string | null;
  symbol: string | null;
  display_name: string;
  asset_class: AssetClass;
  class_basis: B3ClassBasis;
  listed_on_b3: boolean;
  treasury: boolean;
  class_editable: boolean;
  class_needs_confirmation: boolean;
  current_quantity: number;
  final_quantity: number | null;
  opening: B3OpeningPlan | null;
  replaced: B3ReplacedEntry[];
  conflict: B3AssetConflict | null;
  counts: B3AssetCounts;
};

export type B3PlanSummary = {
  assets_created: number;
  assets_updated: number;
  assets_skipped: number;
  transactions_new: number;
  transactions_existing: number;
  income_new: number;
  income_replaced: number;
  income_existing: number;
  openings: number;
  not_imported: number;
  conflicts: number;
  needs_confirmation: number;
};

export type B3Plan = {
  digest: string;
  ready: boolean;
  assets: B3AssetPlan[];
  lines: Record<string, B3LineStatus>;
  summary: B3PlanSummary;
};

export type B3PreviewResponse = B3Preview & { plan: B3Plan };

/** O que a pessoa decidiu para um ativo da prévia. */
export type B3AssetDecision = {
  asset_class?: AssetClass;
  skip?: boolean;
  /** Quantidade zero: sem saldo inicial. Preço nulo: custo desconhecido. */
  opening?: { quantity: number; unit_price: number | null };
};

export type B3Decisions = { assets: Record<string, B3AssetDecision> };

export type B3CommitResult = {
  summary: B3PlanSummary;
  not_imported: { key: string; external_ref: string; reason_code: string }[];
  skipped_assets: string[];
  unrecognized: number;
  ignored: number;
};
