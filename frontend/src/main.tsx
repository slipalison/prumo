import "@fontsource-variable/inter";
import "@fontsource-variable/space-grotesk";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/components.css";
import "./styles/screens.css";
import "./styles/health.css";
import "./styles/analysis.css";
import "./styles/icons.css";
import "./styles/alerts.css";
import "./styles/income.css";
import "./styles/objectives.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { applyTheme, readThemePreference } from "./lib/theme";

applyTheme(readThemePreference());

async function boot(): Promise<void> {
  // `npm run dev:demo`: a API é o banco falso dos testes, no navegador. O
  // build de produção troca `MODE` por "production" e corta este ramo (e o
  // msw) do pacote.
  if (import.meta.env.MODE === "demo") {
    const { startDemo } = await import("./demo/startDemo");
    await startDemo();
  }
  const root = document.getElementById("root");
  if (root) {
    createRoot(root).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  }
}

void boot();
