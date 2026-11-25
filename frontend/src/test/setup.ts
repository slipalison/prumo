import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { MotionGlobalConfig } from "motion/react";
import { afterAll, afterEach, beforeAll, vi } from "vitest";
import { resetDb, server } from "./server";

// Animação pulada: o teste mede o estado final, não o caminho até ele.
MotionGlobalConfig.skipAnimations = true;

// O happy-dom não desenha em canvas; o confete é conferido pela chamada.
vi.mock("canvas-confetti", () => ({ default: vi.fn(() => null) }));

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));

afterEach(() => {
  cleanup();
  server.resetHandlers();
  resetDb();
  window.localStorage.clear();
  window.location.hash = "";
  document.documentElement.removeAttribute("data-theme");
  document.body.style.overflow = "";
});

afterAll(() => server.close());
