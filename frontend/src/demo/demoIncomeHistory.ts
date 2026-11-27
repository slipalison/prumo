import type { AssetClass } from "../api/types";
import { historyHandlers, type FakeIncome, type MonthBase } from "../test/incomeHistoryHandlers";
import { DEMO_CODES } from "./demoIncome";

/*
 * O histórico de proventos da demonstração: os mesmos pagadores FICTÍCIOS da
 * Renda (códigos inventados), dois anos e meio de pagamentos — FII todo mês,
 * ação a cada trimestre (os terminados em 4 com JCP, os outros com dividendo),
 * um ativo já apagado e alguns pagamentos a receber. Nenhum valor tem relação
 * com carteira nenhuma.
 */

const SEGMENTS: Record<"br_reits" | "br_stocks", string[]> = {
  br_reits: ["Tijolo", "Papel", "Fundo de fundos", "Híbrido"],
  br_stocks: ["Bancos", "Energia Elétrica", "Seguradoras", "Saneamento", "Motores, Compressores e Outros"],
};

const round2 = (value: number) => Math.round(value * 100) / 100;
const day = (year: number, month: number, d: number) => `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;

function incomes(): FakeIncome[] {
  const list: FakeIncome[] = [];
  DEMO_CODES.slice(0, 14).forEach((code, index) => {
    const asset_class: AssetClass = code.endsWith("11") ? "br_reits" : "br_stocks";
    const segments = SEGMENTS[asset_class];
    // Uma ação sem classificação pública, para o "sem segmento".
    const segment = index === 7 ? null : (segments[index % segments.length] ?? null);
    const asset = { id: `a-demo-${index}`, label: code, asset_class, segment };
    const jcp = code.endsWith("4");
    const monthly = 70 - index * 4;
    for (let n = 0; n < 32; n += 1) {
      const month0 = 4 + n; // de maio de 2024 a dezembro de 2026
      const year = 2024 + Math.floor(month0 / 12);
      const month = (month0 % 12) + 1;
      if (asset_class === "br_stocks" && month % 3 !== index % 3) continue;
      // Um pouco de sazonalidade e de crescimento, para o mapa ter o que mostrar.
      const season = 1 + 0.25 * Math.sin((month + index) / 2);
      const grow = 1 + n * 0.012;
      const gross = round2((asset_class === "br_stocks" ? monthly * 2.6 : monthly) * season * grow);
      const net = jcp ? round2(gross * (year >= 2026 ? 0.825 : 0.85)) : gross;
      const pay = day(year, month, 10 + (index % 15));
      const record = month === 1 ? day(year - 1, 12, 29) : day(year, month - 1, 27);
      if (pay > "2027-01-31") continue;
      list.push({ pay, record, gross, net, asset });
    }
  });
  // Um FII vendido e apagado da carteira em 2025: o histórico continua com ele.
  for (let month = 1; month <= 8; month += 1) {
    list.push({ pay: day(2025, month, 15), record: day(2025, month, 1), gross: 38, net: 38, asset: null, label: "VELH11" });
  }
  return list;
}

/** Patrimônio e custo médios por mês, crescendo com os aportes, desde maio de 2024. */
const base: MonthBase = (month) => {
  if (month < "2024-05") return { market_value: null, cost: null };
  const n = (Number(month.slice(0, 4)) - 2024) * 12 + Number(month.slice(5, 7)) - 5;
  return { market_value: round2(60000 + n * 2100 + 1800 * Math.sin(n / 3)), cost: round2(56000 + n * 1900) };
};

export function demoIncomeHistoryHandlers() {
  return historyHandlers(incomes(), base);
}
