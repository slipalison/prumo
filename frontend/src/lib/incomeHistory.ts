import type {
  AmountKind,
  DateBasis,
  HeatCell,
  HeatYear,
  HistoryBucket,
  HistoryGroup,
  HistoryGrouping,
  HistoryInterval,
} from "../api/incomeHistoryTypes";
import { CLASS_META } from "./assetClasses";
import { formatPercent } from "./format";
import { MONTH_HEADERS, monthLabel, OTHERS_COLOR, PAYER_COLORS } from "./income";
import { MASK_BRL, type Money } from "./money";
import { hasSectorSource } from "./sectors";

/*
 * O histórico de proventos e o mapa de calor da Renda, do jeito que a tela os
 * diz. A conta é do servidor (`domain/income_history.rs`); aqui só o recorte do
 * período, os nomes, as cores e o texto.
 */

export const INTERVAL_LABEL: Record<HistoryInterval, string> = {
  month: "Mês",
  quarter: "Trimestre",
  half_year: "Semestre",
  year: "Ano",
};

export const GROUPING_LABEL: Record<HistoryGrouping, string> = {
  total: "Total",
  class: "Classe",
  segment: "Segmento",
  asset: "Ativo",
};

export const BASIS_LABEL: Record<DateBasis, string> = { payment: "Pagamento", record: "Data com" };
export const AMOUNT_LABEL: Record<AmountKind, string> = { net: "Líquido", gross: "Bruto" };

export const PERIODS = [
  { id: "12m", label: "12 meses", months: 12 },
  { id: "24m", label: "2 anos", months: 24 },
  { id: "60m", label: "5 anos", months: 60 },
  { id: "all", label: "Tudo", months: null },
] as const;
export type PeriodId = (typeof PERIODS)[number]["id"];

const MONTH_NAMES = [
  "janeiro",
  "fevereiro",
  "março",
  "abril",
  "maio",
  "junho",
  "julho",
  "agosto",
  "setembro",
  "outubro",
  "novembro",
  "dezembro",
] as const;

const monthOf = (start: string) => Number(start.slice(5, 7));
const yearOf = (start: string) => Number(start.slice(0, 4));

/** `2026-10` mais `n` meses. */
export function shiftMonth(start: string, n: number): string {
  const index = yearOf(start) * 12 + monthOf(start) - 1 + n;
  const year = Math.floor(index / 12);
  const month = index - year * 12 + 1;
  return `${year}-${String(month).padStart(2, "0")}`;
}

const SPAN: Record<HistoryInterval, number> = { month: 1, quarter: 3, half_year: 6, year: 12 };

/** O primeiro mês do intervalo que contém `month` (a mesma conta do servidor). */
export function intervalStart(month: string, interval: HistoryInterval): string {
  const span = SPAN[interval];
  const first = Math.floor((monthOf(month) - 1) / span) * span + 1;
  return `${month.slice(0, 4)}-${String(first).padStart(2, "0")}`;
}

/** O rótulo curto do eixo: `nov/26`, `4T26`, `2S26`, `2026`. */
export function bucketShort(start: string, interval: HistoryInterval): string {
  const yy = start.slice(2, 4);
  switch (interval) {
    case "month":
      return monthLabel(start);
    case "quarter":
      return `${Math.ceil(monthOf(start) / 3)}T${yy}`;
    case "half_year":
      return `${Math.ceil(monthOf(start) / 6)}S${yy}`;
    case "year":
      return start.slice(0, 4);
  }
}

/** O nome por extenso: `novembro de 2026`, `4º trimestre de 2026`, `2º semestre de 2026`, `2026`. */
export function bucketLong(start: string, interval: HistoryInterval): string {
  const year = start.slice(0, 4);
  switch (interval) {
    case "month":
      return `${MONTH_NAMES[monthOf(start) - 1] ?? start} de ${year}`;
    case "quarter":
      return `${Math.ceil(monthOf(start) / 3)}º trimestre de ${year}`;
    case "half_year":
      return `${Math.ceil(monthOf(start) / 6)}º semestre de ${year}`;
    case "year":
      return year;
  }
}

/**
 * Os intervalos do período escolhido: os que começam a partir do intervalo de
 * `months − 1` meses atrás (12 meses = os 12 meses até o de hoje), mais os que
 * vêm depois de hoje com algo a receber.
 */
export function bucketsInPeriod(buckets: HistoryBucket[], interval: HistoryInterval, period: PeriodId, today: string): HistoryBucket[] {
  const months = PERIODS.find((p) => p.id === period)?.months ?? null;
  if (months === null) return buckets;
  const from = intervalStart(shiftMonth(today.slice(0, 7), 1 - months), interval);
  return buckets.filter((b) => b.start >= from);
}

/** O nome do grupo na legenda: `FIIs`, `Ações BR · Bancos`, `ITSA4`, `Ativos removidos`. */
export function groupLabel(group: HistoryGroup): string {
  switch (group.kind) {
    case "total":
      return "Proventos";
    case "removed":
      return "Ativos removidos";
    case "asset":
      return group.label ?? group.key;
    case "class":
      return group.asset_class ? CLASS_META[group.asset_class].shortLabel : group.key;
    case "segment": {
      if (!group.asset_class) return group.key;
      const short = CLASS_META[group.asset_class].shortLabel;
      if (group.segment) return `${short} · ${group.segment}`;
      return hasSectorSource(group.asset_class) ? `${short} · sem segmento` : short;
    }
  }
}

/** A chave do grupo que junta o que passa das cores próprias. */
export const OTHERS_GROUP = "others";
const REMOVED_COLOR = "var(--text-subtle)";

export type Series = {
  key: string;
  label: string;
  color: string;
  /** As chaves do servidor que a série soma (várias só em "Outros"). */
  members: string[];
};

/**
 * As séries do gráfico, na ordem do histórico inteiro (a do servidor), para a
 * cor seguir o grupo e não mudar quando o período muda: a classe tem a cor dela;
 * segmento e ativo levam as seis cores de "Quem pagou" pela ordem, e o resto
 * vai para "Outros (N)". Só entra quem tem valor nos intervalos à vista.
 */
export function seriesOf(groups: HistoryGroup[], visible: HistoryBucket[]): Series[] {
  const present = new Set(visible.flatMap((b) => b.groups.filter((p) => p.received + p.upcoming > 0).map((p) => p.key)));
  const own: Series[] = [];
  const rest: string[] = [];
  // A posição do grupo entre os que pegam cor da lista, no histórico inteiro.
  let rank = 0;
  for (const group of groups) {
    const ranked = group.kind === "segment" || group.kind === "asset" ? rank++ : null;
    if (!present.has(group.key)) continue;
    const one = (color: string) => own.push({ key: group.key, label: groupLabel(group), color, members: [group.key] });
    if (group.kind === "total") one("var(--gold-text)");
    else if (group.kind === "class") one(group.asset_class ? `var(--c-${group.asset_class})` : REMOVED_COLOR);
    else if (group.kind === "removed") one(REMOVED_COLOR);
    else if (ranked !== null && ranked < PAYER_COLORS.length) one(PAYER_COLORS[ranked] ?? OTHERS_COLOR);
    else rest.push(group.key);
  }
  if (rest.length === 0) return own;
  return [...own, { key: OTHERS_GROUP, label: `Outros (${rest.length})`, color: OTHERS_COLOR, members: rest }];
}

export type Segment = { series: Series; received: number; upcoming: number };

/** As partes de um intervalo por série, na ordem das séries (de baixo para cima no gráfico). */
export function segmentsOf(bucket: HistoryBucket, series: Series[]): Segment[] {
  const byKey = new Map(bucket.groups.map((p) => [p.key, p]));
  return series
    .map((s) => {
      const parts = s.members.map((key) => byKey.get(key)).filter((p) => p !== undefined);
      return {
        series: s,
        received: parts.reduce((acc, p) => acc + p.received, 0),
        upcoming: parts.reduce((acc, p) => acc + p.upcoming, 0),
      };
    })
    .filter((s) => s.received + s.upcoming > 0);
}

/** A soma de cada série nos intervalos à vista: o número da legenda. */
export function seriesTotals(visible: HistoryBucket[], series: Series[]): Map<string, number> {
  const totals = new Map<string, number>(series.map((s) => [s.key, 0]));
  for (const bucket of visible) {
    for (const segment of segmentsOf(bucket, series)) {
      totals.set(segment.series.key, (totals.get(segment.series.key) ?? 0) + segment.received + segment.upcoming);
    }
  }
  return totals;
}

/** De quantos em quantos intervalos vai um rótulo no eixo, para os rótulos não se encostarem. */
export function labelStep(slot: number, minWidth = 44): number {
  if (!(slot > 0)) return 1;
  return Math.max(1, Math.ceil(minWidth / slot));
}

// ------------------------------------------------------------ mapa de calor

export const HEAT_METRICS = ["amount", "dy", "yoc"] as const;
export type HeatMetric = (typeof HEAT_METRICS)[number];

/** Os níveis de cor do mapa (0 = sem valor). */
export const HEAT_LEVELS = 5;

/** O número da célula na métrica escolhida; `null` sem base (DY e yield on cost). */
export function heatValue(cell: HeatCell, metric: HeatMetric): number | null {
  switch (metric) {
    case "amount":
      return cell.amount;
    case "dy":
      return cell.dy;
    case "yoc":
      return cell.yield_on_cost;
  }
}

export function heatPartial(cell: HeatCell, metric: HeatMetric): boolean {
  if (metric === "dy") return cell.dy_partial;
  if (metric === "yoc") return cell.yield_on_cost_partial;
  return false;
}

/** O maior valor dos meses (não dos totais): o topo da escala de cor. */
export function heatMax(years: HeatYear[], metric: HeatMetric): number {
  return years.reduce((top, year) => year.months.reduce((acc, cell) => Math.max(acc, heatValue(cell, metric) ?? 0), top), 0);
}

/** O nível de cor de 1 a 5 pela fração do maior mês; 0 sem provento ou sem base. */
export function heatLevel(value: number | null, max: number): number {
  if (value === null || !(value > 0) || !(max > 0)) return 0;
  return Math.min(HEAT_LEVELS, Math.max(1, Math.ceil((value / max) * HEAT_LEVELS)));
}

const wholeBrl = new Intl.NumberFormat("pt-BR", { style: "currency", currency: "BRL", maximumFractionDigits: 0 });

/**
 * Reais que cabem na célula: com centavos até R$ 100, sem centavos até mil e
 * `R$ 1,2 mil` daí para cima. O valor exato vai no texto do leitor de tela e no
 * balão.
 */
export function heatMoney(value: number, money: Money): string {
  if (money.hidden) return MASK_BRL;
  if (Math.abs(value) >= 1000) return money.compact(value);
  return Math.abs(value) >= 100 ? wholeBrl.format(value) : money.brl(value);
}

/** O texto curto da célula: `R$ 12,34`, `R$ 275`, `R$ 1,2 mil`, `0,72%` ou `–`. */
export function heatText(cell: HeatCell, metric: HeatMetric, money: Money): string {
  const value = heatValue(cell, metric);
  if (cell.amount <= 0 || value === null) return "–";
  return metric === "amount" ? heatMoney(value, money) : formatPercent(value, 2);
}

/** O que o leitor de tela ouve e o balão do mouse mostra: o valor exato e a conta. */
export function heatDetail(cell: HeatCell, metric: HeatMetric, money: Money): string {
  if (cell.amount <= 0) return "sem provento";
  const upcoming = cell.upcoming > 0 ? `, dos quais ${money.brl(cell.upcoming)} a receber` : "";
  const amount = `${money.brl(cell.amount)}${upcoming}`;
  if (metric === "amount") return amount;
  const base = metric === "dy" ? cell.market_value : cell.cost;
  const value = heatValue(cell, metric);
  if (value === null || base === null) {
    return metric === "dy" ? `${amount}; sem foto do patrimônio no mês, sem DY` : `${amount}; sem custo no mês, sem yield on cost`;
  }
  const over = metric === "dy" ? "do patrimônio médio de" : "do custo médio de";
  const partial = heatPartial(cell, metric) ? " (parcial: parte do provento fica fora da conta)" : "";
  return `${formatPercent(value, 2)}${partial}: ${amount} ${over} ${money.brl(base)}`;
}

/** `jan`, `fev`... para o cabeçalho e para o nome do mês na célula empilhada. */
export const HEAT_MONTHS = MONTH_HEADERS;

/** `março de 2026`, para o balão e a legenda da célula. */
export function monthName(year: number, index: number): string {
  return `${MONTH_NAMES[index] ?? ""} de ${year}`;
}
