const brl = new Intl.NumberFormat("pt-BR", { style: "currency", currency: "BRL" });
const brlCompact = new Intl.NumberFormat("pt-BR", {
  style: "currency",
  currency: "BRL",
  notation: "compact",
  maximumFractionDigits: 1,
});

/** `R$ 1.234,56` (com espaço inseparável, como o Intl escreve). */
export function formatBRL(value: number): string {
  return brl.format(Number.isFinite(value) ? value : 0);
}

/** `R$ 136 mil` — para avisos, onde centavo é ruído. Abaixo de mil, valor cheio. */
export function formatCompactBRL(value: number): string {
  if (Math.abs(value) < 1000) return formatBRL(value);
  return brlCompact.format(value);
}

/** `3,8%`. Percentuais chegam de 0 a 100, nunca de 0 a 1. */
export function formatPercent(value: number, digits = 1): string {
  const safe = Number.isFinite(value) ? value : 0;
  return `${safe.toLocaleString("pt-BR", { minimumFractionDigits: digits, maximumFractionDigits: digits })}%`;
}

/** Quantidade: inteira sem casas; fracionada com até 8, sem zeros à direita. */
export function formatQuantity(value: number, fractional: boolean): string {
  if (!fractional) return Math.round(value).toLocaleString("pt-BR");
  return value.toLocaleString("pt-BR", { maximumFractionDigits: 8 });
}

export function plural(count: number, [singular, pluralForm]: [string, string]): string {
  return Math.abs(count) === 1 ? singular : pluralForm;
}

const dateFormat = new Intl.DateTimeFormat("pt-BR", { dateStyle: "medium", timeStyle: "short" });

export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : dateFormat.format(date);
}

/** "agora há pouco", "há 12 min", "há 3 h", ou a data. */
export function formatRelative(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "nunca";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "nunca";
  const minutes = Math.round((now.getTime() - date.getTime()) / 60000);
  if (minutes < 2) return "agora há pouco";
  if (minutes < 60) return `há ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `há ${hours} h`;
  return formatDateTime(iso);
}

/** Lê número digitado em pt-BR ("1.234,5" ou "1234.5"). `null` se não for número. */
export function parseDecimal(text: string): number | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const normalized = trimmed.includes(",") ? trimmed.replace(/\./g, "").replace(",", ".") : trimmed;
  if (!/^-?\d*(\.\d*)?$/.test(normalized)) return null;
  const value = Number(normalized);
  return Number.isFinite(value) ? value : null;
}
