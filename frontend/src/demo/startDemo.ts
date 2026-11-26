import { setupWorker } from "msw/browser";
import type { LivePrices } from "../api/types";
import { toPortfolio } from "../test/fixtures";
import { allHandlers, getDb } from "../test/handlers";
import { demoHandlers } from "./demoHandlers";

/**
 * O stream da carteira na demonstração: o service worker do msw não
 * intercepta `EventSource`, então este manda um evento `prices` com a carteira
 * falsa logo ao abrir — o selo fica "Ao vivo", como no app de verdade.
 */
class DemoEventSource extends EventTarget {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  readonly url: string;
  readonly withCredentials = true;
  readyState = DemoEventSource.CONNECTING;
  onerror: ((event: Event) => void) | null = null;

  constructor(url: string | URL) {
    super();
    this.url = String(url);
    setTimeout(() => {
      if (this.readyState === DemoEventSource.CLOSED) return;
      this.readyState = DemoEventSource.OPEN;
      const db = getDb();
      const prices: LivePrices = toPortfolio(db.assets, db.goals, db.checklists);
      this.dispatchEvent(new MessageEvent("prices", { data: JSON.stringify(prices) }));
    }, 300);
  }

  close(): void {
    this.readyState = DemoEventSource.CLOSED;
  }
}

/**
 * Liga o modo de demonstração (`npm run dev:demo`): a API inteira é o banco
 * falso dos testes, servido pelo service worker do msw no próprio navegador.
 * Nada sai da máquina e nenhum dado é real. Recarregar a página volta ao
 * começo.
 */
export async function startDemo(): Promise<void> {
  window.EventSource = DemoEventSource as unknown as typeof EventSource;
  const worker = setupWorker(...demoHandlers, ...allHandlers);
  await worker.start({ onUnhandledRequest: "bypass", quiet: true });
}
