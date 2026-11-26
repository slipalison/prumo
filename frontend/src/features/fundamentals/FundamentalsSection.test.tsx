import { screen, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import type { AssetFundamentals, CompanyFundamentals, FundFundamentals } from "../../api/fundamentalsTypes";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { samplePeriod, setFundamentals } from "../../test/fundamentalsHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { GAP_TEXT, UNSUPPORTED_TEXT } from "./fundamentalsText";

/** Companhia fictícia, números inventados. */
function company(overrides: Partial<CompanyFundamentals> = {}): AssetFundamentals {
  return {
    asset_id: "a-itsa",
    symbol: "ITSA4",
    kind: "company",
    fund: null,
    reason: null,
    company: {
      cvm_code: 99901,
      name: "CIA FICTICIA S.A.",
      template: "commercial",
      judicial_recovery: false,
      issuer_status: "FASE OPERACIONAL",
      registration_status: "ATIVO",
      latest_filing: { kind: "quarterly", reference_date: "2026-09-30", version: 1, scope: "consolidated", column: "current" },
      synced_at: "2026-11-26T15:00:00Z",
      trailing: samplePeriod(),
      years: [2021, 2022, 2023, 2024, 2025].map((year) => ({ year, figures: samplePeriod({ period_end: `${year}-12-31` }) })),
      profitable_years: { positive: 4, known: 5, window: 5, gap: null },
      positive_operating_cash_years: { positive: 5, known: 5, window: 5, gap: null },
      revenue_cagr: { value: 0.1, gap: null },
      earnings_cagr: { value: -0.032, gap: null },
      shares: 1_000_000,
      earnings_per_share: { value: 1.8, gap: null },
      book_value_per_share: { value: 11, gap: null },
      ...overrides,
    },
  };
}

function fund(overrides: Partial<FundFundamentals> = {}): AssetFundamentals {
  return {
    asset_id: "a-knri",
    symbol: "KNRI11",
    kind: "fund",
    company: null,
    reason: null,
    fund: {
      cnpj: "00.000.000/0001-91",
      name: "FUNDO FICTICIO FII",
      reference_date: "2026-10-01",
      net_assets: { value: 1_000_000_000, gap: null },
      shares: { value: 10_000_000, gap: null },
      nav_per_share: { value: 100, gap: null },
      price: 90,
      price_to_book: { value: 0.9, gap: null },
      vacancy: { value: 0.15, gap: null },
      vacancy_reference_date: "2026-09-30",
      synced_at: "2026-11-26T15:00:00Z",
      ...overrides,
    },
  };
}

async function openSection(symbol: RegExp, name: string) {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  await rendered.user.click(screen.getByRole("button", { name: symbol }));
  const dialog = await screen.findByRole("dialog", { name });
  const section = within(dialog).getByRole("region", { name: "Fundamentos" });
  return { ...rendered, dialog, section };
}

const value = (section: HTMLElement, label: string) => within(section).getByText(label).nextSibling as HTMLElement;

describe("Fundamentos na folha do ativo", () => {
  it("mostra os números da companhia com o período, a fonte e a data", async () => {
    setFundamentals([company()]);
    const { section } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText("Últimos 12 meses, até 30/09/2026")).toBeInTheDocument();
    expect(value(section, "Receita líquida")).toHaveTextContent(/R\$\s1,5\sbi/);
    expect(value(section, "Lucro líquido")).toHaveTextContent(/R\$\s180\smi/);
    expect(value(section, "ROE")).toHaveTextContent("15,0%");
    expect(value(section, "Margem líquida")).toHaveTextContent("12,0%");
    expect(value(section, "Dív. líquida / EBITDA")).toHaveTextContent("1,0×");
    expect(value(section, "Dív. líquida / PL")).toHaveTextContent("0,3×");
    expect(value(section, "LPA")).toHaveTextContent(/R\$\s1,80/);
    expect(value(section, "VPA")).toHaveTextContent(/R\$\s11,00/);
    expect(value(section, "Anos com lucro")).toHaveTextContent("4 de 5");
    expect(value(section, "Anos com caixa operacional positivo")).toHaveTextContent("5 de 5");
    expect(value(section, "Receita, CAGR 5 anos")).toHaveTextContent("+10,0% a.a.");
    expect(value(section, "Lucro, CAGR 5 anos")).toHaveTextContent("−3,2% a.a.");
    expect(section).toHaveTextContent("Fonte: CVM, ITR de 30/09/2026 · conferido em 26/11/2026");
    expect(section).toHaveTextContent("EBITDA aproximado: EBIT + depreciação e amortização.");
    expect(within(section).queryByText(/recuperação judicial/)).toBeNull();
  });

  it("sem trimestre depois do exercício, diz que é o exercício", async () => {
    const annual = samplePeriod({
      period_end: "2025-12-31",
      sources: [{ kind: "annual", reference_date: "2025-12-31", version: 2, scope: "individual", column: "current" }],
    });
    setFundamentals([
      company({
        trailing: annual,
        latest_filing: { kind: "annual", reference_date: "2025-12-31", version: 2, scope: "individual", column: "current" },
        synced_at: null,
      }),
    ]);
    const { section } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText("Exercício encerrado em 31/12/2025")).toBeInTheDocument();
    expect(section).toHaveTextContent("Fonte: CVM, DFP de 31/12/2025.");
  });

  it("com o olho fechado, R$ sai mascarado e o percentual fica", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    setFundamentals([company(), fund()]);
    const { section, user } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText("ROE")).toBeInTheDocument();
    expect(value(section, "Receita líquida")).toHaveTextContent(MASK_BRL);
    expect(value(section, "Lucro líquido")).toHaveTextContent(MASK_BRL);
    expect(value(section, "LPA")).toHaveTextContent(MASK_BRL);
    expect(value(section, "VPA")).toHaveTextContent(MASK_BRL);
    expect(value(section, "ROE")).toHaveTextContent("15,0%");
    expect(section.textContent).not.toMatch(/R\$\s*\d/);
    for (const element of section.querySelectorAll("[title], [aria-label]")) {
      expect(element.getAttribute("title") ?? "").not.toMatch(/R\$\s*\d/);
      expect(element.getAttribute("aria-label") ?? "").not.toMatch(/R\$\s*\d/);
    }

    await user.keyboard("{Escape}");
    await user.click(await screen.findByRole("button", { name: /KNRI11/ }));
    const dialog = await screen.findByRole("dialog", { name: "KNRI11" });
    const fundSection = within(dialog).getByRole("region", { name: "Fundamentos" });
    expect(await within(fundSection).findByText("P/VP")).toBeInTheDocument();
    expect(value(fundSection, "Valor patrimonial por cota")).toHaveTextContent(MASK_BRL);
    expect(value(fundSection, "Patrimônio líquido")).toHaveTextContent(MASK_BRL);
    expect(value(fundSection, "P/VP")).toHaveTextContent("1,42");
    expect(fundSection.textContent).not.toMatch(/R\$\s*\d/);
  });

  it("banco: só lucro, ROE e anos com lucro; o resto não se aplica", async () => {
    const missing = { value: null, gap: "financial_template" as const };
    setFundamentals([
      company({
        template: "bank",
        trailing: samplePeriod({
          template: "bank",
          revenue: missing,
          net_margin: missing,
          net_debt_to_ebitda: missing,
          net_debt_to_equity: missing,
          roe: { value: null, gap: "non_positive_base" },
        }),
        positive_operating_cash_years: { positive: 0, known: 0, window: 5, gap: "financial_template" },
        revenue_cagr: missing,
        earnings_cagr: { value: null, gap: "non_positive_endpoint" },
        earnings_per_share: { value: null, gap: "no_shares" },
      }),
    ]);
    const { section } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText("Lucro líquido")).toBeInTheDocument();
    for (const label of ["Receita líquida", "Margem líquida", "Dív. líquida / EBITDA", "Dív. líquida / PL", "Receita, CAGR 5 anos", "Anos com caixa operacional positivo"]) {
      expect(within(section).queryByText(label)).toBeNull();
    }
    expect(section).toHaveTextContent("Banco: receita, dívida e EBITDA não se aplicam");
    // O número que falta é um traço com o motivo, nunca um zero.
    expect(value(section, "ROE")).toHaveTextContent("—");
    expect(within(value(section, "ROE")).getByText(GAP_TEXT.non_positive_base)).toHaveClass("sr-only");
    expect(within(value(section, "Lucro, CAGR 5 anos")).getByText(GAP_TEXT.non_positive_endpoint)).toBeInTheDocument();
    expect(within(value(section, "LPA")).getByText(GAP_TEXT.no_shares)).toBeInTheDocument();
  });

  it("seguradora e companhia sem história suficiente", async () => {
    setFundamentals([
      company({
        template: "insurer",
        trailing: null,
        profitable_years: { positive: 0, known: 0, window: 5, gap: "no_filing" },
        earnings_cagr: { value: null, gap: "insufficient_history" },
      }),
    ]);
    const { section } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText(/Seguradora: receita, dívida e EBITDA/)).toBeInTheDocument();
    expect(within(section).queryByText("ROE")).toBeNull();
    expect(within(value(section, "Anos com lucro")).getByText(GAP_TEXT.no_filing)).toBeInTheDocument();
  });

  it("avisa recuperação judicial e outra situação do emissor", async () => {
    setFundamentals([company({ judicial_recovery: true, issuer_status: "EM RECUPERAÇÃO JUDICIAL OU EQUIVALENTE" })]);
    const first = await openSection(/ITSA4/, "ITSA4");
    expect(await within(first.section).findByText(/Em recuperação judicial, segundo o cadastro da CVM/)).toBeInTheDocument();
  });

  it("mostra a situação do emissor quando não é a operacional", async () => {
    setFundamentals([company({ judicial_recovery: false, issuer_status: "FALIDA" })]);
    const { section } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText(/Situação na CVM: falida/)).toBeInTheDocument();
  });

  it("FII: valor patrimonial, P/VP com o preço ao vivo e vacância", async () => {
    setFundamentals([fund()]);
    const { section } = await openSection(/KNRI11/, "KNRI11");
    expect(await within(section).findByText("Valor patrimonial por cota")).toBeInTheDocument();
    expect(value(section, "Valor patrimonial por cota")).toHaveTextContent(/R\$\s100,00/);
    // 142 (o preço da carteira) ÷ 100, não o 0,9 da última leitura.
    expect(value(section, "P/VP")).toHaveTextContent("1,42");
    expect(value(section, "Vacância física")).toHaveTextContent("15,0%");
    expect(value(section, "Patrimônio líquido")).toHaveTextContent(/R\$\s1\sbi/);
    expect(section).toHaveTextContent("Fonte: CVM, informe mensal de 10/2026; vacância do trimestre encerrado em 30/09/2026 · conferido em 26/11/2026.");
  });

  it("FII de papel e sem cotas: o motivo no lugar do número", async () => {
    setFundamentals([
      fund({
        vacancy: { value: null, gap: "no_rental_property" },
        vacancy_reference_date: null,
        nav_per_share: { value: null, gap: "no_shares" },
        price_to_book: { value: null, gap: "no_shares" },
        net_assets: { value: null, gap: "not_reported" },
        synced_at: null,
      }),
    ]);
    const { section } = await openSection(/KNRI11/, "KNRI11");
    expect(await within(section).findByText("Vacância física")).toBeInTheDocument();
    expect(within(value(section, "Vacância física")).getByText(GAP_TEXT.no_rental_property)).toBeInTheDocument();
    expect(within(value(section, "P/VP")).getByText(GAP_TEXT.no_shares)).toBeInTheDocument();
    expect(within(value(section, "Valor patrimonial por cota")).getByText(GAP_TEXT.no_shares)).toBeInTheDocument();
    expect(section).toHaveTextContent("Fonte: CVM, informe mensal de 10/2026.");
  });

  it("sem fundamentos: diz por quê", async () => {
    setFundamentals([
      { asset_id: "a-itsa", symbol: "ITSA4", kind: "unsupported", company: null, fund: null, reason: "no_cvm_code" },
    ]);
    const { section, user } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText(UNSUPPORTED_TEXT.no_cvm_code)).toBeInTheDocument();
    await user.keyboard("{Escape}");
    // O FII sem entrada ainda está sendo buscado.
    await user.click(await screen.findByRole("button", { name: /KNRI11/ }));
    const dialog = await screen.findByRole("dialog", { name: "KNRI11" });
    expect(await within(dialog).findByText(UNSUPPORTED_TEXT.pending)).toBeInTheDocument();
  });

  it("cripto e renda fixa não têm a seção", async () => {
    const rendered = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await rendered.user.click(screen.getByRole("button", { name: /BTC/ }));
    const dialog = await screen.findByRole("dialog", { name: "BTC" });
    expect(within(dialog).queryByRole("region", { name: "Fundamentos" })).toBeNull();
  });

  it("erro do servidor: avisa sem quebrar a folha", async () => {
    server.use(http.get("/api/v1/fundamentals", () => HttpResponse.json({ code: "internal" }, { status: 500 })));
    const { section } = await openSection(/ITSA4/, "ITSA4");
    expect(await within(section).findByText("Não consegui carregar os fundamentos agora.")).toBeInTheDocument();
  });
});
