import type { Asset } from "../api/types";
import type { IncomeKind, IncomeMonth, IncomePayer, IncomeStatus, YieldOnCost } from "../api/incomeTypes";

/** IR retido na fonte sobre o JCP pago até 31/12/2025 — a mesma regra do servidor. */
export const JCP_WITHHOLDING_RATE = 0.15;
/** Desde 1/1/2026 (LC 224/2025): 17,5%. */
export const JCP_WITHHOLDING_RATE_2026 = 0.175;

/** A alíquota do JCP pela data de pagamento (`2026-03-02`). */
export function jcpWithholdingRate(payDate: string): number {
  return payDate >= "2026-01-01" ? JCP_WITHHOLDING_RATE_2026 : JCP_WITHHOLDING_RATE;
}

/** IR retido nos EUA sobre o dividendo pago a residente no Brasil — a mesma regra do servidor. */
export const US_DIVIDEND_WITHHOLDING_RATE = 0.3;

/**
 * Ação ou REIT cotado nos EUA (em dólar): o BDR e o ativo da B3 ficam de fora,
 * porque o valor que a B3 informa já é o líquido da depositária.
 */
export function isUsListed(asset: Pick<Asset, "asset_class" | "listed_on_b3" | "treasury">): boolean {
  return !asset.listed_on_b3 && !asset.treasury && (asset.asset_class === "intl_stocks" || asset.asset_class === "us_reits");
}

export const KIND_LABEL: Record<IncomeKind, string> = {
  dividend: "Dividendo",
  jcp: "JCP",
  reit_income: "Rendimento",
  amortization: "Amortização",
  other: "Outro",
};

export const STATUS_LABEL: Record<IncomeStatus, string> = {
  received: "Recebido",
  upcoming: "A receber",
};

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"] as const;

/** `2026-11` → `nov`. */
export function monthShort(month: string): string {
  const number = Number(month.slice(5, 7));
  return MONTHS[number - 1] ?? month;
}

/** `2026-11` → `nov/26`. */
export function monthLabel(month: string): string {
  return `${monthShort(month)}/${month.slice(2, 4)}`;
}

/** `2026-11-25` → `25/11/2026`, sem passar por `Date` (que deslocaria o dia pelo fuso). */
export function formatIsoDate(iso: string): string {
  const [year, month, day] = iso.split("-");
  return year && month && day ? `${day}/${month}/${year}` : iso;
}

const dayFormat = new Intl.DateTimeFormat("pt-BR", { dateStyle: "medium" });

/** Um instante (`2026-11-25T13:00:00Z`) como dia, no fuso de quem olha: `25 de nov. de 2026`. */
export function formatDay(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : dayFormat.format(date);
}

/** `2026-11-25` → `25/11`. */
export function formatIsoDay(iso: string): string {
  return formatIsoDate(iso).slice(0, 5);
}

/** Os meses de janeiro a dezembro, para o cabeçalho da tabela ano × mês. */
export const MONTH_HEADERS = MONTHS;

const cents = (value: number) => Math.round(value * 100) / 100;

/** O IR retido que a regra manda: JCP pela data, dividendo e rendimento de ativo dos EUA a 30%, o resto zero. */
export function defaultWithholding(kind: IncomeKind, gross: number, payDate: string, usListed: boolean): number {
  if (kind === "jcp") return cents(gross * jcpWithholdingRate(payDate));
  if (usListed && (kind === "dividend" || kind === "reit_income")) return cents(gross * US_DIVIDEND_WITHHOLDING_RATE);
  return 0;
}

export type Amounts = { gross: number; withholding: number; net: number };

/**
 * Bruto, imposto retido e líquido de um provento — a prévia do formulário. O
 * servidor refaz a conta e é quem vale.
 */
export function previewAmounts(
  kind: IncomeKind,
  perUnit: number,
  quantity: number,
  payDate: string,
  withholding?: number,
  usListed = false,
): Amounts {
  const gross = cents(perUnit * quantity);
  const retained = withholding ?? defaultWithholding(kind, gross, payDate, usListed);
  return { gross, withholding: retained, net: cents(gross - retained) };
}

export type PayerSlice = {
  key: string;
  label: string;
  total: number;
  percent: number;
  color: string;
  /** O yield on cost do pagador; `null` no grupo "Outros", que junta vários. */
  yield: YieldOnCost | null;
  /** No grupo "Outros": quem está lá dentro, do maior para o menor; nos demais, vazio. */
  members: PayerSlice[];
};

/** A chave do grupo "Outros" (nenhum pagador a usa: a de um pagador é o id do ativo ou o nome). */
export const OTHERS_KEY = "others";

/** Seis tons que a interface já usa, sempre ao lado do nome (o sétimo é o de "Outros"). */
export const PAYER_COLORS = [
  "var(--c-br_reits)",
  "var(--c-br_stocks)",
  "var(--c-intl_stocks)",
  "var(--c-us_reits)",
  "var(--c-crypto)",
  "var(--c-fixed_income)",
] as const;
export const OTHERS_COLOR = "var(--c-intl_fixed_income)";
const SHOWN_PAYERS = PAYER_COLORS.length;

/** A chave de um pagador: o id do ativo, ou o nome de quem não é ativo. */
export const payerKey = (payer: Pick<IncomePayer, "asset_id" | "label">) => payer.asset_id ?? payer.label;

const sliceOf = (p: IncomePayer, color: string): PayerSlice => ({
  key: payerKey(p),
  label: p.label,
  total: p.total,
  percent: p.percent,
  color,
  yield: p.yield_on_cost,
  members: [],
});

/**
 * Os maiores pagadores um a um; o resto junto, em "Outros". Quem está em
 * "Outros" vem em `members`, com as mesmas cores em ciclo: dentro do grupo o
 * nome sempre vai ao lado (a cor sozinha não distingue 19 fatias).
 */
export function payerSlices(payers: IncomePayer[]): PayerSlice[] {
  const head = payers.slice(0, SHOWN_PAYERS).map((p, index) => sliceOf(p, PAYER_COLORS[index] ?? OTHERS_COLOR));
  const tail = payers.slice(SHOWN_PAYERS);
  if (tail.length === 0) return head;
  return [
    ...head,
    {
      key: OTHERS_KEY,
      label: `Outros (${tail.length})`,
      total: cents(tail.reduce((acc, p) => acc + p.total, 0)),
      percent: cents(tail.reduce((acc, p) => acc + p.percent, 0)),
      color: OTHERS_COLOR,
      yield: null,
      members: tail.map((p, index) => sliceOf(p, PAYER_COLORS[index % PAYER_COLORS.length] ?? OTHERS_COLOR)),
    },
  ];
}

/** Um teto "redondo" para o eixo: 1, 2, 2,5, 5 ou 10 vezes uma potência de dez. */
export function niceCeiling(value: number): number {
  // NaN também cai aqui, como zero e negativo.
  if (Number.isNaN(value) || value <= 0) return 1;
  const exponent = Math.floor(Math.log10(value));
  const base = 10 ** exponent;
  const fraction = value / base;
  const step = [1, 2, 2.5, 5, 10].find((candidate) => fraction <= candidate + 1e-9) ?? 10;
  return step * base;
}

export type BarGeometry = { month: string; x: number; width: number; receivedHeight: number; upcomingHeight: number };

/** A geometria das barras empilhadas (recebido embaixo, a receber em cima). */
export function barGeometry(months: IncomeMonth[], plotWidth: number, plotHeight: number, ceiling: number): BarGeometry[] {
  if (months.length === 0) return [];
  const slot = plotWidth / months.length;
  const width = Math.min(slot * 0.62, 34);
  return months.map((m, index) => ({
    month: m.month,
    x: slot * index + (slot - width) / 2,
    width,
    receivedHeight: (m.received / ceiling) * plotHeight,
    upcomingHeight: (m.upcoming / ceiling) * plotHeight,
  }));
}

/** Frase para o leitor de tela do gráfico de barras. */
export function barsSummary(months: IncomeMonth[], formatMoney: (value: number) => string): string {
  const best = months.reduce<IncomeMonth | null>((top, m) => (top && top.received + top.upcoming >= m.received + m.upcoming ? top : m), null);
  if (!best || best.received + best.upcoming <= 0) return "Renda dos últimos 12 meses: nenhum provento lançado.";
  const total = months.reduce((acc, m) => acc + m.received, 0);
  return `Renda dos últimos 12 meses: ${formatMoney(total)} recebidos. Maior mês: ${monthLabel(best.month)}, ${formatMoney(best.received + best.upcoming)}.`;
}
