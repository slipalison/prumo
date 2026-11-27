import type { IncomeEvent, IncomePayer, IncomeSummary } from "../api/incomeTypes";
import { estimateAsset, estimateOf, knownYield, sampleEvent, sampleSummary, unknownCostYield } from "../test/incomeHandlers";

/*
 * A Renda da demonstração com o que a tela tem de mais longo: 25 pagadores (o
 * grupo "Outros" com 19), 14 pagamentos a receber e uma estimativa de 28
 * ativos. Tudo FICTÍCIO (códigos inventados, valores sem relação com carteira
 * nenhuma), como os fixtures dos testes.
 */

const CODES = [
  "ALFA3", "BRAV11", "CEDR4", "DUNA3", "ESTR11", "FARO3", "GAIA4", "HORT11", "IRIS3", "JADE4",
  "KORA11", "LUMA3", "MIRA4", "NEVA11", "OLMA3", "PRIS4", "QUAR11", "RUBI3", "SOLA4", "TIGR11",
  "UNIO3", "VELA4", "WAVE11", "XENO3", "ZEFI4",
] as const;

/** Do maior para o menor, em reais, somando perto de R$ 5,5 mil em 12 meses. */
const TOTALS = [
  780, 560, 410, 350, 320, 290, 240, 215, 190, 170, 150, 132, 118, 104, 92, 80, 70, 62, 54, 47, 40, 34, 28, 22, 17,
] as const;

const SUM = TOTALS.reduce((acc, value) => acc + value, 0);
const round2 = (value: number) => Math.round(value * 100) / 100;

function payers(): IncomePayer[] {
  return CODES.map((label, index) => {
    const total = TOTALS[index] ?? 0;
    return {
      asset_id: `a-demo-${index}`,
      label,
      total,
      percent: round2((total / SUM) * 100),
      // Dois sem preço médio, para a coluna mostrar o pedido em vez do número.
      yield_on_cost: index === 9 || index === 17 ? unknownCostYield(total) : knownYield(total, total * (8 + (index % 7))),
    };
  });
}

const UPCOMING_DAYS = ["2026-11-28", "2026-11-30", "2026-12-01", "2026-12-05", "2026-12-10", "2026-12-12", "2026-12-15", "2026-12-18", "2026-12-21", "2026-12-23", "2026-12-30", "2027-01-04", "2027-01-27", "2027-03-10"] as const;

function upcoming(): IncomeEvent[] {
  return UPCOMING_DAYS.map((pay_date, index) => {
    const label = CODES[(index * 2) % 8] ?? "ALFA3";
    const net = round2(20 + ((index * 37) % 90));
    return sampleEvent({
      id: `e-demo-up-${index}`,
      asset_id: `a-demo-${CODES.indexOf(label)}`,
      symbol: label,
      display_name: label,
      kind: label.endsWith("11") ? "reit_income" : index % 3 === 0 ? "jcp" : "dividend",
      pay_date,
      gross: net,
      net,
      status: "upcoming",
    });
  });
}

/** Os lançados recentes: um por pagador, do mais novo para o mais velho. */
export function demoIncomeEvents(): IncomeEvent[] {
  return CODES.slice(0, 20).map((label, index) =>
    sampleEvent({
      id: `e-demo-${index}`,
      asset_id: `a-demo-${index}`,
      symbol: label,
      display_name: label,
      kind: label.endsWith("11") ? "reit_income" : "dividend",
      pay_date: `2026-11-${String(24 - Math.floor(index / 2)).padStart(2, "0")}`,
      gross: round2((TOTALS[index] ?? 0) / 12),
      net: round2((TOTALS[index] ?? 0) / 12),
    }),
  );
}

export function demoIncomeSummary(): IncomeSummary {
  const base = sampleSummary();
  const list = upcoming();
  // Os meses passam a somar o que os pagadores somam: o meio da rosca e os números do topo batem.
  const scale = SUM / base.total_12m;
  // Os bancos e a holding pagam JCP (o bruto é maior); três estão acima do
  // habitual (antecipações); os dois últimos vêm do Yahoo; FII não tem JCP.
  const estimateAssets = Array.from({ length: 28 }, (_, index) => {
    const symbol = CODES[index] ?? `FIC${index}`;
    const monthly = round2(130 - index * 4.2);
    const quantity = 100 + index * 25;
    const reit = symbol.endsWith("11");
    const gross = reit || index % 3 !== 0 ? monthly : round2(monthly / 0.9);
    const yahoo = index >= 26;
    const above = [1, 4, 9].includes(index);
    const usualMonthly = round2(above ? monthly / 1.8 : monthly * (index % 2 === 0 ? 0.95 : 1.1));
    return estimateAsset({
      asset_id: `a-demo-${index}`,
      symbol,
      display_name: symbol,
      quantity,
      source: yahoo ? "yahoo" : "b3",
      per_unit_12m: round2((monthly * 12) / quantity),
      per_unit_12m_gross: round2((gross * 12) / quantity),
      monthly,
      monthly_gross: gross,
      usual:
        index === 20
          ? null
          : {
              years: index === 15 ? 3 : 5,
              per_unit_year: round2((usualMonthly * 12) / quantity),
              per_unit_year_gross: round2((usualMonthly * 12 * (gross / monthly)) / quantity),
              monthly: usualMonthly,
              monthly_gross: round2(usualMonthly * (gross / monthly)),
            },
      above_usual: above,
    });
  });
  return {
    ...base,
    total_12m: SUM,
    monthly_average: round2(SUM / 12),
    months: base.months.map((m) => ({ ...m, received: round2(m.received * scale) })),
    goal: base.goal && { ...base.goal, current_monthly: round2(SUM / 12), percent: round2((SUM / 12 / base.goal.target) * 100) },
    by_payer: payers(),
    upcoming: list,
    upcoming_total: round2(list.reduce((acc, e) => acc + e.net, 0)),
    estimate: estimateOf(estimateAssets),
  };
}
