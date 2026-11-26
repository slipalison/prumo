import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DataTable, StatusPill, type DataColumn } from "./DataTable";
import { Disclosure } from "./Disclosure";
import { InfoTip } from "./InfoTip";
import { Grid, Page, PageHeader, Panel, StatStrip } from "./Layout";
import { Sheet } from "./Sheet";
import { placeBubble } from "../lib/placeBubble";
import { Toggletip } from "./Toggletip";

afterEach(() => vi.restoreAllMocks());

describe("Page e PageHeader", () => {
  it("a página se declara larga e o cabeçalho tem um h1 e a explicação no ⓘ", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <Page>
        <PageHeader
          eyebrow="Renda"
          title="Quanto sua carteira paga?"
          summary="20% da meta"
          info="Dividendos, JCP e rendimentos."
          actions={<button type="button">Lançar provento</button>}
        />
      </Page>,
    );
    expect(container.firstElementChild).toHaveClass("page");
    expect(screen.getByRole("heading", { level: 1, name: "Quanto sua carteira paga?" })).toBeInTheDocument();
    expect(screen.getByText("Renda")).toHaveClass("eyebrow");
    expect(screen.getByText("20% da meta")).toBeInTheDocument();
    // A explicação não está na tela até o toque; o nome do ⓘ diz o que ele é.
    expect(screen.queryByText("Dividendos, JCP e rendimentos.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sobre esta tela" }));
    expect(screen.getByRole("status")).toHaveTextContent("Dividendos, JCP e rendimentos.");
    expect(screen.getByRole("button", { name: "Lançar provento" })).toBeInTheDocument();
  });

  it("sem explicação, sem ⓘ; o título pode levar id", () => {
    render(<PageHeader title="Carteira" titleId="t1" />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveAttribute("id", "t1");
  });
});

describe("Grid", () => {
  it("diz o desenho por classe e passa a largura mínima no modo automático", () => {
    const { container, rerender } = render(
      <Grid cols={3} as="section" aria-label="Painéis">
        <div />
      </Grid>,
    );
    const grid = container.firstElementChild as HTMLElement;
    expect(grid.tagName).toBe("SECTION");
    expect(grid).toHaveClass("grid", "grid--3");
    expect(grid).not.toHaveClass("grid--start");
    rerender(
      <Grid cols="auto" minWidth="220px" alignStart style={{ marginTop: 4 }}>
        <div />
      </Grid>,
    );
    const auto = container.firstElementChild as HTMLElement;
    expect(auto).toHaveClass("grid--auto", "grid--start");
    expect(auto.style.getPropertyValue("--grid-min")).toBe("220px");
    expect(auto.style.marginTop).toBe("4px");
  });

  it("o padrão é de duas colunas", () => {
    const { container } = render(<Grid>conteúdo</Grid>);
    expect(container.firstElementChild).toHaveClass("grid--2");
  });
});

describe("Panel", () => {
  it("é uma região nomeada pelo título, com ⓘ e ação", async () => {
    const user = userEvent.setup();
    render(
      <Panel title="Mês a mês" info="Recebido cheio, a receber listrado." actions={<a href="#/renda">Ver tudo</a>}>
        <p>corpo</p>
      </Panel>,
    );
    const region = screen.getByRole("region", { name: "Mês a mês" });
    expect(region).toHaveClass("glass", "panel");
    expect(within(region).getByRole("heading", { level: 2, name: "Mês a mês" })).toBeInTheDocument();
    expect(within(region).getByRole("link", { name: "Ver tudo" })).toBeInTheDocument();
    await user.click(within(region).getByRole("button", { name: "Sobre Mês a mês" }));
    expect(within(region).getByRole("status")).toHaveTextContent("Recebido cheio, a receber listrado.");
  });

  it("compacto, sem recheio e com h3; sem título não tem cabeçalho", () => {
    const { container, rerender } = render(
      <Panel title={<span>Rico</span>} titleAs="h3" compact flush spotlight={false} info="dica" as="article">
        x
      </Panel>,
    );
    const panel = container.firstElementChild as HTMLElement;
    expect(panel.tagName).toBe("ARTICLE");
    expect(panel).toHaveClass("panel--compact", "panel--flush");
    expect(panel).not.toHaveClass("glass--spotlight");
    expect(screen.getByRole("heading", { level: 3 })).toHaveTextContent("Rico");
    expect(screen.getByRole("button", { name: "Sobre este painel" })).toBeInTheDocument();
    rerender(
      <Panel aria-label="Sem título">
        <p>só corpo</p>
      </Panel>,
    );
    expect(container.querySelector(".panel__head")).toBeNull();
    expect(screen.getByRole("region", { name: "Sem título" })).toBeInTheDocument();
  });
});

describe("StatStrip", () => {
  it("cada número é um par rótulo/valor, com detalhe, tom e ⓘ", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <StatStrip
        inset
        items={[
          { label: "Recebido em 12 meses", value: "R$ 1.175,00", tone: "gold", info: "Líquido de IR." },
          { id: "media", label: "Média por mês", value: "R$ 97,92", detail: "dos últimos 12" },
        ]}
      />,
    );
    const list = container.querySelector("dl") as HTMLElement;
    expect(list).toHaveClass("stat-strip", "stat-strip--inset");
    const terms = within(list).getAllByRole("term");
    expect(terms.map((t) => t.textContent)).toEqual(["Recebido em 12 meses", "Média por mês"]);
    expect(within(list).getAllByRole("definition").map((d) => d.textContent)).toEqual([
      "R$ 1.175,00",
      "R$ 97,92",
      "dos últimos 12",
    ]);
    expect(list.querySelector(".stat--gold")).toHaveTextContent("R$ 1.175,00");
    await user.click(within(list).getByRole("button", { name: "Sobre Recebido em 12 meses" }));
    expect(screen.getByRole("status")).toHaveTextContent("Líquido de IR.");
  });
});

type Row = { id: string; name: string; now: string; status: "ok" | "attention" };

const COLUMNS: DataColumn<Row>[] = [
  { key: "name", label: "Regra", rowHeader: true, cell: (r) => r.name },
  { key: "now", label: "Agora", align: "end", cell: (r) => r.now, width: "96px" },
  { key: "detail", label: "Detalhe", wideOnly: true, align: "center", cell: () => "—" },
  { key: "status", label: "Estado", header: <span>Estado ⓘ</span>, cell: (r) => <StatusPill status={r.status} /> },
];

describe("DataTable", () => {
  it("só rola de lado quando a tabela não cabe (data-scrolls), e acompanha o redimensionamento", () => {
    let notify: () => void = () => {};
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          notify = callback;
        }
        observe() {}
        disconnect() {}
      },
    );
    let tableWidth = 500;
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.tagName === "TABLE" ? tableWidth : 0;
    });
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("data-table") ? 400 : 0;
    });
    const { container } = render(
      <DataTable caption="Larga" columns={COLUMNS} rows={[{ id: "a", name: "Exterior", now: "0%", status: "ok" }]} rowKey={(r) => r.id} />,
    );
    const block = container.querySelector(".data-table") as HTMLElement;
    expect(block).toHaveAttribute("data-scrolls");
    tableWidth = 400;
    act(() => notify());
    expect(block).not.toHaveAttribute("data-scrolls");
    vi.unstubAllGlobals();
  });

  it("é uma tabela de verdade: legenda, cabeçalhos de coluna e de linha, rótulo por célula", () => {
    render(
      <DataTable
        caption="Regras de diversificação"
        columns={COLUMNS}
        rows={[
          { id: "a", name: "Exterior", now: "0%", status: "attention" },
          { id: "b", name: "Maior ativo", now: "12%", status: "ok" },
        ]}
        rowKey={(r) => r.id}
        rowClassName={(r) => (r.status === "attention" ? "is-attention" : undefined)}
      />,
    );
    const table = screen.getByRole("table", { name: "Regras de diversificação" });
    expect(table.querySelector("caption")).toHaveClass("sr-only");
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual([
      "Regra",
      "Agora",
      "Detalhe",
      "Estado ⓘ",
    ]);
    expect(within(table).getAllByRole("columnheader")[1]).toHaveClass("is-end");
    expect(within(table).getAllByRole("columnheader")[1]).toHaveStyle({ width: "96px" });
    expect(within(table).getAllByRole("columnheader")[2]).toHaveClass("is-wide-only", "is-center");
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows[0]).toHaveClass("is-attention");
    expect(rows[1]).not.toHaveClass("is-attention");
    expect(within(rows[0] as HTMLElement).getByRole("rowheader")).toHaveTextContent("Exterior");
    const cells = within(rows[0] as HTMLElement).getAllByRole("cell");
    expect(cells.map((c) => c.getAttribute("data-label"))).toEqual(["Agora", "Detalhe", "Estado"]);
    expect(cells[2]).toHaveTextContent("Ponto a ver");
    expect(table.closest(".data-table")).toHaveClass("data-table--stack");
  });

  it("vazia, diz o porquê; legenda visível, linhas de toque e sem empilhar quando pedido", () => {
    const { rerender, container } = render(
      <DataTable caption="Vazia" columns={COLUMNS} rows={[]} rowKey={(r) => r.id} empty="Nenhuma regra ligada." />,
    );
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.getByText("Nenhuma regra ligada.")).toHaveClass("data-table__empty");
    rerender(
      <DataTable
        caption="Com legenda"
        showCaption
        cozy
        stack={false}
        columns={COLUMNS}
        rows={[{ id: "a", name: "Exterior", now: "0%", status: "ok" }]}
        rowKey={(r) => r.id}
      />,
    );
    expect(container.querySelector("caption")).toHaveClass("data-table__caption");
    const wrap = container.querySelector(".data-table");
    expect(wrap).toHaveClass("data-table--cozy");
    expect(wrap).not.toHaveClass("data-table--stack");
  });

  it("sem linhas e sem frase de vazio, mostra só o cabeçalho", () => {
    render(<DataTable caption="Só cabeçalho" columns={COLUMNS} rows={[]} rowKey={(r) => r.id} />);
    expect(screen.getByRole("table", { name: "Só cabeçalho" })).toBeInTheDocument();
  });
});

describe("StatusPill", () => {
  it("diz o estado em texto (a cor só reforça) e aceita outro texto", () => {
    const { rerender, container } = render(<StatusPill status="ok" />);
    expect(container.firstElementChild).toHaveClass("status-pill", "status-pill--ok");
    expect(container.firstElementChild).toHaveTextContent("Ok");
    for (const [status, text] of [
      ["bad", "Fora do limite"],
      ["nodata", "Sem dado"],
      ["info", "Informação"],
    ] as const) {
      rerender(<StatusPill status={status} />);
      expect(container.firstElementChild).toHaveTextContent(text);
      expect(container.querySelector("svg")).not.toBeNull();
    }
    rerender(<StatusPill status="off" />);
    expect(container.firstElementChild).toHaveTextContent("Desligada");
    expect(container.querySelector("svg")).toBeNull();
    rerender(<StatusPill status="attention">A pagar</StatusPill>);
    expect(container.firstElementChild).toHaveTextContent("A pagar");
  });
});

describe("Disclosure", () => {
  it("recolhe com <details>: frase, contagem e abre no clique", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <Disclosure summary="Regras ok" count={8}>
        <p>Dentro</p>
      </Disclosure>,
    );
    const details = container.querySelector("details") as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(container.querySelector("summary")).toHaveTextContent("Regras ok8");
    await user.click(screen.getByText("Regras ok"));
    expect(details.open).toBe(true);
  });

  it("pode abrir já aberto e sem contagem", () => {
    const { container } = render(
      <Disclosure summary="Detalhe" defaultOpen className="extra">
        x
      </Disclosure>,
    );
    const details = container.querySelector("details") as HTMLDetailsElement;
    expect(details.open).toBe(true);
    expect(details).toHaveClass("disclosure", "extra");
    expect(container.querySelector(".disclosure__count")).toBeNull();
  });
});

describe("Toggletip", () => {
  const rect = (r: Partial<DOMRect>) => ({ x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON: () => ({}), ...r });

  it("abre no lado e alinhamento pedidos", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <Toggletip label="Bazin" content="Explicação" placement="bottom" align="end">
        Bazin
      </Toggletip>,
    );
    const root = container.firstElementChild as HTMLElement;
    expect(root).toHaveAttribute("data-placement", "bottom");
    expect(root).toHaveAttribute("data-align", "end");
    await user.hover(screen.getByRole("button", { name: "Bazin" }));
    expect(screen.getByRole("status")).toHaveTextContent("Explicação");
    await user.unhover(screen.getByRole("button", { name: "Bazin" }));
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });

  it("passando da borda da direita, o balão volta para dentro; sem espaço em cima, abre embaixo", async () => {
    const user = userEvent.setup();
    vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(400);
    vi.spyOn(document.documentElement, "clientHeight", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("toggletip__bubble")
        ? rect({ left: 300, right: 450, top: -20, bottom: 40, width: 150, height: 60 })
        : rect({});
    });
    const { container } = render(
      <Toggletip label="Info" content="Uma explicação longa">
        i
      </Toggletip>,
    );
    await user.click(screen.getByRole("button", { name: "Info" }));
    const root = container.firstElementChild as HTMLElement;
    expect(root).toHaveAttribute("data-placement", "bottom");
    expect((root.querySelector(".toggletip__bubble") as HTMLElement).style.getPropertyValue("--tip-shift")).toBe("-58px");
    // Fecha: volta ao lado pedido, sem deslocamento.
    await user.keyboard("{Escape}");
    expect(root).toHaveAttribute("data-placement", "top");
    expect((root.querySelector(".toggletip__bubble") as HTMLElement).style.getPropertyValue("--tip-shift")).toBe("");
  });

  it("passando da borda da esquerda, empurra para a direita; embaixo sem espaço, vira para cima", async () => {
    const user = userEvent.setup();
    vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(400);
    vi.spyOn(document.documentElement, "clientHeight", "get").mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("toggletip__bubble")
        ? rect({ left: -30, right: 120, top: 560, bottom: 620, width: 150, height: 60 })
        : rect({});
    });
    const { container } = render(
      <Toggletip label="Info" content="Outra" placement="bottom">
        i
      </Toggletip>,
    );
    await act(async () => {
      await user.click(screen.getByRole("button", { name: "Info" }));
    });
    const root = container.firstElementChild as HTMLElement;
    expect(root).toHaveAttribute("data-placement", "top");
    expect((root.querySelector(".toggletip__bubble") as HTMLElement).style.getPropertyValue("--tip-shift")).toBe("38px");
  });
});

describe("Toggletip na camada de topo", () => {
  const box = (left: number, top: number, width: number, height: number) => ({ left, top, width, right: left + width, bottom: top + height });
  const viewport = { width: 1000, height: 800 };

  it("posiciona na janela: centrado, alinhado, empurrado para dentro e virado se não couber", () => {
    const trigger = box(500, 300, 16, 16);
    const bubble = { width: 200, height: 60 };
    const anchor = box(0, 0, 400, 100);
    expect(placeBubble(trigger, anchor, bubble, "center", "top", viewport)).toEqual({ left: 408, top: 232, flipped: false });
    expect(placeBubble(trigger, anchor, bubble, "start", "top", viewport).left).toBe(500);
    expect(placeBubble(trigger, anchor, bubble, "end", "top", viewport).left).toBe(316);
    expect(placeBubble(trigger, anchor, bubble, "stretch", "top", viewport).left).toBe(0 + 8);
    expect(placeBubble(box(990, 300, 16, 16), anchor, bubble, "start", "top", viewport).left).toBe(792);
    expect(placeBubble(box(500, 20, 16, 16), anchor, bubble, "center", "top", viewport)).toMatchObject({ top: 44, flipped: true });
    expect(placeBubble(box(500, 760, 16, 16), anchor, bubble, "center", "bottom", viewport)).toMatchObject({ top: 692, flipped: true });
  });

  it("com `popover` no navegador, o balão abre na camada de topo e fecha ao sair", async () => {
    const user = userEvent.setup();
    const show = vi.fn();
    const hide = vi.fn();
    Object.assign(HTMLElement.prototype, { showPopover: show, hidePopover: hide });
    try {
      render(
        <div style={{ overflow: "auto" }}>
          <Toggletip label="Info" content="Dentro de uma tabela que rola">
            i
          </Toggletip>
        </div>,
      );
      const bubble = screen.getByRole("status");
      expect(bubble).toHaveAttribute("popover", "manual");
      await user.click(screen.getByRole("button", { name: "Info" }));
      expect(show).toHaveBeenCalledTimes(1);
      expect(bubble).toHaveTextContent("Dentro de uma tabela que rola");
      expect(bubble.style.left).not.toBe("");
      await user.keyboard("{Escape}");
      expect(hide).toHaveBeenCalled();
      expect(bubble.style.left).toBe("");
    } finally {
      Reflect.deleteProperty(HTMLElement.prototype, "showPopover");
      Reflect.deleteProperty(HTMLElement.prototype, "hidePopover");
    }
  });
});

describe("InfoTip", () => {
  it("é o ⓘ com nome próprio, que abre a explicação no foco do teclado", async () => {
    const user = userEvent.setup();
    render(<InfoTip label="Como se calcula o yield on cost">Renda ÷ custo.</InfoTip>);
    const button = screen.getByRole("button", { name: "Como se calcula o yield on cost" });
    expect(button).toHaveClass("info-tip");
    await user.tab();
    expect(button).toHaveFocus();
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("status")).toHaveTextContent("Renda ÷ custo.");
    await user.tab();
    expect(button).toHaveAttribute("aria-expanded", "false");
  });
});

describe("Sheet larga", () => {
  it("a folha de duas colunas se marca larga; a padrão não", () => {
    const { rerender } = render(
      <Sheet open onClose={() => undefined} title="KNRI11" size="wide">
        corpo
      </Sheet>,
    );
    expect(screen.getByRole("dialog", { name: "KNRI11" })).toHaveClass("sheet--wide");
    rerender(
      <Sheet open onClose={() => undefined} title="Novo ativo">
        corpo
      </Sheet>,
    );
    expect(screen.getByRole("dialog", { name: "Novo ativo" })).not.toHaveClass("sheet--wide");
  });
});
