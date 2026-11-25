/**
 * `EventSource` de mentira: o teste decide quando chega evento, quando cai e
 * como cai. Cada `new EventSource` fica em `FakeEventSource.instances`.
 */
export class FakeEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  static instances: FakeEventSource[] = [];

  readonly url: string;
  readonly withCredentials: boolean;
  readyState = FakeEventSource.CONNECTING;
  onerror: ((event: Event) => void) | null = null;
  private readonly listeners = new Map<string, Set<(event: MessageEvent<string>) => void>>();

  constructor(url: string, init?: { withCredentials?: boolean }) {
    this.url = url;
    this.withCredentials = init?.withCredentials ?? false;
    FakeEventSource.instances.push(this);
  }

  static latest(): FakeEventSource {
    const last = FakeEventSource.instances.at(-1);
    if (!last) throw new Error("nenhum EventSource aberto");
    return last;
  }

  static reset(): void {
    FakeEventSource.instances = [];
  }

  addEventListener(type: string, listener: (event: MessageEvent<string>) => void): void {
    const set = this.listeners.get(type) ?? new Set();
    set.add(listener);
    this.listeners.set(type, set);
  }

  removeEventListener(type: string, listener: (event: MessageEvent<string>) => void): void {
    this.listeners.get(type)?.delete(listener);
  }

  close(): void {
    this.readyState = FakeEventSource.CLOSED;
  }

  get closed(): boolean {
    return this.readyState === FakeEventSource.CLOSED;
  }

  /** Um evento do servidor; `raw` vai como está (para testar JSON quebrado). */
  emit(type: string, data: unknown, raw = false): void {
    this.readyState = FakeEventSource.OPEN;
    const event = new MessageEvent<string>(type, { data: raw ? String(data) : JSON.stringify(data) });
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  /** Queda. `retrying`: o navegador reabre sozinho (rede); senão, desiste (HTTP). */
  fail(retrying: boolean): void {
    this.readyState = retrying ? FakeEventSource.CONNECTING : FakeEventSource.CLOSED;
    this.onerror?.(new Event("error"));
  }
}
