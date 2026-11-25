import { QueryClient } from "@tanstack/react-query";
import { render, type RenderResult } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import type { ReactElement } from "react";
import { Providers } from "../app/Providers";
import { Shell } from "../app/Shell";

export function testClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 0 }, mutations: { retry: false } },
  });
}

export type Rendered = RenderResult & { user: UserEvent; client: QueryClient };

export function renderWithProviders(ui: ReactElement, client: QueryClient = testClient()): Rendered {
  const user = userEvent.setup();
  const result = render(<Providers client={client}>{ui}</Providers>);
  return { ...result, user, client };
}

/** O app inteiro, aberto num endereço (`carteira`, `ajustes/metas`, ou um link antigo). */
export function renderApp(path = "aportar"): Rendered {
  window.location.hash = `#/${path}`;
  return renderWithProviders(<Shell />);
}
