import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import type {
  B3AssetPlan,
  B3AssetGroup,
  B3CommitResult,
  B3FileReport,
  B3Plan,
  B3PreviewResponse,
} from "../../api/b3ImportTypes";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { MAX_FILES } from "./ImportB3Screen";
import { formatBytes, formatIsoDate, reasonText } from "./labels";

const PREVIEW_URL = "/api/v1/imports/b3/preview";
const COMMIT_URL = "/api/v1/imports/b3/commit";

const totals = {
  bought_quantity: 0,
  sold_quantity: 0,
  bought_value: 0,
  sold_value: 0,
  net_quantity: 0,
  income_received: 0,
  income_provisioned: 0,
};

const petr: B3AssetGroup = {
  key: "PETR4",
  name: "PETROLEO BRASILEIRO S.A. PETROBRAS",
  suggested_class: "br_stocks",
  class_basis: "name",
  listed_on_b3: true,
  treasury: false,
  totals: {
    ...totals,
    bought_quantity: 105,
    sold_quantity: 50,
    bought_value: 3150.5,
    sold_value: 1700,
    net_quantity: 55,
    income_received: 213.75,
    income_provisioned: 216,
  },
  trades: [
    {
      external_ref: "b3:v1:trade:aaa",
      date: "2026-01-05",
      side: "buy",
      market: "fractional",
      code: "PETR4",
      institution: "CORRETORA EXEMPLO S/A",
      quantity: 5,
      unit_price: 30.1,
      total: 150.5,
      line: 3,
    },
    {
      external_ref: "b3:v1:trade:bbb",
      date: "2026-02-20",
      side: "sell",
      market: "spot",
      code: "PETR4",
      institution: null,
      quantity: 50,
      unit_price: 34,
      total: 1700,
      line: 2,
    },
  ],
  events: [
    {
      external_ref: "b3:v1:event:ccc",
      date: "2026-03-20",
      kind: "bonus",
      direction: "credit",
      code: "PETR4",
      product: "PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS",
      quantity: 10,
      unit_price: null,
      value: null,
      institution: null,
      description: "Bonificação em Ativos",
      treasury: false,
      source: "movements",
      line: 6,
    },
  ],
  income: [
    {
      external_ref: "b3:v1:income:ddd",
      kind: "jcp",
      status: "received",
      date: "2025-12-18",
      code: "PETR4",
      product: "PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS",
      quantity: 150,
      unit_price: 0.5,
      net_amount: 63.75,
      institution: null,
      source: "movements",
      line: 3,
    },
    {
      external_ref: "b3:v1:income:eee",
      kind: "dividend",
      status: "provisioned",
      date: null,
      code: "PETR4",
      product: "PETR4 - PETROLEO BRASILEIRO S.A. PETROBRAS",
      quantity: null,
      unit_price: null,
      net_amount: 216,
      institution: null,
      source: "income_provisioned",
      line: 2,
    },
  ],
};

const bovv: B3AssetGroup = {
  key: "BOVV11",
  name: null,
  suggested_class: "br_reits",
  class_basis: "code",
  listed_on_b3: true,
  treasury: false,
  totals: { ...totals, bought_quantity: 5, bought_value: 500, net_quantity: 5 },
  trades: [
    {
      external_ref: "b3:v1:trade:fff",
      date: "2026-01-12",
      side: "buy",
      market: "spot",
      code: "BOVV11",
      institution: null,
      quantity: 5,
      unit_price: 100,
      total: 500,
      line: 4,
    },
  ],
  events: [],
  income: [],
};

const bond: B3AssetGroup = {
  key: "TESOURO SELIC 2029",
  name: "Tesouro Selic 2029",
  suggested_class: "fixed_income",
  class_basis: "treasury",
  listed_on_b3: false,
  treasury: true,
  totals,
  trades: [],
  events: [
    {
      external_ref: "b3:v1:event:ggg",
      date: "2026-04-02",
      kind: "treasury_movement",
      direction: "debit",
      code: null,
      product: "Tesouro Selic 2029",
      quantity: 1,
      unit_price: 15000,
      value: 15000,
      institution: null,
      description: "COMPRA / VENDA",
      treasury: true,
      source: "movements",
      line: 7,
    },
  ],
  income: [],
};


const counts = {
  new_transactions: 0,
  existing_transactions: 0,
  new_income: 0,
  replaced_income: 0,
  existing_income: 0,
  not_imported: 0,
};

function assetPlan(overrides: Partial<B3AssetPlan> & Pick<B3AssetPlan, "key">): B3AssetPlan {
  return {
    action: "create",
    asset_id: null,
    symbol: overrides.key,
    display_name: overrides.key,
    asset_class: "br_stocks",
    class_basis: "name",
    listed_on_b3: true,
    treasury: false,
    class_editable: true,
    class_needs_confirmation: false,
    current_quantity: 0,
    final_quantity: 0,
    opening: null,
    replaced: [],
    conflict: null,
    counts,
    ...overrides,
  };
}

const planSummary = {
  assets_created: 2,
  assets_updated: 1,
  assets_skipped: 0,
  transactions_new: 4,
  transactions_existing: 1,
  income_new: 1,
  income_replaced: 0,
  income_existing: 0,
  openings: 1,
  not_imported: 1,
  conflicts: 0,
  needs_confirmation: 1,
};

/** BOVV11 pede a classe; PETR4 já está na carteira e ganha um saldo inicial. */
function plan(overrides: Partial<B3Plan> = {}): B3Plan {
  return {
    digest: "d1",
    ready: false,
    assets: [
      assetPlan({
        key: "BOVV11",
        asset_class: "br_reits",
        class_basis: "code",
        class_needs_confirmation: true,
        final_quantity: 5,
      }),
      assetPlan({
        key: "PETR4",
        action: "update",
        asset_id: "a-petr",
        display_name: "Petrobras",
        class_editable: false,
        current_quantity: 120,
        final_quantity: 127,
        opening: {
          date: "2026-01-04",
          quantity: 72,
          unit_price: null,
          suggested_quantity: 72,
          suggested_unit_price: null,
          basis: "keeps_quantity",
          chosen: false,
        },
        replaced: [{ kind: "opening", date: "2026-05-01", quantity: 120, unit_price: null }],
      }),
      assetPlan({
        key: "TESOURO SELIC 2029",
        symbol: "TD-SELIC-20290301",
        display_name: "Tesouro Selic 2029",
        asset_class: "fixed_income",
        class_basis: "treasury",
        listed_on_b3: false,
        treasury: true,
        class_editable: false,
        final_quantity: 1,
      }),
    ],
    lines: {
      "b3:v1:trade:aaa": { status: "new", reason_code: null },
      "b3:v1:trade:bbb": { status: "existing", reason_code: "same_entry" },
      "b3:v1:event:ccc": { status: "new", reason_code: null },
      "b3:v1:income:ddd": { status: "new", reason_code: "replaces_provisioned" },
      "b3:v1:income:eee": { status: "not_imported", reason_code: "provisioned_without_date" },
      "b3:v1:trade:fff": { status: "new", reason_code: null },
      "b3:v1:event:ggg": { status: "new", reason_code: null },
    },
    summary: planSummary,
    ...overrides,
  };
}

/** O mesmo plano, com a classe do BOVV11 confirmada. */
function readyPlan(): B3Plan {
  const base = plan();
  return {
    ...base,
    digest: "d2",
    ready: true,
    assets: base.assets.map((asset) =>
      asset.key === "BOVV11" ? { ...asset, class_needs_confirmation: false } : asset,
    ),
    summary: { ...planSummary, needs_confirmation: 0 },
  };
}

function report(overrides: Partial<B3FileReport> & Pick<B3FileReport, "name">): B3FileReport {
  return {
    index: 0,
    status: "read",
    kinds: ["trades"],
    problem: null,
    message: null,
    missing_columns: [],
    rows: 5,
    accepted: 4,
    unrecognized: 1,
    ignored: 0,
    ...overrides,
  };
}

function preview(overrides: Partial<B3PreviewResponse> = {}): B3PreviewResponse {
  return {
    plan: plan(),
    files: [report({ name: "negociacao.xlsx" })],
    assets: [bovv, petr, bond],
    unrecognized: [
      {
        file: 0,
        sheet: "Negociação",
        line: 6,
        reason_code: "unsupported_market",
        reason: "mercado ainda nao suportado",
        text: "05/03/2026 | Compra | Opção de Compra sobre Ações | BOVAC130",
      },
      {
        file: 0,
        sheet: "Negociação",
        line: 9,
        reason_code: "codigo_novo",
        reason: "motivo do servidor",
        text: "linha estranha",
      },
    ],
    ignored: [
      { reason_code: "settlement", reason: "x", count: 3 },
      { reason_code: "duplicate_in_request", reason: "x", count: 1 },
    ],
    summary: {
      assets: 3,
      trades: 3,
      events: 2,
      income: 2,
      unrecognized: 2,
      ignored: 4,
      first_date: "2025-12-18",
      last_date: "2026-04-02",
    },
    ...overrides,
  };
}

const xlsx = (name: string, size = 2048) => {
  const file = new File([new Uint8Array(8)], name, {
    type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    lastModified: 1_700_000_000_000,
  });
  Object.defineProperty(file, "size", { value: size });
  return file;
};

async function openImport() {
  const rendered = renderApp("ajustes/importar-b3");
  await screen.findByRole("heading", { name: "Importar da B3" });
  return rendered;
}

/** O item da lista de arquivos com este nome. */
const fileItem = (name: string): HTMLElement => {
  const list = screen.getByRole("list", { name: "Arquivos escolhidos" });
  const found = within(list)
    .getAllByRole("listitem")
    .find((item) => item.querySelector(".import-file__name")?.textContent === name);
  if (!found) throw new Error(`arquivo ${name} fora da lista`);
  return found;
};

const fileInput = () => screen.getByLabelText<HTMLInputElement>("Escolher planilhas da B3");

describe("Importar da B3", () => {
  it("é uma seção de Ajustes e explica como baixar as planilhas, sem imagem de terceiros", async () => {
    const { user } = await openImport();
    expect(document.title).toBe("Ajustes · Prumo");
    const sections = screen.getByRole("navigation", { name: "Seções de Ajustes" });
    expect(within(sections).getByRole("link", { name: "Importar da B3" })).toHaveAttribute("aria-current", "page");
    const how = screen.getByRole("region", { name: "Como baixar as planilhas" });
    expect(within(how).getByText(/investidor\.b3\.com\.br/)).toBeInTheDocument();
    expect(within(how).getAllByRole("listitem").length).toBeGreaterThanOrEqual(4);
    expect(how.querySelector("img")).toBeNull();
    expect(screen.getByRole("button", { name: "Ler planilhas" })).toBeDisabled();
    expect(screen.getByText("Escolha ao menos uma planilha .xlsx.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Importar" })).toBeNull();

    await user.click(within(sections).getByRole("link", { name: "Metas por classe" }));
    await screen.findByRole("heading", { name: "Quanto você quer em cada classe?" });
    await user.click(within(sections).getByRole("link", { name: "Importar da B3" }));
    expect(await screen.findByRole("heading", { name: "Importar da B3" })).toBeInTheDocument();
  });

  it("com o olho fechado, a prévia não mostra quantidade nem valor, nem para o leitor de tela", async () => {
    server.use(http.post(PREVIEW_URL, () => HttpResponse.json(preview())));
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("negociacao.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    await screen.findByRole("region", { name: "O que entraria" });
    await user.click(screen.getByRole("button", { name: "Ver 5 linhas de PETR4" }));
    expect(document.body).toHaveTextContent("R$ 3.150,50");

    await user.click(screen.getByRole("button", { name: "Ocultar valores" }));

    const summary = screen.getByRole("region", { name: "O que entraria" });
    const unrecognized = screen.getByRole("region", { name: /Linhas que não entendi/ });
    const heard: string[] = [];
    for (const root of [summary, unrecognized]) {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) heard.push(node.textContent ?? "");
      for (const element of root.querySelectorAll("[aria-label],[title]")) {
        heard.push(element.getAttribute("aria-label") ?? "", element.getAttribute("title") ?? "");
      }
    }
    const text = heard.join(" | ");
    // Nenhum R$ com número, nenhuma quantidade, nenhuma linha crua da planilha.
    expect(text).not.toMatch(/R\$\s*\d/);
    for (const leak of ["3.150,50", "1.700,00", "213,75", "216,00", "15.000,00", "63,75", "150,50", "105", "Opção de Compra sobre Ações"]) {
      expect(text).not.toContain(leak);
    }
    expect(text).toContain("R$ ••••");
    expect(text).toContain("Conteúdo da linha oculto.");
    // O que não é valor segue legível: ativo, classe, datas e tipos.
    expect(within(summary).getByText("PETR4")).toBeInTheDocument();
    expect(within(summary).getByText(/05\/01\/2026 · Compra \(fracionário\)/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Ocultar valores" }));
    expect(document.body).toHaveTextContent("R$ 3.150,50");
  });

  it("envia as planilhas em multipart e mostra a prévia agrupada por ativo", async () => {
    let sent: { names: string[]; contentType: string | null } | null = null;
    server.use(
      http.post(PREVIEW_URL, async ({ request }) => {
        const form = await request.formData();
        sent = {
          names: form.getAll("files").map((value) => (value as File).name),
          contentType: request.headers.get("content-type"),
        };
        return HttpResponse.json(preview());
      }),
    );
    const { user } = await openImport();
    await user.upload(fileInput(), [xlsx("negociacao.xlsx"), xlsx("movimentacao.xlsx")]);
    expect(screen.getByText("2 arquivos · 4 KB")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));

    const summary = await screen.findByRole("region", { name: "O que entraria" });
    expect(sent).toEqual({
      names: ["negociacao.xlsx", "movimentacao.xlsx"],
      contentType: expect.stringContaining("multipart/form-data; boundary=") as string,
    });
    expect(within(summary).getByText("De 18/12/2025 a 02/04/2026.")).toBeInTheDocument();
    const stat = (label: string) => within(summary).getByText(label).nextElementSibling;
    expect(stat("Ativos")).toHaveTextContent("3");
    expect(stat("Negócios")).toHaveTextContent("3");
    expect(stat("Não entendidas")).toHaveTextContent("2");

    const table = within(summary).getByRole("table");
    const rows = within(table).getAllByRole("row");
    const [, bovvRow, petrRow, bondRow] = rows as [HTMLElement, HTMLElement, HTMLElement, HTMLElement];
    // Cabeçalho + 3 ativos, na ordem que o servidor mandou.
    expect(rows).toHaveLength(4);
    expect(within(petrRow).getByText("PETR4")).toBeInTheDocument();
    expect(within(petrRow).getByText("Ações Nacionais")).toBeInTheDocument();
    // Já está na carteira: vale a classe dela.
    expect(within(petrRow).getByText("a classe do ativo na carteira")).toBeInTheDocument();
    expect(petrRow).toHaveTextContent("já está na carteira");
    expect(petrRow).toHaveTextContent("105 · R$ 3.150,50");
    expect(petrRow).toHaveTextContent("50 · R$ 1.700,00");
    expect(petrRow).toHaveTextContent("R$ 213,75");
    expect(petrRow).toHaveTextContent("R$ 216,00");
    // Só o código: avisa para conferir.
    expect(within(bovvRow).getByText("só pelo código — confira")).toBeInTheDocument();
    expect(within(bovvRow).getByRole("combobox", { name: "Classe de BOVV11" })).toHaveValue("br_reits");
    expect(within(bovvRow).getByText("confira a classe")).toBeInTheDocument();
    // Tesouro: renda fixa, marcado.
    expect(within(bondRow).getByText("Tesouro Direto")).toBeInTheDocument();
    expect(within(bondRow).getByText("Renda Fixa")).toBeInTheDocument();
    expect(within(bondRow).getByText(/o preço vem da frente do Tesouro/)).toBeInTheDocument();

    // O arquivo mostra o que foi lido.
    expect(screen.getByText(/Negociação · 5 linhas · 4 entram · 1 não entendida · 0 deixadas de fora/)).toBeInTheDocument();
  });

  it("o botão Importar espera a classe ser confirmada", async () => {
    server.use(http.post(PREVIEW_URL, () => HttpResponse.json(preview())));
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("negociacao.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    const button = await screen.findByRole("button", { name: "Importar" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/Resolva os conflitos e confirme as classes/);
    const stat = (label: string) =>
      within(screen.getByLabelText("Contra a carteira")).getByText(label).nextElementSibling;
    expect(stat("Entram")).toHaveTextContent("5");
    expect(stat("Já existem")).toHaveTextContent("1");
    expect(stat("Ficam de fora")).toHaveTextContent("1");
  });

  it("expande as linhas de um ativo", async () => {
    server.use(http.post(PREVIEW_URL, () => HttpResponse.json(preview())));
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("negociacao.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    await screen.findByRole("region", { name: "O que entraria" });

    const open = screen.getByRole("button", { name: "Ver 5 linhas de PETR4" });
    expect(open).toHaveAttribute("aria-expanded", "false");
    await user.click(open);
    const lines = screen.getByRole("list", { name: "Linhas de PETR4" });
    expect(within(lines).getByText(/05\/01\/2026 · Compra \(fracionário\) · 5 × R\$ 30,10 = R\$ 150,50/)).toBeInTheDocument();
    expect(within(lines).getByText(/20\/02\/2026 · Venda · 50 × R\$ 34,00 = R\$ 1\.700,00/)).toBeInTheDocument();
    expect(within(lines).getByText(/20\/03\/2026 · Bonificação \(entrada\) · 10/)).toBeInTheDocument();
    expect(within(lines).getByText(/18\/12\/2025 · JCP \(líquido\) · recebido · R\$ 63,75/)).toBeInTheDocument();
    expect(within(lines).getByText(/sem data · Dividendo · a receber · R\$ 216,00/)).toBeInTheDocument();
    // O destino de cada linha contra a carteira.
    expect(within(lines).getByText(/— já existe: é o lançamento que você já fez/)).toBeInTheDocument();
    expect(within(lines).getByText(/— entra: substitui o provento a receber/)).toBeInTheDocument();
    expect(within(lines).getByText(/— fica de fora: a receber sem data/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Ver 1 linha de Tesouro Direto" }));
    expect(screen.getByText(/Tesouro Direto \(saída\) · 1 · R\$ 15\.000,00/)).toBeInTheDocument();
    // Abrir outro fecha o anterior.
    expect(screen.queryByRole("list", { name: "Linhas de PETR4" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Esconder linhas de Tesouro Direto" }));
    expect(screen.queryByRole("list", { name: "Linhas de Tesouro Direto" })).toBeNull();
  });

  it("lista o que não foi entendido e o que ficou de fora, com o motivo em português", async () => {
    server.use(http.post(PREVIEW_URL, () => HttpResponse.json(preview())));
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("negociacao.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));

    const unrecognized = await screen.findByRole("region", { name: "Linhas que não entendi (2)" });
    expect(
      within(unrecognized).getByText(/negociacao\.xlsx · linha 6: mercado que o Prumo ainda não suporta/),
    ).toBeInTheDocument();
    expect(within(unrecognized).getByText(/Opção de Compra sobre Ações/)).toBeInTheDocument();
    // Motivo que a tela não conhece: cai no texto do servidor.
    expect(within(unrecognized).getByText(/linha 9: motivo do servidor/)).toBeInTheDocument();

    const ignored = screen.getByRole("region", { name: "Linhas deixadas de fora de propósito" });
    expect(within(ignored).getByText("3 linhas: compra ou venda liquidada: vem da planilha de Negociação")).toBeInTheDocument();
    expect(within(ignored).getByText("1 linha: já veio em outra planilha enviada")).toBeInTheDocument();
  });

  it("arquivo que não foi lido diz por quê, sem derrubar o resto", async () => {
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json(
          preview({
            files: [
              report({ name: "ok.xlsx", index: 0 }),
              report({ name: "estranha.xlsx", index: 1, status: "unknown_format", problem: "unknown_format", kinds: [] }),
              report({
                name: "layout.xlsx",
                index: 2,
                status: "unknown_format",
                problem: "missing_columns",
                missing_columns: ["Mercado", "Preco"],
                kinds: [],
              }),
              report({ name: "quebrada.xlsx", index: 3, status: "unreadable", problem: "corrupt", kinds: [] }),
              report({ name: "outra.xlsx", index: 4, status: "unreadable", problem: null, kinds: [] }),
            ],
          }),
        ),
      ),
    );
    const { user } = await openImport();
    await user.upload(fileInput(), ["ok.xlsx", "estranha.xlsx", "layout.xlsx", "quebrada.xlsx", "outra.xlsx"].map((n) => xlsx(n)));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    await screen.findByRole("region", { name: "O que entraria" });

    const item = fileItem;
    expect(within(item("estranha.xlsx")).getByRole("alert")).toHaveTextContent(/Não reconheci este formato/);
    expect(within(item("layout.xlsx")).getByRole("alert")).toHaveTextContent(
      "Parece uma planilha da B3, mas faltam colunas. A B3 pode ter mudado o layout. Faltam: Mercado, Preco.",
    );
    expect(within(item("quebrada.xlsx")).getByRole("alert")).toHaveTextContent(/corrompida ou cortada/);
    expect(within(item("outra.xlsx")).getByRole("alert")).toHaveTextContent("Não consegui ler este arquivo.");
    expect(within(item("ok.xlsx")).queryByRole("alert")).toBeNull();
  });

  it("sem nada aproveitável, avisa em vez de mostrar tabela vazia", async () => {
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json(
          preview({
            files: [report({ name: "estranha.xlsx", status: "unknown_format", problem: "unknown_format", kinds: [] })],
            assets: [],
            unrecognized: [],
            ignored: [],
            summary: { assets: 0, trades: 0, events: 0, income: 0, unrecognized: 0, ignored: 0, first_date: null, last_date: null },
          }),
        ),
      ),
    );
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("estranha.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    expect(await screen.findByText(/Não achei nenhuma linha que o Prumo consiga importar/)).toBeInTheDocument();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByRole("region", { name: /Linhas que não entendi/ })).toBeNull();
  });

  it("erro do servidor vira aviso e a lista de arquivos fica", async () => {
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json(
          { type: "urn:prumo:error:payload_too_large", title: "Envio grande demais", status: 413, code: "payload_too_large" },
          { status: 413, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("negociacao.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    expect(await screen.findByText(/Os arquivos passam de 10 MB juntos/)).toBeInTheDocument();
    expect(fileItem("negociacao.xlsx")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ler planilhas" })).toBeEnabled();
  });

  it("arquivo que quebrou o leitor no servidor vira aviso, sem derrubar a tela", async () => {
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json(
          {
            type: "urn:prumo:error:unreadable_file",
            title: "Arquivo ilegivel",
            status: 422,
            code: "unreadable_file",
            detail: "Nao consegui ler este arquivo (quebrada.xlsx).",
          },
          { status: 422, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("quebrada.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    expect(await screen.findByText(/Não consegui ler um dos arquivos/)).toBeInTheDocument();
    expect(fileItem("quebrada.xlsx")).toBeInTheDocument();
  });

  it("413 sem corpo também vira o aviso de tamanho", async () => {
    server.use(http.post(PREVIEW_URL, () => new HttpResponse(null, { status: 413 })));
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("negociacao.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    expect(await screen.findByText(/Os arquivos passam de 10 MB juntos/)).toBeInTheDocument();
  });

  it("tira um arquivo da lista, não repete o mesmo e limita a quantidade", async () => {
    const { user } = await openImport();
    await user.upload(fileInput(), [xlsx("a.xlsx"), xlsx("b.xlsx")]);
    await user.upload(fileInput(), xlsx("a.xlsx"));
    expect(within(screen.getByRole("list", { name: "Arquivos escolhidos" })).getAllByRole("listitem")).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "Tirar a.xlsx" }));
    const list = screen.getByRole("list", { name: "Arquivos escolhidos" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
    expect(screen.getByText("1 arquivo · 2 KB")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tirar b.xlsx" }));
    expect(screen.queryByRole("list", { name: "Arquivos escolhidos" })).toBeNull();

    const many = Array.from({ length: MAX_FILES + 2 }, (_, n) => xlsx(`planilha-${n}.xlsx`));
    await user.upload(fileInput(), many);
    expect(screen.getByText(`Máximo de ${MAX_FILES} planilhas por vez.`)).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "Arquivos escolhidos" })).getAllByRole("listitem")).toHaveLength(MAX_FILES);
  });

  it("não deixa enviar mais de 10 MB e avisa antes de subir", async () => {
    const { user } = await openImport();
    await user.upload(fileInput(), [xlsx("grande.xlsx", 6 * 1024 * 1024), xlsx("outra.xlsx", 5 * 1024 * 1024)]);
    expect(screen.getByRole("alert")).toHaveTextContent(/passam de 10 MB juntos/);
    expect(screen.getByRole("button", { name: "Ler planilhas" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Tirar outra.xlsx" }));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("button", { name: "Ler planilhas" })).toBeEnabled();
  });

  it("escolher outro arquivo apaga a prévia antiga", async () => {
    server.use(http.post(PREVIEW_URL, () => HttpResponse.json(preview())));
    const { user } = await openImport();
    await user.upload(fileInput(), xlsx("a.xlsx"));
    await user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    await screen.findByRole("region", { name: "O que entraria" });
    await user.upload(fileInput(), xlsx("b.xlsx"));
    await waitFor(() => expect(screen.queryByRole("region", { name: "O que entraria" })).toBeNull());
  });

  it("ignora a escolha vazia do seletor", async () => {
    await openImport();
    fireEvent.change(fileInput(), { target: { files: [] } });
    expect(screen.queryByRole("list", { name: "Arquivos escolhidos" })).toBeNull();
  });
});

describe("Importar da B3: gravar", () => {
  type Sent = { decisions: unknown; names: string[] };

  const stringOf = (value: FormDataEntryValue | null) => (typeof value === "string" ? value : "null");

  /** Guarda o que cada chamada mandou: as decisões (JSON) e os nomes dos arquivos. */
  async function captured(request: Request): Promise<Sent> {
    const form = await request.formData();
    return {
      decisions: JSON.parse(stringOf(form.get("decisions"))) as unknown,
      names: form.getAll("files").map((value) => (value as File).name),
    };
  }

  const done: B3CommitResult = {
    summary: { ...planSummary, needs_confirmation: 0, income_replaced: 1 },
    not_imported: [
      { key: "PETR4", external_ref: "b3:v1:income:eee", reason_code: "provisioned_without_date" },
      { key: "PETR4", external_ref: "b3:v1:event:x", reason_code: "fraction_in_kind" },
    ],
    skipped_assets: [],
    unrecognized: 2,
    ignored: 4,
  };

  async function readSheets() {
    const rendered = await openImport();
    await rendered.user.upload(fileInput(), [xlsx("negociacao.xlsx"), xlsx("movimentacao.xlsx")]);
    await rendered.user.click(screen.getByRole("button", { name: "Ler planilhas" }));
    await screen.findByRole("region", { name: "Conferir antes de importar" });
    return rendered;
  }

  it("confirma a classe, atualiza a prévia e importa com o digest dela, reenviando os arquivos", async () => {
    const previews: Sent[] = [];
    const commits: Sent[] = [];
    server.use(
      http.post(PREVIEW_URL, async ({ request }) => {
        previews.push(await captured(request));
        return HttpResponse.json(preview({ plan: previews.length === 1 ? plan() : readyPlan() }));
      }),
      http.post(COMMIT_URL, async ({ request }) => {
        commits.push(await captured(request));
        return HttpResponse.json(done);
      }),
    );
    const { user, client } = await readSheets();
    expect(previews[0]?.decisions).toEqual({ assets: {} });
    const review = screen.getByRole("region", { name: "Conferir antes de importar" });
    const bovv = within(review).getByRole("listitem", { name: "BOVV11" });
    expect(within(bovv).getByText(/A classe veio só pelo código: Fundos Imobiliários/)).toBeInTheDocument();
    await user.click(within(bovv).getByRole("button", { name: "Confirmar Fundos Imobiliários para BOVV11" }));

    // A escolha mudou: a prévia precisa ser refeita antes de importar.
    expect(screen.getByRole("button", { name: "Importar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Importar" })).toHaveAccessibleDescription(/atualize a prévia/);
    await user.click(screen.getByRole("button", { name: "Atualizar prévia" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Importar" })).toBeEnabled());
    expect(previews[1]?.decisions).toEqual({ assets: { BOVV11: { asset_class: "br_reits" } } });
    expect(screen.getByRole("button", { name: "Importar" })).toHaveAccessibleDescription(
      /Grava 4 lançamentos e 1 provento, tudo de uma vez/,
    );

    const invalidated = vi.spyOn(client, "invalidateQueries");
    await user.click(screen.getByRole("button", { name: "Importar" }));
    const result = await screen.findByRole("region", { name: "Importação concluída" });
    expect(commits).toHaveLength(1);
    expect(commits[0]).toEqual({
      decisions: { assets: { BOVV11: { asset_class: "br_reits" } }, digest: "d2" },
      names: ["negociacao.xlsx", "movimentacao.xlsx"],
    });
    expect(within(result).getByRole("status")).toHaveTextContent(
      "Entraram 4 lançamentos e 2 proventos (1 substituiu um provento a receber).",
    );
    expect(within(result).getByText("1 linha: a receber sem data: entra quando a B3 marcar a data")).toBeInTheDocument();
    expect(within(result).getByText(/1 linha: fração de ação/)).toBeInTheDocument();
    expect(within(result).getByText(/2 linhas não entendidas e 4 deixadas de fora/)).toBeInTheDocument();
    // A prévia some: o que ela mostrava já está gravado.
    expect(screen.queryByRole("region", { name: "O que entraria" })).toBeNull();
    // Carteira, livro, renda e desempenho são lidos de novo.
    const refreshed = invalidated.mock.calls.map(([filters]) => JSON.stringify(filters?.queryKey));
    for (const key of [["portfolio"], ["transactions"], ["income"], ["cash"], ["net-worth"], ["performance"]]) {
      expect(refreshed).toContain(JSON.stringify(key));
    }
  });

  it("o saldo inicial e o deixar de fora viram decisões; o conflito diz o que fazer", async () => {
    const previews: Sent[] = [];
    server.use(
      http.post(PREVIEW_URL, async ({ request }) => {
        previews.push(await captured(request));
        const base = plan();
        const conflicted: B3Plan = {
          ...base,
          assets: base.assets.map((asset) =>
            asset.key === "PETR4"
              ? { ...asset, conflict: { reason_code: "exceeds_position", detail: "venda passa da posicao de 3" } }
              : asset,
          ),
        };
        return HttpResponse.json(preview({ plan: previews.length === 1 ? base : conflicted }));
      }),
    );
    const { user } = await readSheets();
    const review = screen.getByRole("region", { name: "Conferir antes de importar" });
    const petr = within(review).getByRole("listitem", { name: "PETR4" });
    expect(within(petr).getByText(/Saldo inicial em 04\/01\/2026: 72 a custo desconhecido/)).toBeInTheDocument();
    expect(within(petr).getByText(/Mantém a quantidade que você tinha lançado/)).toBeInTheDocument();
    expect(within(petr).getByText("Saldo inicial de 120 em 01/05/2026")).toBeInTheDocument();
    expect(within(petr).getByText(/Na carteira hoje: 120 · depois de importar: 127/)).toBeInTheDocument();

    const quantity = within(petr).getByLabelText("Saldo inicial de PETR4");
    await user.clear(quantity);
    await user.type(quantity, "3");
    await user.type(within(petr).getByLabelText("Preço médio de PETR4 (zero se não souber)"), "2550");
    await user.click(screen.getByRole("button", { name: "Atualizar prévia" }));
    await waitFor(() => expect(previews).toHaveLength(2));
    expect(previews[1]?.decisions).toEqual({ assets: { PETR4: { opening: { quantity: 3, unit_price: 25.5 } } } });
    const alert = await within(screen.getByRole("listitem", { name: "PETR4" })).findByRole("alert");
    expect(alert).toHaveTextContent(/Aumente o saldo inicial ou deixe o ativo de fora/);
    // O texto do servidor (com quantidade) não aparece.
    expect(alert).not.toHaveTextContent("3");

    const skip = within(screen.getByRole("listitem", { name: "PETR4" })).getByRole("switch", {
      name: "Deixar PETR4 de fora desta importação",
    });
    await user.click(skip);
    await user.click(screen.getByRole("button", { name: "Atualizar prévia" }));
    await waitFor(() => expect(previews).toHaveLength(3));
    expect(previews[2]?.decisions).toEqual({
      assets: { PETR4: { opening: { quantity: 3, unit_price: 25.5 }, skip: true } },
    });
  });

  it("prévia vencida: avisa, lê de novo e não grava", async () => {
    let previews = 0;
    server.use(
      http.post(PREVIEW_URL, () => {
        previews += 1;
        return HttpResponse.json(preview({ plan: readyPlan() }));
      }),
      http.post(COMMIT_URL, () =>
        HttpResponse.json(
          { type: "urn:prumo:error:preview_stale", title: "A previa mudou", status: 409, code: "preview_stale" },
          { status: 409, headers: { "Content-Type": "application/problem+json" } },
        ),
      ),
    );
    const { user } = await readSheets();
    await user.click(screen.getByRole("button", { name: "Importar" }));
    expect(await screen.findByText(/A carteira mudou desde a prévia/)).toBeInTheDocument();
    await waitFor(() => expect(previews).toBe(2));
    expect(screen.queryByRole("region", { name: "Importação concluída" })).toBeNull();
    expect(screen.getByRole("region", { name: "O que entraria" })).toBeInTheDocument();
  });

  it("nada novo: o Importar fica desligado e diz por quê", async () => {
    const nothingNew = readyPlan();
    nothingNew.summary = { ...nothingNew.summary, transactions_new: 0, income_new: 0, income_replaced: 0, openings: 0 };
    server.use(http.post(PREVIEW_URL, () => HttpResponse.json(preview({ plan: nothingNew }))));
    await readSheets();
    const button = screen.getByRole("button", { name: "Importar" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription("Nada novo: tudo já está na carteira.");
  });

  it("com o olho fechado, a conferência e o resultado não mostram quantidade nem valor", async () => {
    server.use(
      http.post(PREVIEW_URL, () => HttpResponse.json(preview({ plan: readyPlan() }))),
      http.post(COMMIT_URL, () => HttpResponse.json(done)),
    );
    const { user } = await readSheets();
    await user.click(screen.getByRole("button", { name: "Ocultar valores" }));

    const heard = (root: HTMLElement) => {
      const parts: string[] = [];
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) parts.push(node.textContent ?? "");
      for (const element of root.querySelectorAll("[aria-label],[title]")) {
        parts.push(element.getAttribute("aria-label") ?? "", element.getAttribute("title") ?? "");
      }
      for (const input of root.querySelectorAll("input")) parts.push(input.value);
      return parts.join(" | ");
    };
    const review = heard(screen.getByRole("region", { name: "Conferir antes de importar" }));
    for (const leak of ["72", "120", "127"]) expect(review).not.toContain(leak);
    expect(review).toContain("••");
    expect(review).toContain("R$ ••••");

    await user.click(screen.getByRole("button", { name: "Importar" }));
    const result = heard(await screen.findByRole("region", { name: "Importação concluída" }));
    expect(result).not.toMatch(/R\$\s*\d/);
  });
});

describe("textos da importação", () => {
  it("formata data, tamanho e motivo", () => {
    expect(formatIsoDate("2026-03-05")).toBe("05/03/2026");
    expect(formatIsoDate(null)).toBe("sem data");
    expect(formatIsoDate("lixo")).toBe("lixo");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3 MB");
    expect(reasonText("settlement", "x")).toBe("compra ou venda liquidada: vem da planilha de Negociação");
    expect(reasonText("novo", "do servidor")).toBe("do servidor");
  });
});
