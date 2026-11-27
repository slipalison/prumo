import { screen, waitFor, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { MASK_BRL } from "../../lib/money";
import { HIDE_VALUES_KEY } from "../../lib/privacy";
import { agendaOf, failAgenda, sampleMonth, setAgenda, setUnusualIncome } from "../../test/agendaHandlers";
import { sampleSummary } from "../../test/incomeHandlers";
import { renderApp } from "../../test/render";
import { server } from "../../test/server";
import { addMonths, monthOf, monthTitle, todayInBrazil } from "./agendaText";

const MONEY = /R\$\s*\d/;

async function openAgenda() {
  setAgenda((from) => sampleMonth(from));
  const rendered = renderApp("agenda");
  await screen.findByRole("heading", { name: "Agenda da carteira" });
  await screen.findByText(/Corte de 25% no provento de MXRF11/);
  return rendered;
}

const timeline = () => screen.getByRole("region", { name: "Linha do tempo" });

describe("Agenda", () => {
  it("é uma tela de topo, com a linha do tempo do mês dos ativos da carteira", async () => {
    await openAgenda();
    expect(screen.getAllByRole("link", { name: /Agenda/ }).length).toBeGreaterThan(0);
    const list = timeline();
    // Os dias com evento, cada um com o nome do dia.
    // Nove dias com evento e hoje (o 15 do servidor falso), que aparece mesmo vazio.
    expect(within(list).getAllByRole("heading", { level: 3 }).length).toBe(10);
    // Provento: o líquido; "a definir" é estimado e diz que o pagamento não tem data.
    expect(within(list).getByText(/pagamento a definir/)).toBeInTheDocument();
    expect(within(list).getByText("~R$ 41,25")).toBeInTheDocument();
    expect(within(list).getAllByText("R$ 7,50")).toHaveLength(2);
    // O corte marca a data com e o pagamento, e é um evento no dia do anúncio.
    expect(within(list).getAllByText("Corte de 25%")).toHaveLength(2);
    expect(within(list).getByText(/R\$\s0,075 por cota; habitual de R\$\s0,10 nos 12 meses e R\$\s0,098 em 5 anos/)).toBeInTheDocument();
    // Documentos com o link do original, que abre em nova aba.
    const fact = within(list).getByRole("link", { name: /Abrir o documento: Fato relevante de ITSA4/ });
    expect(fact).toHaveAttribute("href", expect.stringMatching(/^https:\/\/www\.rad\.cvm\.gov\.br\//));
    expect(fact).toHaveAttribute("target", "_blank");
    expect(fact).toHaveAttribute("rel", "noopener noreferrer");
    expect(within(list).getByText(/ITR do trimestre até 30\/09\/2026/)).toBeInTheDocument();
    expect(within(list).getByText(/AGO · Edital de Convocação/)).toBeInTheDocument();
    expect(within(list).getByText("Finados")).toBeInTheDocument();
    expect(within(list).getByText("hoje")).toBeInTheDocument();
    expect(within(list).getByText("Nada hoje.")).toBeInTheDocument();
  });

  it("filtra por tipo de evento e mostra quantos há de cada", async () => {
    const { user } = await openAgenda();
    const types = screen.getByRole("group", { name: "Tipos de evento" });
    const facts = within(types).getByRole("button", { name: /Fato relevante/ });
    expect(facts).toHaveAttribute("aria-pressed", "true");
    expect(facts).toHaveTextContent("1");
    expect(within(types).getByRole("button", { name: /Data com/ })).toHaveTextContent("2");
    await user.click(facts);
    expect(facts).toHaveAttribute("aria-pressed", "false");
    expect(within(timeline()).queryByRole("link", { name: /Fato relevante de ITSA4/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Limpar filtros" }));
    expect(facts).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: "Limpar filtros" })).toBeNull();
  });

  it("filtra por classe e por ativo; o dia sem pregão fica", async () => {
    const { user } = await openAgenda();
    await user.selectOptions(screen.getByLabelText("Classe"), "br_stocks");
    expect(within(timeline()).queryByText(/MXRF11/)).toBeNull();
    expect(within(timeline()).getByText(/ITR do trimestre/)).toBeInTheDocument();
    expect(within(timeline()).getByText("Finados")).toBeInTheDocument();
    // O ativo só oferece os da classe escolhida.
    const asset = screen.getByLabelText("Ativo");
    expect(within(asset).queryByRole("option", { name: "MXRF11" })).toBeNull();
    await user.selectOptions(screen.getByLabelText("Classe"), "");
    await user.selectOptions(screen.getByLabelText("Ativo"), "a-mxrf");
    expect(within(timeline()).queryByText(/ITSA4/)).toBeNull();
    expect(within(timeline()).getAllByText(/MXRF11/).length).toBeGreaterThan(0);
  });

  it("mostra o mês em calendário e a lista do dia escolhido", async () => {
    const { user } = await openAgenda();
    await user.click(screen.getByRole("button", { name: "Calendário" }));
    const calendar = screen.getByRole("region", { name: "Calendário" });
    const month = monthOf(todayInBrazil());
    const tenth = `${month.year}-${String(month.month).padStart(2, "0")}-10`;
    const day10 = within(calendar).getByRole("button", { name: /, 10 de .*: 1 evento: Data com/ });
    // Hoje (o 15) vem escolhido.
    const today = within(calendar).getByRole("button", { name: /\(hoje\)/ });
    expect(today).toHaveAttribute("aria-pressed", "true");
    await user.click(day10);
    expect(day10).toHaveAttribute("aria-pressed", "true");
    const panel = screen.getByRole("region", { name: new RegExp(`10 de ${monthTitle(month).split(" ")[0]}`, "i") });
    expect(within(panel).getByText("Data com")).toBeInTheDocument();
    expect(within(panel).getByText(/data ex/)).toBeInTheDocument();
    expect(tenth).toMatch(/-10$/);
    // Dia sem evento.
    await user.click(within(calendar).getByRole("button", { name: /, 3 de .*nenhum evento/ }));
    expect(screen.getByText("Nada neste dia com esses filtros.")).toBeInTheDocument();
    // Dia sem pregão listra o quadrado e diz o motivo na lista.
    await user.click(within(calendar).getByRole("button", { name: /, 2 de .*Sem pregão/ }));
    expect(screen.getByText("Finados")).toBeInTheDocument();
  });

  it("anda de mês em mês e volta para hoje", async () => {
    const asked: string[] = [];
    const { user } = await openAgenda();
    server.use(
      http.get("/api/v1/agenda", ({ request }) => {
        const url = new URL(request.url);
        const from = url.searchParams.get("from") ?? "";
        asked.push(from);
        return HttpResponse.json(agendaOf(from, url.searchParams.get("to") ?? "", [], todayInBrazil()));
      }),
    );
    const now = monthOf(todayInBrazil());
    await user.click(screen.getByRole("button", { name: "Próximo mês" }));
    const next = addMonths(now, 1);
    const title = monthTitle(next);
    expect(await screen.findByRole("heading", { name: new RegExp(title, "i") })).toBeInTheDocument();
    await waitFor(() => expect(asked).toContain(`${next.year}-${String(next.month).padStart(2, "0")}-01`));
    expect(await screen.findByText(/Nada na agenda de/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Voltar para hoje" }));
    expect(await screen.findByRole("heading", { name: new RegExp(monthTitle(now), "i") })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Mês anterior" }));
    const previous = addMonths(now, -1);
    await waitFor(() => expect(asked).toContain(`${previous.year}-${String(previous.month).padStart(2, "0")}-01`));
  });

  it("avisa quando o mês passa do calendário de pregões cadastrado", async () => {
    server.use(
      http.get("/api/v1/agenda", ({ request }) => {
        const url = new URL(request.url);
        const from = url.searchParams.get("from") ?? "";
        return HttpResponse.json({ ...agendaOf(from, url.searchParams.get("to") ?? "", [], todayInBrazil()), calendar_until: 2020 });
      }),
    );
    renderApp("agenda");
    expect(await screen.findByText(/O calendário de pregões da B3 está cadastrado até 2020/)).toBeInTheDocument();
  });

  it("diz quando não consegue carregar e tenta de novo", async () => {
    failAgenda(true);
    const { user } = renderApp("agenda");
    expect(await screen.findByText("Não consegui carregar a agenda")).toBeInTheDocument();
    failAgenda(false);
    setAgenda((from) => sampleMonth(from));
    await user.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByText(/Corte de 25% no provento de MXRF11/)).toBeInTheDocument();
  });

  it("com o olho fechado, nenhum valor aparece nem é lido", async () => {
    window.localStorage.setItem(HIDE_VALUES_KEY, "1");
    await openAgenda();
    const list = timeline();
    expect(within(list).getAllByText(MASK_BRL).length).toBeGreaterThan(0);
    expect(within(list).getByText(`~${MASK_BRL}`)).toBeInTheDocument();
    const heard: string[] = [];
    const walker = document.createTreeWalker(list, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) heard.push(node.textContent ?? "");
    for (const element of list.querySelectorAll("*")) {
      for (const attribute of ["aria-label", "title"]) {
        const value = element.getAttribute(attribute);
        if (value) heard.push(value);
      }
    }
    for (const text of heard) expect(text).not.toMatch(MONEY);
  });
});

describe("Renda: o selo de fora do padrão", () => {
  it("marca o próximo pagamento cujo valor por cota se afastou do habitual", async () => {
    const summary = sampleSummary();
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(summary)));
    const next = summary.upcoming[0];
    expect(next).toBeDefined();
    setUnusualIncome([
      {
        income_id: next?.id ?? "",
        asset_id: next?.asset_id ?? "",
        record_date: "2026-11-27",
        ex_date: "2026-11-30",
        per_unit: 1.5,
        direction: "increase",
        recent: { usual: 1.1, change: 0.3636, payments: 12 },
        long: null,
        years: 0,
      },
    ]);
    renderApp("renda");
    const upcoming = await screen.findByRole("region", { name: "Próximos pagamentos" });
    expect(await within(upcoming).findByText("Aumento de 36%")).toBeInTheDocument();
    expect(within(upcoming).getByText(/do habitual, que é de R\$\s1,10 nos 12 meses/)).toBeInTheDocument();
  });

  it("sem nada fora do padrão, a lista fica como era", async () => {
    server.use(http.get("/api/v1/income/summary", () => HttpResponse.json(sampleSummary())));
    renderApp("renda");
    const upcoming = await screen.findByRole("region", { name: "Próximos pagamentos" });
    await waitFor(() => expect(within(upcoming).getAllByRole("listitem").length).toBeGreaterThan(0));
    expect(within(upcoming).queryByText(/Corte de|Aumento de/)).toBeNull();
  });
});
