import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { MotionGlobalConfig } from "motion/react";
import { afterAll, afterEach, beforeAll, vi } from "vitest";
import { resetAgenda } from "./agendaHandlers";
import { resetAnalysis } from "./analysisHandlers";
import { resetCompare } from "./compareHandlers";
import { FakeEventSource } from "./eventSource";
import { resetFundamentals } from "./fundamentalsHandlers";
import { resetIcons } from "./iconsHandlers";
import { resetPanel } from "./panelHandlers";
import { resetDb, server } from "./server";
import { resetTaxDb } from "./taxHandlers";

// Animação pulada: o teste mede o estado final, não o caminho até ele.
MotionGlobalConfig.skipAnimations = true;

// O happy-dom não desenha em canvas; o confete é conferido pela chamada.
vi.mock("canvas-confetti", () => ({ default: vi.fn(() => null) }));

// O happy-dom não fala SSE, e o msw não intercepta `EventSource`: o stream da
// carteira é este falso, dirigido pelo teste.
vi.stubGlobal("EventSource", FakeEventSource);

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));

afterEach(() => {
  cleanup();
  FakeEventSource.reset();
  server.resetHandlers();
  resetDb();
  resetIcons();
  resetPanel();
  resetAgenda();
  resetAnalysis();
  resetCompare();
  resetFundamentals();
  resetTaxDb();
  window.localStorage.clear();
  window.location.hash = "";
  document.documentElement.removeAttribute("data-theme");
  document.body.style.overflow = "";
});

afterAll(() => server.close());
