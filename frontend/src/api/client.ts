import type {
  Asset,
  ChecklistKind,
  ChecklistQuestion,
  Contribution,
  ContributionRequest,
  CreateAssetRequest,
  GoalsRequest,
  GoalsResponse,
  Info,
  Me,
  Plan,
  Portfolio,
  Problem,
  QuotesRefreshResult,
  SymbolSuggestion,
  UpdateAssetRequest,
} from "./types";

const BASE = "/api/v1";

/** Mensagem que a pessoa lê para cada `code` do contrato (docs/api.md § Erros). */
const INTERNAL = "Algo deu errado do nosso lado. Tente de novo em instantes.";
const NETWORK = "Sem conexão com o servidor. Confira sua internet e tente de novo.";

const FRIENDLY: Partial<Record<string, string>> = {
  unauthenticated: "Sua sessão expirou. Recarregue a página para entrar de novo.",
  targets_must_sum_100: "As metas precisam somar exatamente 100%.",
  asset_already_exists: "Esse ativo já está na sua carteira.",
  not_found: "Não encontrei esse item. Ele pode ter sido removido.",
  quote_provider_unavailable: "A busca de cotações não respondeu agora. Tente de novo em instantes.",
  unreadable_file: "Não consegui ler um dos arquivos. Tire-o da lista (ou baixe de novo na Área do Investidor) e tente outra vez.",
  payload_too_large: "Os arquivos passam de 10 MB juntos. Envie menos arquivos ou um período menor.",
  exceeds_position:
    "Esse lançamento deixaria a posição negativa em alguma data. Confira a quantidade, a data e os lançamentos depois dele.",
  preview_stale: "A carteira mudou desde a prévia. Li as planilhas de novo: confira e importe outra vez.",
  import_blocked: "Ainda há conflito ou classe por confirmar. Resolva na prévia antes de importar.",
  internal: INTERNAL,
  network: NETWORK,
};

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly detail: string | undefined;

  constructor(status: number, code: string, message: string, detail?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.detail = detail;
  }
}

/** Texto amigável para qualquer erro que chegue a uma tela. */
export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  return INTERNAL;
}

function codeFromStatus(status: number): string {
  if (status === 401) return "unauthenticated";
  if (status === 404) return "not_found";
  if (status === 409) return "asset_already_exists";
  if (status === 413) return "payload_too_large";
  if (status === 422) return "validation_failed";
  if (status === 502) return "quote_provider_unavailable";
  return "internal";
}

async function toApiError(response: Response): Promise<ApiError> {
  let problem: Problem = {};
  const contentType = response.headers.get("content-type") ?? "";
  if (contentType.includes("json")) {
    try {
      problem = (await response.json()) as Problem;
    } catch {
      problem = {};
    }
  }
  const code = problem.code ?? codeFromStatus(response.status);
  // `validation_failed` diz o campo no `detail`, que é texto para gente: ele
  // vale mais que uma frase genérica.
  const fromProblem = [problem.title, problem.detail].filter(Boolean).join(" ");
  const message = FRIENDLY[code] ?? (fromProblem || INTERNAL);
  return new ApiError(response.status, code, message, problem.detail);
}

export async function request<T>(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
  const init: RequestInit = { method, headers, credentials: "same-origin" };
  if (body instanceof FormData) {
    // Upload: o navegador escreve o Content-Type com o boundary.
    init.body = body;
  } else if (body !== undefined) {
    headers["Content-Type"] = "application/json";
    init.body = JSON.stringify(body);
  }
  if (signal) init.signal = signal;

  let response: Response;
  try {
    response = await fetch(`${BASE}${path}`, init);
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "AbortError") throw cause;
    throw new ApiError(0, "network", NETWORK);
  }

  if (!response.ok) throw await toApiError(response);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  info: () => request<Info>("GET", "/info"),
  me: () => request<Me>("GET", "/me"),
  portfolio: () => request<Portfolio>("GET", "/portfolio"),
  saveGoals: (body: GoalsRequest) => request<GoalsResponse>("PUT", "/goals", body),
  createAsset: (body: CreateAssetRequest) => request<Asset>("POST", "/assets", body),
  updateAsset: (id: string, body: UpdateAssetRequest) =>
    request<Asset>("PATCH", `/assets/${encodeURIComponent(id)}`, body),
  deleteAsset: (id: string) => request<undefined>("DELETE", `/assets/${encodeURIComponent(id)}`),
  addQuestion: (checklist: ChecklistKind, prompt: string) =>
    request<ChecklistQuestion>("POST", `/checklists/${checklist}/questions`, { prompt }),
  updateQuestion: (id: string, prompt: string) =>
    request<ChecklistQuestion>("PATCH", `/checklist-questions/${encodeURIComponent(id)}`, { prompt }),
  deleteQuestion: (id: string) =>
    request<undefined>("DELETE", `/checklist-questions/${encodeURIComponent(id)}`),
  resetChecklist: (checklist: ChecklistKind) =>
    request<ChecklistQuestion[]>("POST", `/checklists/${checklist}/reset`),
  plan: (amount: number) => request<Plan>("POST", "/plans", { amount }),
  contribute: (body: ContributionRequest) => request<Contribution>("POST", "/contributions", body),
  contributions: () => request<Contribution[]>("GET", "/contributions"),
  searchSymbols: (query: string, signal?: AbortSignal) =>
    request<SymbolSuggestion[]>("GET", `/symbols?query=${encodeURIComponent(query)}`, undefined, signal),
  refreshQuotes: () => request<QuotesRefreshResult>("POST", "/quotes/refresh"),
};
