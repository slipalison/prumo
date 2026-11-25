import { act, renderHook, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openPriceStream, retryDelay, STREAM_URL, type LiveStatus } from "../api/live";
import type { LivePrices, Portfolio } from "../api/types";
import { mergePrices } from "../lib/livePrices";
import { getLiveStatus } from "../lib/liveStatus";
import { usePriceTrend } from "../lib/usePriceTrend";
import { FakeEventSource } from "../test/eventSource";
import { toPortfolio } from "../test/fixtures";
import { renderApp } from "../test/render";
import { getDb } from "../test/server";

/** O evento `prices` que o servidor mandaria para a carteira falsa de agora. */
function pricesNow(change?: (portfolio: Portfolio) => void): LivePrices {
  const db = getDb();
  const portfolio = toPortfolio(db.assets, db.goals, db.checklists);
  change?.(portfolio);
  return {
    total_value: portfolio.total_value,
    quotes_updated_at: portfolio.quotes_updated_at,
    classes: portfolio.classes,
    assets: portfolio.assets.map(
      ({ id, unit_price, currency, price_source, quote_updated_at, current_value, class_percent }) => ({
        id,
        unit_price,
        currency,
        price_source,
        quote_updated_at,
        current_value,
        class_percent,
      }),
    ),
  };
}

/** KNRI11 a `price`: o servidor recalcula a carteira inteira. */
function knriAt(price: number): LivePrices {
  const knri = getDb().assets.find((a) => a.symbol === "KNRI11");
  if (!knri) throw new Error("fixture sem KNRI11");
  knri.quote_price = price;
  return pricesNow();
}

const setVisibility = (state: DocumentVisibilityState) => {
  Object.defineProperty(document, "visibilityState", { configurable: true, value: state });
  document.dispatchEvent(new Event("visibilitychange"));
};

afterEach(() => {
  setVisibility("visible");
  vi.useRealTimers();
});

describe("stream de preços", () => {
  it("abre com credencial, avisa o estado e reabre o que o navegador desiste", () => {
    vi.useFakeTimers();
    const statuses: LiveStatus[] = [];
    const received: LivePrices[] = [];
    const close = openPriceStream({ onPrices: (p) => received.push(p), onStatus: (s) => statuses.push(s) });

    const first = FakeEventSource.latest();
    expect(first.url).toBe(STREAM_URL);
    expect(first.withCredentials).toBe(true);
    expect(statuses).toEqual(["connecting"]);

    first.emit("prices", pricesNow());
    expect(received).toHaveLength(1);
    expect(statuses.at(-1)).toBe("live");
    // JSON quebrado não derruba nada nem muda o estado.
    first.emit("prices", "{", true);
    expect(received).toHaveLength(1);

    // Queda de rede: o próprio EventSource reabre.
    first.fail(true);
    expect(statuses.at(-1)).toBe("connecting");
    expect(FakeEventSource.instances).toHaveLength(1);

    // Resposta que não é stream (sessão do Access vencida): reabre com espera.
    first.fail(false);
    expect(statuses.at(-1)).toBe("offline");
    vi.advanceTimersByTime(retryDelay(0) - 1);
    expect(FakeEventSource.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeEventSource.instances).toHaveLength(2);
    const second = FakeEventSource.latest();
    second.fail(false);
    vi.advanceTimersByTime(retryDelay(1));
    expect(FakeEventSource.instances).toHaveLength(3);

    // Evento que chega zera a espera; fechar cancela o que estava agendado.
    FakeEventSource.latest().emit("prices", pricesNow());
    FakeEventSource.latest().fail(false);
    close();
    vi.advanceTimersByTime(60_000);
    expect(FakeEventSource.instances).toHaveLength(3);
    expect(FakeEventSource.latest().closed).toBe(true);
    expect(statuses.at(-1)).toBe("off");
  });

  it("a espera cresce até um minuto", () => {
    expect([0, 1, 2, 5, 6, 10].map(retryDelay)).toEqual([1000, 2000, 4000, 32_000, 60_000, 60_000]);
  });

  it("o evento troca os números e deixa metas e ativos novos como estão", () => {
    const db = getDb();
    const portfolio = toPortfolio(db.assets, db.goals, db.checklists);
    const prices = knriAt(150);
    // O evento saiu antes de a meta nova ser salva, e não conhece um ativo novo.
    const optimistic: Portfolio = {
      ...portfolio,
      classes: portfolio.classes.map((c) => (c.asset_class === "br_reits" ? { ...c, target_percent: 99 } : c)),
      assets: [...portfolio.assets, { ...portfolio.assets[0], id: "novo" } as Portfolio["assets"][number]],
    };
    const merged = mergePrices(optimistic, prices);
    expect(merged.total_value).toBe(prices.total_value);
    const reits = merged.classes.find((c) => c.asset_class === "br_reits");
    expect(reits?.target_percent).toBe(99);
    expect(reits?.current_value).toBe(prices.classes.find((c) => c.asset_class === "br_reits")?.current_value);
    const knri = merged.assets.find((a) => a.symbol === "KNRI11");
    const before = portfolio.assets.find((a) => a.symbol === "KNRI11");
    // Preço e valor do evento; nota e respostas, as da tela.
    expect([knri?.unit_price, knri?.current_value]).toEqual([150, 1500]);
    expect([knri?.score, knri?.answers]).toEqual([before?.score, before?.answers]);
    expect(merged.assets.find((a) => a.id === "novo")).toBeDefined();
    expect(merged.checklists).toBe(portfolio.checklists);
  });

  it("a tendência dura um instante e recomeça a cada mudança", () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ value }) => usePriceTrend(value, 100), {
      initialProps: { value: 10 },
    });
    expect(result.current.trend).toBeNull();
    rerender({ value: 11 });
    expect(result.current).toEqual({ trend: "up", version: 1 });
    rerender({ value: 12 });
    expect(result.current).toEqual({ trend: "up", version: 2 });
    rerender({ value: 9 });
    expect(result.current.trend).toBe("down");
    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(result.current.trend).toBeNull();
  });
});

describe("Carteira ao vivo", () => {
  async function openLive() {
    const rendered = renderApp("carteira");
    await screen.findByRole("heading", { name: "Tudo o que você tem, no eixo certo" });
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    return rendered;
  }

  it("patrimônio e ativos mudam sem recarregar a página", async () => {
    await openLive();
    expect(await screen.findByText("Conectando ao vivo…")).toBeInTheDocument();

    act(() => FakeEventSource.latest().emit("prices", knriAt(150)));
    expect(await screen.findByText("Ao vivo")).toBeInTheDocument();
    // A carteira de exemplo tem ativo da B3: o atraso do dado gratuito aparece.
    expect(screen.getByText("B3 com 15 min de atraso")).toBeInTheDocument();
    // 120.132,23 + 10 × (150 − 142)
    expect(await screen.findByText(/R\$\s120\.212,23/)).toBeInTheDocument();
    const fiis = screen.getByRole("region", { name: "Fundos Imobiliários" });
    const knri = within(fiis).getByRole("button", { name: /KNRI11/ });
    expect(within(knri).getByText(/R\$\s1\.500,00/)).toBeInTheDocument();
    expect(within(knri).getByText(/10 cotas · R\$\s150,00/)).toBeInTheDocument();
    expect(within(knri).getByText("▲")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Patrimônio" })).toHaveTextContent("▲");

    act(() => FakeEventSource.latest().emit("prices", knriAt(140)));
    expect(await within(knri).findByText(/R\$\s1\.400,00/)).toBeInTheDocument();
    expect(within(knri).getByText("▼")).toBeInTheDocument();
    expect(getLiveStatus()).toBe("live");
  });

  it("sem ativo da B3 não fala de atraso; caído, mostra a hora da cotação", async () => {
    getDb().assets = getDb().assets.filter((a) => a.asset_class === "crypto");
    await openLive();
    act(() => FakeEventSource.latest().emit("prices", pricesNow()));
    expect(await screen.findByText("Ao vivo")).toBeInTheDocument();
    expect(screen.queryByText("B3 com 15 min de atraso")).not.toBeInTheDocument();

    act(() => FakeEventSource.latest().fail(false));
    expect(await screen.findByText(/^Cotações /)).toBeInTheDocument();
    // E tenta de novo sozinho.
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2), { timeout: 3000 });
  });

  it("aba escondida fecha a conexão; ao voltar, abre de novo", async () => {
    const { unmount } = await openLive();
    const first = FakeEventSource.latest();
    act(() => setVisibility("hidden"));
    expect(first.closed).toBe(true);
    expect(getLiveStatus()).toBe("off");
    act(() => setVisibility("visible"));
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.latest().closed).toBe(false);
    unmount();
    expect(FakeEventSource.latest().closed).toBe(true);
  });
});
