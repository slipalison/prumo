/// <reference types="vitest/config" />
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

/**
 * Modo de demonstração (`npm run dev:demo`, docs/layout.md): serve o service
 * worker do msw direto do pacote instalado, em vez de copiá-lo para `public/`
 * — assim ele nunca entra no build de produção e nunca fica de versão
 * diferente da biblioteca.
 */
function demoWorker(): Plugin {
  const file = createRequire(import.meta.url).resolve("msw/mockServiceWorker.js");
  return {
    name: "prumo-demo-worker",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/mockServiceWorker.js", (_request, response) => {
        response.setHeader("Content-Type", "text/javascript");
        response.setHeader("Service-Worker-Allowed", "/");
        response.end(readFileSync(file));
      });
    },
  };
}

export default defineConfig(({ mode }) => ({
  plugins: mode === "demo" ? [react(), demoWorker()] : [react()],
  server: {
    // Em produção o mesmo binário serve a API e os estáticos, então o caminho
    // relativo `/api/v1/...` vale nos dois lados e não há URL para configurar.
    // `PRUMO_API` troca o alvo quando a 8080 da máquina já tem dono. Na
    // demonstração não há servidor: o msw responde no navegador.
    ...(mode === "demo"
      ? {}
      : { proxy: { "/api": { target: process.env.PRUMO_API ?? "http://127.0.0.1:8080", changeOrigin: false } } }),
  },
  build: {
    target: "es2022",
    rollupOptions: {
      output: {
        // Biblioteca muda menos que o app: em chunks próprios, o navegador
        // mantém o cache delas entre uma versão e outra.
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          if (/[\\/]node_modules[\\/](react|react-dom|scheduler)[\\/]/.test(id)) return "react";
          if (/[\\/]node_modules[\\/](motion|framer-motion|motion-dom|motion-utils)[\\/]/.test(id)) return "motion";
          return "vendor";
        },
      },
    },
  },
  test: {
    // happy-dom, e não jsdom: o jsdom instala o próprio AbortController, que o
    // fetch do Node (undici) recusa — medido no basalto.
    environment: "happy-dom",
    globals: false,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    coverage: {
      provider: "v8",
      reporter: ["text-summary", "text", "lcov"],
      reportsDirectory: "coverage",
      thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
      // Sem `include`, o vitest 4 só conta arquivo que algum teste importa — um
      // módulo esquecido sumiria do denominador em vez de derrubar o piso.
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.test.{ts,tsx}", "src/test/**", "src/demo/**", "src/main.tsx", "src/**/*.d.ts"],
    },
  },
}));
