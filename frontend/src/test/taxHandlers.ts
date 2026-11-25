import { http, HttpResponse } from "msw";
import type {
  Carryforward,
  CarryforwardRequest,
  Darf,
  Instrument,
  Irpf,
  Pool,
  TaxMonth,
  TaxMonths,
} from "../api/taxTypes";

/** Os anos com regra no servidor falso, como no de verdade. */
export const COVERED_YEARS = [2025, 2026];

const POOLS: Pool[] = ["common", "day_trade", "real_estate_fund", "agribusiness_fund"];
const RATES: Record<Pool, number> = { common: 0.15, day_trade: 0.2, real_estate_fund: 0.2, agribusiness_fund: 0.2 };
const INSTRUMENTS: Instrument[] = ["stock", "etf", "bdr", "real_estate_fund", "agribusiness_fund", "crypto"];

function lastDay(year: number, month: number): string {
  const next = month === 12 ? new Date(Date.UTC(year + 1, 0, 31)) : new Date(Date.UTC(year, month + 1, 0));
  return next.toISOString().slice(0, 10);
}

function darf(code: "6015" | "4600", year: number, month: number): Darf {
  return {
    code,
    regime: code === "6015" ? "exchange_net_gain" : "capital_gain",
    tax: 0,
    pending_before: 0,
    total: 0,
    payable: false,
    amount: 0,
    due_date: lastDay(year, month),
    pending_after: 0,
    paid: null,
  };
}

export function emptyMonth(year: number, month: number): TaxMonth {
  return {
    year,
    month,
    sales: INSTRUMENTS.map((instrument) => ({
      instrument,
      common_sales: 0,
      day_trade_sales: 0,
      common_result: 0,
      day_trade_result: 0,
    })),
    exemptions: [
      { id: "stock_sales", sales: 0, limit: 20000, remaining: 20000, exempt: true, exempt_gain: 0 },
      { id: "crypto_sales", sales: 0, limit: 35000, remaining: 35000, exempt: true, exempt_gain: 0 },
    ],
    pools: POOLS.map((pool) => ({
      pool,
      result: 0,
      loss_before: 0,
      loss_used: 0,
      base: 0,
      rate: RATES[pool],
      tax: 0,
      loss_after: 0,
    })),
    capital_gain: { gains: 0, losses: 0, exempt: true, base: 0, tax: 0 },
    withholding: { withheld: 0, carried_in: 0, used: 0, carried_out: 0 },
    darfs: [darf("6015", year, month), darf("4600", year, month)],
    notices: [],
    doubts: [],
  };
}

/** Carteira FICTÍCIA: um ano com isenção, DARF a pagar, DARF pago, acúmulo abaixo do mínimo e cripto. */
export function sampleMonths(year: number): TaxMonths {
  const months = Array.from({ length: 12 }, (_, index) => emptyMonth(year, index + 1));
  const at = (month: number) => months[month - 1] as TaxMonth;
  const stockSales = (month: TaxMonth, common: number, dayTrade: number, result: number) => {
    const line = month.sales[0];
    if (line) Object.assign(line, { common_sales: common, day_trade_sales: dayTrade, common_result: result });
    const exemption = month.exemptions[0];
    if (exemption) {
      const total = common + dayTrade;
      Object.assign(exemption, {
        sales: total,
        remaining: Math.max(0, 20000 - total),
        exempt: total <= 20000,
        exempt_gain: total <= 20000 && result > 0 ? result : 0,
      });
    }
  };

  if (year === 2026) {
    // Março: 6 reais de imposto, abaixo do mínimo.
    stockSales(at(3), 22000, 0, 40);
    Object.assign(at(3).pools[0] ?? {}, { result: 40, base: 40, tax: 6 });
    Object.assign(at(3).darfs[0] ?? {}, { tax: 6, total: 6, pending_after: 6 });
    // Abril: soma os 6 de março, passa do mínimo e já foi pago.
    stockSales(at(4), 21000, 0, 66.67);
    Object.assign(at(4).pools[0] ?? {}, { result: 66.67, base: 66.67, tax: 10 });
    Object.assign(at(4).darfs[0] ?? {}, {
      tax: 10,
      pending_before: 6,
      total: 16,
      payable: true,
      amount: 16,
      due_date: "2026-05-29",
      paid: { amount: 16, paid_at: "2026-05-20T12:00:00Z", same_amount: true },
    });
    // Maio: lucro tributável.
    stockSales(at(5), 24000, 0, 3000);
    Object.assign(at(5).pools[0] ?? {}, { result: 3000, base: 3000, tax: 450 });
    Object.assign(at(5).darfs[0] ?? {}, { tax: 450, total: 450, payable: true, amount: 450, due_date: "2026-06-30" });
    // Agosto: prejuízo de Fiagro.
    Object.assign(at(8).pools[3] ?? {}, { result: -100, loss_after: 100 });
    at(8).doubts = ["agribusiness_separate_from_real_estate"];
    // Setembro: cripto acima de 35 mil, com lucro e prejuízo.
    Object.assign(at(9).sales[5] ?? {}, { common_sales: 40000, common_result: 4800 });
    Object.assign(at(9).exemptions[1] ?? {}, { sales: 40000, remaining: 0, exempt: false });
    at(9).capital_gain = { gains: 5000, losses: 200, exempt: false, base: 5000, tax: 750 };
    Object.assign(at(9).darfs[1] ?? {}, { tax: 750, total: 750, payable: true, amount: 750, due_date: "2026-10-30" });
    at(9).doubts = ["crypto_per_sale"];
    // Novembro, o mês de hoje: 12 mil vendidos, com day trade no total.
    stockSales(at(11), 11500, 500, 900);
    at(11).doubts = ["day_trade_in_stock_exemption"];
  } else {
    stockSales(at(6), 30000, 0, 2000);
    Object.assign(at(6).pools[0] ?? {}, { result: 2000, base: 2000, tax: 300 });
    Object.assign(at(6).darfs[0] ?? {}, { tax: 300, total: 300, payable: true, amount: 300, due_date: "2025-07-31" });
  }

  return {
    year,
    since_year: 2025,
    covered_years: COVERED_YEARS,
    today: "2026-11-25",
    months,
    closing: {
      losses: { common: 0, day_trade: 0, real_estate_fund: 0, agribusiness_fund: year === 2026 ? 100 : 0 },
      pending_darf: { exchange_net_gain: 0, capital_gain: 0 },
    },
    skipped: [
      { subject: "CDB Exemplo", date: null, reason: "unsupported_instrument", amount: null },
      { subject: "KNRI11", date: `${year}-03-10`, reason: "amortization", amount: 10 },
    ],
  };
}

export function sampleIrpf(year: number): Irpf {
  return {
    year,
    since_year: 2025,
    covered_years: COVERED_YEARS,
    assets: [
      {
        symbol: "ITSA4",
        instrument: "stock",
        quantity: 2500,
        total_cost: 25000,
        average_price: 10,
        group: "03",
        code: "01",
        label: "Acoes (inclusive as listadas em bolsa)",
        code_source: "Programa IRPF",
      },
      {
        symbol: "BTC",
        instrument: "crypto",
        quantity: 0.05,
        total_cost: 15000,
        average_price: 300000,
        group: "08",
        code: "01",
        label: "Criptomoeda Bitcoin (BTC)",
        code_source: "P&R 473",
      },
    ],
    exempt_income: [
      {
        group: "isentos",
        code: "09",
        label: "Lucros e dividendos recebidos",
        code_source: "x",
        gross: 300,
        withheld: 0,
        net: 300,
        by_payer: [{ symbol: "ITSA4", gross: 300, withheld: 0 }],
      },
      {
        group: "isentos",
        code: "99",
        label: "Outros (rendimentos de FII e Fiagro)",
        code_source: "x",
        gross: 1200,
        withheld: 0,
        net: 1200,
        by_payer: [{ symbol: "KNRI11", gross: 1200, withheld: 0 }],
      },
      { group: "isentos", code: "20", label: "Ganhos", code_source: "x", gross: 900, withheld: 0, net: 900, by_payer: [] },
      { group: "isentos", code: "05", label: "Ganho", code_source: "x", gross: 0, withheld: 0, net: 0, by_payer: [] },
    ],
    exclusive_income: [
      {
        group: "exclusiva",
        code: "10",
        label: "Juros sobre capital proprio",
        code_source: "x",
        gross: 200,
        withheld: 35,
        net: 165,
        by_payer: [{ symbol: "ITSA4", gross: 200, withheld: 35 }],
      },
    ],
    year_end: { losses: { agribusiness_fund: 100 }, pending_darf: { exchange_net_gain: 0, capital_gain: 0 } },
    withholding_left: 0,
    darf_total: 1216,
    doubts: ["irpf_codes", "interest_on_equity_net"],
    skipped: [
      { subject: "CDB Exemplo", date: null, reason: "unsupported_instrument", amount: null },
      { subject: "AAPL34", date: `${year}-05-02`, reason: "income_from_abroad", amount: 12 },
    ],
  };
}

type TaxDb = {
  /** DARF pago por `ano-mês-código`. */
  paid: Map<string, { amount: number; paid_at: string }>;
  carryforward: Carryforward;
};

function createTaxDb(): TaxDb {
  return {
    paid: new Map([["2026-4-6015", { amount: 16, paid_at: "2026-05-20T12:00:00Z" }]]),
    carryforward: {
      year: 2025,
      informed: false,
      covered_years: COVERED_YEARS,
      common_loss: 0,
      day_trade_loss: 0,
      real_estate_fund_loss: 0,
      agribusiness_fund_loss: 0,
      pending_exchange_darf: 0,
      pending_capital_gain_darf: 0,
    },
  };
}

let taxDb = createTaxDb();

export function getTaxDb(): TaxDb {
  return taxDb;
}

export function resetTaxDb(): void {
  taxDb = createTaxDb();
}

const notCovered = (year: number) =>
  HttpResponse.json(
    {
      type: "urn:prumo:error:tax_year_not_covered",
      title: "Ano sem regra de IR",
      status: 422,
      code: "tax_year_not_covered",
      detail: `O Prumo tem a regra de IR de 2025 e 2026 e ainda nao a de ${year}. Ele nao usa a regra de outro ano.`,
    },
    { status: 422, headers: { "Content-Type": "application/problem+json" } },
  );

function withPaid(data: TaxMonths): TaxMonths {
  for (const month of data.months) {
    for (const d of month.darfs) {
      const paid = taxDb.paid.get(`${month.year}-${month.month}-${d.code}`);
      d.paid = paid ? { ...paid, same_amount: paid.amount === d.amount } : null;
    }
  }
  return data;
}

export const taxHandlers = [
  http.get("/api/v1/tax/months", ({ request }) => {
    const year = Number(new URL(request.url).searchParams.get("year"));
    if (!COVERED_YEARS.includes(year)) return notCovered(year);
    return HttpResponse.json(withPaid(sampleMonths(year)));
  }),

  http.get("/api/v1/tax/irpf/:year", ({ params }) => {
    const year = Number(params.year);
    if (!COVERED_YEARS.includes(year)) return notCovered(year);
    return HttpResponse.json(sampleIrpf(year));
  }),

  http.post("/api/v1/tax/darfs/:year/:month/paid", async ({ request, params }) => {
    const year = Number(params.year);
    const month = Number(params.month);
    const { code } = (await request.json()) as { code: "6015" | "4600" };
    const found = sampleMonths(year).months[month - 1]?.darfs.find((d) => d.code === code);
    if (!found?.payable) {
      return HttpResponse.json(
        { title: "Nao ha DARF a pagar", status: 422, code: "darf_not_due", detail: "Nada a pagar." },
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      );
    }
    const paid = { amount: found.amount, paid_at: "2026-11-25T12:00:00Z" };
    taxDb.paid.set(`${year}-${month}-${code}`, paid);
    return HttpResponse.json({ ...found, paid: { ...paid, same_amount: true } });
  }),

  http.delete("/api/v1/tax/darfs/:year/:month/paid", ({ request, params }) => {
    const code = new URL(request.url).searchParams.get("code");
    const key = `${String(params.year)}-${String(params.month)}-${code ?? ""}`;
    if (!taxDb.paid.delete(key)) return new HttpResponse(null, { status: 404 });
    return new HttpResponse(null, { status: 204 });
  }),

  http.get("/api/v1/tax/carryforward", () => HttpResponse.json(taxDb.carryforward)),

  http.put("/api/v1/tax/carryforward", async ({ request }) => {
    const body = (await request.json()) as CarryforwardRequest;
    taxDb.carryforward = { ...body, informed: true, covered_years: COVERED_YEARS };
    return HttpResponse.json(taxDb.carryforward);
  }),
];
