import type {
  B3AssetAction,
  B3ClassBasis,
  B3EventKind,
  B3FileKind,
  B3FileProblem,
  B3IncomeKind,
  B3LineState,
  B3OpeningBasis,
} from "../../api/b3ImportTypes";

export const FILE_KIND_LABEL: Record<B3FileKind, string> = {
  trades: "Negociação",
  movements: "Movimentação",
  income_received: "Proventos recebidos",
  income_provisioned: "Proventos a receber",
};

export const EVENT_KIND_LABEL: Record<B3EventKind, string> = {
  split: "Desdobramento",
  reverse_split: "Grupamento",
  bonus: "Bonificação",
  fraction_in_kind: "Fração em ativos",
  amortization: "Amortização",
  redemption: "Resgate",
  maturity: "Vencimento",
  treasury_movement: "Tesouro Direto",
};

export const INCOME_KIND_LABEL: Record<B3IncomeKind, string> = {
  dividend: "Dividendo",
  jcp: "JCP (líquido)",
  yield: "Rendimento",
  reimbursement: "Reembolso",
  fraction_auction: "Leilão de frações",
};

export const CLASS_BASIS_HINT: Record<B3ClassBasis, string> = {
  name: "pelo nome do produto",
  code: "só pelo código — confira",
  treasury: "Tesouro Direto: o preço vem da frente do Tesouro",
};

/** O que a pessoa lê quando o arquivo não foi lido. */
export const FILE_PROBLEM_TEXT: Record<B3FileProblem, string> = {
  unknown_format:
    "Não reconheci este formato. Procurei os cabeçalhos de Negociação, Movimentação e Proventos da Área do Investidor. Envie a planilha como a B3 exporta, sem editar.",
  missing_columns: "Parece uma planilha da B3, mas faltam colunas. A B3 pode ter mudado o layout.",
  position_sheet: "Esta é a planilha de Posição, que o Prumo ainda não lê. Envie Negociação, Movimentação ou Proventos.",
  not_xlsx: "Este arquivo não é uma planilha .xlsx. Envie o arquivo que a Área do Investidor exporta, sem converter.",
  legacy_xls: "Este é o formato antigo do Excel (.xls). Baixe a planilha em Excel (.xlsx).",
  corrupt: "Não consegui abrir: a planilha parece corrompida ou cortada. Baixe de novo na Área do Investidor.",
  too_large: "A planilha é grande demais para ler. Baixe um período menor.",
};

/** Por que uma linha ficou de fora, pelo `reason_code` do servidor. */
export const REASON_TEXT: Record<string, string> = {
  invalid_date: "data ilegível",
  invalid_number: "quantidade, preço ou valor ilegível ou zerado",
  unknown_side: "tipo de movimentação diferente de Compra e Venda",
  unsupported_market: "mercado que o Prumo ainda não suporta (opções, termo, futuro)",
  invalid_code: "código de negociação ilegível",
  unknown_direction: "entrada/saída diferente de Crédito e Débito",
  unknown_movement: "movimentação que o Prumo ainda não entende",
  update: "atualização: pode ter mudado a quantidade — confira e lance à mão",
  subscription: "subscrição ou cessão de direitos: ainda não suportado",
  no_code: "produto sem código de negociação (renda fixa privada, opção ou futuro)",
  missing_value: "linha sem o valor",
  missing_quantity: "linha sem a quantidade",
  unknown_income_type: "tipo de evento que o Prumo ainda não entende",
  income_debit: "provento debitado, não recebido",
  settlement: "compra ou venda liquidada: vem da planilha de Negociação",
  custody_transfer: "transferência de custódia entre corretoras",
  transferred: "repete outro lançamento da planilha",
  lending: "empréstimo de ativos (aluguel)",
  duplicate_in_request: "já veio em outra planilha enviada",
};

export function reasonText(code: string, fallback: string): string {
  return REASON_TEXT[code] ?? fallback;
}

/** `2026-03-05` → `05/03/2026`. */
export function formatIsoDate(iso: string | null): string {
  if (!iso) return "sem data";
  const [year, month, day] = iso.split("-");
  return year && month && day ? `${day}/${month}/${year}` : iso;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 0 })} KB`;
  return `${(bytes / (1024 * 1024)).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} MB`;
}

/** O destino de cada linha, como a tela o diz. */
export const LINE_STATE_LABEL: Record<B3LineState, string> = {
  new: "entra",
  existing: "já existe",
  conflict: "conflita",
  not_imported: "fica de fora",
};

/** Por que a linha já existe, conflita ou fica de fora (`plan.lines[].reason_code`). */
export const LINE_REASON_TEXT: Record<string, string> = {
  same_ref: "já importada antes",
  same_entry: "é o lançamento que você já fez (mesma data, quantidade e preço próximo)",
  same_income: "é o provento que já está na Renda",
  replaces_provisioned: "substitui o provento a receber",
  superseded_by_received: "o recebido do mesmo provento já entra",
  provisioned_without_date: "a receber sem data: entra quando a B3 marcar a data",
  asset_skipped: "você deixou o ativo de fora",
  treasury_unidentified: "não achei o título do Tesouro (tipo e vencimento)",
  treasury_interest: "juros do título: lance como provento à mão, se quiser",
  fraction_in_kind: "fração de ação: a sobra vai a leilão e o dinheiro entra como provento",
  event_debit: "evento como débito: confira e lance à mão",
  reverse_split_unclear: "grupamento sem quantidade clara: confira e lance à mão",
  missing_quantity: "linha sem a quantidade",
  missing_value: "linha sem o preço e sem o valor",
  exceeds_position: "passa da posição naquela data",
  unit: "quantidade que esse ativo não aceita",
  invalid: "o livro recusou este lançamento",
};

export function lineReason(code: string | null): string | null {
  return code ? (LINE_REASON_TEXT[code] ?? null) : null;
}

/** O que acontece com o ativo. */
export const ACTION_LABEL: Record<B3AssetAction, string> = {
  create: "entra na carteira",
  update: "já está na carteira",
  skip: "deixado de fora",
  nothing: "nada novo",
};

/** Por que o ativo trava a importação (`plan.assets[].conflict.reason_code`). */
export const CONFLICT_TEXT: Record<string, string> = {
  exceeds_position: "Uma venda passa da posição com esse saldo inicial. Aumente o saldo inicial ou deixe o ativo de fora.",
  invalid_opening: "Esse saldo inicial não vale para o ativo: confira a quantidade (ação e cota são inteiras; título, centésimos).",
  unit: "Uma linha tem quantidade que esse ativo não aceita. Deixe o ativo de fora e lance à mão.",
  invalid: "O livro recusou uma linha deste ativo. Deixe o ativo de fora e lance à mão.",
};

/** De onde veio o saldo inicial sugerido. */
export const OPENING_BASIS_TEXT: Record<B3OpeningBasis, string> = {
  covers_sales: "A B3 mostra vendas antes de compras: o histórico dela começa em ~2019. Este saldo cobre o que você já tinha.",
  keeps_quantity: "Mantém a quantidade que você tinha lançado; o que a B3 explica sai do saldo inicial.",
  none: "O histórico da B3 explica toda a posição.",
};

export const REPLACED_KIND_LABEL: Record<string, string> = {
  opening: "Saldo inicial",
  adjustment: "Ajuste de quantidade",
};
