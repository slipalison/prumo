import type { Drawdown, RiskWindow } from "../../api/riskTypes";
import { formatPercent } from "../../lib/format";
import { formatDay } from "../performance/performanceFormat";

export const WINDOWS: Array<{ id: RiskWindow; label: string; long: string }> = [
  { id: "6m", label: "6 meses", long: "6 meses" },
  { id: "12m", label: "12 meses", long: "12 meses" },
  { id: "24m", label: "2 anos", long: "2 anos" },
  { id: "36m", label: "3 anos", long: "3 anos" },
  { id: "60m", label: "5 anos", long: "5 anos" },
];

/** "12 meses", "2 anos". */
export function windowLabel(window: RiskWindow): string {
  return WINDOWS.find((w) => w.id === window)?.long ?? window;
}

/** Correlação com duas casas e o sinal escrito: `0,85`, `−0,12`; a diagonal é `1`. */
export function formatCoefficient(value: number): string {
  if (value >= 0.9995) return "1";
  if (value <= -0.9995) return "−1";
  const text = Math.abs(value).toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return value < -0.005 ? `−${text}` : text;
}

/** A força de uma correlação (de 0,2 para cima), em uma palavra. */
function strengthWord(strength: number): string {
  if (strength >= 0.7) return "fortemente";
  return strength >= 0.4 ? "moderadamente" : "fracamente";
}

/** O que a correlação diz, em palavras (sem juízo: só o sentido e a força). */
export function describeCoefficient(value: number): string {
  const strength = Math.abs(value);
  if (strength < 0.2) return "quase sem relação";
  const word = strengthWord(strength);
  return value > 0 ? `andam juntos ${word}` : `andam em sentidos opostos ${word}`;
}

/** `22,1% ao ano`. */
export function formatVolatility(value: number): string {
  return `${formatPercent(value, 1)} ao ano`;
}

/** As datas da queda, do pico ao fundo: "de 04/03/2026 a 08/04/2026; recuperou em 21/05/2026" ou "...; ainda não recuperou". */
export function drawdownDates(drawdown: Drawdown): string {
  const span = `de ${formatDay(drawdown.peak, true)} a ${formatDay(drawdown.trough, true)}`;
  return drawdown.recovered
    ? `${span}; recuperou em ${formatDay(drawdown.recovered, true)}`
    : `${span}; ainda não recuperou`;
}

/** `2026-03-04` → `04/03/26`, para caber numa linha de tabela. */
export function shortDay(iso: string): string {
  const [year, month, day] = iso.split("-");
  if (!year || !month || !day) return iso;
  return `${day}/${month}/${year.slice(2)}`;
}

/** A mesma coisa, curta: "04/03/26 a 08/04/26 · recuperou em 21/05/26". */
export function drawdownShort(drawdown: Drawdown): string {
  const span = `${shortDay(drawdown.peak)} a ${shortDay(drawdown.trough)}`;
  return drawdown.recovered ? `${span} · recuperou em ${shortDay(drawdown.recovered)}` : `${span} · ainda não recuperou`;
}

/** Escala de 3 ou 4 marcas "redondas" entre `min` e `max`. */
export function niceTicks(min: number, max: number, count = 4): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) return [min];
  const raw = (max - min) / count;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = ([1, 2, 2.5, 5, 10].map((m) => m * magnitude).find((s) => s >= raw) ?? raw);
  const first = Math.ceil(min / step) * step;
  const ticks: number[] = [];
  for (let tick = first; tick <= max + step * 1e-9; tick += step) ticks.push(Math.round(tick / step) * step);
  return ticks;
}
