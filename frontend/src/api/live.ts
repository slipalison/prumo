import type { LivePrices } from "./types";

/**
 * - `off`: sem conexão porque ninguém pediu (aba escondida, fora do app).
 * - `connecting`: abrindo ou reabrindo.
 * - `live`: o primeiro preço chegou; os próximos chegam sozinhos.
 * - `offline`: o servidor recusou ou caiu; nova tentativa agendada.
 */
export type LiveStatus = "off" | "connecting" | "live" | "offline";

export type PriceStreamHandlers = {
  onPrices: (prices: LivePrices) => void;
  onStatus: (status: LiveStatus) => void;
};

export const STREAM_URL = "/api/v1/portfolio/stream";

/** Espera antes de reabrir uma conexão recusada: 1 s, 2 s, 4 s… até 60 s. */
export function retryDelay(attempt: number): number {
  return Math.min(60_000, 1000 * 2 ** attempt);
}

/**
 * Abre o stream da carteira e devolve quem o fecha.
 *
 * Queda de rede o próprio `EventSource` reabre (`readyState` volta a
 * CONNECTING, no ritmo do `retry` que o servidor manda). Resposta que não é
 * stream — 401, ou o redirecionamento do Access quando a sessão vence — o
 * navegador não tenta de novo (`readyState` CLOSED): aqui ele reabre com
 * espera crescente.
 */
export function openPriceStream(handlers: PriceStreamHandlers): () => void {
  let source: EventSource | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let attempt = 0;

  const connect = () => {
    handlers.onStatus("connecting");
    const current = new EventSource(STREAM_URL, { withCredentials: true });
    source = current;
    current.addEventListener("prices", (event: MessageEvent<string>) => {
      let prices: LivePrices;
      try {
        prices = JSON.parse(event.data) as LivePrices;
      } catch {
        return;
      }
      attempt = 0;
      handlers.onStatus("live");
      handlers.onPrices(prices);
    });
    current.onerror = () => {
      if (current.readyState !== EventSource.CLOSED) {
        handlers.onStatus("connecting");
        return;
      }
      handlers.onStatus("offline");
      timer = setTimeout(connect, retryDelay(attempt));
      attempt += 1;
    };
  };

  connect();
  return () => {
    clearTimeout(timer);
    source?.close();
    source = null;
    handlers.onStatus("off");
  };
}
