import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { LivePrices } from "../../api/types";
import { FakeEventSource } from "../../test/eventSource";
import { defaultAssets, toPortfolio } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { getDb, resetDb } from "../../test/server";

async function openPortfolio() {
  const rendered = renderApp("carteira");
  await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
  await screen.findByRole("region", { name: "Fundos Imobiliários" });
  return rendered;
}

/** O número "abaixo do ideal" da faixa do topo. */
const belowStat = () => screen.getByRole("region", { name: "Abaixo do ideal" });

const row = (region: HTMLElement, symbol: string) => within(region).getByRole("button", { name: new RegExp(symbol) });

/** O evento `prices` que o servidor mandaria para a carteira falsa de agora. */
function pricesNow(): LivePrices {
  const db = getDb();
  const portfolio = toPortfolio(db.assets, db.goals, db.checklists);
  return {
    total_value: portfolio.total_value,
    quotes_updated_at: portfolio.quotes_updated_at,
    classes: portfolio.classes,
    profit: portfolio.profit,
    assets: portfolio.assets.map(
      ({
        id,
        unit_price,
        currency,
        price_source,
        quote_updated_at,
        current_value,
        class_percent,
        ideal_class_percent,
        unrealized_gain,
        return_percent,
      }) => ({
        id,
        unit_price,
        currency,
        price_source,
        quote_updated_at,
        current_value,
        class_percent,
        ideal_class_percent,
        unrealized_gain,
        return_percent,
      }),
    ),
  };
}

/*
 * Fundos Imobiliários da carteira de exemplo: notas 6, 6, 4, 4 e uma de nota 0
 * (MXRF11). O ideal é nota ÷ soma das notas de quem recebe: 30%, 30%, 20%, 20%.
 */
describe("Carteira: atual × ideal", () => {
  it("mostra a % atual, a % ideal e o selo só em quem está abaixo", async () => {
    await openPortfolio();
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });

    const knri = row(fiis, "KNRI11");
    expect(knri).toHaveTextContent("25,3% da classe");
    expect(knri).toHaveTextContent("ideal 30,0%");
    expect(within(knri).getByText("abaixo do ideal")).toBeInTheDocument();

    const hglg = row(fiis, "HGLG11");
    expect(hglg).toHaveTextContent("30,1% da classe");
    expect(hglg).toHaveTextContent("ideal 30,0%");
    expect(within(hglg).queryByText("abaixo do ideal")).not.toBeInTheDocument();

    // Acima do ideal: sem selo.
    const btlg = row(fiis, "BTLG11");
    expect(btlg).toHaveTextContent("ideal 20,0%");
    expect(within(btlg).queryByText("abaixo do ideal")).not.toBeInTheDocument();

    const trxf = row(fiis, "TRXF11");
    expect(trxf).toHaveTextContent("19,2% da classe");
    expect(within(trxf).getByText("abaixo do ideal")).toBeInTheDocument();
  });

  it("ativo sem nota não tem ideal nem selo: mostra só a fatia que ocupa", async () => {
    await openPortfolio();
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    const mxrf = row(fiis, "MXRF11");
    expect(mxrf).not.toHaveTextContent("ideal");
    expect(within(mxrf).queryByText("abaixo do ideal")).not.toBeInTheDocument();
    expect(mxrf).toHaveTextContent(/\d+,\d% da classe/);
    expect(within(mxrf).getByRole("img", { name: "Sem nota" })).toBeInTheDocument();
  });

  it("a barra é só desenho: o leitor de tela lê o texto, que diz atual e ideal", async () => {
    await openPortfolio();
    const knri = row(screen.getByRole("region", { name: "Fundos Imobiliários" }), "KNRI11");
    expect(knri.querySelector(".ideal-bar")).toHaveAttribute("aria-hidden", "true");
    expect(knri).toHaveTextContent(/25,3% da classe · ideal 30,0%/);
  });

  it("o topo diz quantos ativos estão abaixo do ideal", async () => {
    await openPortfolio();
    // KNRI11 e TRXF11; as outras classes têm um ativo só, no ideal.
    expect(within(belowStat()).getByText("2 ativos")).toBeInTheDocument();
    expect(belowStat()).toHaveTextContent("abaixo do ideal da classe");
  });

  it("com um só, o texto vai no singular", async () => {
    // A BTLG11 cai para R$ 80: a TRXF11 chega na fatia dela, só a KNRI11 fica abaixo.
    const btlg = getDb().assets.find((a) => a.symbol === "BTLG11");
    if (!btlg) throw new Error("fixture sem BTLG11");
    btlg.quote_price = 80;
    await openPortfolio();
    expect(within(belowStat()).getByText("1 ativo")).toBeInTheDocument();
  });

  it("com todos no ideal, avisa que nenhum está abaixo", async () => {
    resetDb({ assets: defaultAssets().filter((a) => a.asset_class !== "br_reits") });
    await openPortfolio();
    expect(await within(await screen.findByRole("region", { name: "Abaixo do ideal" })).findByText("Nenhum ativo")).toBeInTheDocument();
    expect(belowStat()).not.toHaveTextContent("da classe");
  });

  it("sem ativo que receba aporte, a linha do topo não aparece", async () => {
    resetDb({ assets: [] });
    renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    expect(screen.queryByRole("region", { name: "Abaixo do ideal" })).not.toBeInTheDocument();
    expect(screen.queryByText(/abaixo do ideal/)).not.toBeInTheDocument();
  });

  it("acompanha o preço ao vivo: o evento leva o ideal e o selo se acerta sozinho", async () => {
    await openPortfolio();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(within(belowStat()).getByText("2 ativos")).toBeInTheDocument();

    // A BTLG11 cai para R$ 80: a TRXF11 chega na fatia dela, só a KNRI11 fica abaixo.
    const btlg = getDb().assets.find((a) => a.symbol === "BTLG11");
    if (!btlg) throw new Error("fixture sem BTLG11");
    btlg.quote_price = 80;
    act(() => FakeEventSource.latest().emit("prices", pricesNow()));

    expect(await within(belowStat()).findByText("1 ativo")).toBeInTheDocument();
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    expect(within(row(fiis, "TRXF11")).queryByText("abaixo do ideal")).not.toBeInTheDocument();
    expect(within(row(fiis, "KNRI11")).getByText("abaixo do ideal")).toBeInTheDocument();
  });

  it("um ativo que ganha preço no evento passa a ter ideal", async () => {
    const mxrf = getDb().assets.find((a) => a.symbol === "MXRF11");
    if (!mxrf) throw new Error("fixture sem MXRF11");
    mxrf.answers = {};
    mxrf.score_override = 4;
    mxrf.quote_price = null;
    await openPortfolio();
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    expect(row(fiis, "MXRF11")).not.toHaveTextContent("ideal");

    mxrf.quote_price = 8.37;
    act(() => FakeEventSource.latest().emit("prices", pricesNow()));
    await waitFor(() => expect(row(fiis, "MXRF11")).toHaveTextContent(/ideal \d+,\d%/));
  });
});
